#!/usr/bin/env node
/* test-emotion-overlay.js — efek emosi app-level harus cocok & menyala.
 *
 * WHY THIS EXISTS
 * Sebagian efek rig hanya hidup di proyek Cubism Editor — di moc3 hasil
 * ekspor binding-nya tidak ada (lumine: 'heart eye','blush','tear',
 * 'Sparkling eye','sweat','dizzy'; kalibrasi mengukurnya 0 piksel, dan
 * identik di rig v4.2/hasil import/rig sumber v5.0, core 4.2 maupun 5.1).
 * emotion-overlay.js menggambar efeknya sendiri (Canvas 2D di atas panggung)
 * saat ekspresi yang cocok dipasang — jadi ekspresi tetap TERLIHAT.
 *
 * KONTRAK yang di-guard:
 *   1. resolveEmotionFx(): pemetaan nama → efek — kanonik (.exp3 rigger:
 *      exp_heart/exp_blush/…), alias Indonesia (malu/senang/sedih/kaget/
 *      pusing/…), prefix preset 'user:', dan JANGAN memicu untuk nama
 *      netral/properti ('normal','collar_blue','exp_zitome' — zitome adalah
 *      ganti bentuk mata yang rig-nya justru hidup).
 *   2. Wiring level sumber: modul dimuat index.html, app.js menyalakan
 *      overlay di TIGA jalur pemasangan ekspresi (native universal, .exp3,
 *      synthetic) dan memadamkan di resetEmotion, config 'overlay' dibaca.
 *   3. Tanpa Canvas/model, modul tetap aman dimuat & onExpression tak melempar.
 *   4. Renderer = Canvas 2D murni (getContext('2d') + fillText), TANPA Pixi 6:
 *      modul tak mereferensikan PIXI, index.html tak memuat pixi.6.5.10.
 *   5. Kontrak perilaku: dengan model + kanvas palsu, efek benar-benar
 *      spawn → update → render (fillText/ellipse) dan padam saat clear().
 *
 * Run: node test/legacy/test-emotion-overlay.js
 */
'use strict';

const fs = require('fs');
const path = require('path');
const vm = require('vm');

const ROOT = path.join(__dirname, '..', '..');
const modSrc = fs.readFileSync(path.join(ROOT, 'static', 'js', 'emotion-overlay.js'), 'utf8');
const appSrc = fs.readFileSync(path.join(ROOT, 'static', 'js', 'app.js'), 'utf8');
const htmlSrc = fs.readFileSync(path.join(ROOT, 'static', 'index.html'), 'utf8');
// Server = Rust core (arsip Bun src/server dihapus Batch A 2026-09-23):
// config overlay diteruskan ke client di core/src/config.rs.
const serverSrc = fs.readFileSync(path.join(ROOT, 'core', 'src', 'config.rs'), 'utf8');

let pass = 0, fail = 0;
function ok(name, cond, detail) {
  if (cond) { pass++; console.log(`  PASS  ${name}${detail ? '  -> ' + detail : ''}`); }
  else { fail++; console.log(`  FAIL  ${name}${detail ? '  -> ' + detail : ''}`); }
}
function section(t) { console.log(`\n${t}`); }

// Jalankan modul utuh dalam sandbox: tanpa model & tanpa DOM live — modul
// harus tetap aman dimuat (tak menyentuh DOM tanpa model) dan API-nya tersedia.
const sandbox = {
  window: {}, console,
  performance: { now: () => 0 },
  requestAnimationFrame: () => 0,
  cancelAnimationFrame: () => {},
  // Tanpa model: ensureCanvas() harus keluar lebih dulu, tak menyentuh document.
  document: { createElement: () => ({ getContext: () => null }) },
};
sandbox.window = sandbox;             // modul menulis window.__emotionOverlay
vm.createContext(sandbox);
let modOk = true;
try { vm.runInContext(modSrc, sandbox); } catch (e) { modOk = false; console.error(e.message); }

