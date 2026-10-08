import assert from "node:assert/strict";
import { getDocument } from "pdfjs-dist/legacy/build/pdf.mjs";
import { assertRenderablePage } from "../src/renderValidation.ts";
import "./test-raster-annotations.mjs";

function fixture(corrupt) {
  const image = corrupt
    ? Buffer.from("invalid JPEG bytes")
    : Buffer.from([240, 80, 20]);
  const body = "q 10 0 0 10 0 0 cm /Image Do Q";
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] /Resources << /XObject << /Image 5 0 R >> >> /Contents 4 0 R >>",
    `<< /Length ${body.length} >>\nstream\n${body}\nendstream`,
    Buffer.concat([
      Buffer.from(
        `<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 8 ${corrupt ? "/Filter /DCTDecode " : ""}/Length ${image.length} >>\nstream\n`,
      ),
      image,
      Buffer.from("\nendstream"),
    ]),
  ];
  const parts = [Buffer.from("%PDF-1.7\n")],
    offsets = [0];
  let length = parts[0].length;
  for (const [i, object] of objects.entries()) {
    offsets.push(length);
    const bytes = Buffer.concat([
      Buffer.from(`${i + 1} 0 obj\n`),
      Buffer.from(object),
      Buffer.from("\nendobj\n"),
    ]);
    parts.push(bytes);
    length += bytes.length;
  }
  parts.push(
    Buffer.from(
      `xref\n0 6\n0000000000 65535 f \n${offsets
        .slice(1)
        .map((n) => String(n).padStart(10, "0") + " 00000 n \n")
        .join(
          "",
        )}trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n${length}\n%%EOF\n`,
    ),
  );
  return new Uint8Array(Buffer.concat(parts));
}

for (const corrupt of [false, true]) {
  const task = getDocument({ data: fixture(corrupt), stopAtErrors: true });
  try {
    const doc = await task.promise;
    const page = await doc.getPage(1);
    const canvas = doc.canvasFactory.create(10, 10);
    try {
      await page.render({
        canvas: canvas.canvas,
        canvasContext: canvas.context,
        viewport: page.getViewport({ scale: 1 }),
        background: "rgb(255,255,255)",
      }).promise;
      if (corrupt)
        assert.throws(() => assertRenderablePage(page, 1), /无法解码/);
      else {
        assertRenderablePage(page, 1);
        assert.ok(canvas.context.getImageData(5, 5, 1, 1).data[0] < 250);
      }
    } finally {
      doc.canvasFactory.destroy(canvas);
    }
  } finally {
    await task.destroy();
  }
}
for (const [local, shared] of [
  [[null], []],
  [[], [null]],
  [[], ["font loading error"]],
]) {
  assert.throws(
    () =>
      assertRenderablePage(
        {
          objs: local.map((v, i) => [i, v]),
          commonObjs: shared.map((v, i) => [i, v]),
        },
        3,
      ),
    /第 3 页/,
  );
}
console.log(
  "PDF.js valid pixels, silent image failure, and shared/font failure checks passed.",
);
