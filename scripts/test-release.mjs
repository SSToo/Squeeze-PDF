import { strict as assert } from "node:assert";
import {
  createHash,
  generateKeyPairSync,
  randomBytes,
  sign,
} from "node:crypto";
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
import {
  checkVersion,
  prepareAssets,
  publishRelease,
  verifyUpdateSignature,
} from "./release.mjs";

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
    // Ephemeral fixture keys never leave this test process.
    const { publicKey, privateKey } = generateKeyPairSync("ed25519");
    const keyId = randomBytes(8);
    const publicBytes = Buffer.concat([
      Buffer.from("Ed"),
      keyId,
      publicKey.export({ format: "der", type: "spki" }).subarray(-32),
    ]);
    const configPath = join(root, "src-tauri/tauri.conf.json");
    const config = JSON.parse(readFileSync(configPath));
    config.plugins.updater.pubkey = Buffer.from(
      `untrusted comment: test key\n${publicBytes.toString("base64")}\n`,
    ).toString("base64");
    writeFileSync(configPath, JSON.stringify(config));
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
    const updaters = [
      installers[0],
      ...["aarch64-apple-darwin", "x86_64-apple-darwin"].map((target, i) => {
        const file = join(
          artifacts,
          `squeeze-${target}`,
          "target",
          target,
          "release/bundle/macos/Squeeze PDF.app.tar.gz",
        );
        mkdirSync(dirname(file), { recursive: true });
        writeFileSync(file, `signed update payload ${i}`);
        return file;
      }),
    ];
    for (const file of updaters) {
      const signature = sign(
        null,
        createHash("blake2b512").update(readFileSync(file)).digest(),
        privateKey,
      );
      const signed = Buffer.concat([Buffer.from("ED"), keyId, signature]);
      const comment = `timestamp:0\tversion:${version}`;
      const global = sign(
        null,
        Buffer.concat([signature, Buffer.from(comment)]),
        privateKey,
      );
      const text = `untrusted comment: test signature\n${signed.toString("base64")}\ntrusted comment: ${comment}\n${global.toString("base64")}\n`;
      writeFileSync(`${file}.sig`, Buffer.from(text).toString("base64"));
    }
    return run({
      root,
      artifacts,
      installers,
      updaters,
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
        assert.equal(release.assets.length, 10);
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

test("installers and signed updates get platform names, matching feed URLs and exact checksums", () =>
  fixture(({ root, tag, artifacts, output, installers }) => {
    const files = prepareAssets(tag, artifacts, output, root);
    assert.equal(files.length, 10);
    assert.deepEqual(
      readdirSync(output).sort(),
      [
        "SHA256SUMS.txt",
        "Squeeze-PDF_0.1.24_macos-arm64.dmg",
        "Squeeze-PDF_0.1.24_macos-x64.dmg",
        "Squeeze-PDF_0.1.24_windows-x64.exe",
        "Squeeze-PDF_0.1.24_macos-arm64.app.tar.gz",
        "Squeeze-PDF_0.1.24_macos-x64.app.tar.gz",
        "Squeeze-PDF_0.1.24_macos-arm64.app.tar.gz.sig",
        "Squeeze-PDF_0.1.24_macos-x64.app.tar.gz.sig",
        "Squeeze-PDF_0.1.24_windows-x64.exe.sig",
        "latest.json",
      ].sort(),
    );
    const sums = readFileSync(join(output, "SHA256SUMS.txt"), "utf8");
    for (const file of files.filter(
      (file) => basename(file) !== "SHA256SUMS.txt",
    ))
      assert(sums.includes(`${hash(file)}  ${basename(file)}\n`));
    assert.deepEqual(
      new Set(files.filter((file) => /\.(exe|dmg)$/.test(file)).map(hash)),
      new Set(installers.map(hash)),
    );
    const manifest = JSON.parse(readFileSync(join(output, "latest.json")));
    assert.equal(manifest.version, "0.1.24");
    assert.deepEqual(Object.keys(manifest.platforms).sort(), [
      "darwin-aarch64",
      "darwin-x86_64",
      "windows-x86_64",
    ]);
    for (const [platform, entry] of Object.entries(manifest.platforms)) {
      const url = new URL(entry.url);
      assert.equal(url.origin, "https://github.com");
      assert(
        url.pathname.startsWith(
          "/SSToo/Squeeze-PDF/releases/download/v0.1.24/",
        ),
      );
      const binary = basename(url.pathname);
      assert.equal(
        entry.signature,
        readFileSync(join(output, `${binary}.sig`), "utf8").trim(),
      );
      assert(
        binary.endsWith(platform.startsWith("darwin") ? ".app.tar.gz" : ".exe"),
      );
    }
  }));

test("missing signatures or altered updater payloads prevent any release preparation", () =>
  fixture(({ root, tag, artifacts, output, updaters }) => {
    writeFileSync(updaters[1], "tampered update");
    assert.throws(
      () => prepareAssets(tag, artifacts, output, root),
      /signature verification failed/,
    );
    assert.equal(existsSync(output), false);
    rmSync(`${updaters[0]}.sig`);
    assert.throws(() => prepareAssets(tag, artifacts, output, root));
    assert.equal(existsSync(output), false);
  }));

test("a changed signing key or trusted comment is rejected", () =>
  fixture(({ root, tag, artifacts, output, updaters }) => {
    const file = `${updaters[0]}.sig`;
    const original = readFileSync(file, "utf8");
    writeFileSync(
      file,
      Buffer.from(
        Buffer.from(original, "base64")
          .toString()
          .replace("timestamp:0", "timestamp:1"),
      ).toString("base64"),
    );
    assert.throws(
      () => prepareAssets(tag, artifacts, output, root),
      /signature verification failed/,
    );
    writeFileSync(file, original);
    const path = join(root, "src-tauri/tauri.conf.json");
    const config = JSON.parse(readFileSync(path));
    const text = Buffer.from(config.plugins.updater.pubkey, "base64")
      .toString()
      .split("\n");
    const key = Buffer.from(text[1], "base64");
    key[2] ^= 1;
    text[1] = key.toString("base64");
    config.plugins.updater.pubkey = Buffer.from(text.join("\n")).toString(
      "base64",
    );
    writeFileSync(path, JSON.stringify(config));
    assert.throws(
      () => prepareAssets(tag, artifacts, output, root),
      /verification key/,
    );
    assert.equal(existsSync(output), false);
  }));

test("an authentic older artifact cannot be announced as a newer version", () =>
  fixture(({ root, updaters }) => {
    const key = JSON.parse(
      readFileSync(join(root, "src-tauri/tauri.conf.json")),
    ).plugins.updater.pubkey;
    const signature = readFileSync(`${updaters[0]}.sig`, "utf8");
    assert.throws(
      () => verifyUpdateSignature(updaters[0], signature, key, "9.9.9"),
      /not bound to version/,
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
