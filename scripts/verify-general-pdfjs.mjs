// Independent PDF.js image and text validation at 200 DPI.
import assert from "node:assert/strict";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { getDocument } from "pdfjs-dist/legacy/build/pdf.mjs";
import { assertRenderablePage } from "../src/renderValidation.ts";
const root = resolve(process.argv[2]),
  rows = JSON.parse(readFileSync(join(root, "reports.json"), "utf8"));
const directory = join(root, "pdfjs");
mkdirSync(directory, { recursive: true });
const cache = new Map();
async function load(path) {
  if (cache.has(path)) return cache.get(path);
  const task = getDocument({
    data: new Uint8Array(readFileSync(path)),
    stopAtErrors: true,
    useSystemFonts: false,
    standardFontDataUrl:
      resolve("node_modules/pdfjs-dist/standard_fonts") + "/",
    wasmUrl: resolve("node_modules/pdfjs-dist/wasm") + "/",
  });
  try {
    const doc = await task.promise;
    assert.equal(doc.numPages, 1);
    const page = await doc.getPage(1),
      viewport = page.getViewport({ scale: 200 / 72 });
    const canvas = doc.canvasFactory.create(
      Math.ceil(viewport.width),
      Math.ceil(viewport.height),
    );
    try {
      await page.render({
        canvas: canvas.canvas,
        canvasContext: canvas.context,
        viewport,
        background: "rgb(255,255,255)",
      }).promise;
      assertRenderablePage(page, 1);
      const result = {
        pixels: canvas.context.getImageData(
          0,
          0,
          canvas.canvas.width,
          canvas.canvas.height,
        ).data,
        text: JSON.stringify(
          (await page.getTextContent()).items.map((i) => ({
            s: i.str,
            t: i.transform,
            w: i.width,
            h: i.height,
          })),
        ),
      };
      cache.set(path, result);
      return result;
    } finally {
      doc.canvasFactory.destroy(canvas);
      page.cleanup();
    }
  } finally {
    await task.destroy();
  }
}
const quality = [];
for (const r of rows.filter(
  (r) =>
    r.version === "latest" &&
    r.mode === "preserve" &&
    ["lossless", "fidelity"].includes(r.preset),
)) {
  const a = await load(r.input),
    b = await load(r.output || r.input);
  assert.equal(a.text, b.text);
  assert.equal(a.pixels.length, b.pixels.length);
  assert.deepEqual(a.pixels, b.pixels, `${r.name} ${r.preset} differs`);
  quality.push({
    name: r.name,
    preset: r.preset,
    pixel_equal: true,
    text_equal: true,
  });
}
writeFileSync(
  join(directory, "results.json"),
  JSON.stringify(quality, null, 2) + "\n",
);
console.log(
  `${quality.length} preserve output pages match source pixels and text in PDF.js`,
);
