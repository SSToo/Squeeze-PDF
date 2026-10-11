import { strict as assert } from "node:assert";
import { createHash } from "node:crypto";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";
import { test } from "node:test";
import { checkVersion, prepareAssets, publishRelease } from "./release.mjs";

const hash = (file) =>
  createHash("sha256").update(readFileSync(file)).digest("hex");
const asset = (file) => ({
  name: basename(file),
  state: "uploaded",
  size: readFileSync(file).length,
  digest: `sha256:${hash(file)}`,
});

function fixture(run, version = "0.1.24") {
  const root = mkdtempSync(join(tmpdir(), "squeeze-release-test-"));
  try {
    for (const file of [
      "package.json",
      "package-lock.json",
      "Cargo.toml",
      "src-tauri/tauri.conf.json",
    ]) {
      mkdirSync(dirname(join(root, file)), { recursive: true });
      cpSync(file, join(root, file));
      const original = JSON.parse(readFileSync("package.json")).version;
      writeFileSync(
        join(root, file),
        readFileSync(join(root, file), "utf8").replaceAll(original, version),
      );
    }
    const artifacts = join(root, "artifacts");
    const installers = [
      ["x86_64-pc-windows-msvc", "nsis", "x64-setup.exe"],
      ["aarch64-apple-darwin", "dmg", "aarch64.dmg"],
      ["x86_64-apple-darwin", "dmg", "x64.dmg"],
    ].map(([target, bundle, suffix], i) => {
      const file = join(
        artifacts,
        `squeeze-${target}`,
        "target",
        target,
        "release/bundle",
        bundle,
        `Squeeze PDF_${version}_${suffix}`,
      );
      mkdirSync(dirname(file), { recursive: true });
      writeFileSync(file, Buffer.from([0, 13, 10, 255, i]));
      return file;
    });
    return run({
      root,
      artifacts,
      installers,
      output: join(root, "output"),
      tag: `v${version}`,
    });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

function remote(initial = null, options = {}) {
  let release = initial ? structuredClone(initial) : null;
  const calls = [];
  return {
    calls,
    get release() {
      return release;
    },
    run(args) {
      calls.push(args);
      const [command, operation] = args;
      if (command === "api") return JSON.stringify(release);
      if (command === "release" && operation === "view") {
        if (options.readError) throw options.readError;
        if (!release)
          throw Object.assign(new Error("not found"), {
            stderr: "release not found",
          });
        return JSON.stringify({
          apiUrl: "https://api.github.com/repos/example/repo/releases/1",
        });
      }
      if (operation === "create") {
        assert(args.includes("--verify-tag"));
        assert(args.includes("--draft"));
        release = {
          draft: true,
          prerelease: args.includes("--prerelease"),
          assets: [],
        };
      } else if (operation === "upload") {
        assert(!args.includes("--clobber"));
        if (options.uploadError) throw new Error("upload interrupted");
        for (const file of args.slice(3, args.indexOf("--repo")))
          release.assets.push(asset(file));
        if (options.corruptUpload) release.assets[0].digest = "sha256:changed";
      } else if (operation === "edit") {
        assert.equal(release.assets.length, 4);
        assert(args.includes("--draft=false"));
        release.draft = false;
      } else throw new Error(`Unexpected gh command: ${args.join(" ")}`);
      return "";
    },
  };
}

test("version tags must match all application and lockfile versions", () =>
  fixture(({ root, tag }) => {
    assert.equal(checkVersion(tag, root), "0.1.24");
    assert.throws(() => checkVersion("v9.9.9", root), /must match/);
    const file = join(root, "src-tauri/tauri.conf.json");
    const config = JSON.parse(readFileSync(file));
    config.version = "9.9.9";
    writeFileSync(file, JSON.stringify(config));
    assert.throws(() => checkVersion(tag, root), /must match/);
  }));

test("all three installers get platform names and exact checksums", () =>
  fixture(({ root, tag, artifacts, output, installers }) => {
    const files = prepareAssets(tag, artifacts, output, root);
    assert.equal(files.length, 4);
    assert.deepEqual(
      readdirSync(output).sort(),
      [
        "SHA256SUMS.txt",
        "Squeeze-PDF_0.1.24_macos-arm64.dmg",
        "Squeeze-PDF_0.1.24_macos-x64.dmg",
        "Squeeze-PDF_0.1.24_windows-x64.exe",
      ].sort(),
    );
    const sums = readFileSync(join(output, "SHA256SUMS.txt"), "utf8");
    for (const file of files.slice(0, 3))
      assert(sums.includes(`${hash(file)}  ${basename(file)}\n`));
    assert.deepEqual(
      new Set(files.slice(0, 3).map(hash)),
      new Set(installers.map(hash)),
    );
  }));

for (const empty of [false, true])
  test(`${empty ? "empty" : "missing"} installers stop release preparation`, () =>
    fixture(({ root, tag, artifacts, output, installers }) => {
      if (empty) writeFileSync(installers[1], "");
      else rmSync(installers[1]);
      assert.throws(() => prepareAssets(tag, artifacts, output, root));
      assert.equal(existsSync(output), false);
    }));

test("release preparation cannot reuse a directory containing stale assets", () =>
  fixture(({ root, tag, artifacts, output }) => {
    mkdirSync(output);
    writeFileSync(join(output, "old.exe"), "old");
    assert.throws(
      () => prepareAssets(tag, artifacts, output, root),
      /must be empty/,
    );
  }));

for (const version of ["0.1.24", "0.1.25-rc.1"])
  test(`new ${version} release is published only after asset verification`, () =>
    fixture(({ root, tag, artifacts, output }) => {
      const files = prepareAssets(tag, artifacts, output, root);
      const github = remote();
      publishRelease(tag, "example/repo", files, github.run);
      assert.equal(github.release.draft, false);
      assert.equal(github.release.prerelease, version.includes("-"));
      assert.deepEqual(github.release.assets, files.map(asset));
    }, version));

test("an interrupted upload leaves the release in draft", () =>
  fixture(({ root, tag, artifacts, output }) => {
    const files = prepareAssets(tag, artifacts, output, root);
    const github = remote(null, { uploadError: true });
    assert.throws(
      () => publishRelease(tag, "example/repo", files, github.run),
      /interrupted/,
    );
    assert.equal(github.release.draft, true);
    assert(!github.calls.some((args) => args[1] === "edit"));
  }));

test("remote checksum mismatches leave the release in draft", () =>
  fixture(({ root, tag, artifacts, output }) => {
    const files = prepareAssets(tag, artifacts, output, root);
    const github = remote(null, { corruptUpload: true });
    assert.throws(
      () => publishRelease(tag, "example/repo", files, github.run),
      /remote SHA-256/,
    );
    assert.equal(github.release.draft, true);
  }));

test("retrying a partial draft uploads only missing files and keeps release metadata", () =>
  fixture(({ root, tag, artifacts, output }) => {
    const files = prepareAssets(tag, artifacts, output, root);
    const github = remote({
      draft: true,
      prerelease: true,
      body: "manual release notes",
      assets: [asset(files[0])],
    });
    publishRelease(tag, "example/repo", files, github.run);
    assert.equal(github.release.draft, false);
    assert.equal(github.release.body, "manual release notes");
    assert.equal(github.release.prerelease, true);
    const upload = github.calls.find((args) => args[1] === "upload");
    assert.deepEqual(upload.slice(3, upload.indexOf("--repo")), files.slice(1));
  }));

test("rerunning an identical published immutable release performs no writes", () =>
  fixture(({ root, tag, artifacts, output }) => {
    const files = prepareAssets(tag, artifacts, output, root);
    const github = remote({
      draft: false,
      immutable: true,
      assets: files.map(asset),
    });
    publishRelease(tag, "example/repo", files, github.run);
    assert(
      !github.calls.some((args) =>
        ["create", "upload", "edit"].includes(args[1]),
      ),
    );
  }));

test("different existing asset bytes are never overwritten", () =>
  fixture(({ root, tag, artifacts, output }) => {
    const files = prepareAssets(tag, artifacts, output, root);
    const existing = asset(files[0]);
    existing.digest = "sha256:changed";
    const github = remote({ draft: false, assets: [existing] });
    assert.throws(
      () => publishRelease(tag, "example/repo", files, github.run),
      /different bytes/,
    );
    assert(
      !github.calls.some((args) =>
        ["create", "upload", "edit"].includes(args[1]),
      ),
    );
  }));

test("API failures are surfaced instead of being treated as missing releases", () =>
  fixture(({ root, tag, artifacts, output }) => {
    const files = prepareAssets(tag, artifacts, output, root);
    const error = Object.assign(new Error("server error"), {
      stderr: "HTTP 503",
    });
    const github = remote(null, { readError: error });
    assert.throws(
      () => publishRelease(tag, "example/repo", files, github.run),
      /server error/,
    );
    assert(!github.calls.some((args) => args[1] === "create"));
  }));
