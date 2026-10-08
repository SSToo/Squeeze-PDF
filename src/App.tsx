import { useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ArrowDownToLine,
  ArrowRight,
  Check,
  CheckCheck,
  FilePlus2,
  FileText,
  Folder,
  Leaf,
  LockKeyhole,
  Plus,
  Settings2,
  ShieldCheck,
  Sun,
  Moon,
  ExternalLink,
  X,
  RefreshCw,
  Languages,
} from "lucide-react";
import PdfPreview from "./PdfPreview";
import { useI18n } from "./i18n/I18n";
import { languages, type Language } from "./i18n/locale";
import type { MessageKey } from "./i18n/catalog";
import { compressRaster, cancelRaster } from "./rasterCompress";
import { version } from "../package.json";
import {
  type Imported,
  type Job,
  type Preset,
  type OutputMode,
  type Report,
} from "./types";

const native = isTauri();
const modes = [
  {
    id: "lossless" as const,
    title: "保真",
    tag: "优先画质",
    description: "保持图片分辨率，复用重复矢量轮廓。",
    icon: ShieldCheck,
  },
  {
    id: "balanced" as const,
    title: "均衡",
    tag: "推荐",
    description: "适度优化矢量与图片，兼顾清晰与大小。",
    icon: Leaf,
  },
  {
    id: "small" as const,
    title: "更小",
    tag: "体积优先",
    description: "复用重复矢量，并降低图片分辨率和质量。",
    icon: ArrowDownToLine,
  },
] as const;
const phases: Record<string, MessageKey> = {
  reading: "读取文件",
  inspecting: "检查文档",
  images: "优化图片",
  rasterizing: "整页渲染为图片",
  optimizing: "优化文档结构",
  saving: "写入候选",
  validating: "校验结果",
  completed: "处理完成",
};
const statuses: Record<Job["status"], MessageKey> = {
  queued: "等待压缩",
  processing: "处理中",
  saved: "压缩完成",
  unchanged: "无需压缩",
  failed: "处理失败",
  cancelled: "已取消",
  skipped: "暂不支持",
};
function preference(key: string, fallback: string) {
  try {
    return localStorage.getItem(key) || fallback;
  } catch {
    return fallback;
  }
}
function store(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* preferences are optional */
  }
}
export default function App() {
  const { t, raw, bytes, number, percent, language, setLanguage } = useI18n();
  const [jobs, setJobs] = useState<Job[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [outputMode, setOutputMode] = useState<OutputMode>(() =>
    preference("squeeze-output-mode", "preserve") === "raster"
      ? "raster"
      : "preserve",
  );
  const [preset, setPreset] = useState<Preset>(() => {
    const saved = preference("squeeze-preset", "balanced");
    return modes.some((m) => m.id === saved) ? (saved as Preset) : "balanced";
  });
  const [directory, setDirectory] = useState(() =>
    preference("squeeze-directory", ""),
  );
  const [dark, setDark] = useState(
    () => preference("squeeze-theme", "light") === "dark",
  );
  const [busy, setBusy] = useState(false);
  const [importing, setImporting] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [message, setMessage] = useState("");
  const [done, setDone] = useState(0);
  const cancel = useRef(false);
  const busyRef = useRef(false);
  const importRef = useRef(false);
  const filesRef = useRef<Job[]>([]);
  useEffect(() => {
    document.documentElement.dataset.theme = dark ? "dark" : "light";
    store("squeeze-theme", dark ? "dark" : "light");
  }, [dark]);
  useEffect(() => {
    store("squeeze-preset", preset);
  }, [preset]);
  useEffect(() => {
    store("squeeze-output-mode", outputMode);
  }, [outputMode]);
  useEffect(() => {
    store("squeeze-directory", directory);
  }, [directory]);
  useEffect(() => {
    filesRef.current = jobs;
  }, [jobs]);
  const update = (path: string, patch: Partial<Job>) =>
    setJobs((current) =>
      current.map((j) => (j.path === path ? { ...j, ...patch } : j)),
    );
  const add = async (paths: string[]) => {
    if (busyRef.current || importRef.current) return;
    const pdfPaths = paths.filter((path) => /\.pdf$/i.test(path));
    if (!pdfPaths.length) {
      setMessage("请添加 PDF 文件。");
      return;
    }
    importRef.current = true;
    setImporting(true);
    setMessage("");
    try {
      const imported = await invoke<Imported[]>("import_files", {
        paths: pdfPaths,
      });
      setJobs((current) => {
        const known = new Set(current.map((j) => j.path));
        return [
          ...current,
          ...imported
            .filter((j) => {
              if (known.has(j.path)) return false;
              known.add(j.path);
              return true;
            })
            .map((j) => ({
              ...j,
              status: j.error ? ("skipped" as const) : ("queued" as const),
            })),
        ];
      });
    } catch (e) {
      setMessage(String(e));
    } finally {
      importRef.current = false;
      setImporting(false);
    }
  };
  useEffect(() => {
    if (!native) return;
    let disposed = false;
    const cleanups: (() => void)[] = [];
    const attach = (cleanup: () => void) =>
      disposed ? cleanup() : cleanups.push(cleanup);
    void getCurrentWebviewWindow()
      .onDragDropEvent((event) => {
        setDragging(
          event.payload.type === "enter" || event.payload.type === "over",
        );
        if (event.payload.type === "drop") {
          setDragging(false);
          void add(event.payload.paths);
        }
      })
      .then(attach)
      .catch((e) => setMessage(String(e)));
    void listen<{
      path: string;
      progress: { phase: string; completed: number; total: number };
    }>("squeeze-progress", (event) => {
      const { path, progress } = event.payload;
      update(path, { phase: phases[progress.phase] || "处理中" });
    })
      .then(attach)
      .catch((e) => setMessage(String(e)));
    return () => {
      disposed = true;
      cleanups.forEach((fn) => fn());
    };
  }, []);
  async function pickFiles() {
    if (!native) {
      setMessage("请运行桌面应用来添加和压缩本地文件。");
      return;
    }
    try {
      const paths = await open({
        title: t("选择 PDF 文件"),
        multiple: true,
        directory: false,
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      });
      if (paths) await add(Array.isArray(paths) ? paths : [paths]);
    } catch (e) {
      setMessage(String(e));
    }
  }
  async function pickDirectory() {
    if (!native) {
      setMessage("请在桌面应用中选择输出目录。");
      return;
    }
    try {
      const path = await open({
        title: t("选择输出文件夹"),
        directory: true,
        multiple: false,
        defaultPath: directory || undefined,
      });
      if (typeof path === "string") setDirectory(path);
    } catch (e) {
      setMessage(String(e));
    }
  }
  async function run(only?: string) {
    if (busyRef.current) return;
    if (!directory) {
      setMessage("请先选择输出文件夹。");
      return;
    }
    const queue = filesRef.current.filter(
      (j) => !j.error && (!only || j.path === only),
    );
    if (!queue.length) return;
    busyRef.current = true;
    setBusy(true);
    cancel.current = false;
    setDone(0);
    setMessage("");
    queue.forEach((j) =>
      update(j.path, { status: "queued", report: undefined, phase: undefined }),
    );
    let finished = 0;
    let failed = 0;
    try {
      for (const job of queue) {
        if (cancel.current) {
          update(job.path, { status: "cancelled" });
          continue;
        }
        update(job.path, { status: "processing", phase: "准备压缩" });
        try {
          const report =
            outputMode === "raster"
              ? await compressRaster(job.path, directory, preset, (phase) =>
                  update(job.path, { phase }),
                )
              : await invoke<Report>("compress_file", {
                  path: job.path,
                  directory,
                  preset,
                  outputMode,
                });
          update(job.path, {
            status: report.outcome,
            report,
            phase: undefined,
          });
        } catch (e) {
          const error = String(e);
          if (!cancel.current) failed++;
          update(job.path, {
            status: cancel.current ? "cancelled" : "failed",
            phase: error,
          });
        }
        finished++;
        setDone(finished);
      }
      setMessage(
        cancel.current
          ? "任务已停止，已完成的结果保留在输出文件夹。"
          : failed
            ? `完成：${finished - failed} · 失败：${failed}。失败原因显示在对应文件下方，可点击重试。`
            : "处理完成。点击文件名可检查画质，或打开输出文件夹。",
      );
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }
  async function stop() {
    cancel.current = true;
    cancelRaster();
    try {
      await invoke("cancel_task");
    } catch (e) {
      setMessage(String(e));
    }
  }
  async function openResult(path: string, folder: boolean) {
    try {
      await invoke("open_result", { path, folder });
    } catch (e) {
      setMessage(String(e));
    }
  }
  const active = jobs.find((j) => j.path === selected);
  const ready = jobs.filter((j) => !j.error);
  const saved = jobs.reduce(
    (sum, j) =>
      sum + (j.report ? j.report.original_bytes - j.report.result_bytes : 0),
    0,
  );
  return (
    <div className={"app" + (dragging ? " dragging" : "")}>
      <a className="skip-link" href="#workspace">
        {t("跳到文件列表")}
      </a>
      <header className="topbar">
        <div className="brand">
          <img src="/icon.svg" alt="" />
          <span>
            Squeeze <b>PDF</b>
          </span>
          <small>{version}</small>
        </div>
        <div className="header-actions">
          <span className="offline">
            <LockKeyhole size={13} /> {t("完全本地处理")}
          </span>
          <button
            className="icon-button"
            aria-label={dark ? t("切换浅色主题") : t("切换深色主题")}
            onClick={() => setDark((d) => !d)}
          >
            {dark ? <Sun size={18} /> : <Moon size={18} />}
          </button>
          <label className="language-picker icon-button" title={t("语言")}>
            <Languages size={18} aria-hidden="true" />
            <span className="visually-hidden">{t("语言")}</span>
            <select
              id="language"
              value={language}
              onChange={(event) => setLanguage(event.target.value as Language)}
            >
              {languages.map((item) => (
                <option key={item.id} value={item.id} lang={item.tag}>
                  {item.name}
                </option>
              ))}
            </select>
          </label>
        </div>
      </header>
      <main id="workspace" tabIndex={-1}>
        <div className="workspace">
          <aside className="settings">
            <div className="panel-heading">
              <Settings2 size={17} />
              <h2>{t("压缩设置")}</h2>
            </div>
            <fieldset disabled={busy}>
              <legend>{t("输出内容")}</legend>
              <div className="content-modes">
                <label className={outputMode === "preserve" ? "chosen" : ""}>
                  <input
                    type="radio"
                    name="outputMode"
                    value="preserve"
                    checked={outputMode === "preserve"}
                    onChange={() => setOutputMode("preserve")}
                  />
                  <span>
                    <b>{t("保留原始内容")}</b>
                    <small>{t("原有矢量、文字仍然保留")}</small>
                  </span>
                </label>
                <label className={outputMode === "raster" ? "chosen" : ""}>
                  <input
                    type="radio"
                    name="outputMode"
                    value="raster"
                    checked={outputMode === "raster"}
                    onChange={() => setOutputMode("raster")}
                  />
                  <span>
                    <b>{t("整页转图片")}</b>
                    <small>{t("每一页是一张独立图片")}</small>
                  </span>
                </label>
              </div>
            </fieldset>
            {outputMode === "raster" && (
              <p className="raster-note">
                {t(
                  "文字不能选取，矢量会变成像素。可见表单和批注会合并到图片，附件保留；转换后可能更大。",
                )}
              </p>
            )}
            <fieldset disabled={busy}>
              <legend>{t("压缩档位")}</legend>
              <div className="modes">
                {modes.map((mode) => (
                  <label
                    className={"mode" + (preset === mode.id ? " chosen" : "")}
                    key={mode.id}
                    htmlFor={"mode-" + mode.id}
                  >
                    <input
                      type="radio"
                      name="preset"
                      id={"mode-" + mode.id}
                      value={mode.id}
                      checked={preset === mode.id}
                      onChange={() => setPreset(mode.id)}
                    />
                    <mode.icon size={20} />
                    <span className="mode-copy">
                      <span>
                        <b>
                          {outputMode === "raster" && mode.id === "lossless"
                            ? t("清晰")
                            : t(mode.title)}
                        </b>
                        <small>
                          {outputMode === "raster"
                            ? mode.id === "lossless"
                              ? "300 DPI"
                              : mode.id === "balanced"
                                ? t("宽 {width} px", { width: 1400 })
                                : t("宽 {width} px", { width: 1000 })
                            : mode.id === "small"
                              ? "100 DPI"
                              : t(mode.tag)}
                        </small>
                      </span>
                      <span className="mode-description">
                        {outputMode === "raster" && mode.id === "lossless"
                          ? t("整页转图片，无损编码页面像素。")
                          : outputMode === "raster"
                            ? mode.id === "balanced"
                              ? t("保留更多像素，适合文字和图表。")
                              : t("兼顾体积与小字清晰度，适合分享。")
                            : t(mode.description)}
                      </span>
                    </span>
                  </label>
                ))}
              </div>
            </fieldset>
            <div className="quality-note">
              <span className="note-dot" />
              {outputMode === "raster"
                ? t(
                    "按页选择图片编码；宽度为最终像素尺寸，高度保持页面比例。导出后请检查小字和画质。",
                  )
                : preset === "lossless"
                  ? t(
                      "图片保持原分辨率并高质量重编码；建议导出后检查细字和细节。",
                    )
                  : t("图片与极细矢量边缘可能变化，建议导出后检查小字和细节。")}
            </div>
            <div className="output-setting">
              <h3>{t("保存到")}</h3>
              <button
                className="folder-picker"
                onClick={pickDirectory}
                disabled={busy}
              >
                <Folder size={19} />
                <span>
                  {directory
                    ? directory.split(/[\\/]/).filter(Boolean).pop()
                    : t("选择输出文件夹")}
                </span>
                <ArrowRight size={15} />
              </button>
              {directory && (
                <p className="directory" title={directory}>
                  {directory}
                </p>
              )}
              <p>{t("自动添加 _compressed，重名时编号。")}</p>
            </div>
            <div className="local-note">
              <LockKeyhole size={16} />
              <p>
                {t("无需上传，无需账号。")}
                <br />
                {t("压缩和预览均在本地完成。")}
              </p>
            </div>
          </aside>
          <section className="files-panel" aria-label={t("PDF 文件列表")}>
            <div className="section-toolbar">
              <div>
                <h2>
                  {t("你的文件")}{" "}
                  <span className="count">{number(jobs.length)}</span>
                </h2>
                <p>
                  {jobs.length
                    ? t("点击文件名预览，所有文件按队列处理。")
                    : t("添加一个或多个 PDF，开始轻松压缩。")}
                </p>
              </div>
              <div className="toolbar-actions">
                {jobs.length > 0 && (
                  <button
                    className="text-button"
                    disabled={busy || importing}
                    onClick={() => {
                      setJobs([]);
                      setSelected(null);
                      setMessage("");
                    }}
                  >
                    {t("清空")}
                  </button>
                )}
                <button
                  className="secondary"
                  disabled={busy || importing}
                  onClick={pickFiles}
                >
                  <Plus size={16} />
                  {importing ? t("检查文件…") : t("添加文件")}
                </button>
              </div>
            </div>
            {jobs.length === 0 ? (
              <div className="empty-state">
                <div className="file-illustration" aria-hidden="true">
                  <div className="back-sheet" />
                  <div className="front-sheet">
                    <FileText size={46} strokeWidth={1.3} />
                    <span>PDF</span>
                  </div>
                  <div className="small-arrow">
                    <ArrowDownToLine size={20} />
                  </div>
                </div>
                <h3>{dragging ? t("松开即可添加") : t("把 PDF 拖到这里")}</h3>
                <p>{t("支持批量添加，每个文件最多 256 MB。")}</p>
                <button
                  className="primary"
                  disabled={importing}
                  onClick={pickFiles}
                >
                  <FilePlus2 size={17} /> {t("选择 PDF 文件")}
                </button>
                <span className="empty-footnote">
                  <Check size={13} /> {t("原文件不会被覆盖")}
                </span>
              </div>
            ) : (
              <div className="file-list">
                <table>
                  <caption className="visually-hidden">
                    {t("已添加的 PDF 及压缩结果")}
                  </caption>
                  <thead>
                    <tr>
                      <th scope="col">{t("文件")}</th>
                      <th scope="col">{t("大小")}</th>
                      <th scope="col">{t("状态")}</th>
                      <th scope="col">
                        <span className="visually-hidden">{t("操作")}</span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {jobs.map((job) => (
                      <tr
                        key={job.path}
                        className={selected === job.path ? "selected" : ""}
                      >
                        <td>
                          <div className="file-cell">
                            <div className="pdf-icon">
                              <FileText size={19} />
                            </div>
                            <div>
                              <button
                                className="file-name"
                                disabled={!job.info}
                                aria-pressed={selected === job.path}
                                onClick={() =>
                                  setSelected((p) =>
                                    p === job.path ? null : job.path,
                                  )
                                }
                              >
                                {job.name}
                              </button>
                              <p>
                                {job.info
                                  ? t("页数：{pages} · 图片：{images}", {
                                      pages: job.info.pages,
                                      images: job.info.images,
                                    })
                                  : raw(job.error)}
                              </p>
                              {job.status === "failed" && job.phase && (
                                <p className="row-error">{raw(job.phase)}</p>
                              )}
                            </div>
                          </div>
                        </td>
                        <td>
                          <span
                            className={
                              job.report?.outcome === "saved" ? "old-size" : ""
                            }
                          >
                            {bytes(
                              job.info?.bytes ||
                                job.report?.original_bytes ||
                                0,
                            )}
                          </span>
                          {job.report?.outcome === "saved" && (
                            <strong className="new-size">
                              {bytes(job.report.result_bytes)}
                            </strong>
                          )}
                        </td>
                        <td>
                          <span className={"status " + job.status}>
                            {job.status === "saved" ? (
                              <>
                                <CheckCheck size={14} />{" "}
                                {job.report!.reduction_percent < 0 ? "+" : "−"}
                                {percent(
                                  Math.abs(job.report!.reduction_percent) / 100,
                                  1,
                                )}
                              </>
                            ) : job.status === "processing" ? (
                              <>
                                <span className="spinner" />
                                {raw(job.phase)}
                              </>
                            ) : (
                              t(statuses[job.status])
                            )}
                          </span>
                        </td>
                        <td>
                          <div className="row-actions">
                            {job.report?.output && (
                              <>
                                <button
                                  className="icon-button"
                                  aria-label={t("打开 {name} 的压缩结果", {
                                    name: job.name,
                                  })}
                                  onClick={() =>
                                    void openResult(job.report!.output!, false)
                                  }
                                >
                                  <ExternalLink size={16} />
                                </button>
                                <button
                                  className="icon-button"
                                  aria-label={t("打开 {name} 的输出文件夹", {
                                    name: job.name,
                                  })}
                                  onClick={() =>
                                    void openResult(job.report!.output!, true)
                                  }
                                >
                                  <Folder size={17} />
                                </button>
                              </>
                            )}
                            {["failed", "cancelled"].includes(job.status) && (
                              <button
                                className="icon-button"
                                disabled={busy}
                                aria-label={t("重试 {name}", {
                                  name: job.name,
                                })}
                                onClick={() => void run(job.path)}
                              >
                                <RefreshCw size={16} />
                              </button>
                            )}
                            <button
                              className="icon-button"
                              disabled={busy}
                              aria-label={t("移除 {name}", { name: job.name })}
                              onClick={() => {
                                setJobs((all) =>
                                  all.filter((j) => j.path !== job.path),
                                );
                                if (selected === job.path) setSelected(null);
                              }}
                            >
                              <X size={16} />
                            </button>
                          </div>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
            {active && (
              <>
                {active.status !== "processing" && <PdfPreview job={active} />}
                {active.phase && active.status === "failed" && (
                  <p className="file-error">{raw(active.phase)}</p>
                )}
                {active.report && (
                  <details className="report-details">
                    <summary>{t("查看处理说明")}</summary>
                    <p>
                      {t("耗时 {seconds} 秒 · 优化 {streams} 个数据流", {
                        seconds: number(active.report.elapsed_ms / 1000, 1),
                        streams: active.report.optimized_streams,
                      })}
                    </p>
                    {active.report.outcome === "unchanged" && (
                      <p>{t("候选结果没有比原件更小，因此未生成副本。")}</p>
                    )}
                    {active.report.warnings.map((w, i) => (
                      <p key={i}>{raw(w)}</p>
                    ))}
                    {active.report.images.map((img, i) => (
                      <p key={i}>
                        {raw(img.reason)} (
                        {active.report!.output_mode === "preserve"
                          ? img.before_dimensions
                              .map((n) => number(n))
                              .join(" × ") + " → "
                          : ""}
                        {img.after_dimensions.map((n) => number(n)).join(" × ")}
                        {active.report!.output_mode === "raster"
                          ? " " + t("像素")
                          : ""}
                        )
                      </p>
                    ))}
                  </details>
                )}
              </>
            )}
            {jobs.some((j) => j.status === "failed") && !active && (
              <p className="file-error">
                {t("部分文件处理失败，点击对应文件名查看原因或重试。")}
              </p>
            )}
            <footer className="batch-footer">
              <div>
                <span className="footer-stat">
                  {saved !== 0 ? (
                    <>
                      <CheckCheck size={18} />{" "}
                      {saved > 0 ? t("已节省") : t("体积增加")}{" "}
                      <b>{bytes(Math.abs(saved))}</b>
                    </>
                  ) : (
                    <>
                      <FileText size={18} />
                      {ready.length
                        ? t("可压缩文件：{count}", { count: ready.length })
                        : t("等待添加 PDF 文件")}
                    </>
                  )}
                </span>
                <p>
                  {busy
                    ? t("已完成 {done} / {total} 个文件", {
                        done,
                        total: ready.length,
                      })
                    : t("每次压缩都从原件开始。")}
                </p>
              </div>
              {busy ? (
                <button className="secondary" onClick={() => void stop()}>
                  <X size={16} />
                  {t("取消任务")}
                </button>
              ) : (
                <button
                  className="primary start"
                  disabled={!ready.length || importing}
                  onClick={() => void run()}
                >
                  {t("开始压缩")} <ArrowRight size={17} />
                </button>
              )}
            </footer>
          </section>
        </div>
        <div className="message" role="status" aria-live="polite">
          {raw(message)}
        </div>
      </main>
      {dragging && (
        <div className="drop-overlay" aria-hidden="true">
          <FilePlus2 size={42} />
          <strong>{t("松开以添加 PDF")}</strong>
        </div>
      )}
    </div>
  );
}
