import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
const directory = resolve(process.argv[2] || "tmp/benchmark-" + Date.now());
mkdirSync(directory, { recursive: true });
execFileSync(
  "cargo",
  ["run", "-p", "squeeze-core", "--example", "make_sample", "--", directory],
  { stdio: "inherit" },
);
execFileSync("cargo", ["build", "-p", "squeeze-cli", "--release", "--locked"], {
  stdio: "inherit",
});
const worker = resolve(
  "target/release/squeeze" + (process.platform === "win32" ? ".exe" : ""),
);
execFileSync(
  "cargo",
  [
    "run",
    "-p",
    "squeeze-core",
    "--example",
    "make_encoding_samples",
    "--",
    directory,
  ],
  { stdio: "inherit" },
);
const rows = [];
for (const file of [
  "scan-rgb",
  "photo-jpeg",
  "binary",
  "palette",
  "gray-gradient",
  "jpeg-entropy",
]) {
  const input = resolve(directory, file + ".pdf");
  const hash = () =>
    createHash("sha256").update(readFileSync(input)).digest("hex");
  const before = hash();
  for (const mode of ["preserve", "raster"]) {
    const presets =
      mode === "preserve"
        ? ["fidelity", "balanced", "small"]
        : ["lossless", "balanced", "small"];
    for (const preset of presets) {
      const output = resolve(
        directory,
        file + "-" + mode + "-" + preset + ".pdf",
      );
      const report = JSON.parse(
        execFileSync(
          worker,
          [
            "compress",
            input,
            "-o",
            output,
            "--preset",
            preset,
            "--output-mode",
            mode,
            "--json",
            ...(mode === "preserve"
              ? ["--compact-paths", "--reuse-paths"]
              : []),
          ],
          { encoding: "utf8" },
        ),
      );
      if (hash() !== before) throw new Error("Source changed");
      rows.push(report);
      console.log(
        file +
          " / " +
          mode +
          " / " +
          preset +
          ": " +
          report.original_bytes +
          " → " +
          report.result_bytes +
          " bytes, " +
          report.reduction_percent.toFixed(2) +
          "%, " +
          report.elapsed_ms +
          " ms",
      );
    }
  }
}
writeFileSync(
  resolve(directory, "reports.json"),
  JSON.stringify(rows, null, 2) + "\n",
);
console.log(
  "Synthetic sample results only; not a real-document compatibility benchmark.",
);
