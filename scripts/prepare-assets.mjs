import { cpSync, mkdirSync } from "node:fs";
import { resolve } from "node:path";
const dest = resolve("public/pdf-assets");
mkdirSync(dest, { recursive: true });
for (const name of ["cmaps", "standard_fonts", "wasm"]) {
  cpSync(resolve("node_modules/pdfjs-dist", name), resolve(dest, name), {
    recursive: true,
  });
}
