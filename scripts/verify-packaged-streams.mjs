// Verify that the distributed worker produces the exact validated output bytes.
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
const run = promisify(execFile),
  [recordsArg, targetArg, workerArg] = process.argv.slice(2);
const rows = JSON.parse(readFileSync(recordsArg)).filter(
    (r) => r.version === "latest",
  ),
  target = resolve(targetArg),
  worker = resolve(workerArg);
mkdirSync(target, { recursive: true });
const sha = (f) => createHash("sha256").update(readFileSync(f)).digest("hex"),
  results = [];
for (const row of rows) {
  const output = join(target, `${row.name}-${row.mode}-${row.preset}.pdf`),
    args = [
      "compress",
      row.input,
      "-o",
      output,
      "--preset",
      row.preset,
      "--output-mode",
      row.mode,
      "--json",
      ...(row.mode === "preserve" && row.preset !== "lossless"
        ? ["--compact-paths", "--reuse-paths"]
        : []),
    ];
  const { stdout } = await run(worker, args, { maxBuffer: 16 * 1024 * 1024 }),
    result = JSON.parse(stdout),
    actual = result.output || row.input,
    expected = row.output || row.input;
  if (
    sha(actual) !== sha(expected) ||
    result.result_bytes !== row.result_bytes ||
    sha(row.input) !== row.input_sha256
  )
    throw Error(
      "Packaged output mismatch: " +
        row.name +
        "/" +
        row.mode +
        "/" +
        row.preset,
    );
  writeFileSync(output.replace(/\.pdf$/, ".json"),JSON.stringify(result,null,2));
  results.push({
    name: row.name,
    mode: row.mode,
    preset: row.preset,
    bytes: result.result_bytes,
    sha256: sha(actual),
    byte_equal: true,
    report: result,
  });
}
writeFileSync(join(target, "results.json"), JSON.stringify(results, null, 2));
console.log(
  `${results.length} packaged outputs match development bytes; source hashes unchanged`,
);
