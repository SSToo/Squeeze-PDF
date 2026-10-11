import { strict as assert } from "node:assert";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { test } from "node:test";
import {
  cpSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import {
  verifyLicensedAssets,
  verifyLicenseBundle,
} from "./license-assets.mjs";

function fixture(run) {
  const directory = mkdtempSync(join(tmpdir(), "squeeze-license-test-"));
  try {
    return run(directory);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}
function pdfjsCopy(directory) {
  const manifest = JSON.parse(readFileSync("licenses/pdfjs-assets.json"));
  writeFileSync(
    join(directory, "package.json"),
    JSON.stringify({ version: manifest.pdfjs_version }),
  );
  for (const component of manifest.components)
    for (const file of component.files)
      cpSync(
        join("node_modules/pdfjs-dist", file.path),
        join(directory, file.path),
        { recursive: true },
      );
  return directory;
}
test("project metadata consistently declares Apache-2.0", () => {
  assert.match(readFileSync("Cargo.toml", "utf8"), /^license = "Apache-2.0"$/m);
  assert.equal(JSON.parse(readFileSync("package.json")).license, "Apache-2.0");
  assert.equal(
    JSON.parse(readFileSync("package-lock.json")).packages[""].license,
    "Apache-2.0",
  );
  const bundle = JSON.parse(readFileSync("src-tauri/tauri.conf.json")).bundle;
  assert.equal(bundle.license, "Apache-2.0");
  assert.equal(resolve("src-tauri", bundle.licenseFile), resolve("LICENSE"));
});
test("reviewed assets, source archives, full notices and MPL sources are present", () => {
  assert.equal(verifyLicensedAssets().components.length, 2);
  assert.deepEqual(verifyLicenseBundle(resolve("third-party")), {
    components: 2,
    sources: 2,
  });
});
test("Windows Git checkout preserves reviewed license and source bytes", () =>
  fixture((dir) => {
    const manifest = JSON.parse(readFileSync("licenses/pdfjs-assets.json"));
    cpSync(".gitattributes", join(dir, ".gitattributes"));
    cpSync("licenses", join(dir, "licenses"), { recursive: true });
    for (const name of ["LICENSE", "NOTICE"]) cpSync(name, join(dir, name));
    const git = (...args) =>
      execFileSync("git", ["-c", "core.autocrlf=true", ...args], {
        cwd: dir,
        stdio: "pipe",
      });
    git("init", "--quiet");
    git("add", "--", ".");
    rmSync(join(dir, "licenses"), { recursive: true });
    for (const name of ["LICENSE", "NOTICE"]) rmSync(join(dir, name));
    git("checkout-index", "--all", "--force");
    for (const component of manifest.components)
      for (const file of [
        ...component.license_files,
        ...(component.sources || []),
        ...(component.provenance || []),
      ])
        assert.equal(
          createHash("sha256")
            .update(readFileSync(join(dir, file.path)))
            .digest("hex"),
          file.sha256,
          `Git changed reviewed file bytes: ${file.path}`,
        );
    for (const name of ["LICENSE", "NOTICE", "licenses/MPL-2.0.txt"])
      assert.deepEqual(readFileSync(join(dir, name)), readFileSync(name));
  }));
test("a PDF.js version upgrade requires a fresh license and source review", () =>
  fixture((dir) => {
    writeFileSync(
      join(dir, "package.json"),
      JSON.stringify({ version: "99.0.0" }),
    );
    assert.throws(
      () => verifyLicensedAssets({ pdfjsRoot: dir }),
      /has not been checked/,
    );
  }));
test("changed font bytes cannot silently keep an old source declaration", () =>
  fixture((dir) => {
    pdfjsCopy(dir);
    writeFileSync(
      join(dir, "standard_fonts/LiberationSans-Regular.ttf"),
      "changed font",
    );
    assert.throws(
      () => verifyLicensedAssets({ pdfjsRoot: dir }),
      /checksum changed/,
    );
  }));
test("new QuickJS payloads cannot bypass the explicit asset inventory", () =>
  fixture((dir) => {
    pdfjsCopy(dir);
    writeFileSync(join(dir, "wasm/quickjs-extra.wasm"), "new payload");
    assert.throws(
      () => verifyLicensedAssets({ pdfjsRoot: dir }),
      /Unreviewed licensed/,
    );
  }));
test("a missing font source archive rejects the generated or packaged bundle", () =>
  fixture((dir) => {
    cpSync("third-party", dir, { recursive: true });
    rmSync(join(dir, "font-sources/pdfjs-liberation-1.07.4-ttx.tar.gz"));
    assert.throws(() => verifyLicenseBundle(dir), /ENOENT/);
  }));
test("removing QuickJS copyright attribution rejects the bundle", () =>
  fixture((dir) => {
    cpSync("third-party", dir, { recursive: true });
    const file = join(dir, "NOTICES.txt");
    writeFileSync(
      file,
      readFileSync(file, "utf8").replace(
        "Copyright (c) 2017-2021 Fabrice Bellard",
        "removed attribution",
      ),
    );
    assert.throws(() => verifyLicenseBundle(dir), /Missing full copyright/);
  }));
test("omitting an MPL source file rejects the bundle", () =>
  fixture((dir) => {
    cpSync("third-party", dir, { recursive: true });
    rmSync(join(dir, "mpl-sources/selectors-0.38.0/lib.rs"));
    assert.throws(() => verifyLicenseBundle(dir), /Incomplete or changed MPL/);
  }));
test("truncating the project license rejects the bundle", () =>
  fixture((dir) => {
    cpSync("third-party", dir, { recursive: true });
    writeFileSync(join(dir, "PROJECT-LICENSE.txt"), "Apache License 2.0");
    assert.throws(
      () => verifyLicenseBundle(dir),
      /Missing full project license/,
    );
  }));
test("omitting the project NOTICE rejects the bundle", () =>
  fixture((dir) => {
    cpSync("third-party", dir, { recursive: true });
    rmSync(join(dir, "PROJECT-NOTICE.txt"));
    assert.throws(() => verifyLicenseBundle(dir), /ENOENT/);
  }));
