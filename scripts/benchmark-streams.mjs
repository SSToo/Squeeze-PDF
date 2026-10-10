// Both-mode complete-file comparison. CLI cannot rasterize the stamp fixture;
// its preserve cases and the desktop renderer cover that separate contract.
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { basename, resolve } from "node:path";
const [corpus, target, baseline] = process.argv.slice(2);
if (!baseline) throw Error("corpus output-directory baseline-worker");
mkdirSync(target, { recursive: true });
const latest = resolve(process.env.SQUEEZE_WORKER || "target/release/squeeze");
const hash = (f) => createHash("sha256").update(readFileSync(f)).digest("hex");
const tasks = [],
  reused = [];
const prior = (process.env.SQUEEZE_BASELINE_REPORTS || "")
  .split(",")
  .filter(Boolean)
  .flatMap((path) => JSON.parse(readFileSync(path, "utf8")))
  .filter((r) => r.version === "baseline");
for (const file of readdirSync(corpus)
  .filter((f) => f.endsWith(".pdf"))
  .sort()) {
  const input = resolve(corpus, file),
    before = hash(input);
  for (const mode of process.env.SQUEEZE_MODES?.split(",") ||
    (file === "appearance-protected-wrapper.pdf"
      ? ["preserve"]
      : ["preserve", "raster"]))
    for (const preset of mode === "preserve"
      ? ["lossless", "fidelity", "balanced", "small"]
      : ["lossless", "balanced", "small"])
      for (const [version, worker] of [
        ["baseline", resolve(baseline)],
        ["latest", latest],
      ]) {
        const old =
          version === "baseline" &&
          prior.findLast(
            (r) =>
              r.name === basename(file, ".pdf") &&
              r.mode === mode &&
              r.preset === preset &&
              r.input_sha256 === before &&
              !r.error,
          );
        if (old) reused.push({ ...old, input, baseline_reused: true });
        else tasks.push({ input, before, mode, preset, version, worker });
      }
}
const rows = reused;
async function run(t) {
  const name = basename(t.input, ".pdf"),
    stem = resolve(target, `${name}-${t.mode}-${t.preset}-${t.version}`);
  const args = [
    "compress",
    t.input,
    "-o",
    stem + ".pdf",
    "--preset",
    t.preset,
    "--output-mode",
    t.mode,
    "--json",
    ...(t.mode === "preserve" && t.preset !== "lossless"
      ? ["--compact-paths", "--reuse-paths"]
      : []),
  ];
  const result = await new Promise((ok, fail) => {
    const c = spawn(t.worker, args, { stdio: ["ignore", "pipe", "pipe"] });
    let out = "",
      err = "";
    c.stdout.on("data", (b) => (out += b));
    c.stderr.on("data", (b) => (err += b));
    c.on("error", fail);
    c.on("close", (code) =>
      ok(code ? { error: err, exit_code: code } : JSON.parse(out)),
    );
  });
  if (hash(t.input) !== t.before) throw Error("Source changed: " + name);
  writeFileSync(stem + ".json", JSON.stringify(result, null, 2) + "\n");
  rows.push({
    name,
    version: t.version,
    input_sha256: t.before,
    input: t.input,
    mode: t.mode,
    preset: t.preset,
    ...result,
  });
  writeFileSync(
    resolve(target, "reports.json"),
    JSON.stringify(rows, null, 2) + "\n",
  );
  console.log(
    `${name} ${t.mode}/${t.preset} ${t.version}: ${result.error ? "FAILED " + result.error : result.result_bytes + " B"}`,
  );
}
async function consumer() {
  while (tasks.length) await run(tasks.shift());
}
await Promise.all([consumer(), consumer()]);
for (const r of rows.filter((r) => r.version === "latest")) {
  const b = rows.find(
    (b) =>
      b.version === "baseline" &&
      b.name === r.name &&
      b.mode === r.mode &&
      b.preset === r.preset,
  );
  if (r.error || b.error || r.result_bytes > b.result_bytes)
    throw Error(
      "Regression: " +
        JSON.stringify([
          r.name,
          r.mode,
          r.preset,
          r.result_bytes,
          b.result_bytes,
          r.error,
          b.error,
        ]),
    );
}
console.log(
  `Compared ${rows.length / 2} complete-file pairs; no size regressions.`,
);
