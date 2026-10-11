import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";

export function sourceFileHashes(directory, prefix = "") {
  return readdirSync(directory, { withFileTypes: true })
    .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))
    .flatMap((entry) => {
      const path = join(directory, entry.name);
      const relative = prefix + entry.name;
      if (entry.isDirectory()) return sourceFileHashes(path, relative + "/");
      if (!entry.isFile()) throw new Error(`Unexpected source entry: ${path}`);
      return [
        {
          path: relative,
          sha256: createHash("sha256").update(readFileSync(path)).digest("hex"),
        },
      ];
    });
}

export function verifyLicensedAssets({
  root = resolve("."),
  pdfjsRoot = join(root, "node_modules/pdfjs-dist"),
} = {}) {
  const manifest = JSON.parse(
    readFileSync(join(root, "licenses/pdfjs-assets.json"), "utf8"),
  );
  const { version } = JSON.parse(
    readFileSync(join(pdfjsRoot, "package.json"), "utf8"),
  );
  if (version !== manifest.pdfjs_version)
    throw new Error(
      `PDF.js ${version} has not been checked against the bundled licenses and font sources. Update licenses/pdfjs-assets.json first.`,
    );
  const verify = (base, item) => {
    const hash = createHash("sha256")
      .update(readFileSync(join(base, item.path)))
      .digest("hex");
    if (hash !== item.sha256)
      throw new Error(
        `Licensed asset or source checksum changed: ${item.path}`,
      );
  };
  const covered = new Set();
  for (const component of manifest.components) {
    for (const file of component.files) {
      verify(pdfjsRoot, file);
      covered.add(file.path);
    }
    for (const item of [
      ...component.license_files,
      ...(component.sources || []),
      ...(component.provenance || []),
    ])
      verify(root, item);
  }
  for (const [folder, pattern] of [
    ["standard_fonts", /^Liberation.*\.ttf$/],
    ["wasm", /^quickjs/],
  ]) {
    for (const name of readdirSync(join(pdfjsRoot, folder))) {
      if (pattern.test(name) && !covered.has(`${folder}/${name}`))
        throw new Error(`Unreviewed licensed PDF.js asset: ${folder}/${name}`);
    }
  }
  return manifest;
}

export function verifyLicenseBundle(directory, root = resolve(".")) {
  for (const [original, bundled] of [
    ["LICENSE", "PROJECT-LICENSE.txt"],
    ["NOTICE", "PROJECT-NOTICE.txt"],
  ]) {
    if (
      !readFileSync(join(directory, bundled)).equals(
        readFileSync(join(root, original)),
      )
    )
      throw new Error(`Missing full project license or notice: ${bundled}`);
  }
  const manifest = JSON.parse(
    readFileSync(join(root, "licenses/pdfjs-assets.json")),
  );
  const bundledManifest = JSON.parse(
    readFileSync(join(directory, "pdfjs-assets.json")),
  );
  if (JSON.stringify(manifest) !== JSON.stringify(bundledManifest))
    throw new Error(
      "Bundled asset license manifest differs from the reviewed manifest.",
    );
  const notices = readFileSync(join(directory, "NOTICES.txt"), "utf8");
  const instructions = readFileSync(join(directory, "SOURCE-CODE.md"));
  if (!instructions.equals(readFileSync(join(root, "licenses/SOURCE-CODE.md"))))
    throw new Error("Bundled source instructions are incomplete or stale.");
  for (const component of manifest.components) {
    for (const file of component.license_files) {
      if (!notices.includes(readFileSync(join(root, file.path), "utf8")))
        throw new Error(
          `Missing full copyright and license notice: ${file.path}`,
        );
    }
    for (const source of component.sources || []) {
      const path = "font-sources/" + source.path.split("/").at(-1);
      const hash = createHash("sha256")
        .update(readFileSync(join(directory, path)))
        .digest("hex");
      if (hash !== source.sha256)
        throw new Error(`Bundled font source checksum changed: ${path}`);
    }
  }
  const inventory = JSON.parse(
    readFileSync(join(directory, "dependencies.json")),
  );
  const mplLicense = readFileSync(join(root, "licenses/MPL-2.0.txt"));
  if (
    !readFileSync(join(directory, "mpl-sources/LICENSE.txt")).equals(
      mplLicense,
    ) ||
    !notices.includes(mplLicense.toString("utf8"))
  )
    throw new Error("Missing full MPL-2.0 license.");
  for (const component of manifest.components) {
    if (
      !inventory.dependencies.some(
        (item) =>
          item.ecosystem === "asset" &&
          item.name === component.name &&
          item.license === component.license,
      )
    )
      throw new Error(
        `Missing separately licensed asset in inventory: ${component.name}`,
      );
  }
  for (const item of inventory.dependencies) {
    if (item.ecosystem === "cargo" && item.license?.includes("MPL-2.0")) {
      const source = join(
        directory,
        "mpl-sources",
        `${item.name}-${item.version}`,
      );
      readFileSync(join(source, "Cargo.toml"));
      if (
        !item.source_files?.length ||
        JSON.stringify(sourceFileHashes(source)) !==
          JSON.stringify(item.source_files)
      )
        throw new Error(`Incomplete or changed MPL sources: ${item.name}`);
    }
  }
  return {
    components: manifest.components.length,
    sources: manifest.components.flatMap((component) => component.sources || [])
      .length,
  };
}
