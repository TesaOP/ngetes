// test-exp3-adoption.ts — adopsi `.exp3` tak terdaftar, bagian CLIENT.
//
// Bagian SERVER (discovery via dispatcher in-process + fetchability static)
// sudah DIHAPUS bersama arsip Bun `src/server/` (Batch A 2026-09-23) dan
// kontraknya kini dijaga test Rust: core/src/expressions.rs
// (nested/bom/cjk/file_relatif, params_edge, traversal_readonly) — jalankan
// `cargo test -p live2d-core expressions`. Yang tersisa di sini: bagian
// CLIENT murni (vm-extract buildModelSettings dari app.js ASLI) — fungsi
// yang jalan di browser dan tak punya padanan Rust.
//
// Jalankan: bun test/legacy/test-exp3-adoption.ts   (atau via run-guards)
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
  const start = src.indexOf('async function ' + name + '(') >= 0
    ? src.indexOf('async function ' + name + '(')
    : src.indexOf('function ' + name + '(');
  if (start === -1) return null;
  let depth = 0, i = src.indexOf('{', start);
  for (; i < src.length; i++) {
    if (src[i] === '{') depth++;
    else if (src[i] === '}') { depth--; if (depth === 0) return src.slice(start, i + 1); }
  }
  return null;
}

function model3(expressions: any[] | null) {
  const fr: any = { Moc: 'x.moc3', Textures: ['x.2048/texture_00.png'] };
  if (expressions) fr.Expressions = expressions;
  return { Version: 3, FileReferences: fr };
}

