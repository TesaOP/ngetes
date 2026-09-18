/**
 * build.ts — Bundle the TypeScript client into static/js/bundle.js.
 *
 * The bundle is the SINGLE SOURCE OF TRUTH for the motion DSL / registry / runtime
 * (window.MotionDSL / MotionRegistry / MotionRuntime) and the agent brain
 * (window.__agent, installed inside brain.ts). It installs itself onto `window` as
 * side effects and starts no render loop — static/js/app.js owns the engine, model
 * loading, render loop and UI, and consumes those globals. This makes the rewrite's
 * TS logic actually execute in the browser instead of being dead code.
 */
import { Glob } from "bun";

const result = await Bun.build({
  entrypoints: ["./src/client/bundle-entry.ts"],
  outdir: "./static/js",
  naming: "bundle.[ext]",
  target: "browser",
  format: "iife",
  splitting: false,
  minify: false,
  sourcemap: "inline",
});

if (!result.success) {
  console.error("Build failed:");
  for (const msg of result.logs) {
    console.error(msg);
  }
  process.exit(1);
}

// Entry kedua: core i18n saja (kamus + t()) untuk static/pet.html yang tidak
// memuat bundle.js penuh. index.html tidak memuat file ini.
const i18n = await Bun.build({
  entrypoints: ["./src/client/i18n-entry.ts"],
  outdir: "./static/js",
  naming: "i18n.[ext]",
  target: "browser",
  format: "iife",
  splitting: false,
  minify: false,
  sourcemap: "inline",
});

if (!i18n.success) {
  console.error("i18n build failed:");
  for (const msg of i18n.logs) {
    console.error(msg);
  }
  process.exit(1);
}

// Entry ketiga: integrasi view Pixi 8 (Fase A panggung+chat). ESM karena
// harus import "pixi.js" via importmap index.html ke ./js/pixi8.mjs;
// di-serve sebagai module (MIME .mjs sudah text/javascript di server).
const view = await Bun.build({
  entrypoints: ["./src/live2d/view/view-entry.ts"],
  outdir: "./static/js",
  naming: "live2d-view.mjs",
  target: "browser",
  format: "esm",
  splitting: false,
  minify: false,
  sourcemap: "none",
  external: ["pixi.js"],
});

if (!view.success) {
  console.error("live2d-view build failed:");
  for (const msg of view.logs) {
    console.error(msg);
  }
  process.exit(1);
}

console.log("✓ Client bundle built → static/js/bundle.js + static/js/i18n.js + static/js/live2d-view.mjs (TS is now the live client source-of-truth)");
