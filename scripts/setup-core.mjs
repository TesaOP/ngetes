/**
 * setup-core.mjs — unduh Cubism Core for Web dari CDN resmi Live2D ke
 * static/js/live2dcubismcore.min.js.
 *
 * Core adalah kode proprietary Live2D ("Redistributable Code") — lisensinya
 * melarang Core dipublish berdiri sendiri di repo publik, jadi file ini
 * di-gitignore dan diunduh terpisah. Dengan menjalankan script ini kamu
 * dianggap sudah menyetujui Live2D Proprietary Software License Agreement.
 *
 * Jalur: bun run setup:core
 */
import { writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";

const OUT = "static/js/live2dcubismcore.min.js";
const SOURCES = [
  "https://cubism.live2d.com/sdk-web/cubismcore/live2dcubismcore.min.js",
];

if (existsSync(OUT)) {
  console.log("✓ Core sudah ada di", OUT, "— skip (hapus dulu bila ingin unduh ulang)");
  process.exit(0);
}

for (const url of SOURCES) {
  try {
    const res = await fetch(url);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const buf = new Uint8Array(await res.arrayBuffer());
    if (buf.length < 100_000) throw new Error(`isi terlalu kecil (${buf.length} B) — kemungkinan bukan core`);
    await writeFile(OUT, buf);
    console.log(`✓ Cubism Core terunduh (${(buf.length / 1024 / 1024).toFixed(2)} MB) →`, OUT);
    process.exit(0);
  } catch (e) {
    console.warn(`✗ gagal dari ${url}:`, e.message);
  }
}

console.error(`
Gagal mengunduh Cubism Core otomatis. Unduh manual:
  1. Buka https://www.live2d.com/sdk/download/web/ (setujui lisensinya)
  2. Ekstrak, lalu salin live2dcubismcore.min.js ke static/js/
Setelah itu jalankan ulang: bun run build`);
process.exit(1);
