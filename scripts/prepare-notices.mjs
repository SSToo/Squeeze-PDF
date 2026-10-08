import { execFileSync } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";

const target =
  process.env.CARGO_BUILD_TARGET ||
  execFileSync("rustc", ["--print", "host-tuple"], { encoding: "utf8" }).trim();
const metadata = JSON.parse(
  execFileSync(
    "cargo",
    [
      "metadata",
      "--locked",
      "--format-version",
      "1",
      "--filter-platform",
      target,
    ],
    { encoding: "utf8", maxBuffer: 32 * 1024 * 1024 },
  ),
);
const dest = resolve("third-party");
mkdirSync(dest, { recursive: true });
rmSync(join(dest, "mpl-sources"), { recursive: true, force: true });
const notices = [
  "Squeeze PDF — Third-party notices",
  "Generated from Cargo.lock and package-lock.json. Includes build dependencies.",
  "Third-party code and assets retain their original licenses.",
  "Unmodified MPL dependency sources are provided under mpl-sources/.",
  "",
];
const inventory = [];
function appendFiles(root, depth = 0, prefix = "") {
  for (const item of readdirSync(root, { withFileTypes: true })) {
    const path = join(root, item.name);
    const relative = prefix + item.name;
    if (
      item.isFile() &&
      /(?:^|[._-])(license|licence|copying|notice|copyright)(?:[._-]|$)/i.test(
        item.name,
      )
    ) {
      const text = readFileSync(path, "utf8");
      if (!text.includes("\0"))
        notices.push("--- " + relative + " ---", text, "");
    } else if (
      item.isDirectory() &&
      depth < 2 &&
      item.name !== "node_modules" &&
      !item.name.startsWith(".")
    ) {
      appendFiles(path, depth + 1, relative + "/");
    }
  }
}
notices.push(
  "=== hayro upstream NOTICE (pinned revision 1e5b8eb7bfe03afe431ed0bc1d704c9432323ef6) ===",
  readFileSync("licenses/hayro-NOTICE.md", "utf8"),
  "",
);
// The registry wrappers contain an abbreviated IJG notice. Include the full
// upstream documents for the exact native source revision vendored by 2.2.3.
const mozjpegSys = metadata.packages.find((p) => p.name === "mozjpeg-sys");
if (mozjpegSys) {
  if (mozjpegSys.version !== "2.2.3")
    throw new Error(
      "Update pinned MozJPEG license documents for this version.",
    );
  notices.push(
    "=== MozJPEG native source revision c2bc3511d547870c4f67bbafda5fba794ba71f7f ===",
    "This software is based in part on the work of the Independent JPEG Group.",
    "Source: https://github.com/mozilla/mozjpeg/tree/c2bc3511d547870c4f67bbafda5fba794ba71f7f",
    readFileSync("licenses/mozjpeg-README.ijg", "utf8"),
    readFileSync("licenses/mozjpeg-LICENSE.md", "utf8"),
    "",
  );
}
for (const pkg of metadata.packages
  .filter((p) => p.source)
  .sort((a, b) => a.name.localeCompare(b.name))) {
  const root = dirname(pkg.manifest_path);
  const title = pkg.name + " " + pkg.version;
  inventory.push({
    ecosystem: "cargo",
    name: pkg.name,
    version: pkg.version,
    license: pkg.license,
    repository: pkg.repository,
  });
  notices.push(
    "=== " + title + " ===",
    "License: " + (pkg.license || "See package source"),
    "Source: " +
      (pkg.repository ||
        "https://crates.io/crates/" + pkg.name + "/" + pkg.version),
    "",
  );
  appendFiles(root);
  if (pkg.license?.includes("MPL-2.0"))
    cpSync(root, join(dest, "mpl-sources", title.replaceAll(" ", "-")), {
      recursive: true,
    });
}
const lock = JSON.parse(readFileSync("package-lock.json", "utf8"));
for (const [path, entry] of Object.entries(lock.packages).filter(
  ([path, entry]) => path && !entry.dev,
)) {
  if (!existsSync(path)) continue;
  const pkg = JSON.parse(readFileSync(join(path, "package.json"), "utf8"));
  inventory.push({
    ecosystem: "npm",
    name: pkg.name,
    version: pkg.version,
    license: pkg.license,
    repository: pkg.repository,
  });
  notices.push(
    "=== " + pkg.name + " " + pkg.version + " ===",
    "License: " + (pkg.license || "See package source"),
    "",
  );
  appendFiles(path);
}
writeFileSync(join(dest, "NOTICES.txt"), notices.join("\n"));
writeFileSync(
  join(dest, "dependencies.json"),
  JSON.stringify({ target, dependencies: inventory }, null, 2) + "\n",
);
console.log(
  "Prepared notices for " +
    inventory.length +
    " dependencies (" +
    target +
    ").",
);
