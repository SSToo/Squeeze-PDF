export function annotationFixture({ sticky = false, broken = false } = {}) {
  const stream = (content, extra = "") =>
    `<< /Length ${Buffer.byteLength(content)} ${extra} >>\nstream\n${content}\nendstream`;
  const appearance = (content, w, h, extra = "") =>
    stream(
      content,
      `/Type /XObject /Subtype /Form /BBox [0 0 ${w} ${h}] ${extra}`,
    );
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [6 0 R 7 0 R 8 0 R] /DR << /Font << /F1 5 0 R >> >> /DA (/F1 18 Tf 0 g) >> >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 300] /Resources << >> /Contents 4 0 R /Annots [6 0 R 7 0 R 8 0 R 9 0 R 10 0 R 11 0 R 12 0 R${sticky ? " 17 0 R" : ""}] >>`,
    stream("q Q"),
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    "<< /Type /Annot /Subtype /Widget /FT /Tx /T (with-appearance) /V (AP VALUE) /Rect [20 240 220 280] /F 4 /AP << /N 13 0 R >> >>",
    "<< /Type /Annot /Subtype /Widget /FT /Tx /T (without-appearance) /V (GENERATED VALUE) /Rect [20 180 240 220] /F 4 /Ff 1 /DA (/F1 18 Tf 0 g) >>",
    "<< /Type /Annot /Subtype /Widget /FT /Btn /T (checked) /V /Yes /AS /Yes /Rect [300 240 330 270] /F 4 /AP << /N << /Yes 14 0 R /Off 15 0 R >> >> >>",
    "<< /Type /Annot /Subtype /Highlight /Rect [20 130 220 155] /QuadPoints [20 155 220 155 20 130 220 130] /C [1 1 0] >>",
    "<< /Type /Annot /Subtype /Text /Rect [300 130 330 160] /F 4 /AP << /N 16 0 R >> /Contents (comment popup is not burned into the page) >>",
    "<< /Type /Annot /Subtype /Link /Rect [20 70 220 110] /Border [0 0 0] /A << /S /URI /URI (https://example.invalid/) >> >>",
    "<< /Type /Annot /Subtype /Widget /FT /Tx /T (hidden) /V (HIDDEN SECRET) /Rect [20 10 220 50] /F 34 /AP << /N 13 0 R >> >>",
    appearance(
      "BT /F1 18 Tf 4 12 Td (AP VALUE) Tj ET",
      200,
      40,
      "/Resources << /Font << /F1 5 0 R >> >>",
    ),
    appearance("0 0.6 0 rg 0 0 30 30 re f", 30, 30),
    appearance("1 1 1 rg 0 0 30 30 re f", 30, 30),
    broken
      ? appearance(
          "/Broken Do",
          30,
          30,
          "/Resources << /XObject << /Broken null >> >>",
        )
      : appearance("0 0 1 rg 0 0 30 30 re f", 30, 30),
    "<< /Type /Annot /Subtype /Text /Rect [260 70 280 90] /Contents (HTML-only sticky note) >>",
  ];
  const parts = [Buffer.from("%PDF-1.7\n")];
  const offsets = [];
  let length = parts[0].length;
  for (const [i, object] of objects.entries()) {
    offsets.push(length);
    const bytes = Buffer.from(`${i + 1} 0 obj\n${object}\nendobj\n`);
    parts.push(bytes);
    length += bytes.length;
  }
  parts.push(
    Buffer.from(
      `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n${offsets.map((n) => String(n).padStart(10, "0") + " 00000 n \n").join("")}trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${length}\n%%EOF\n`,
    ),
  );
  return new Uint8Array(Buffer.concat(parts));
}