section('modul aman dimuat tanpa Canvas/model');
ok('modul dieksekusi tanpa throw', modOk);
ok('window.__emotionOverlay terpasang', !!sandbox.__emotionOverlay);
ok('onExpression tanpa model tidak melempar', (() => {
  try { sandbox.__emotionOverlay.onExpression('malu'); sandbox.__emotionOverlay.clear(); return true; }
  catch (e) { return false; }
})());

const resolve = (n) => sandbox.__emotionOverlay._resolve(n);

section('pemetaan nama → efek (kanonik .exp3 rigger)');
ok('exp_heart → heart', resolve('exp_heart') && resolve('exp_heart').key === 'heart');
ok('exp_blush → blush', resolve('exp_blush') && resolve('exp_blush').key === 'blush');
ok('exp_sparkling → sparkle', resolve('exp_sparkling') && resolve('exp_sparkling').key === 'sparkle');
ok('exp_tear → tear', resolve('exp_tear') && resolve('exp_tear').key === 'tear');
ok('exp_sweat → sweat', resolve('exp_sweat') && resolve('exp_sweat').key === 'sweat');
ok('exp_dizzy → dizzy', resolve('exp_dizzy') && resolve('exp_dizzy').key === 'dizzy');
ok('exp_angry → anger', resolve('exp_angry') && resolve('exp_angry').key === 'anger');
ok('exp_sad → tear', resolve('exp_sad') && resolve('exp_sad').key === 'tear');

section('pemetaan alias Indonesia + prefix preset');
ok('user:malu → blush (prefix preset ditangani)', resolve('user:malu') && resolve('user:malu').key === 'blush');
ok('malu → blush', resolve('malu') && resolve('malu').key === 'blush');
ok('senang → sparkle', resolve('senang') && resolve('senang').key === 'sparkle');
ok('sedih → tear', resolve('sedih') && resolve('sedih').key === 'tear');
ok('kaget → shock', resolve('kaget') && resolve('kaget').key === 'shock');
ok('marah → anger', resolve('marah') && resolve('marah').key === 'anger');
ok('bingung → dizzy', resolve('bingung') && resolve('bingung').key === 'dizzy');
ok('bersih-case: "User:Malu " (kapital+spasi) tetap cocok',
  resolve('User:Malu ') && resolve('User:Malu ').key === 'blush');

section('nama yang TIDAK boleh memicu overlay');
ok('normal → null', resolve('normal') === null);
ok('default → null', resolve('default') === null);
ok('collar_blue → null (properti warna, bukan efek wajah)', resolve('collar_blue') === null);
ok('exp_zitome → null (ganti bentuk mata — rig-nya hidup, jangan dobel)', resolve('exp_zitome') === null);
ok('string kosong/null/number → null',
  resolve('') === null && resolve(null) === null && resolve(42) === null);

section('wiring di app.js / index.html / server');
ok('modul dimuat di index.html setelah voice-input',
  htmlSrc.indexOf('voice-input.js') < htmlSrc.indexOf('emotion-overlay.js'));
ok('app.js menyalakan overlay di jalur native universal',
  /playEmotionClip\(name\);\s*\n\s*fireOverlay\(name\);/.test(appSrc));
