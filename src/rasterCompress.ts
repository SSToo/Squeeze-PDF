import { invoke } from "@tauri-apps/api/core";
import {
  getDocument,
  GlobalWorkerOptions,
  type PDFPageProxy,
} from "pdfjs-dist/legacy/build/pdf.mjs";
import workerUrl from "pdfjs-dist/legacy/build/pdf.worker.mjs?url";
import type { Preset, Report } from "./types";
import { assertRenderablePage } from "./renderValidation";
import {
  prepareRasterAnnotations,
  rasterRenderOptions,
} from "./rasterAnnotations";

GlobalWorkerOptions.workerSrc = workerUrl;
let active: AbortController | undefined;
export function cancelRaster() {
  active?.abort();
}
type Start = {
  id: string;
  plan: {
    source_sha256: string;
    pages: {
      dimensions: [number, number];
      points: [number, number];
      annotation_ids: string[];
    }[];
  };
};
const yieldToBrowser = () => {
  const scheduler = (
    globalThis as typeof globalThis & {
      scheduler?: { yield: () => Promise<void> };
    }
  ).scheduler;
  return scheduler?.yield
    ? scheduler.yield()
    : new Promise<void>((resolve) => setTimeout(resolve, 0));
};

export async function compressRaster(
  path: string,
  directory: string,
  preset: Preset,
  progress: (label: string) => void,
): Promise<Report> {
  const started = performance.now();
  const controller = new AbortController();
  active = controller;
  let start: Start | undefined;
  let task: ReturnType<typeof getDocument> | undefined;
  let render: ReturnType<PDFPageProxy["render"]> | undefined;
  let timedOut = false;
  let timeout: ReturnType<typeof setTimeout> | undefined;
  const abort = () => {
    render?.cancel();
    void task?.destroy().catch(() => {});
  };
  controller.signal.addEventListener("abort", abort);
  const check = () => {
    if (controller.signal.aborted) throw new Error("任务已取消");
  };
  try {
    start = await invoke<Start>("begin_raster", { path, directory, preset });
    check();
    timeout = setTimeout(() => {
      timedOut = true;
      controller.abort();
    }, 120_000);
    const data = await invoke<ArrayBuffer>("read_pdf", { path });
    const hash = Array.from(
      new Uint8Array(await crypto.subtle.digest("SHA-256", data)),
    )
      .map((n) => n.toString(16).padStart(2, "0"))
      .join("");
    if (hash !== start.plan.source_sha256)
      throw new Error("原件已变化，未导出结果");
    check();
    task = getDocument({
      data: new Uint8Array(data),
      useSystemFonts: false,
      stopAtErrors: true,
      cMapUrl: "/pdf-assets/cmaps/",
      cMapPacked: true,
      standardFontDataUrl: "/pdf-assets/standard_fonts/",
      wasmUrl: "/pdf-assets/wasm/",
    });
    const doc = await task.promise;
    if (doc.numPages !== start.plan.pages.length)
      throw new Error("两个解析器读取到的页数不同");
    for (let index = 0; index < doc.numPages; index++) {
      check();
      progress(`整页渲染为图片 ${index + 1} / ${doc.numPages}`);
      const page = await doc.getPage(index + 1);
      const plan = start.plan.pages[index];
      await prepareRasterAnnotations(
        page,
        doc.annotationStorage,
        index + 1,
        plan.annotation_ids,
      );
      check();
      const base = page.getViewport({ scale: 1 });
      if (
        Math.abs(base.width - plan.points[0]) > 0.001 ||
        Math.abs(base.height - plan.points[1]) > 0.001
      ) {
        throw new Error(`第 ${index + 1} 页的可见范围与解析计划不符`);
      }
      const [width, height] = plan.dimensions;
      const canvas = document.createElement("canvas");
      canvas.width = width;
      canvas.height = height;
      try {
        const context = canvas.getContext("2d");
        if (!context) throw new Error("页面绘图上下文不可用");
        const viewport = page.getViewport({ scale: width / base.width });
        render = page.render({
          ...rasterRenderOptions,
          canvas,
          canvasContext: context,
          viewport,
          background: "rgb(255,255,255)",
        });
        await render.promise;
        render = undefined;
        check();
        assertRenderablePage(page, index + 1);
        const rgba = context.getImageData(0, 0, width, height).data;
        const rgb = new Uint8Array(width * height * 3);
        let deadline = performance.now() + 50;
        for (let src = 0, dst = 0; src < rgba.length; src += 4) {
          rgb[dst++] = rgba[src];
          rgb[dst++] = rgba[src + 1];
          rgb[dst++] = rgba[src + 2];
          // Let the UI handle cancellation during a large pixel conversion.
          if ((src & 0x3fff) === 0 && performance.now() >= deadline) {
            await yieldToBrowser();
            check();
            deadline = performance.now() + 50;
          }
        }
        await invoke("write_raster_page", rgb, {
          headers: {
            "x-squeeze-session": start.id,
            "x-squeeze-page": String(index),
          },
        });
      } finally {
        canvas.width = 0;
        canvas.height = 0;
        page.cleanup();
      }
      await yieldToBrowser();
    }
    check();
    clearTimeout(timeout);
    progress("编码页面图片");
    const report = await invoke<Report>("finish_raster", { id: start.id });
    report.elapsed_ms = Math.round(performance.now() - started);
    return report;
  } catch (error) {
    if (timedOut) throw new Error("页面渲染超时，未导出结果");
    throw error;
  } finally {
    clearTimeout(timeout);
    controller.signal.removeEventListener("abort", abort);
    await task?.destroy().catch(() => {});
    if (start) await invoke("abort_raster", { id: start.id }).catch(() => {});
    if (active === controller) active = undefined;
  }
}
