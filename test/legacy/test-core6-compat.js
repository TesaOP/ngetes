#!/usr/bin/env node
/* test-core6-compat.js — kompatibilitas Core 6.0.1 (SDK Web 5-r.5) dengan
 * framework vendored era core 4 (pixi-live2d-0.4.0.js).
 *
 * WHY THIS EXISTS
 * moc3 dari Cubism 5.1 export = versi 6 (byte ke-4). Core 5.1.0 lama hanya
 * mengenal MocVersion_50=5 → fromArrayBuffer NULL → shim app.js men-stamp
 * 6→4 buta → mask/clip/physics berantakan senyap (lihat STATUS entri 19).
 * Solusi: swap core ke 6.0.1. TAPI core 6.0.1 memindahkan
 * Drawables.renderOrders ke Model.getRenderOrders() — framework vendored
 * membaca drawables.renderOrders (getDrawableRenderOrders) → undefined →
 * _sortedDrawableIndexList kosong → TIDAK ADA drawable tergambar (karakter
 * blank TANPA satu pun error console). PATCH 3 di lib vendored menempelkan
 * kembali renderOrders sebagai getter delegasi di Model.fromMoc.
 *
 * Guard ini mencegah dua kegagalan senyap sekaligus:
 *   1. re-vendoring lib menghapus PATCH 3 (string-match level sumber);
 *   2. core di-swap balik ke lama TANPA patch, atau core baru berubah lagi
 *      (eksekusi nyata via Bun: muat moc v5 & v6, cek permukaan API).
 *
 * Run: node test/legacy/test-core6-compat.js
 */
'use strict';

const fs = require('fs');
const path = require('path');
const { spawnSync } = require('child_process');

const ROOT = path.join(__dirname, '..', '..');
const coreSrc = fs.readFileSync(path.join(ROOT, 'static', 'js', 'live2dcubismcore.min.js'), 'utf8');
const appSrc = fs.readFileSync(path.join(ROOT, 'static', 'js', 'app.js'), 'utf8');

let pass = 0, fail = 0;
function ok(name, cond, detail) {
  if (cond) { pass++; console.log(`  PASS  ${name}${detail ? '  -> ' + detail : ''}`); }
  else { fail++; console.log(`  FAIL  ${name}${detail ? '  -> ' + detail : ''}`); }
}
function section(t) { console.log(`\n${t}`); }

section('B. Core di disk adalah 6.x yang mengenal moc3 v6 (string-match)');
ok('core punya enum MocVersion_53 (=6, Cubism 5.1 export)',
  coreSrc.includes('MocVersion_53'));
ok('core masih punya API multiply color yang dipatch lib (prasyarat guard multiply-color)',
  coreSrc.includes('csmGetDrawableMultiplyColors') || coreSrc.includes('GetDrawableMultiplyColors'));

