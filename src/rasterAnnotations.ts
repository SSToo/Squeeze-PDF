import {
  AnnotationMode,
  OPS,
  type PDFDocumentProxy,
  type PDFPageProxy,
} from "pdfjs-dist/legacy/build/pdf.mjs";

export const rasterRenderOptions = {
  intent: "display",
  annotationMode: AnnotationMode.ENABLE_STORAGE,
};

export async function prepareCanvasAnnotations(
  page: PDFPageProxy,
  storage: PDFDocumentProxy["annotationStorage"],
) {
  const annotations = await page.getAnnotations({ intent: "any" });
  for (const a of annotations) {
    if (
      !(a.annotationFlags & (1 | 2 | 32)) &&
      a.subtype === "Widget" &&
      !a.hasAppearance &&
      (a.fieldType === "Tx" || a.fieldType === "Ch")
    ) {
      storage.setValue(a.id, { value: a.fieldValue ?? "" });
    }
  }
  return annotations;
}

// PDF.js can catch an annotation failure and resolve rendering with it omitted.
// Check the parsed IDs against the Rust plan, then check appearance operators.
// Use display visibility so hidden and print-only widgets are not exposed.
export async function prepareRasterAnnotations(
  page: PDFPageProxy,
  storage: PDFDocumentProxy["annotationStorage"],
  number: number,
  sourceIds: readonly string[] = [],
) {
  const fail = () => {
    throw new Error(
      `第 ${number} 页的批注或表单无法完整转为图片，请使用保留原始内容模式`,
    );
  };
  const annotations = await prepareCanvasAnnotations(page, storage);
  const parsed = new Set(annotations.map((a) => a.id));
  if (sourceIds.some((id) => !parsed.has(id))) fail();
  const visible = annotations.filter(
    (a) => !(a.annotationFlags & (1 | 2 | 32)),
  );
  const expected = visible.filter(
    (a) => a.subtype !== "Popup" && (a.subtype !== "Link" || a.hasAppearance),
  );
  for (const annotation of expected) {
    if (
      !annotation.hasAppearance &&
      (annotation.subtype === "Text" || annotation.subtype === "FileAttachment")
    ) {
      // Their default icons live in PDF.js's HTML annotation layer. A canvas
      // would omit them; do not export a misleadingly incomplete page.
      fail();
    }
  }
  if (!expected.length) return;
  const operators = await page.getOperatorList(rasterRenderOptions);
  const painted = new Set<string>();
  for (let i = 0; i < operators.fnArray.length; i++) {
    if (operators.fnArray[i] === OPS.beginAnnotation) {
      painted.add(operators.argsArray[i][0]);
    }
  }
  if (expected.some((a) => !painted.has(a.id))) fail();
}
