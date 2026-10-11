import type { DownloadEvent } from "@tauri-apps/plugin-updater";

export interface AppUpdate {
  version: string;
  body?: string;
  download(
    onEvent: (event: DownloadEvent) => void,
    options: { timeout: number },
  ): Promise<void>;
  install(): Promise<void>;
  close(): Promise<void>;
}
export interface UpdateState {
  phase:
    | "idle"
    | "checking"
    | "current"
    | "available"
    | "downloading"
    | "installing"
    | "installed"
    | "error";
  version?: string;
  notes?: string;
  downloaded: number;
  total?: number;
  error?: "check" | "install" | "restart";
}

// Keep a single update resource and serialize checks, installation and restart.
// The shared activity lock excludes PDF jobs even before React renders again.
export class UpdateController {
  private state: UpdateState = { phase: "idle", downloaded: 0 };
  private listeners = new Set<() => void>();
  private update: AppUpdate | null = null;
  private running = false;
  private installed = false;
  private downloaded = false;
  private adapter: {
    check(): Promise<AppUpdate | null>;
    relaunch(): Promise<void>;
  };
  private activity: { acquire(): boolean; release(): void };
  constructor(
    adapter: {
      check(): Promise<AppUpdate | null>;
      relaunch(): Promise<void>;
    },
    activity: { acquire(): boolean; release(): void },
  ) {
    this.adapter = adapter;
    this.activity = activity;
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private set(patch: Partial<UpdateState>) {
    this.state = { ...this.state, ...patch };
    this.listeners.forEach((listener) => listener());
  }
  async check(manual = false) {
    if (this.running || this.installed) return;
    this.running = true;
    this.set({ phase: "checking", error: undefined });
    try {
      const next = await this.adapter.check();
      const previous = this.update;
      this.update = next;
      this.downloaded = false;
      this.set({
        phase: next ? "available" : "current",
        version: next?.version,
        notes: next?.body,
        downloaded: 0,
        total: undefined,
      });
      if (previous && previous !== next) await previous.close().catch(() => {});
    } catch {
      // Offline startup must not interrupt the local compression workflow.
      this.set({
        phase: manual ? "error" : this.update ? "available" : "idle",
        error: manual ? "check" : undefined,
      });
    } finally {
      this.running = false;
    }
  }
  async install() {
    if (this.running || !this.update || !this.activity.acquire()) return;
    this.running = true;
    let restarting = false;
    try {
      if (!this.installed) {
        if (!this.downloaded) {
          this.set({
            phase: "downloading",
            downloaded: 0,
            total: undefined,
            error: undefined,
          });
          await this.update.download(
            (event) => {
              if (event.event === "Started")
                this.set({ total: event.data.contentLength || undefined });
              else if (event.event === "Progress")
                this.set({
                  downloaded: this.state.downloaded + event.data.chunkLength,
                });
            },
            { timeout: 600_000 },
          );
          this.downloaded = true;
        }
        // The official updater verifies the signature before installation.
        this.set({ phase: "installing" });
        await this.update.install();
        this.downloaded = false;
        this.installed = true;
      }
      this.set({ phase: "installed", error: undefined });
      await this.adapter.relaunch();
      restarting = true;
    } catch {
      this.set({
        phase: "error",
        error: this.installed ? "restart" : "install",
      });
    } finally {
      this.running = false;
      if (!restarting) this.activity.release();
    }
  }
}
