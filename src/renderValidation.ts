import type { PDFPageProxy } from "pdfjs-dist/legacy/build/pdf.mjs";

// PDF.js resolves failed asynchronous image decodes to null even when
// stopAtErrors is true. Rendering can then succeed with the image omitted.
// Check both local objects and globally cached images / Type3 resources.
export function assertRenderablePage(
  page: Pick<PDFPageProxy, "objs" | "commonObjs">,
  number: number,
) {
  for (const objects of [page.objs, page.commonObjs]) {
    for (const [, data] of objects) {
      if (data == null || typeof data === "string") {
        throw new Error(
          `第 ${number} 页存在无法解码的图片或字体，未导出不完整页面`,
        );
      }
    }
  }
}
