// Independent exact source/previous-output checks for stream transformations.
import assert from "node:assert/strict";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { getDocument } from "pdfjs-dist/legacy/build/pdf.mjs";
import { assertRenderablePage } from "../src/renderValidation.ts";
const root = resolve(process.argv[2]),
  rows = JSON.parse(readFileSync(join(root, "reports.json"))),
  cache = new Map(),
  warnings = [];
const log = console.log.bind(console);
console.log = (...args) => {
  if (args.some((s) => typeof s === "string" && s.startsWith("Warning:")))
    warnings.push(args.join(" "));
  log(...args);
};
async function render(path, pageNumber) {
  const key = path + "#" + pageNumber;
  if (cache.has(key)) return cache.get(key);
  const task = getDocument({
    data: new Uint8Array(readFileSync(path)),
    stopAtErrors: true,
    useSystemFonts: false,
    standardFontDataUrl:
      resolve("node_modules/pdfjs-dist/standard_fonts") + "/",
    wasmUrl: resolve("node_modules/pdfjs-dist/wasm") + "/",
    cMapUrl: resolve("node_modules/pdfjs-dist/cmaps") + "/",
    cMapPacked: true,
  });
  try {
    const doc = await task.promise;
    const page = await doc.getPage(pageNumber),
      viewport = page.getViewport({ scale: 200 / 72 }),
      target = doc.canvasFactory.create(
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
      assertRenderablePage(page, pageNumber);
      const result = {
        pixels: target.context.getImageData(
          0,
          0,
          target.canvas.width,
          target.canvas.height,
        ).data,
        text: JSON.stringify(
          (await page.getTextContent()).items.map((i) => ({
            str: i.str,
            transform: i.transform,
            width: i.width,
            height: i.height,
          })),
        ),
      };
      cache.set(key, result);
      if (cache.size > 8) cache.delete(cache.keys().next().value);
      return result;
    } finally {
      doc.canvasFactory.destroy(target);
      page.cleanup();
    }
  } finally {
    await task.destroy();
  }
}
const results = [];
for (const row of rows.filter((r) => r.version === "latest")) {
  const old = rows.find(
      (r) =>
        r.version === "baseline" &&
        r.name === row.name &&
        r.mode === row.mode &&
        r.preset === row.preset,
    ),
    output = row.output || row.input;
  function rmse(a, b) {
    assert.equal(a.length, b.length);
    let sum = 0;
    for (let i = 0; i < a.length; i++)
      if (i % 4 !== 3) sum += (a[i] - b[i]) ** 2;
    return Math.sqrt(sum / ((a.length * 3) / 4));
  }
  for (let page = 1; page <= row.pages; page++) {
    const a = await render(old.output || old.input, page),
      b = await render(output, page),
      source = await render(row.input, page);
    assert.equal(a.text, b.text, `${row.name} page ${page} text geometry`);
    if (row.mode === "preserve")
      assert.equal(
        source.text,
        b.text,
        `${row.name} page ${page} source text geometry`,
      );
    const difference = rmse(a.pixels, b.pixels),
      sourceError = rmse(source.pixels, b.pixels),
      baselineError = rmse(source.pixels, a.pixels);
    if (row.mode === "preserve" && row.preset === "lossless")
      assert.equal(sourceError, 0, `${row.name} page ${page} lossless pixels`);
    const newlyEligibleLossy =
      row.mode === "preserve" &&
      ["balanced", "small"].includes(row.preset) &&
      (row.image_preparation?.changed > 0 ||
        row.advanced_optimization?.transparency?.changed > 0);
    if (difference !== 0 && !newlyEligibleLossy)
      assert(
        sourceError <= baselineError + 1e-6,
        `${row.name}/${row.mode}/${row.preset} page ${page} error ${sourceError} > ${baselineError}`,
      );
    results.push({
      name: row.name,
      mode: row.mode,
      preset: row.preset,
      page,
      previous_pixels_equal: difference === 0,
      previous_text_positions_equal: true,
      source_pixels_equal: sourceError === 0,
      source_rmse: sourceError,
      baseline_source_rmse: baselineError,
      newly_eligible_lossy: newlyEligibleLossy,
    });
  }
}
assert.deepEqual(warnings, [], "Unexpected PDF.js warnings");
mkdirSync(join(root, "pdfjs"), { recursive: true });
writeFileSync(
  join(root, "pdfjs", "results.json"),
  JSON.stringify(results, null, 2),
);
log(
  `${results.length} previous-output checks and ${results.filter((r) => r.source_pixels_equal).length} source pixel/text/position checks passed without warnings`,
);
