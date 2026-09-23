// test-overlay-gate.ts — gate overlay-vs-native: overlay TIDAK boleh dobel
// menggambar efek yang rig-nya hidup.
//
// WHY THIS EXISTS
// emotion-overlay.js menggambar efek app-level untuk ekspresi yang rig-nya
// TIDAK mengikat art (0 piksel via kalibrasi). Setelah fix shim stamp moc
// v5→v4 (try-genuine-first), keyform BlendShape rig v5 hidup — exp_heart
// dsb. menggambar efeknya sendiri, dan overlay menambahkan efek LAGI
// (dobel). Gate memakai dua data terukur: bindings param dari ISI file
// .exp3 (bukan tebakan nama) × hasil kalibrasi visual per param. Menekan
// overlay hanya boleh terjadi dengan bukti `changed > 0`; tanpa data →
// fail-open (overlay jalan seperti dulu — kegagalan paling parah adalah
// dobel seperti sebelum fix, bukan efek yang hilang).
//
// KONTRAK yang di-guard:
//   1. CLIENT (vm-extract overlayGateSuppress dari app.js): keputusan murni
//      fail-open pada semua keadaan tanpa bukti (tanpa kalibrasi, tanpa
//      bindings, alias emosi universal), menekan HANYA saat minimal satu
//      param bindings terukur hidup; prefix preset 'user:' ditangani;
//      model-agnostic (nama/id arbitrer m_001 tak meledak).
//   2. Wiring level sumber (app.js): fireOverlay memanggil gate sebelum
//      onExpression, loadModel prefetch bindings, client membaca `params`
//      dari endpoint, cache bindings dibuang saat model ganti.
//
// Bagian SERVER (kontrak `params` per ekspresi — dulu in-process lewat
// handleAPI) DIHAPUS bersama arsip Bun `src/server/` (Batch A 2026-09-23);
// fixture-nya kini dijaga identik oleh test Rust:
// core/src/expressions.rs :: params_edge_cases — `cargo test -p
// live2d-core expressions`.
//
// Run: bun test/legacy/test-overlay-gate.ts
import * as fs from "fs";
import * as path from "path";
import * as vm from "vm";

const ROOT = path.join(import.meta.dir, "..", "..");
const appSrc = fs.readFileSync(path.join(ROOT, "static", "js", "app.js"), "utf8");

let pass = 0, fail = 0;
function ok(name: string, cond: boolean, detail?: string) {
  if (cond) { pass++; console.log(`  PASS  ${name}${detail ? '  -> ' + detail : ''}`); }
  else { fail++; console.log(`  FAIL  ${name}${detail ? '  -> ' + detail : ''}`); }
}
function section(t: string) { console.log(`\n${t}`); }

function extractFn(src: string, name: string): string | null {
  const start = src.indexOf('function ' + name + '(');
  if (start === -1) return null;
  let depth = 0, i = src.indexOf('{', start);
  for (; i < src.length; i++) {
    if (src[i] === '{') depth++;
    else if (src[i] === '}') { depth--; if (depth === 0) return src.slice(start, i + 1); }
  }
  return null;
}

