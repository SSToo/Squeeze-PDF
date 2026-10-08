import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  getDocument,
  GlobalWorkerOptions,
  type PDFDocumentProxy,
} from "pdfjs-dist/legacy/build/pdf.mjs";
import workerUrl from "pdfjs-dist/legacy/build/pdf.worker.mjs?url";
import { ChevronLeft, ChevronRight, Minus, Plus, FileText } from "lucide-react";
import type { Job } from "./types";
import { useI18n } from "./i18n/I18n";
import {
  prepareCanvasAnnotations,
  rasterRenderOptions,
} from "./rasterAnnotations";
GlobalWorkerOptions.workerSrc = workerUrl;

function Page({
  doc,
  page,
  zoom,
  label,
}: {
  doc: PDFDocumentProxy | null;
  page: number;
  zoom: number;
  label: string;
}) {
  const { t, raw } = useI18n();
  const ref = useRef<HTMLCanvasElement>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let active = true;
    let task:
      | ReturnType<Awaited<ReturnType<PDFDocumentProxy["getPage"]>>["render"]>
      | undefined;
    setError("");
    if (!doc) return;
    void doc
      .getPage(page)
      .then(async (p) => {
        await prepareCanvasAnnotations(p, doc.annotationStorage);
        const canvas = ref.current;
        if (!active || !canvas) return;
        const viewport = p.getViewport({ scale: 0.62 * zoom });
        const ratio = Math.min(window.devicePixelRatio || 1, 2);
        canvas.width = Math.ceil(viewport.width * ratio);
        canvas.height = Math.ceil(viewport.height * ratio);
        canvas.style.width = viewport.width + "px";
        canvas.style.height = viewport.height + "px";
        task = p.render({
          ...rasterRenderOptions,
          canvas,
          viewport,
          transform: [ratio, 0, 0, ratio, 0, 0],
        });
        return task.promise;
      })
      .catch((e) => {
        if (active && e.name !== "RenderingCancelledException")
          setError("这页无法预览，请用系统阅读器打开。");
      });
    return () => {
      active = false;
      task?.cancel();
    };
  }, [doc, page, zoom]);
  return (
    <figure className="preview-page">
      <figcaption>{label}</figcaption>
      <div className="paper-scroll">
        {doc ? (
          <canvas
            ref={ref}
            aria-label={t("{label}，第 {page} 页", { label, page })}
            role="img"
          />
        ) : (
          <div className="preview-placeholder">
            <FileText size={30} />
            <span>{t("压缩完成后显示结果")}</span>
          </div>
        )}
        {error && <p role="status">{raw(error)}</p>}
      </div>
    </figure>
  );
}
export default function PdfPreview({ job }: { job: Job }) {
  const { t, raw, number, percent } = useI18n();
  const [original, setOriginal] = useState<PDFDocumentProxy | null>(null);
  const [result, setResult] = useState<PDFDocumentProxy | null>(null);
  const [error, setError] = useState("");
  const [page, setPage] = useState(1);
  const [zoom, setZoom] = useState(1);
  useEffect(() => {
    let active = true;
    const tasks: ReturnType<typeof getDocument>[] = [];
    setOriginal(null);
    setResult(null);
    setError("");
    setPage(1);
    const load = async (path: string, set: (doc: PDFDocumentProxy) => void) => {
      const buffer = await invoke<ArrayBuffer>("read_pdf", { path });
      if (!active) return;
      const task = getDocument({
        data: new Uint8Array(buffer),
        useSystemFonts: true,
        cMapUrl: "/pdf-assets/cmaps/",
        cMapPacked: true,
        standardFontDataUrl: "/pdf-assets/standard_fonts/",
        wasmUrl: "/pdf-assets/wasm/",
      });
      tasks.push(task);
      const doc = await task.promise;
      if (active) set(doc);
    };
    void Promise.all([
      load(job.path, setOriginal),
      ...(job.report?.output ? [load(job.report.output, setResult)] : []),
    ]).catch(() => {
      if (active) setError("暂时无法预览这个文件，可在系统阅读器中打开。");
    });
    return () => {
      active = false;
      tasks.forEach((task) => {
        void task.destroy().catch(() => {});
      });
    };
  }, [job.path, job.report?.output]);
  return (
    <section className="preview" aria-label={t("文件预览")}>
      <div className="section-toolbar">
        <h3>{t("画质对比")}</h3>
        <div className="preview-controls">
          <button
            aria-label={t("上一页")}
            disabled={page <= 1}
            onClick={() => setPage((p) => p - 1)}
          >
            <ChevronLeft size={16} />
          </button>
          <span>
            {number(page)} /{" "}
            {original?.numPages
              ? number(original.numPages)
              : job.info?.pages
                ? number(job.info.pages)
                : "—"}
          </span>
          <button
            aria-label={t("下一页")}
            disabled={!original || page >= original.numPages}
            onClick={() => setPage((p) => p + 1)}
          >
            <ChevronRight size={16} />
          </button>
          <i />
          <button
            aria-label={t("缩小预览")}
            disabled={zoom <= 0.5}
            onClick={() => setZoom((z) => z - 0.25)}
          >
            <Minus size={15} />
          </button>
          <span>{percent(zoom)}</span>
          <button
            aria-label={t("放大预览")}
            disabled={zoom >= 3}
            onClick={() => setZoom((z) => z + 0.25)}
          >
            <Plus size={15} />
          </button>
        </div>
      </div>
      {error ? (
        <p className="preview-error">{raw(error)}</p>
      ) : (
        <div className="preview-grid">
          <Page doc={original} page={page} zoom={zoom} label={t("原文件")} />
          <Page
            doc={job.report?.outcome === "unchanged" ? original : result}
            page={page}
            zoom={zoom}
            label={t("压缩结果")}
          />
        </div>
      )}
    </section>
  );
}
