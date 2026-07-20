import { defineConfig } from "tsdown";

// The published package keeps its historical layout (index.js + bin/hayate.js
// at the package root) — npm-release.mts copies these dist outputs into place.
// `.js` (not `.mjs`) extensions are forced to match that layout.
export default defineConfig([
  {
    entry: { index: "src/index.ts" },
    format: ["esm"],
    platform: "node",
    target: "node18",
    dts: true,
    clean: true,
    outDir: "dist",
    treeshake: true,
    minify: true,
    outExtensions: () => ({ js: ".js", dts: ".d.ts" }),
  },
  {
    // CLI shim: bundled (index.ts inlined) so bin.js is self-contained.
    entry: { "bin/hayate": "src/bin.ts" },
    format: ["esm"],
    platform: "node",
    target: "node18",
    outDir: "dist",
    treeshake: true,
    minify: true,
    outExtensions: () => ({ js: ".js" }),
  },
]);
