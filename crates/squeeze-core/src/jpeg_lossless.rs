//! jpegtran-style entropy recoding. No inverse DCT, resampling or quantization.
//! Native structs and callbacks stay boxed for stable addresses; C-unwind is
//! explicitly supported by mozjpeg-sys. Both native pools are destroyed on all
//! paths, including errors; the bounded destination is owned entirely by Rust.
use mozjpeg_sys as ffi;
use std::{ffi::c_int, mem, ptr};

type JpegResult<T> = std::result::Result<T, String>;
#[repr(C)]
struct Destination {
    manager: ffi::jpeg_destination_mgr,
    buffer: [u8; 16_384],
    output: Vec<u8>,
    limit: usize,
}
impl Destination {
    fn append(&mut self, count: usize) {
        if self.output.len().saturating_add(count) > self.limit {
            std::panic::resume_unwind(Box::new("JPEG output exceeds limit"));
        }
        self.output.extend_from_slice(&self.buffer[..count]);
    }
}
unsafe extern "C-unwind" fn init(c: &mut ffi::jpeg_compress_struct) {
    // SAFETY: dest always points to the first repr(C) field of our boxed state.
    let d = unsafe { &mut *c.dest.cast::<Destination>() };
    d.output.clear();
    d.manager.next_output_byte = d.buffer.as_mut_ptr();
    d.manager.free_in_buffer = d.buffer.len();
}
unsafe extern "C-unwind" fn empty(c: &mut ffi::jpeg_compress_struct) -> c_int {
    let d = unsafe { &mut *c.dest.cast::<Destination>() };
    d.append(d.buffer.len());
    d.manager.next_output_byte = d.buffer.as_mut_ptr();
    d.manager.free_in_buffer = d.buffer.len();
    1
}
unsafe extern "C-unwind" fn term(c: &mut ffi::jpeg_compress_struct) {
    let d = unsafe { &mut *c.dest.cast::<Destination>() };
    d.append(d.buffer.len() - d.manager.free_in_buffer);
}
unsafe extern "C-unwind" fn fatal(_: &mut ffi::jpeg_common_struct) {
    std::panic::resume_unwind(Box::new("Invalid JPEG coefficient stream"));
}
unsafe extern "C-unwind" fn message(_: &mut ffi::jpeg_common_struct, level: c_int) {
    // Truncated streams must not be silently repaired and called lossless.
    if level < 0 {
        std::panic::resume_unwind(Box::new("JPEG coefficient warning"));
    }
}
struct Native {
    decoder: Box<ffi::jpeg_decompress_struct>,
    encoder: Box<ffi::jpeg_compress_struct>,
    decode_error: Box<ffi::jpeg_error_mgr>,
    encode_error: Box<ffi::jpeg_error_mgr>,
    destination: Box<Destination>,
}
impl Native {
    fn new(limit: usize) -> Self {
        // SAFETY: libjpeg requires zero-initialized structs before creation.
        unsafe {
            let mut native = Self {
                decoder: Box::new(mem::zeroed()),
                encoder: Box::new(mem::zeroed()),
                decode_error: Box::new(mem::zeroed()),
                encode_error: Box::new(mem::zeroed()),
                destination: Box::new(Destination {
                    manager: mem::zeroed(),
                    buffer: [0; 16_384],
                    output: Vec::new(),
                    limit,
                }),
            };
            ffi::jpeg_std_error(&mut native.decode_error);
            ffi::jpeg_std_error(&mut native.encode_error);
            for err in [&mut native.decode_error, &mut native.encode_error] {
                err.error_exit = Some(fatal);
                err.emit_message = Some(message);
            }
            native.decoder.common.err = &mut *native.decode_error;
            native.encoder.common.err = &mut *native.encode_error;
            ffi::jpeg_create_decompress(&mut *native.decoder);
            ffi::jpeg_create_compress(&mut *native.encoder);
            native.destination.manager.init_destination = Some(init);
            native.destination.manager.empty_output_buffer = Some(empty);
            native.destination.manager.term_destination = Some(term);
            native.encoder.dest = &mut native.destination.manager;
            native
        }
    }
    unsafe fn coefficients(
        &mut self,
        bytes: &[u8],
        dimensions: [u32; 2],
        max_pixels: u64,
    ) -> JpegResult<*mut *mut ffi::jvirt_barray_control> {
        unsafe {
            ffi::jpeg_mem_src(
                &mut self.decoder,
                bytes.as_ptr(),
                bytes.len().try_into().map_err(|_| "JPEG input too large")?,
            );
            for marker in (0xe0..=0xef).chain([0xfe]) {
                ffi::jpeg_save_markers(&mut self.decoder, marker, 0xffff);
            }
            if ffi::jpeg_read_header(&mut self.decoder, 1) != 1 {
                return Err("JPEG header invalid".into());
            }
            let d = &self.decoder;
            if [d.image_width, d.image_height] != dimensions
                || !matches!(d.num_components, 1 | 3)
                || !matches!(
                    d.jpeg_color_space,
                    ffi::JCS_GRAYSCALE | ffi::JCS_RGB | ffi::JCS_YCbCr
                )
                || dimensions[0] as u64 * dimensions[1] as u64 > max_pixels
            {
                return Err("Unsupported JPEG dimensions or color".into());
            }
            // Bound coefficient buffers separately from decoded 8-bit pixels.
            let padded =
                (dimensions[0] as u64).div_ceil(8) * 8 * ((dimensions[1] as u64).div_ceil(8) * 8);
            if padded * d.num_components as u64 * 2 > self.destination.limit as u64 {
                return Err("JPEG coefficients exceed limit".into());
            }
            let coefficients = ffi::jpeg_read_coefficients(&mut self.decoder);
            if coefficients.is_null() {
                return Err("Missing JPEG coefficients".into());
            }
            Ok(coefficients)
        }
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        // Creation can fail partway; non-null mem identifies allocated pools.
        unsafe {
            if !self.encoder.common.mem.is_null() {
                ffi::jpeg_destroy_compress(&mut self.encoder);
            }
            if !self.decoder.common.mem.is_null() {
                ffi::jpeg_destroy_decompress(&mut self.decoder);
            }
        }
    }
}
pub(crate) fn optimize(
    bytes: &[u8],
    dimensions: [u32; 2],
    max_pixels: u64,
    limit: usize,
) -> JpegResult<Vec<(&'static str, Vec<u8>)>> {
    if bytes.len() > limit || !bytes.starts_with(&[0xff, 0xd8]) || dimensions.contains(&0) {
        return Err("Invalid JPEG input".into());
    }
    std::panic::catch_unwind(|| {
        let mut native = Native::new(limit);
        // SAFETY: boxes keep native addresses alive through both encodes; the
        // shared virtual coefficient arrays belong to the still-live decoder.
        unsafe {
            let coefficients = native.coefficients(bytes, dimensions, max_pixels)?;
            let mut outputs = Vec::new();
            for progressive in [false, true] {
                let c = &mut native.encoder;
                ffi::jpeg_copy_critical_parameters(&native.decoder, c);
                c.optimize_coding = 1;
                c.scan_info = ptr::null();
                c.num_scans = 0;
                c.write_JFIF_header = 0;
                c.write_Adobe_marker = 0;
                ffi::jpeg_c_set_bool_param(c, ffi::JBOOLEAN_TRELLIS_QUANT, 0);
                ffi::jpeg_c_set_bool_param(c, ffi::JBOOLEAN_TRELLIS_QUANT_DC, 0);
                ffi::jpeg_c_set_bool_param(c, ffi::JBOOLEAN_TRELLIS_Q_OPT, 0);
                ffi::jpeg_c_set_bool_param(c, ffi::JBOOLEAN_OPTIMIZE_SCANS, progressive.into());
                if progressive {
                    ffi::jpeg_simple_progression(c);
                }
                ffi::jpeg_write_coefficients(c, coefficients);
                let mut marker = native.decoder.marker_list;
                while let Some(m) = marker.as_ref() {
                    ffi::jpeg_write_marker(c, m.marker as c_int, m.data, m.data_length);
                    marker = m.next;
                }
                ffi::jpeg_finish_compress(c);
                outputs.push((
                    if progressive {
                        "jpeg-lossless/progressive"
                    } else {
                        "jpeg-lossless/sequential"
                    },
                    mem::take(&mut native.destination.output),
                ));
                ffi::jpeg_abort_compress(c);
            }
            ffi::jpeg_finish_decompress(&mut native.decoder);
            Ok(outputs)
        }
    })
    .map_err(|_| "JPEG coefficient optimization failed".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    type Fingerprint = (Vec<i16>, Vec<u16>, Vec<i32>, Vec<(u8, Vec<u8>)>);
    fn fingerprint(bytes: &[u8], dims: [u32; 2]) -> Fingerprint {
        let mut n = Native::new(16 * 1024 * 1024);
        unsafe {
            let arrays = n.coefficients(bytes, dims, 1_000_000).unwrap();
            let mut quant = Vec::new();
            let mut sampling = Vec::new();
            let mut coefs = Vec::new();
            let mut markers = Vec::new();
            for &q in &n.decoder.quant_tbl_ptrs {
                if let Some(q) = q.as_ref() {
                    quant.extend(q.quantval);
                }
            }
            let access = (*n.decoder.common.mem).access_virt_barray.unwrap();
            for i in 0..n.decoder.num_components as usize {
                let comp = &*n.decoder.comp_info.add(i);
                sampling.extend([
                    comp.component_id,
                    comp.h_samp_factor,
                    comp.v_samp_factor,
                    comp.quant_tbl_no,
                ]);
                for y in 0..comp.height_in_blocks {
                    let row = access(&mut n.decoder.common, *arrays.add(i), y, 1, 0);
                    for x in 0..comp.width_in_blocks as usize {
                        coefs.extend(*(*row).add(x));
                    }
                }
            }
            let mut marker = n.decoder.marker_list;
            while let Some(m) = marker.as_ref() {
                markers.push((
                    m.marker,
                    std::slice::from_raw_parts(m.data, m.data_length as usize).to_vec(),
                ));
                marker = m.next;
            }
            (coefs, quant, sampling, markers)
        }
    }
    #[test]
    fn entropy_recode_keeps_coefficients_quantization_sampling_markers_and_pixels() {
        for gray in [false, true] {
            for quality in [60, 95] {
                let pixels: Vec<_> = (0..47 * 33 * if gray { 1 } else { 3 })
                    .map(|i| ((i * 37 + i / 47 * 19) % 256) as u8)
                    .collect();
                let mut original = crate::jpeg::encode(&pixels, 47, 33, gray, quality).unwrap();
                // ICC-like APP2, EXIF-like APP1 and COM payloads including zeros.
                for (tag, data) in [
                    (0xe2, &b"ICC_PROFILE\0\x01\x01exact profile"[..]),
                    (0xe1, &b"Exif\0\0exact metadata"[..]),
                    (0xfe, &b"comment\0"[..]),
                ] {
                    let mut segment = vec![0xff, tag];
                    segment.extend(((data.len() + 2) as u16).to_be_bytes());
                    segment.extend(data);
                    original.splice(2..2, segment);
                }
                let before = fingerprint(&original, [47, 33]);
                for (_, out) in optimize(&original, [47, 33], 1_000_000, 16 * 1024 * 1024).unwrap()
                {
                    assert_eq!(fingerprint(&out, [47, 33]), before);
                    assert_eq!(
                        image::load_from_memory(&out).unwrap().into_bytes(),
                        image::load_from_memory(&original).unwrap().into_bytes()
                    );
                }
            }
        }
    }
    #[test]
    fn rejects_truncation_mismatched_dimensions_and_memory_budget() {
        let jpg = crate::jpeg::encode(&vec![128; 64 * 64 * 3], 64, 64, false, 85).unwrap();
        for (bytes, dims, limit) in [
            (&jpg[..jpg.len() / 2], [64, 64], 1_000_000),
            (&jpg, [63, 64], 1_000_000),
            (&jpg, [64, 64], 4096),
        ] {
            assert!(optimize(bytes, dims, 1_000_000, limit).is_err());
        }
    }
}
