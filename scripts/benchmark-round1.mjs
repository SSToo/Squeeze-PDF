import { execFileSync, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, resolve } from "node:path";
const directory = resolve(process.argv[2] || "tmp/round1-" + Date.now());
const baseline = process.argv[3] ? resolve(process.argv[3]) : null;
const external = process.argv.slice(4).map((p) => resolve(p));
mkdirSync(directory, { recursive: true });
execFileSync("cargo", ["build", "-p", "squeeze-cli", "--release", "--locked"], {
  stdio: "inherit",
});
const latest = resolve(
  "target/release/squeeze" + (process.platform === "win32" ? ".exe" : ""),
);
let files = external;
if (!files.length) {
  execFileSync(
    "cargo",
    [
      "run",
      "-p",
      "squeeze-core",
      "--example",
      "make_encoding_samples",
      "--locked",
      "--",
      directory,
    ],
    { stdio: "inherit" },
  );
  files = ["binary", "palette", "gray-gradient", "jpeg-entropy"].map((name) =>
    resolve(directory, name + ".pdf"),
  );
}
const hash = (file) =>
  createHash("sha256").update(readFileSync(file)).digest("hex");
const tasks = [];
for (const input of files) {
  const before = hash(input);
  for (const mode of external.length ? ["preserve"] : ["preserve", "raster"]) {
    for (const preset of mode === "preserve"
      ? ["fidelity", "balanced", "small"]
      : ["lossless", "balanced", "small"]) {
      for (const [version, worker] of [
        ["latest", latest],
        ...(baseline ? [["baseline", baseline]] : []),
      ]) {
        tasks.push({ input, before, mode, preset, version, worker });
      }
    }
  }
}
const rows = [];
async function run(task) {
  const { input, before, mode, preset, version, worker } = task;
  const name = basename(input, ".pdf");
  const stem = resolve(directory, `${name}-${mode}-${preset}-${version}`);
  const args = [
    "compress",
    input,
    "-o",
    stem + ".pdf",
    "--preset",
    preset,
    "--output-mode",
    mode,
    "--json",
    ...(mode === "preserve" ? ["--compact-paths", "--reuse-paths"] : []),
  ];
  const report = await new Promise((ok, fail) => {
    const child = spawn(worker, args, { stdio: ["ignore", "pipe", "pipe"] });
    let out = "",
      err = "";
    child.stdout.on("data", (chunk) => (out += chunk));
    child.stderr.on("data", (chunk) => (err += chunk));
    child.on("error", fail);
    child.on("close", (code) =>
      code ? fail(new Error(err)) : ok(JSON.parse(out)),
    );
  });
  if (hash(input) !== before) throw new Error("Source changed: " + input);
  writeFileSync(stem + ".json", JSON.stringify(report, null, 2) + "\n");
  rows.push({ name, version, input_sha256: before, ...report });
  console.log(
    `${name} / ${mode} / ${preset} / ${version}: ${report.result_bytes} bytes, ${report.elapsed_ms} ms`,
  );
}
async function consumer() {
  while (tasks.length) await run(tasks.shift());
}
await Promise.all([consumer(), consumer()]);
writeFileSync(
  resolve(directory, "reports.json"),
  JSON.stringify(rows, null, 2) + "\n",
);
