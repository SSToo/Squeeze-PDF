// Development-only lossless format probe. Node, PDF.js and its canvas backend.
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { createHash } from "node:crypto";
import { join, resolve } from "node:path";
import { getDocument } from "pdfjs-dist/legacy/build/pdf.mjs";
import { assertRenderablePage } from "../src/renderValidation.ts";

const root = process.argv[2];
if (!root) throw new Error("experiment-directory");
const results = [];
for (const name of [
  "document-a",
  "document-b",
  "font",
  "raster",
  "binary",
  "interactive",
]) {
  const tasks = [];
  const row = { name, reader: "PDF.js", pages: [], errors: [] };
  try {
    const docs = [];
    for (const variant of ["baseline", "brotli"]) {
      const task = getDocument({
        data: new Uint8Array(readFileSync(join(root, name, `${variant}.pdf`))),
        stopAtErrors: true,
        useSystemFonts: false,
        standardFontDataUrl:
          resolve("node_modules/pdfjs-dist/standard_fonts") + "/",
        wasmUrl: resolve("node_modules/pdfjs-dist/wasm") + "/",
      });
      tasks.push(task);
      docs.push(await task.promise);
    }
    if (docs[0].numPages !== docs[1].numPages)
      throw new Error("page count differs");
    const dpi = docs[0].numPages > 1 ? 100 : 200;
    row.dpi = dpi;
    for (let number = 1; number <= docs[0].numPages; number++) {
      const arrays = [];
      try {
        for (const doc of docs) {
          const page = await doc.getPage(number);
          const viewport = page.getViewport({ scale: dpi / 72 });
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
            assertRenderablePage(page, number);
            arrays.push({
              width: target.canvas.width,
              height: target.canvas.height,
              data: target.context.getImageData(
                0,
                0,
                target.canvas.width,
                target.canvas.height,
              ).data,
              text: (await page.getTextContent()).items.map(
                (item) => item.str ?? "",
              ),
            });
          } finally {
            doc.canvasFactory.destroy(target);
            page.cleanup();
          }
        }
        const [a, b] = arrays;
        if (a.width !== b.width || a.height !== b.height)
          throw new Error("page dimensions differ");
        if (name === "font" && number === 1) {
          // Exercise the real PDF.js -> staged-pixels -> worker boundary after
          // the CLI/hayro incompatibility guard has been added.
          const directory = join(root, name, "staged-brotli");
          mkdirSync(directory, { recursive: true });
          const rgb = Buffer.alloc(b.width * b.height * 3);
          for (let p = 0, q = 0; p < b.data.length; p += 4, q += 3) {
            rgb[q] = b.data[p];
            rgb[q + 1] = b.data[p + 1];
            rgb[q + 2] = b.data[p + 2];
          }
          writeFileSync(join(directory, "0.rgb"), rgb);
          writeFileSync(
            join(directory, "manifest.json"),
            JSON.stringify({
              source_sha256: createHash("sha256")
                .update(readFileSync(join(root, name, "brotli.pdf")))
                .digest("hex"),
              pages: 1,
            }),
          );
        }
        let changed = 0,
          sum = 0,
          ink = 0;
        for (let p = 0; p < a.data.length; p += 4) {
          let altered = false;
          if (a.data[p] < 250 || a.data[p + 1] < 250 || a.data[p + 2] < 250)
            ink++;
          for (let c = 0; c < 3; c++) {
            const d = a.data[p + c] - b.data[p + c];
            sum += d * d;
            altered ||= d !== 0;
          }
          if (altered) changed++;
        }
        row.pages.push({
          number,
          equal: changed === 0,
          text_equal: JSON.stringify(a.text) === JSON.stringify(b.text),
          rmse: Math.sqrt(sum / ((a.data.length / 4) * 3)),
          changed_pixels: changed,
          baseline_ink_pixels: ink,
        });
      } catch (error) {
        row.errors.push({ number, message: String(error) });
      }
    }
    row.passed =
      row.errors.length === 0 &&
      row.pages.length === docs[0].numPages &&
      row.pages.every((p) => p.equal && p.text_equal);
  } catch (error) {
    row.errors.push({ message: String(error) });
    row.passed = false;
  } finally {
    for (const task of tasks) await task.destroy();
  }
  results.push(row);
  writeFileSync(
    join(root, "pdfjs-verification.json"),
    JSON.stringify(results, null, 2) + "\n",
  );
  console.log(
    JSON.stringify({
      name,
      passed: row.passed,
      tested: row.pages.length,
      errors: row.errors.length,
      differing: row.pages.filter((p) => !p.equal).length,
    }),
  );
}
