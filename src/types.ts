export type Preset = "lossless" | "balanced" | "small";
export type OutputMode = "preserve" | "raster";
export type Info = {
  bytes: number;
  pages: number;
  images: number;
  has_interactive_content: boolean;
};
export type Report = {
  output_mode: OutputMode;
  outcome: "saved" | "unchanged";
  output: string | null;
  original_bytes: number;
  result_bytes: number;
  reduction_percent: number;
  saved_bytes: number;
  pages: number;
  elapsed_ms: number;
  optimized_streams: number;
  warnings: string[];
  images: {
    changed: boolean;
    reason: string;
    before_dimensions: number[];
    after_dimensions: number[];
  }[];
};
export type Imported = {
  path: string;
  name: string;
  info: Info | null;
  error: string | null;
};
export type Job = Imported & {
  status:
    | "queued"
    | "processing"
    | "saved"
    | "unchanged"
    | "failed"
    | "cancelled"
    | "skipped";
  report?: Report;
  phase?: string;
};
