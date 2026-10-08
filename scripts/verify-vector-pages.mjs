import assert from "node:assert/strict";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { resolve, join } from "node:path";
import { getDocument } from "pdfjs-dist/legacy/build/pdf.mjs";
import { assertRenderablePage } from "../src/renderValidation.ts";

const [before, after, directory, pageList] = process.argv.slice(2);
assert.ok(
  before && after && directory && pageList,
  "before after directory comma-separated-pages",
);
mkdirSync(directory, { recursive: true });
const open = async (path) => {
  const task = getDocument({
    data: new Uint8Array(readFileSync(path)),
    stopAtErrors: true,
    useSystemFonts: false,
    standardFontDataUrl:
      resolve("node_modules/pdfjs-dist/standard_fonts") + "/",
    wasmUrl: resolve("node_modules/pdfjs-dist/wasm") + "/",
  });
  return [task, await task.promise];
};
const [taskA, a] = await open(before);
const [taskB, b] = await open(after);
const rows = [];
try {
  assert.equal(a.numPages, b.numPages);
  for (const index of pageList.split(",").map(Number)) {
    const render = async (doc, label) => {
      const page = await doc.getPage(index);
      const viewport = page.getViewport({ scale: 200 / 72 });
      const target = doc.canvasFactory.create(
        Math.ceil(viewport.width),
        Math.ceil(viewport.height),
      );
      try {
        await page.render({
          canvas: target.canvas,
          canvasContext: target.context,
          viewport,
          background: "rgb(255,255,255)",
        }).promise;
        assertRenderablePage(page, index);
        writeFileSync(
          join(directory, `${index}-${label}.png`),
          target.canvas.toBuffer("image/png"),
        );
        return target.context.getImageData(
          0,
          0,
          target.canvas.width,
          target.canvas.height,
        ).data;
      } finally {
        doc.canvasFactory.destroy(target);
        page.cleanup();
      }
    };
    const x = await render(a, "before"),
      y = await render(b, "after");
    assert.equal(x.length, y.length);
    let sum = 0,
      max = 0,
      changed = 0;
    for (let p = 0; p < x.length; p += 4) {
      let altered = false;
      for (let c = 0; c < 3; c++) {
        const d = Math.abs(x[p + c] - y[p + c]);
        sum += d * d;
        max = Math.max(max, d);
        altered ||= d > 0;
      }
      if (altered) changed++;
    }
    const row = {
      page: index,
      rmse: Math.sqrt(sum / ((x.length / 4) * 3)),
      max_error: max,
      changed_fraction: changed / (x.length / 4),
    };
    rows.push(row);
    console.log(row);
    // Catch a broad content/layout regression; local inspection is still required.
    assert.ok(row.rmse < 2 && row.changed_fraction < 0.02, JSON.stringify(row));
  }
  writeFileSync(join(directory, "results.json"), JSON.stringify(rows, null, 2));
} finally {
  await taskA.destroy();
  await taskB.destroy();
}
