import { execFileSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  symlinkSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { readFileSync } from "node:fs";

if (process.platform !== "darwin") throw new Error("Run this script on macOS.");
const target = process.env.CARGO_BUILD_TARGET;
const bundleRoot = resolve(
  "target",
  ...(target ? [target] : []),
  "release/bundle",
);
const app = join(bundleRoot, "macos/Squeeze PDF.app");
if (!existsSync(app)) throw new Error("Build the app bundle first.");
const triple =
  target ||
  execFileSync("rustc", ["--print", "host-tuple"], { encoding: "utf8" }).trim();
const version = JSON.parse(readFileSync("package.json", "utf8")).version;
const output = join(
  bundleRoot,
  "dmg",
  "Squeeze PDF_" + version + "_" + triple.split("-")[0] + ".dmg",
);
mkdirSync(join(bundleRoot, "dmg"), { recursive: true });
const stage = mkdtempSync(join(tmpdir(), "squeeze-dmg-"));
try {
  execFileSync("ditto", [app, join(stage, "Squeeze PDF.app")]);
  symlinkSync("/Applications", join(stage, "Applications"));
  execFileSync(
    "hdiutil",
    [
      "create",
      "-volname",
      "Squeeze PDF",
      "-srcfolder",
      stage,
      "-ov",
      "-format",
      "UDZO",
      output,
    ],
    { stdio: "inherit" },
  );
  execFileSync("hdiutil", ["verify", output], { stdio: "inherit" });
  console.log(output);
} finally {
  rmSync(stage, { recursive: true, force: true });
}
