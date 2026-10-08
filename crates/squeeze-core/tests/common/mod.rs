use image::{DynamicImage, RgbImage, codecs::jpeg::JpegEncoder};
use lopdf::{Document, Object, ObjectId, Stream, dictionary};
#[allow(dead_code)]
pub struct Fixture {
    pub doc: Document,
    pub page: ObjectId,
    pub image: ObjectId,
    pub content: ObjectId,
    pub resources: ObjectId,
}
pub fn fixture(jpeg: bool) -> Fixture {
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let font =
        doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
    let mut pixels = Vec::with_capacity(1200 * 1600 * 3);
    for y in 0..1600u32 {
        for x in 0..1200u32 {
            let stripe = if ((x / 30) + (y / 30)) % 2 == 0 {
                24
            } else {
                0
            };
            pixels.extend_from_slice(&[
                ((x * 220 / 1200) as u8).saturating_add(stripe),
                ((y * 180 / 1600) as u8).saturating_add(stripe),
                160,
            ]);
        }
    }
    let mut dict = dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>1200,"Height"=>1600,"ColorSpace"=>"DeviceRGB","BitsPerComponent"=>8};
    let bytes = if jpeg {
        let mut bytes = Vec::new();
        let image = DynamicImage::ImageRgb8(RgbImage::from_raw(1200, 1600, pixels).unwrap());
        JpegEncoder::new_with_quality(&mut bytes, 95)
            .encode_image(&image)
            .unwrap();
        dict.set("Filter", "DCTDecode");
        bytes
    } else {
        pixels
    };
    let image = doc.add_object(Stream::new(dict, bytes));
    let resources = doc.add_object(
        dictionary! {"Font"=>dictionary! {"F1"=>font},"XObject"=>dictionary! {"Photo"=>image}},
    );
    let mut body=b"BT /F1 18 Tf 30 470 Td (Squeeze PDF - Rust engine sample) Tj ET\nq 300 0 0 400 30 30 cm /Photo Do Q\n".to_vec();
    for _ in 0..200 {
        body.extend_from_slice(b"% repetitive non-rendering comment for lossless verification\n");
    }
    let content = doc.add_object(Stream::new(dictionary! {}, body));
    let page=doc.add_object(dictionary! {"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),360.into(),510.into()],"Resources"=>resources,"Contents"=>content});
    doc.objects.insert(
        pages,
        Object::Dictionary(dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}),
    );
    let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    doc.trailer.set("Root", root);
    let info=doc.add_object(dictionary! {"Title"=>Object::string_literal("Rust compression sample"),"Author"=>Object::string_literal("Squeeze PDF")});
    doc.trailer.set("Info", info);
    doc.trailer.set(
        "ID",
        vec![
            Object::string_literal("squeeze-fixture-identifier"),
            Object::string_literal("squeeze-fixture-identifier"),
        ],
    );
    Fixture {
        doc,
        page,
        image,
        content,
        resources,
    }
}
