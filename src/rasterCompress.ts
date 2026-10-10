import { invoke } from "@tauri-apps/api/core";
import {
  getDocument,
  GlobalWorkerOptions,
  PDFWorker,
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
  return new Promise<void>((resolve) => {
    // Occluded WKWebViews can indefinitely deprioritize scheduler.yield().
    // Keep a timer fallback even when the API exists, and release that timer
    // immediately when the scheduler does resume the task.
    const fallback = setTimeout(resolve, 0);
    if (scheduler?.yield) {
      void scheduler.yield().then(
        () => {
          clearTimeout(fallback);
          resolve();
        },
        () => {
          /* The timer still yields if the scheduler rejects. */
        },
      );
    }
  });
};

export async function compressRaster(
  path: string,
  directory: string,
  preset: Preset,
  progress: (label: string) => void,
): Promise<Report> {
  if (active) throw new Error("已有压缩任务在运行");
  const started = performance.now();
  const controller = new AbortController();
  active = controller;
  let start: Start | undefined;
  let task: ReturnType<typeof getDocument> | undefined;
  let worker: PDFWorker | undefined;
  let render: ReturnType<PDFPageProxy["render"]> | undefined;
  let timedOut = false;
  let timeout: ReturnType<typeof setTimeout> | undefined;
  let stallTimeout: ReturnType<typeof setTimeout> | undefined;
  const touch = () => {
    clearTimeout(stallTimeout);
    stallTimeout = setTimeout(() => {
      timedOut = true;
      controller.abort();
    }, 120_000);
  };
  const abort = () => {
    render?.cancel();
    void task?.destroy().catch(() => {});
    if (!task) worker?.destroy();
  };
  controller.signal.addEventListener("abort", abort);
  const check = () => {
    if (controller.signal.aborted) throw new Error("任务已取消");
  };
  try {
    start = await invoke<Start>("begin_raster", { path, directory, preset });
    check();
    const sessionId = start.id;
    timeout = setTimeout(
      () => {
        timedOut = true;
        controller.abort();
      },
      Math.max(120_000, Math.min(1_800_000, start.plan.pages.length * 45_000)),
    );
    touch();
    const data = await invoke<ArrayBuffer>("read_pdf", { path });
    const hash = Array.from(
      new Uint8Array(await crypto.subtle.digest("SHA-256", data)),
    )
      .map((n) => n.toString(16).padStart(2, "0"))
      .join("");
    if (hash !== start.plan.source_sha256)
      throw new Error("原件已变化，未导出结果");
    check();
    worker = new PDFWorker();
    task = getDocument({
      worker,
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
    let rgbBuffer = new Uint8Array(0);
    let supersamplingMs = 0;
    let supersamplingBytes = 0;
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
      let highResolution: HTMLCanvasElement | undefined;
      let alternateStarted = false;
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
        render.onContinue = (continueRendering: () => void) => {
          touch();
          continueRendering();
        };
        await render.promise;
        render = undefined;
        check();
        assertRenderablePage(page, index + 1);
        const store = async (variant: "direct" | "supersampled") => {
          const rgba = context.getImageData(0, 0, width, height).data;
          if (rgbBuffer.length !== width * height * 3)
            rgbBuffer = new Uint8Array(width * height * 3);
          const rgb = rgbBuffer;
          let deadline = performance.now() + 50;
          for (let src = 0, dst = 0; src < rgba.length; src += 4) {
            rgb[dst++] = rgba[src];
            rgb[dst++] = rgba[src + 1];
            rgb[dst++] = rgba[src + 2];
            // Let the UI handle cancellation during a large pixel conversion.
            if ((src & 0x3fff) === 0 && performance.now() >= deadline) {
              await yieldToBrowser();
              check();
              touch();
              deadline = performance.now() + 50;
            }
          }
          for (let offset = 0; offset < rgb.length; offset += 1024 * 1024) {
            check();
            if (variant === "supersampled") alternateStarted = true;
            await invoke(
              "write_raster_page",
              rgb.subarray(offset, Math.min(offset + 1024 * 1024, rgb.length)),
              {
                headers: {
                  "x-squeeze-session": sessionId,
                  "x-squeeze-page": String(index),
                  "x-squeeze-offset": String(offset),
                  "x-squeeze-variant": variant,
                },
              },
            );
            touch();
            await yieldToBrowser();
          }
        };
        await store("direct");
        // Bound the extra canvas to 12 million pixels and optional work to 45 s.
        // Final dimensions are always the plan's dimensions, independent of DPR.
        if (
          preset !== "lossless" &&
          width * height * 4 <= 12_000_000 &&
          supersamplingMs < 45_000 &&
          supersamplingBytes + width * height * 3 <= 32 * 1024 * 1024
        ) {
          const extraStarted = performance.now();
          try {
            highResolution = document.createElement("canvas");
            highResolution.width = width * 2;
            highResolution.height = height * 2;
            const highContext = highResolution.getContext("2d");
            if (!highContext) throw new Error("页面绘图上下文不可用");
            render = page.render({
              ...rasterRenderOptions,
              canvas: highResolution,
              canvasContext: highContext,
              viewport: page.getViewport({ scale: (width * 2) / base.width }),
              background: "rgb(255,255,255)",
            });
            render.onContinue = (continueRendering: () => void) => {
              touch();
              if (
                performance.now() - extraStarted + supersamplingMs >=
                45_000
              ) {
                render?.cancel();
              } else {
                continueRendering();
              }
            };
            await render.promise;
            render = undefined;
            check();
            assertRenderablePage(page, index + 1);
            context.imageSmoothingEnabled = true;
            context.imageSmoothingQuality = "high";
            context.drawImage(highResolution, 0, 0, width, height);
            await store("supersampled");
            supersamplingBytes += width * height * 3;
          } catch (error) {
            check();
            // Failure before any alternate bytes are submitted retains direct pixels.
            if (render) {
              render.cancel();
              render = undefined;
            }
            // Transport failures cannot leave a partial alternate silently accepted.
            if (alternateStarted) throw error;
          } finally {
            supersamplingMs += performance.now() - extraStarted;
          }
        }
      } finally {
        if (highResolution) {
          highResolution.width = 0;
          highResolution.height = 0;
        }
        canvas.width = 0;
        canvas.height = 0;
        page.cleanup();
      }
      await yieldToBrowser();
    }
    check();
    clearTimeout(timeout);
    clearTimeout(stallTimeout);
    rgbBuffer = new Uint8Array(0);
    progress("编码页面图片");
    const report = await invoke<Report>("finish_raster", { id: start.id });
    report.elapsed_ms = Math.round(performance.now() - started);
    return report;
  } catch (error) {
    if (timedOut) throw new Error("页面渲染超时，未导出结果");
    if (controller.signal.aborted) throw new Error("任务已取消");
    throw error;
  } finally {
    clearTimeout(timeout);
    clearTimeout(stallTimeout);
    controller.signal.removeEventListener("abort", abort);
    if (start) await invoke("abort_raster", { id: start.id }).catch(() => {});
    let cleanupTimeout: ReturnType<typeof setTimeout> | undefined;
    try {
      await Promise.race([
        task?.destroy().catch(() => {}),
        new Promise<void>((resolve) => {
          cleanupTimeout = setTimeout(resolve, 2000);
        }),
      ]);
    } finally {
      clearTimeout(cleanupTimeout);
      worker?.destroy();
      if (active === controller) active = undefined;
    }
  }
}