// ═══════════ PART — buildModelSettings(), body asli yang diekstrak ═══════════
async function clientTests() {
  section('CLIENT  buildModelSettings() merge logic');

  const fnSrc = extractFn(appSrc, 'buildModelSettings');
  ok('buildModelSettings() found in static/js/app.js', !!fnSrc);
  if (!fnSrc) return;
  const filterSrc = extractFn(appSrc, 'filterAdoptable');
  ok('filterAdoptable() found in static/js/app.js', !!filterSrc);
  const combined = (filterSrc ? filterSrc + '\n' : '') + fnSrc;

  async function run(manifest: any, discovery: any, modelPath = 'model/foo/sub/char.model3.json') {
    const logs: string[] = [];
    const sandbox: any = {
      API: 'http://127.0.0.1:9999',
      location: { href: 'http://127.0.0.1:9999/index.html' },
      URL,
      console: { log: (...a: any[]) => logs.push(a.join(' ')), warn: (...a: any[]) => logs.push('WARN ' + a.join(' ')) },
      fetch: async (url: any) => {
        if (String(url).includes('/api/model/expressions-adoption')) {
          return { ok: true, json: async () => ({ expressions: [], disabled: [] }) };
        }
        if (String(url).includes('/api/model/expressions')) {
          return { ok: discovery !== null, json: async () => discovery };
        }
        return { ok: manifest !== null, json: async () => manifest };
      },
      Promise, Array, Set, String, JSON, Object,
      result: undefined,
    };
    vm.createContext(sandbox);
    vm.runInContext(combined + `;result = buildModelSettings(${JSON.stringify(modelPath)});`, sandbox);
    return { out: await sandbox.result, logs };
  }

  const disc3 = {
    expressions: [
      { Name: 'joy', File: 'expr/joy.exp3.json', declared: false },
      { Name: 'rage', File: 'expr/rage.exp3.json', declared: false },
    ],
  };

  let r = await run(model3(null), disc3);
  ok('adopts orphans when manifest declares none', !!r.out);
  ok('adopted count is 2',
    r.out && r.out.FileReferences.Expressions.length === 2,
    r.out ? String(r.out.FileReferences.Expressions.length) : 'null');
  ok('entries carry Name + File only',
    r.out && r.out.FileReferences.Expressions.every((e: any) =>
      Object.keys(e).sort().join(',') === 'File,Name'));
  ok('settings.url set (loader needs it to resolve moc/textures)',
    r.out && typeof r.out.url === 'string' && r.out.url.endsWith('model/foo/sub/char.model3.json'),
    r.out ? r.out.url : '-');
  ok('other FileReferences untouched',
    r.out && r.out.FileReferences.Moc === 'x.moc3' && r.out.FileReferences.Textures.length === 1);

  r = await run(model3([{ Name: 'a', File: 'a.exp3.json' }]),
               { expressions: [{ Name: 'a', File: 'a.exp3.json', declared: true }] });
  ok('complete manifest → null (plain URL load, no interference)', r.out === null);

  r = await run(model3([{ Name: 'known', File: 'known.exp3.json' }]), {
    expressions: [
      { Name: 'known', File: 'known.exp3.json', declared: true },
      { Name: 'newone', File: 'newone.exp3.json', declared: false },
    ],
  });
  ok('partial: result has 2 entries', r.out && r.out.FileReferences.Expressions.length === 2);
  ok('partial: original declaration kept first (rigger order preserved)',
    r.out && r.out.FileReferences.Expressions[0].Name === 'known');
  ok('partial: only the undeclared entry appended',
    r.out && r.out.FileReferences.Expressions[1].Name === 'newone');

  r = await run(model3([{ Name: 'joy', File: 'other/joy.exp3.json' }]), {
    expressions: [{ Name: 'joy', File: 'expr/joy.exp3.json', declared: false }],
  });
  ok('duplicate Name never appended (would shadow via findIndex)', r.out === null,
    r.out ? JSON.stringify(r.out.FileReferences.Expressions) : 'null');

  r = await run(model3([{ Name: 'alias', File: 'expr/joy.exp3.json' }]), {
    expressions: [{ Name: 'joy', File: 'expr/joy.exp3.json', declared: false }],
  });
  ok('duplicate File skipped even under a new Name', r.out === null);

  section('CLIENT  failure modes must never block model loading');
  r = await run(null, disc3);
  ok('manifest fetch fails → null (fallback to URL load)', r.out === null);
  r = await run(model3(null), null);
  ok('discovery fetch fails → null', r.out === null);
  r = await run({ Version: 3 }, disc3);
  ok('manifest without FileReferences → null', r.out === null);
  r = await run(model3(null), { expressions: [] });
  ok('server reports no .exp3 at all → null', r.out === null);
  r = await run(model3(null), {});
  ok('malformed discovery payload → null', r.out === null);
  r = await run(model3(null), disc3, 'model/only-two-parts.json');
  ok('unexpected modelPath shape → null', r.out === null);
  r = await run(model3(null), disc3, 'sheets/notamodel.json');
  ok('path outside model/ → null', r.out === null);

  section('CLIENT  model-agnostic guarantees');
  r = await run(model3(null), {
    expressions: [
      { Name: '呆猫', File: '呆猫.exp3.json', declared: false },
      { Name: '01', File: 'numbered/01.exp3.json', declared: false },
      { Name: 'exp_angry', File: 'mothion/exp_angry.exp3.json', declared: false },
    ],
  });
  const names: string[] = r.out ? r.out.FileReferences.Expressions.map((e: any) => e.Name) : [];
  ok('CJK / numeric / snake_case names all preserved verbatim',
    names.join(',') === '呆猫,01,exp_angry', names.join(','));

  const banned = [/['"]lumine['"]/i, /神宫白子/, /exp_angry/, /['"]mothion['"]/i, /呆猫/];
  const hits = banned.filter(re => re.test(fnSrc));
  ok('buildModelSettings() hardcodes no model/expression/folder name',
    hits.length === 0, hits.length ? hits.map(String).join(' ') : 'clean');
}

clientTests().then(() => {
  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail ? 1 : 0);
}).catch((e: any) => {
  console.log('  FAIL  harness: ' + (e && e.message));
  console.log(`\n${pass} passed, ${fail + 1} failed`);
  process.exit(1);
});
