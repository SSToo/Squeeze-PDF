import { cpSync, mkdirSync, rmSync } from "node:fs";
import { resolve } from "node:path";
import { verifyLicensedAssets } from "./license-assets.mjs";
verifyLicensedAssets();
const dest = resolve("public/pdf-assets");
rmSync(dest, { recursive: true, force: true });
mkdirSync(dest, { recursive: true });
for (const name of ["cmaps", "standard_fonts", "wasm"]) {
  cpSync(resolve("node_modules/pdfjs-dist", name), resolve(dest, name), {
    recursive: true,
  });
}
