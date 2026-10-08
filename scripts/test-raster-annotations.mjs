import assert from "node:assert/strict";
import { resolve } from "node:path";
import { mkdirSync, writeFileSync } from "node:fs";
import { getDocument } from "pdfjs-dist/legacy/build/pdf.mjs";
import { annotationFixture } from "./pdf-annotation-fixtures.mjs";
import {
  prepareRasterAnnotations,
  rasterRenderOptions,
} from "../src/rasterAnnotations.ts";
import { assertRenderablePage } from "../src/renderValidation.ts";

const sourceIds = [6, 7, 8, 9, 10, 11, 12].map((id) => `${id}R`);
for (const variant of ["valid", "sticky", "broken"]) {
  const data = annotationFixture({
    sticky: variant === "sticky",
    broken: variant === "broken",
  });
  if (process.argv.includes("--write-fixtures")) {
    mkdirSync("tmp/interactive-0.1.7", { recursive: true });
    writeFileSync(`tmp/interactive-0.1.7/${variant}.pdf`, data);
  }
  const task = getDocument({
    data,
    stopAtErrors: true,
    useSystemFonts: false,
    standardFontDataUrl:
      resolve("node_modules/pdfjs-dist/standard_fonts") + "/",
  });
  try {
    const doc = await task.promise;
    const page = await doc.getPage(1);
    if (variant !== "valid") {
      await assert.rejects(
        () =>
          prepareRasterAnnotations(page, doc.annotationStorage, 1, sourceIds),
        /第 1 页的批注或表单无法完整转为图片/,
      );
      continue;
    }
    await assert.rejects(
      () =>
        prepareRasterAnnotations(page, doc.annotationStorage, 1, [
          ...sourceIds,
          "999R",
        ]),
      /无法完整转为图片/,
    );
    await prepareRasterAnnotations(page, doc.annotationStorage, 1, sourceIds);
    const canvas = doc.canvasFactory.create(400, 300);
    try {
      await page.render({
        ...rasterRenderOptions,
        canvas: canvas.canvas,
        canvasContext: canvas.context,
        viewport: page.getViewport({ scale: 1 }),
        background: "rgb(255,255,255)",
      }).promise;
      assertRenderablePage(page, 1);
      const rgb = (x, y) =>
        Array.from(canvas.context.getImageData(x, 300 - y, 1, 1).data).slice(
          0,
          3,
        );
      const ink = (x, y, w, h) => {
        const pixels = canvas.context.getImageData(x, 300 - y - h, w, h).data;
        let count = 0;
        for (let i = 0; i < pixels.length; i += 4)
          if (pixels[i] < 120 && pixels[i + 1] < 120 && pixels[i + 2] < 120)
            count++;
        return count;
      };
      assert.ok(
        ink(20, 240, 200, 40) > 150,
        "appearance field value must be present",
      );
      assert.ok(
        ink(20, 180, 220, 40) > 300,
        "missing appearance must be generated from the stored field value",
      );
      assert.deepEqual(
        rgb(310, 250),
        [0, 153, 0],
        "checkbox state must be retained",
      );
      assert.deepEqual(
        rgb(100, 142),
        [255, 255, 0],
        "non-printable visible highlight must be retained",
      );
      assert.deepEqual(
        rgb(310, 145),
        [0, 0, 255],
        "own-canvas comment appearance must be burned into the page",
      );
      assert.equal(
        ink(20, 10, 200, 40),
        0,
        "hidden field values must not be exposed",
      );
      if (process.argv.includes("--write-fixtures"))
        writeFileSync(
          "tmp/interactive-0.1.7/valid-render.png",
          canvas.canvas.toBuffer("image/png"),
        );
    } finally {
      doc.canvasFactory.destroy(canvas);
    }
  } finally {
    await task.destroy();
  }
}
console.log(
  "PDF.js link, form appearances, generated values, checkbox, highlight, hidden field and incomplete annotation checks passed.",
);
