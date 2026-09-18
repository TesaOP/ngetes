# Audit dependency renderer lama (Fase 1)

> Tanggal: 2026-09-18 — branch `migration/pixi8-cubism` (commit 01191f8).
> Hasil grep rekursif (rg, exclude `node_modules`, `static/js/bundle.js`/`i18n.js`).
> Hanya catat — tidak ada kode yang diubah di fase ini.

## Ringkasan

- `pixi-live2d-display@0.4.0` (vendored `static/js/pixi-live2d-0.4.0.js`, termasuk `patchCore6Compat`) adalah satu-satunya jembatan Live2D. `pixi.6.5.10.min.js` adalah runtime Pixi yang dipakai (tidak ada `pixi.js` di `package.json` — client pakai script tag).
- Titik masuk model: `static/js/app.js:542` `PIXI.live2d.Live2DModel.from(settings||modelPath, {autoInteract:false})`.
- Init Pixi: `static/js/app.js:310` `new PIXI.Application({view: $("#live2d-canvas"), … backgroundAlpha:0})` — style Pixi 6 (sinkron, opsi di konstruktor, `view`).
- Core WASM: `static/js/live2dcubismcore.min.js` dimuat via `<script>` sebelum bridge (`static/index.html:12-13`), dipakai oleh shim `static/js/app.js:10` `core.Moc.fromArrayBuffer` (fail-loud v6+).
- Fallback model tanpa manifest (`.moc3` saja): `src/server/rescue.ts:139` (+ `src/server/index.ts:44`, `1469`, `1679` mime & auto-rescue).
- Permukaan yang harus diputus di Fase 4 (adapter): semua `PIXI.live2d.*` → `src/live2d/*`.

## Daftar file + baris (lengkap, tanpa elide)

### PI XI.live2d / Live2DModel / pixi-live2d
| File | Baris | Fragmen |
|---|---|---|
| `static/js/app.js` | 542 | `state.model = await PIXI.live2d.Live2DModel.from(settings \|\| modelPath, {` |
| `static/js/app.js` | 1628 | `// Kalau peringatan ini muncul di console, pixi-live2d kemungkinan besar` |
| `static/index.html` | 12 | `<!-- Cubism Core HARUS dimuat duluan sebelum pixi-live2d -->` |
| `static/index.html` | 960 | `<!-- Scripts: PixiJS v6 (required by pixi-live2d-display@0.4.0) THEN Live2D bridge -->` |
| `static/index.html` | 962 | `<script src="js/pixi-live2d-0.4.0.js?v=3"></script>` |
| `static/js/motion-editor.js` | 14 | `* sekali saja tidak cukup karena internalModel.update() milik pixi-live2d` |
| `static/js/pixi-live2d-0.4.0.js` | 6 | `;(function patchCore6Compat() {` (vendored, ada 4 patch: renderOrders/opacity/blend/premultiply) |
| `static/js/pixi.6.5.10.min.js` | — | vendor script, dipakai sebagai slot Pixi 6 (tidak ada import ESM) |
| `test/legacy/test-core6-compat.js` | 3 | `* framework vendored era core 4 (pixi-live2d-0.4.0.js).` |
| `test/legacy/test-core6-compat.js` | 30,111 | `fs.readFileSync('static/js/pixi-live2d-0.4.0.js'…)` (guard baca sumber vendored) |
| `test/legacy/test-multiply-color.js` | 8,15,35 | guard untuk patch framework yang sama |
| `src/client/engine/native-expressions.ts` | 1 | `/** Koleksi nama ekspresi native dari seluruh surface pixi-live2d yang dikenal. */` |
| `src/server/rescue.ts` | 201 | `// (pixi-live2d memutar grup "Idle" otomatis — sama seperti maksud` |

### PIXI.Application / canvas (Pixi 6 style — akan jadi Pixi 8 async di Fase 3)
| File | Baris | Fragmen |
|---|---|---|
| `static/js/app.js` | 310 | `const app = new PIXI.Application({` |
| `static/js/app.js` | 311 | `view: $("#live2d-canvas"),` |
| `static/js/app.js` | 315 | `// dibutuhkan blend Atop/Out moc3 v6 (patchCore6Compat membaca` |

### Core WASM + MOC (hack versi — hapus di Fase 7)
| File | Baris | Fragmen |
|---|---|---|
| `static/js/app.js` | 10 | `if (!core \|\| !core.Moc \|\| !core.Moc.fromArrayBuffer) return;` |
| `static/js/app.js` | 11 | `const orig = core.Moc.fromArrayBuffer.bind(core.Moc);` |
| `static/js/app.js` | 12 | `core.Moc.fromArrayBuffer = function (buf) {` |
| `static/index.html` | 13 | `<script src="js/live2dcubismcore.min.js"></script>` |
| `src/server/rescue.ts` | 139 | `if (low.endsWith(".moc3")) {` |
| `src/server/index.ts` | 44 | `".moc3":"application/octet-stream",` |

### Window bridge (dipakai adapter Fase 4 — tetap, tapi sumbernya ganti)
| Pola | File |
|---|---|
| `window.__live2dAgent` | `static/js/app.js:6792`, `src/client/agent/brain.ts:92`, `src/client/agent/panel/*`, `src/client/window-contract.ts:32` |
| `window.__agent` / `__agentPanel` | `src/client/agent/*`, `src/build.ts` (bundle) |

## Yang TIDAK ada (diverifikasi)
- Tidak ada `import { Live2DModel } from 'pixi-live2d-display'` ESM di `src/` — client lama pakai global `PIXI.live2d` via script tag.
- Tidak ada `pixi.js` / `@pixi/*` di `package.json` — cocok dengan strategi Pixi 8 (akan `bun add pixi.js@8.20.1` di Fase 3 di repo, bukan temp).
- Tidak ada import Cubism SDK (`CubismFramework`, `CubismMoc`) di repo — semua Live2D lewat bridge vendored.

## Dampak ke fase berikutnya
- Fase 3: ganti `<script src="js/pixi.6.5.10.min.js">` → `npm pixi.js@8.20.1` + `await app.init()` (async), `view→canvas`, cek `backgroundAlpha:0` tetap (diperlukan Atop/Out).
- Fase 4: putus `app.js:542` dari `PIXI.live2d` → `src/live2d/Live2DModel.ts` (facade).
- Fase 7: hapus shim byte-hack `app.js:10-...` — diganti `CubismMoc.create(buffer, version-native)`.