// ═════════════════ PART 1 — keputusan murni di app.js (vm) ═══════════════════
function clientTests() {
  section('CLIENT  overlayGateSuppress (vm-extract)');
  const src = extractFn(appSrc, 'overlayGateSuppress');
  ok('fungsi gate ditemukan di app.js', !!src);
  if (!src) return;

  const sandbox: any = {};
  vm.createContext(sandbox);
  vm.runInContext(src + '\nthis.__f = overlayGateSuppress;', sandbox);
  const gate: (n: string, b: any, v: any, r: any) => boolean = sandbox.__f;

  const visfxAlive = { ParamEX04: { changed: 512, maxDelta: 255, at: 1 } };
  const visfxDead = { ParamEX04: { changed: 0, maxDelta: 0, at: 1 } };
  const resolveHeart = (n: string) => (n && /heart/i.test(String(n))) ? { key: 'heart' } : null;
  const bind = { exp_heart: ['ParamEX04'] };

  ok('binding alive → ditekan (jangan dobel-gambar)', gate('exp_heart', bind, visfxAlive, resolveHeart) === true);
  ok("prefix preset 'user:exp_heart' → tetap ditekan", gate('user:exp_heart', bind, visfxAlive, resolveHeart) === true);
  ok('binding terukur mati → overlay jalan (kompensasi)', gate('exp_heart', bind, visfxDead, resolveHeart) === false);
  ok('binding ada tapi belum dikalibrasi → fail-open', gate('exp_heart', bind, null, resolveHeart) === false);
  ok('param belum diukur (tanpa entri visfx) → fail-open',
    gate('exp_heart', bind, { ParamLain: { changed: 999, maxDelta: 9, at: 1 } }, resolveHeart) === false);
  ok('tanpa entri bindings (alias emosi universal, mis. sedih) → jalan',
    gate('sedih', bind, visfxAlive, (n: string) => (n === 'sedih' ? { key: 'tear' } : null)) === false);
  ok('resolveFx null / bukan fungsi → jalan',
    gate('exp_heart', bind, visfxAlive, null) === false && gate('exp_heart', bind, visfxAlive, 42 as any) === false);
  ok('nama yang tak memetakan efek overlay → jalan',
    gate('collar_blue', bind, visfxAlive, resolveHeart) === false);
  ok('bindings [] / null → jalan',
    gate('exp_heart', { exp_heart: [] }, visfxAlive, resolveHeart) === false &&
    gate('exp_heart', null, visfxAlive, resolveHeart) === false);
  ok('campuran: satu param hidup di antara yang mati → ditekan',
    gate('exp_heart', { exp_heart: ['ParamEX04', 'ParamEX08', 'ParamEX11'] },
      { ParamEX04: { changed: 0, maxDelta: 0, at: 1 }, ParamEX08: { changed: 77, maxDelta: 77, at: 1 } }, resolveHeart) === true);

  section('CLIENT  model-agnostic — nama & id arbitrer');
  const opaque = { m_001: ['m_01', 'm_02'] };
  const resolveOpaque = (n: string) => (n === 'm_001' ? { key: 'sparkle' } : null);
  ok('rig opaque hidup → ditekan (tanpa asumsi kosakata)',
    gate('m_001', opaque, { m_01: { changed: 3, maxDelta: 3, at: 1 } }, resolveOpaque) === true);
  ok('rig opaque mati → jalan',
    gate('m_001', opaque, { m_01: { changed: 0, maxDelta: 0, at: 1 }, m_02: { changed: 0, maxDelta: 0, at: 1 } }, resolveOpaque) === false);
  ok('input aneh (null/nomor) tidak meledak',
    gate(null as any, opaque, visfxAlive, resolveHeart) === false &&
    gate(7 as any, opaque, visfxAlive, resolveHeart) === false);

  // id bernomor DI SINI sah: ini ISI deklarasi rigger yang dibaca, bukan tabel
  // universal yang menebak makna — guard model-agnostic melarang TABEL id
  // bernomor, bukan melaporkan apa yang rigger tulis.
}

// ═════════════════ PART 2 — wiring level sumber ══════════════════════════════
function wiringTests() {
  section('WIRING  app.js level sumber');
  ok('fireOverlay memanggil gate SEBELUM onExpression',
    /overlayShouldSuppress\(name\)[\s\S]{0,200}__emotionOverlay && window\.__emotionOverlay\.onExpression\(name\)/.test(appSrc));
  ok('loadModel prefetch bindings gate',
    /detectModelCapabilities\(\);[\s\S]{0,120}prefetchOverlayGate\(\)/.test(appSrc));
  ok('gate membaca state.visfxMap (kalibrasi) & bindings cache',
    /overlayGateSuppress\(\s*\n\s*name,\s*\n\s*overlayGateExpBindingsSync\(\),\s*\n\s*state\.visfxMap/.test(appSrc));
  ok('client membaca field `params` dari endpoint',
    appSrc.includes('Array.isArray(e.params)'));
  ok('cache bindings dibuang saat model ganti (guard modelPath)',
    /overlayGateModelPath !== \(state\.modelPath \|\| ''\)/.test(appSrc));
  ok('server (Rust core) membaca isi file .exp3 untuk params',
    /read_to_string\(&full\)[\s\S]{0,200}"Parameters"/.test(fs.readFileSync(path.join(ROOT, 'core', 'src', 'expressions.rs'), 'utf8')));
}

clientTests();
wiringTests();
console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
