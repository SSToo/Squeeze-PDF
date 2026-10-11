import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  copyFileSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from "node:fs";
import { basename, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export function checkVersion(tag, root = resolve(".")) {
  const json = (name) => JSON.parse(readFileSync(join(root, name), "utf8"));
  const version = json("package.json").version;
  const cargo = readFileSync(join(root, "Cargo.toml"), "utf8")
    .split("[workspace.package]")[1]
    ?.split(/^\[/m)[0]
    .match(/^version = "([^"]+)"/m)?.[1];
  const lock = json("package-lock.json");
  if (
    tag !== `v${version}` ||
    cargo !== version ||
    json("src-tauri/tauri.conf.json").version !== version ||
    lock.version !== version ||
    lock.packages[""].version !== version
  )
    throw new Error(
      `Release tag ${tag} must match every project version (v${version}).`,
    );
  return version;
}

export function prepareAssets(tag, artifacts, output, root = resolve(".")) {
  const version = checkVersion(tag, root);
  const platforms = [
    ["x86_64-pc-windows-msvc", "nsis", "x64-setup.exe", "windows-x64.exe"],
    ["aarch64-apple-darwin", "dmg", "aarch64.dmg", "macos-arm64.dmg"],
    ["x86_64-apple-darwin", "dmg", "x64.dmg", "macos-x64.dmg"],
  ];
  const installers = platforms.map(([target, bundle, suffix, platform]) => {
    const source = join(
      artifacts,
      `squeeze-${target}`,
      "target",
      target,
      "release/bundle",
      bundle,
      `Squeeze PDF_${version}_${suffix}`,
    );
    const stat = lstatSync(source);
    if (!stat.isFile() || stat.size === 0)
      throw new Error(`Missing or empty installer: ${source}`);
    return { source, name: `Squeeze-PDF_${version}_${platform}` };
  });
  mkdirSync(output, { recursive: true });
  if (readdirSync(output).length)
    throw new Error(`Release output directory must be empty: ${output}`);
  const files = installers
    .map(({ source, name }) => {
      const path = join(output, name);
      copyFileSync(source, path);
      return path;
    })
    .sort();
  const checksum = join(output, "SHA256SUMS.txt");
  writeFileSync(
    checksum,
    files.map((file) => `${digest(file)}  ${basename(file)}\n`).join(""),
  );
  return [...files, checksum];
}

function digest(file) {
  return createHash("sha256").update(readFileSync(file)).digest("hex");
}

function gh(args) {
  return execFileSync("gh", args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
}

export function publishRelease(tag, repository, files, run = gh) {
  if (!/^[\w.-]+\/[\w.-]+$/.test(repository || ""))
    throw new Error("GITHUB_REPOSITORY must be owner/repository.");
  const readRelease = () => {
    try {
      // The CLI also finds drafts, which the release-by-tag API may omit.
      const { apiUrl } = JSON.parse(
        run(["release", "view", tag, "--repo", repository, "--json", "apiUrl"]),
      );
      return JSON.parse(run(["api", apiUrl]));
    } catch (error) {
      if (/HTTP 404|release not found/i.test(String(error.stderr))) return null;
      throw error;
    }
  };
  const expected = files.map((path) => ({
    path,
    name: basename(path),
    size: lstatSync(path).size,
    digest: `sha256:${digest(path)}`,
  }));
  const matches = (asset, file) =>
    asset.state === "uploaded" &&
    asset.size === file.size &&
    asset.digest === file.digest;
  let release = readRelease();
  if (!release) {
    run([
      "release",
      "create",
      tag,
      "--repo",
      repository,
      "--verify-tag",
      "--draft",
      "--title",
      `Squeeze PDF ${tag}`,
      "--generate-notes",
      ...(tag.includes("-") ? ["--prerelease"] : []),
    ]);
    release = readRelease();
  }
  if (!release) throw new Error("Unable to read the release after creation.");
  const missing = [];
  for (const file of expected) {
    const existing = release.assets.find((asset) => asset.name === file.name);
    if (!existing) missing.push(file.path);
    else if (!matches(existing, file))
      throw new Error(
        `Release asset already exists with different bytes: ${file.name}. Use a new version tag.`,
      );
  }
  if (missing.length)
    run(["release", "upload", tag, ...missing, "--repo", repository]);
  const uploaded = readRelease();
  if (
    !uploaded ||
    !expected.every((file) =>
      uploaded.assets.some(
        (asset) => asset.name === file.name && matches(asset, file),
      ),
    )
  )
    throw new Error(
      "Release assets failed the remote SHA-256 verification; the draft was not published.",
    );
  if (uploaded.draft)
    run(["release", "edit", tag, "--repo", repository, "--draft=false"]);
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const [command, tag, artifacts, output] = process.argv.slice(2);
  if (command === "check-version") checkVersion(tag);
  else if (["prepare", "publish"].includes(command) && artifacts && output) {
    const files = prepareAssets(tag, resolve(artifacts), resolve(output));
    if (command === "publish")
      publishRelease(tag, process.env.GITHUB_REPOSITORY, files);
    console.log(files.map((file) => basename(file)).join("\n"));
  } else
    throw new Error(
      "Usage: node scripts/release.mjs check-version TAG | prepare|publish TAG ARTIFACTS OUTPUT",
    );
}
