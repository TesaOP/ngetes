/**
 * build-proof.ts — bundle entry proof Live2D → static/js/live2d-render.mjs.
 *
 * live2d-render.mjs adalah artefak build yang DI-COMMIT supaya halaman proof
 * (live2d-render-proof.html dst.) bisa dimuat tanpa step build lokal — tapi
 * artefak tanpa skrip build akan drift diam-diam dari src/live2d. Skrip ini
 * mengunci cara regenerasinya: ESM, pixi.js di-external (diresolve lewat
 * importmap proof pages ke ./js/pixi8.mjs), tanpa minify.
 */
const result = await Bun.build({
  entrypoints: ["./src/live2d/render-proof-entry.ts"],
  outdir: "./static/js",
  // nama literal ".mjs": proof pages memuatnya sebagai ESM module file;
  // token [ext] akan menghasilkan .js — jangan dipakai di sini.
  naming: "live2d-render.mjs",
  target: "browser",
  format: "esm",
  splitting: false,
  minify: false,
  sourcemap: "none",
  external: ["pixi.js"],
});

if (!result.success) {
  console.error("build proof gagal:");
  for (const msg of result.logs) {
    console.error(msg);
  }
  process.exit(1);
}

console.log("✓ Proof bundle built → static/js/live2d-render.mjs");

export {};
