import { execFileSync } from "node:child_process";
import { cpSync, mkdirSync } from "node:fs";
import { resolve } from "node:path";
const target =
  process.env.CARGO_BUILD_TARGET ||
  execFileSync("rustc", ["--print", "host-tuple"], { encoding: "utf8" }).trim();
const release = process.argv.includes("--release");
execFileSync(
  "cargo",
  [
    "build",
    "-p",
    "squeeze-cli",
    "--locked",
    "--target",
    target,
    ...(release ? ["--release"] : []),
  ],
  { stdio: "inherit" },
);
const extension = target.includes("windows") ? ".exe" : "";
mkdirSync("src-tauri/binaries", { recursive: true });
cpSync(
  resolve(
    "target",
    target,
    release ? "release" : "debug",
    "squeeze" + extension,
  ),
  resolve("src-tauri/binaries", "squeeze-worker-" + target + extension),
);
