// Generated corpus comparison, including both modes and strict preserve lossless.
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { basename, resolve } from "node:path";
const [corpus, target, baselineArg] = process.argv.slice(2);
if (!corpus || !target)
  throw new Error(
    "corpus directory, output directory, optional baseline worker",
  );
const directory = resolve(target);
mkdirSync(directory, { recursive: true });
const latest = resolve(process.env.SQUEEZE_WORKER || "target/release/squeeze");
const workers = [
  ["latest", latest],
  ...(baselineArg ? [["baseline", resolve(baselineArg)]] : []),
];
const hash = (file) =>
  createHash("sha256").update(readFileSync(file)).digest("hex");
const tasks = [];
for (const file of readdirSync(corpus)
  .filter((n) => n.endsWith(".pdf"))
  .sort()) {
  const input = resolve(corpus, file),
    before = hash(input);
  for (const mode of ["preserve", "raster"])
    for (const preset of mode === "preserve"
      ? ["lossless", "fidelity", "balanced", "small"]
      : ["lossless", "balanced", "small"])
      for (const [version, worker] of workers)
        tasks.push({ input, before, mode, preset, version, worker });
}
const rows = [];
async function run(t) {
  const name = basename(t.input, ".pdf"),
    stem = resolve(directory, `${name}-${t.mode}-${t.preset}-${t.version}`);
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
  if (hash(t.input) !== t.before) throw new Error("Source changed: " + t.input);
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
  console.log(
    `${name} / ${t.mode} / ${t.preset} / ${t.version}: ${result.error ? "FAILED " + result.error : result.result_bytes + " B, " + result.elapsed_ms + " ms"}`,
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