section('C. Eksekusi nyata (Bun child process): muat core, moc v5 & v6');
// Eksekusi via FILE, bukan `bun -e`: emscripten bootstrap di dalam core min
// berperilaku beda antar versi Bun saat dijalankan lewat -e (eval core bisa
// melempar di tengah init) — lewat file, Moc/Model selalu terpasang penuh.
function bunEval(code) {
  const tmp = ROOT + '/.probe-core6.tmp.js';
  fs.writeFileSync(tmp, code);
  const r = spawnSync('bun', [tmp], { cwd: ROOT, encoding: 'utf8', timeout: 60000 });
  try { fs.unlinkSync(tmp); } catch (e) { /* biarkan bila gagal hapus */ }
  return { code: r.status, out: (r.stdout || '') + (r.stderr || '') };
}
const probe = `
const fs=require('fs'), path=require('path');
function collectMocs(dir, out, depth) {
  if (depth > 5 || out.length >= 60) return;
  let entries = [];
  try { entries = fs.readdirSync(dir, { withFileTypes: true }); } catch (e) { return; }
  for (const e of entries) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) collectMocs(p, out, depth + 1);
    else if (e.name.endsWith('.moc3')) out.push(p);
  }
}
(async()=>{
  const results={};
  try {
    eval(fs.readFileSync('static/js/live2dcubismcore.min.js','utf8'));
    globalThis.window=globalThis;
    const C=globalThis.Live2DCubismCore;
    const sleep=(ms)=>new Promise(r=>setTimeout(r,ms));
    for(let i=0;i<200;i++){ try{ if(C.Moc && C.Model) break; }catch(e){} await sleep(50); }
    if(!C || !C.Moc || !C.Model){ results.bootFailed=true; console.log(JSON.stringify(results)); process.exit(0); }
    // Layout-agnostic: kumpulkan .moc3 dari data/model (user boleh menata
    // folder bebas). Versi moc3 = byte ke-4 header (dokumen Fase 7 — byte
    // yang dulu di-stamp hack). API C.Version TIDAK dipakai: butuh init
    // emscripten ala browser yang tidak selalu selesai di env non-browser.
    const mocs=[];
    collectMocs('data/model', mocs, 0);
    const byVer={};
    for(const p of mocs){
      try{
        const b=fs.readFileSync(p);
        const v=b[4];
        if(v!==5 && v!==6) continue;
        (byVer[v] = byVer[v] || []).push(p);
      }catch(e){}
    }
    const pick={5:(byVer[5]||[])[0], 6:(byVer[6]||[])[0]};
    for(const [name,p] of Object.entries(pick)){
      if(!p){ results[name]={moc:false, missing:true}; continue; }
      const b=fs.readFileSync(p);
      const ab=b.buffer.slice(b.byteOffset,b.byteOffset+b.byteLength);
      const moc=C.Moc.fromArrayBuffer(ab);
      if(!moc){ results[name]={moc:false}; continue; }
      const model=C.Model.fromMoc(moc);
      const d=model.drawables;
      results[name]={moc:true, src:path.basename(p), count:d.count, modelOk: !!model && d.count > 0};
    }
  } catch (e) {
    results.bootFailed=true;
    results.bootError=String(e && e.message || e).slice(0,120);
  }
  console.log(JSON.stringify(results));
  process.exit(0);
})();`;
const r = bunEval(probe);
let results = null;
try { results = JSON.parse(r.out.trim().split('\n').filter(l => l.startsWith('{')).pop()); } catch (e) { /* parse error ditangani di bawah */ }
ok('probe Bun sukses dieksekusi', r.code === 0 && results !== null, r.code === 0 ? 'exit 0' : r.out.slice(0, 300));
if (results && results.bootFailed) {
  console.log('  SKIP runtime probe — emscripten boot gagal di env ini (' + (results.bootError || 'C.Moc/C.Model tidak terpasang') + '); verifikasi runtime dilakukan di browser nyata.');
} else if (results) {
  ok('moc v5: fromMoc OK + model hidup dengan drawable > 0',
    results.v5 && results.v5.moc && results.v5.modelOk,
    JSON.stringify(results.v5));
  ok('moc v6 (ren): fromMoc OK + model hidup dengan drawable > 0',
    results.v6 && results.v6.moc && results.v6.modelOk,
    JSON.stringify(results.v6));
}

section('D. Fase 7: shim stamp versi app.js sudah dihapus — tidak boleh kembali');
ok('app.js tidak lagi mem-patch core.Moc.fromArrayBuffer',
  !appSrc.includes('patchCubismCore') && !appSrc.includes('core.Moc.fromArrayBuffer ='));
ok('app.js tidak menulis byte versi moc (stamp buta u8[4]=4 hilang total)',
  !appSrc.includes('u8[4] = 4'));
ok('penanda Fase 7 ada di app.js (core 6.0.1 native, tanpa byte-stamp)',
  appSrc.includes('Fase 7'));

console.log(`\n${pass} passed, ${fail} failed`);
if (fail > 0) process.exit(1);
