import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { ArrowDownToLine, Check, RefreshCw, X } from "lucide-react";
import { useI18n } from "./i18n/I18n";
import type { UpdateController } from "./updater";
import { version } from "../package.json";

export default function UpdateCenter({
  controller,
  occupied,
}: {
  controller: UpdateController;
  occupied: boolean;
}) {
  const { t, bytes, percent } = useI18n();
  const state = useSyncExternalStore(
    controller.subscribe,
    controller.getSnapshot,
  );
  const [visible, setVisible] = useState(false);
  const dialog = useRef<HTMLDialogElement>(null);
  const notified = useRef<string | undefined>(undefined);
  const working =
    state.phase === "downloading" ||
    state.phase === "installing" ||
    state.phase === "installed";
  const native = isTauri();
  useEffect(() => {
    if (!native || !import.meta.env.PROD) return;
    const startup = window.setTimeout(() => void controller.check(), 5000);
    const daily = window.setInterval(
      () => void controller.check(),
      24 * 60 * 60 * 1000,
    );
    return () => {
      window.clearTimeout(startup);
      window.clearInterval(daily);
    };
  }, [controller, native]);
  useEffect(() => {
    if (state.phase === "available" && state.version !== notified.current) {
      notified.current = state.version;
      setVisible(true);
    }
  }, [state.phase, state.version]);
  useEffect(() => {
    if (visible && !dialog.current?.open) dialog.current?.showModal();
    else if (!visible && dialog.current?.open) dialog.current.close();
  }, [visible]);
  const label = state.version
    ? t("发现新版本 {version}", { version: state.version })
    : t("检查更新");
  const status = !native
    ? t("请在桌面应用中检查更新")
    : state.phase === "checking"
      ? t("正在检查更新…")
      : state.phase === "current"
        ? t("已是最新版本")
        : state.phase === "downloading"
          ? t("正在下载更新…")
          : state.phase === "installing" || state.phase === "installed"
            ? t("正在安装并重启…")
            : state.error === "check"
              ? t("无法检查更新，请检查网络后重试。")
              : state.error === "restart"
                ? t("更新已安装，请重启应用。")
                : state.error === "install"
                  ? t("更新未完成，请重试或从 Releases 下载安装包。")
                  : state.version
                    ? t("发现新版本 {version}", { version: state.version })
                    : t("检查更新");
  return (
    <>
      <button
        className="icon-button update-trigger"
        title={label}
        aria-label={label}
        aria-haspopup="dialog"
        onClick={() => {
          setVisible(true);
          if (native && !state.version && !working) void controller.check(true);
        }}
      >
        <RefreshCw
          size={18}
          className={state.phase === "checking" ? "update-spinning" : undefined}
        />
        {state.version && <span className="update-dot" aria-hidden="true" />}
      </button>
      <dialog
        ref={dialog}
        className="update-dialog"
        aria-labelledby="update-title"
        aria-describedby="update-status"
        onCancel={(event) => {
          if (working) event.preventDefault();
        }}
        onClose={() => setVisible(false)}
      >
        <div className="update-heading">
          <div>
            <h2 id="update-title">{t("软件更新")}</h2>
            <p>{t("当前版本 {version}", { version })}</p>
          </div>
          <button
            className="icon-button"
            aria-label={t("关闭")}
            disabled={working}
            autoFocus
            onClick={() => setVisible(false)}
          >
            <X size={18} />
          </button>
        </div>
        <div
          className="update-status"
          id="update-status"
          role="status"
          aria-live="polite"
        >
          {state.phase === "current" ? (
            <Check size={22} />
          ) : (
            <ArrowDownToLine size={22} />
          )}
          <strong>{status}</strong>
        </div>
        {state.notes && <p className="update-notes">{state.notes}</p>}
        {state.phase === "downloading" && (
          <div className="update-progress">
            <progress
              aria-label={t("正在下载更新…")}
              max={state.total || 1}
              value={
                state.total
                  ? Math.min(state.downloaded, state.total)
                  : undefined
              }
            />
            <span>
              {bytes(state.downloaded)}
              {state.total
                ? ` / ${bytes(state.total)} · ${percent(Math.min(state.downloaded / state.total, 1))}`
                : ""}
            </span>
          </div>
        )}
        {occupied && state.version && !working && (
          <p className="update-hint">{t("请等待当前文件任务完成后再更新。")}</p>
        )}
        <p className="update-hint">{t("仅检查软件版本，不上传 PDF 文件。")}</p>
        <div className="update-actions">
          <a
            className="secondary"
            href="https://github.com/SSToo/Squeeze-PDF/releases"
            target="_blank"
            rel="noreferrer"
            onClick={(event) => {
              if (native) {
                event.preventDefault();
                void invoke("open_release_notes").catch(() => {});
              }
            }}
          >
            {t("查看发布说明")}
          </a>
          {state.version && state.error !== "check" ? (
            <button
              className="primary"
              disabled={occupied || working || state.phase === "checking"}
              onClick={() => void controller.install()}
            >
              {t(state.error === "restart" ? "重启应用" : "下载安装并重启")}
            </button>
          ) : (
            <button
              className="primary"
              disabled={!native || state.phase === "checking" || working}
              onClick={() => void controller.check(true)}
            >
              {t("检查更新")}
            </button>
          )}
        </div>
      </dialog>
    </>
  );
}