ok('app.js menyalakan overlay di jalur .exp3 native — SEBELUM try, karena justru ekspresi tak terdaftar di model3.json yang membuat expression() melempar',
  /fireOverlay\(name\);\s*try \{\s*await state\.model\.expression\(nativeName\);/.test(appSrc));
ok('app.js menyalakan overlay di jalur synthetic (fallback terakhir)',
  /playEmotionClip\(name\); \/\/ body follows the face \(see native branch\)\s*\n\s*fireOverlay\(name\);/.test(appSrc));
ok('resetEmotion memadamkan overlay',
  /function resetEmotion\(\)[\s\S]{0,800}__emotionOverlay && window\.__emotionOverlay\.clear\(\)/.test(appSrc));
ok('ekspresi bawaan model dicocokkan case-insensitive dari modelExpressions (menang atas sintetis)',
  /const nativeName = \(state\.modelExpressions \|\| \[\]\)\.find\(/.test(appSrc));
ok('supportedEmotions TIDAK lagi diisi emosi sintetis — hardcode hanya fallback terakhir',
  !/state\.supportedEmotions = Object\.assign\(\{\}, state\.roleEmotions\)/.test(appSrc));
ok('inspectModel tidak menanam emosi sintetis ke sheet baru',
  !/const supportedEmotions = buildRoleEmotions\(\)/.test(appSrc));
ok('nama tak dikenal (mis. exp_heart) tetap memicu overlay setelah blok fallback sintetis',
  /const synth = state\.roleEmotions && state\.roleEmotions\[name\];[\s\S]*?\r?\n    fireOverlay\(name\);\r?\n  \}/.test(appSrc));
ok('config.json "overlay" diteruskan server ke client',
  /"overlay":\s*sect\("overlay"\)/.test(serverSrc));
ok('app.js membaca config overlay',
  appSrc.includes('if (d.overlay) window.__overlayCfg'));

section('renderer Canvas 2D & tanpa Pixi 6');
ok('modul TIDAK mereferensikan global PIXI', !/PIXI/.test(modSrc));
ok('modul memakai Canvas 2D (getContext 2d)', /getContext\(\s*['"]2d['"]\s*\)/.test(modSrc));
ok('modul menggambar teks/emoji via fillText', /\.fillText\(/.test(modSrc));
ok('modul menggambar blush via ellipse (path vektor)', /\.ellipse\(/.test(modSrc));
ok('index.html tidak lagi memuat pixi.6.5.10', htmlSrc.indexOf('pixi.6.5.10') === -1);
ok('index.html tetap memuat emotion-overlay.js', htmlSrc.indexOf('emotion-overlay.js') !== -1);

section('kontrak perilaku (model + kanvas palsu): spawn → update → render');
(function behavioral() {
  const calls = { fillText: 0, ellipse: 0, clearRect: 0 };
  const ctxInst = {
    globalAlpha: 1, font: '', fillStyle: '', textAlign: '', textBaseline: '',
    save() {}, restore() {}, setTransform() {}, translate() {}, scale() {},
    beginPath() {}, fill() {},
    clearRect() { calls.clearRect++; },
    ellipse() { calls.ellipse++; },
    fillText() { calls.fillText++; },
  };
  const fakeCanvas = { width: 0, height: 0, style: {}, setAttribute() {}, getContext() { return ctxInst; } };
  const live = {
    clientWidth: 400, clientHeight: 600, offsetLeft: 0, offsetTop: 0,
    width: 800, height: 1200, style: {}, parentNode: { appendChild() {} },
  };
  const clock = { t: 0 };
  const sb = {
    console,
    performance: { now: () => clock.t },
    requestAnimationFrame: () => 1,
    cancelAnimationFrame: () => {},
    devicePixelRatio: 2,
    document: {
      createElement: (tag) => (tag === 'canvas' ? fakeCanvas : { style: {}, setAttribute() {} }),
      getElementById: (id) => (id === 'live2d-canvas' ? live : null),
    },
  };
  sb.window = sb;
  sb.window.__l2dDebug = { state: { model: { x: 50, y: 40, width: 200, height: 300 } } };
  vm.createContext(sb);
  vm.runInContext(modSrc, sb);
  const ov = sb.__emotionOverlay;

  clock.t = 0; ov.onExpression('heart');   // spawn 3 hati
  clock.t = 30; ov._tick(30);
  clock.t = 120; ov._tick(120);
  const st = ov._status();
  ok('efek aktif setelah onExpression("heart")', st.active === true && st.key === 'heart');
  ok('partikel ter-spawn (>0)', st.particles > 0, 'particles=' + st.particles);
  ok('kanvas ter-attach & fillText terpanggil (render emoji)', st.attached === true && calls.fillText > 0);

  ov.onExpression('malu');                 // ganti ke blush (vektor)
  clock.t = 140; ov._tick(140);
  ok('blush menggambar ellipse', calls.ellipse > 0);

  ov.clear();
  const sc = ov._status();
  ok('clear() memadamkan efek (active=false, particles=0)', sc.active === false && sc.particles === 0);
})();

console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
