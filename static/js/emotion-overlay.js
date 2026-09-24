/* emotion-overlay.js — efek emosi app-level (overlay visual, Canvas 2D) ────
 *
 * KENAPA ADA
 * Sebagian efek rig hanya hidup di proyek Cubism Editor — di moc3 hasil
 * ekspor binding-nya tidak ada. Contoh terukur lumine: 'heart eye',
 * 'blush', 'tear', 'Sparkling eye', 'sweat', 'dizzy' (label cdi3 rigger
 * mengonfirmasi semuanya efek overlay; kalibrasi efek mengukurnya
 * 0 piksel — identik di rig v4.2, hasil import web, maupun salinan
 * langsung rig sumber v5.0, dengan core 4.2 maupun resmi 5.1). Modul
 * ini menggambar efeknya sendiri di kanvas 2D terpisah di atas panggung
 * saat ekspresi/emosi yang cocok dipasang, sehingga ekspresi tetap
 * TERLIHAT tanpa memodifikasi rig.
 *
 * RENDERER
 * Canvas 2D murni (tanpa Pixi). Satu <canvas> overlay ditumpuk tepat di
 * atas #live2d-canvas (transparan, pointer-events:none); partikel emoji
 * digambar via fillText, blush via ellipse. Live2D sendiri tetap Pixi 8 —
 * overlay ini BUKAN bagian renderer model dan tak menyentuh parameter rig.
 *
 * MODEL-AGNOSTIC
 * Dicocokkan dari NAMA ekspresi/emosi (kanonik + alias Indonesia),
 * bukan id parameter; posisi mengikuti kepala model dari bounds panggung
 * (kepala ≈ 16% tinggi model) — bukan dari nama/range parameter tertentu.
 * Tidak ada satu pun id rig yang di-hardcode.
 *
 * API: window.__emotionOverlay
 *   .onExpression(name) — cocokkan nama & nyalakan efek (app.js memanggil
 *     ini di titik ekspresi dipasang; 'user:' prefix ditangani)
 *   .clear()            — hentikan semua efek (resetEmotion memanggil)
 *   ._status()          — debug: { active, key, particles, attached }
 *   ._tick(now)         — loop animasi; diekspos agar pengujian headless
 *                         bisa memompa manual (rAF hanya jalan saat aktif)
 *
 * KONFIGURASI: data/config.json → "overlay": { enabled, alpha, size }
 * (dibaca app.js loadAppConfig → window.__overlayCfg; tanpa key itu =
 * enabled, alpha 0.9, size 1).
 */
(function () {
  'use strict';
  if (window.__emotionOverlay) return;

  // ── Tabel efek ──────────────────────────────────────────────────
  // emoji: daftar karakter yang diputar untuk partikel (null = digambar
  // bentuk vektor). dur: lama efek aktif (ms). spawnEvery: interval spawn
  // partikel (0 = sekali di awal).
  var EFFECTS = {
    heart:   { emoji: ['💗', '💜', '💖', '💙'], dur: 3600, spawnEvery: 380 },
    blush:   { emoji: null,                     dur: 3200, spawnEvery: 0 },
    sparkle: { emoji: ['✨', '⭐', '🌟'],        dur: 3200, spawnEvery: 340 },
    tear:    { emoji: ['💧'],                    dur: 2800, spawnEvery: 640 },
    sweat:   { emoji: ['💦'],                    dur: 2200, spawnEvery: 0 },
    dizzy:   { emoji: ['💫', '🌀'],              dur: 3200, spawnEvery: 300 },
    anger:   { emoji: ['💢'],                    dur: 2000, spawnEvery: 0 },
    shock:   { emoji: ['❗'],                    dur: 1500, spawnEvery: 0 },
  };
  // ── Pemetaan nama → efek (kanonik + alias, pakai contains) ──────
  // Urutan = prioritas pencocokan. Nama datang dari banyak sumber:
  // emosi bawaan ('malu','senang',…), file .exp3 rigger ('exp_heart',…),
  // preset user ('user:malu'). Bukan match eksak — nama rig tiap model
  // berbeda, jadi contains pada alias adalah kontrak sengaja dipilih.
  var ALIASES = [
    ['heart',   ['heart', 'cinta', 'love', 'sayang']],
    ['blush',   ['blush', 'malu']],
    ['tear',    ['tear', 'nangis', 'menangis', 'sedih', 'sad']],
    ['sweat',   ['sweat', 'keringat', 'panik', 'gugup', 'deg-degan']],
    ['sparkle', ['sparkl', 'senang', 'tersenyum', 'senyum', 'seneng', 'kagum', 'excited']],
    ['dizzy',   ['dizzy', 'pusing', 'bingung']],
    ['anger',   ['angry', 'marah', 'kesal', 'jengkel', 'murka']],
    ['shock',   ['kaget', 'shock', 'terkejut', 'gaspet']],
  ];

  // MURNI — dipakai guard test (vm), tidak menyentuh DOM.
  function resolveEmotionFx(name) {
    if (!name || typeof name !== 'string') return null;
    var n = String(name).toLowerCase().replace(/^user:/, '').trim();
    if (!n || n === 'normal' || n === 'default') return null;
    for (var i = 0; i < ALIASES.length; i++) {
      var key = ALIASES[i][0], list = ALIASES[i][1];
      for (var j = 0; j < list.length; j++) {
        if (n.indexOf(list[j]) !== -1) return { key: key, dur: EFFECTS[key].dur };
      }
    }
    return null;
  }

  // ── State ───────────────────────────────────────────────────────
  var ocanvas = null;          // <canvas> overlay (Canvas 2D)
  var octx = null;             // CanvasRenderingContext2D
  var particles = [];          // { kind, born, seed, x, y, vx, vy, glyph, baseFont, r, rx, ry, alpha, scale }
  var current = null;          // { key, until, lastSpawn, dur }
  var rafId = null;

  function cfg() {
    var o = window.__overlayCfg || {};
    return {
      enabled: o.enabled !== false,
      alpha: (typeof o.alpha === 'number') ? o.alpha : 0.9,
      size: (typeof o.size === 'number') ? o.size : 1,
    };
  }

  function model() {
    var st = window.__l2dDebug && window.__l2dDebug.state;
    return (st && st.model) ? st.model : null;
  }
  // Anchor kepala dari bounds panggung model (kepala ≈ 16% dari atas). Koordinat
  // panggung = piksel CSS dari pojok kiri-atas #live2d-canvas (stage di 0,0),
  // jadi identik dengan sistem koordinat kanvas overlay. Dibaca tiap frame agar
  // efek ikut saat model bergerak / viewport berubah.
  function headAnchor() {
    try {
      var m = model();
      if (!m) return null;
      return {
        cx: m.x + m.width * 0.5,
        headY: m.y + m.height * 0.16,
        h: m.height,
        w: m.width,
      };
    } catch (e) { return null; }
  }

  // Buat / selaraskan kanvas overlay. Hanya saat model hidup (app jalan) —
  // tanpa model (mis. sandbox guard) langsung null, tak menyentuh DOM.
  function ensureCanvas() {
    if (!model()) return null;
    if (typeof document === 'undefined' || typeof document.getElementById !== 'function') return null;
    var live = document.getElementById('live2d-canvas');
    if (!live || !live.parentNode) return null;
    if (!ocanvas) {
      ocanvas = document.createElement('canvas');
      ocanvas.id = 'emotion-overlay-canvas';
      ocanvas.setAttribute('aria-hidden', 'true');
      var s = ocanvas.style;
      s.position = 'absolute';
      s.pointerEvents = 'none';   // jangan menangkap klik — panggung di bawahnya
      s.zIndex = '5';             // di atas #live2d-canvas, di bawah panel UI
    }
    if (ocanvas.parentNode !== live.parentNode) live.parentNode.appendChild(ocanvas);
    if (!octx) octx = ocanvas.getContext('2d');
    if (!octx) return null;
    syncCanvasBox(live);
    return octx;
  }

  // Samakan kotak & resolusi kanvas overlay dengan #live2d-canvas (DPR di-cap 2,
  // sama dengan renderer.resolution Pixi 8). Transform 1 unit = 1 px CSS supaya
  // koordinat partikel (koordinat panggung) langsung dipakai.
  function syncCanvasBox(live) {
    if (!ocanvas || !octx) return;
    live = live || (typeof document !== 'undefined' && document.getElementById && document.getElementById('live2d-canvas'));
    if (!live) return;
    var dpr = Math.min((typeof window !== 'undefined' && window.devicePixelRatio) || 1, 2);
    var w = live.clientWidth || parseInt(live.style && live.style.width) || live.width || 0;
    var h = live.clientHeight || parseInt(live.style && live.style.height) || live.height || 0;
    var st = ocanvas.style;
    st.left = (live.offsetLeft || 0) + 'px';
    st.top = (live.offsetTop || 0) + 'px';
    st.width = w + 'px';
    st.height = h + 'px';
    var bw = Math.max(1, Math.round(w * dpr)), bh = Math.max(1, Math.round(h * dpr));
    if (ocanvas.width !== bw || ocanvas.height !== bh) { ocanvas.width = bw; ocanvas.height = bh; }
    octx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }
  function spawnParticle(kind, i, cfgv) {
    var a = headAnchor();
    if (!a) return;
    var def = EFFECTS[kind];
    var glyph = null, baseFont = 0, r = 0;
    if (def.emoji) {
      var idx = (i + Math.floor(Math.random() * def.emoji.length)) % def.emoji.length;
      glyph = def.emoji[idx];
      baseFont = Math.max(18, Math.round(a.w * 0.05 * cfgv.size));
    } else if (kind === 'blush') {
      r = Math.max(8, a.w * 0.032 * cfgv.size);
    }
    var p = {
      kind: kind, born: performance.now(),
      seed: Math.random() * Math.PI * 2, emojiIdx: i,
      x: a.cx, y: a.headY, vx: 0, vy: 0,
      glyph: glyph, baseFont: baseFont, r: r,
      rx: a.cx, ry: a.headY, alpha: 0, scale: 1,
    };
    if (kind === 'heart') {
      p.x = a.cx + (Math.random() - 0.5) * a.w * 0.24;
      p.y = a.headY - a.h * (0.02 + Math.random() * 0.04);
      p.vy = -a.h * 0.055;               // naik
    } else if (kind === 'blush') {
      p.x = a.cx + (i === 0 ? -1 : 1) * a.w * 0.058;
      p.y = a.headY + a.h * 0.055;
    } else if (kind === 'sparkle') {
      var ang = p.seed;
      p.x = a.cx + Math.cos(ang) * a.w * 0.26;
      p.y = a.headY - a.h * 0.06 + Math.sin(ang) * a.h * 0.07;
    } else if (kind === 'tear') {
      p.x = a.cx + (i % 2 === 0 ? -1 : 1) * a.w * 0.05;
      p.y = a.headY + a.h * 0.045;
      p.vy = a.h * 0.03;                 // jatuh pelan, dipercepat di update
    } else if (kind === 'sweat') {
      p.x = a.cx + a.w * 0.075;
      p.y = a.headY - a.h * 0.025;
      p.vy = a.h * 0.02;
    } else if (kind === 'dizzy') {
      p.x = a.cx;
      p.y = a.headY - a.h * 0.05;
    } else if (kind === 'anger' || kind === 'shock') {
      p.x = a.cx + (Math.random() - 0.5) * a.w * 0.1;
      p.y = a.headY - a.h * 0.09;
    }
    p.rx = p.x; p.ry = p.y;
    particles.push(p);
  }

  // Perbarui transform render tiap partikel (rx/ry/alpha/scale).
  // now = performance.now().
  function updateParticles(now, a, cfgv) {
    var dead = [];
    for (var i = 0; i < particles.length; i++) {
      var p = particles[i];
      var t = (now - p.born) / 1000;
      var prog = Math.min(1, (now - p.born) / (current ? current.dur : 2200));
      var k = p.kind;
      if (k === 'heart') {
        p.ry = p.y + p.vy * t;
        p.rx = p.x + Math.sin(t * 2.2 + p.seed) * a.w * 0.02;
        p.alpha = (prog < 0.15 ? prog / 0.15 : 1 - (prog - 0.15) / 0.85) * cfgv.alpha;
        p.scale = cfgv.size * (0.85 + 0.15 * Math.sin(t * 3 + p.seed));
      } else if (k === 'blush') {
        p.alpha = (prog < 0.1 ? prog / 0.1 : prog > 0.8 ? (1 - prog) / 0.2 : 1) * cfgv.alpha * 0.9;
      } else if (k === 'sparkle') {
        p.rx = p.x + Math.sin(t * 1.6 + p.seed) * a.w * 0.015;
        p.ry = p.y + Math.cos(t * 1.3 + p.seed) * a.h * 0.012;
        p.alpha = Math.max(0, Math.sin(t * 4 + p.seed)) * cfgv.alpha;
        p.scale = cfgv.size * (0.7 + 0.3 * Math.sin(t * 5 + p.seed));
      } else if (k === 'tear') {
        p.vy += a.h * 0.0007 * 1000 * 0.016 * 60 * 0.016; // gravitasi lembut
        p.ry = p.y + p.vy * t + 0.5 * a.h * 0.35 * t * t;
        p.rx = p.x;
        p.alpha = (1 - prog) * cfgv.alpha;
      } else if (k === 'sweat') {
        p.ry = p.y + p.vy * t * t * 2.2;
        p.alpha = (1 - prog) * cfgv.alpha;
        p.scale = cfgv.size * (1 + 0.4 * prog);
      } else if (k === 'dizzy') {
        var orb = t * 2.6 + p.seed;
        p.rx = a.cx + Math.cos(orb) * a.w * 0.13;
        p.ry = a.headY - a.h * 0.05 + Math.sin(orb * 2) * a.h * 0.02;
        p.alpha = (1 - prog) * cfgv.alpha;
      } else if (k === 'anger' || k === 'shock') {
        var pop = Math.min(1, t / 0.18);
        p.scale = cfgv.size * (0.4 + 0.6 * (1 + 0.25 * Math.sin(pop * Math.PI)) * pop);
        p.alpha = (1 - prog) * cfgv.alpha;
      }
      if (prog >= 1) dead.push(i);
    }
    for (var d = dead.length - 1; d >= 0; d--) particles.splice(dead[d], 1);
  }
  function clearCanvas() {
    if (!octx || !ocanvas) return;
    octx.save();
    octx.setTransform(1, 0, 0, 1, 0, 0);
    octx.clearRect(0, 0, ocanvas.width, ocanvas.height);
    octx.restore();
  }

  function draw() {
    if (!octx || !ocanvas) return;
    clearCanvas();
    for (var i = 0; i < particles.length; i++) {
      var p = particles[i];
      var alpha = p.alpha < 0 ? 0 : (p.alpha > 1 ? 1 : p.alpha);
      if (alpha <= 0) continue;
      octx.save();
      octx.globalAlpha = alpha;
      octx.translate(p.rx, p.ry);
      if (p.scale && p.scale !== 1) octx.scale(p.scale, p.scale);
      if (p.glyph) {
        // Emoji: jangkar kiri-atas (textAlign left / baseline top); skala
        // tumbuh dari titik jangkar itu.
        octx.font = p.baseFont + 'px sans-serif';
        octx.textAlign = 'left';
        octx.textBaseline = 'top';
        octx.fillStyle = '#ffffff';
        octx.fillText(p.glyph, 0, 0);
      } else if (p.kind === 'blush') {
        // Blush: ellipse pink lembut (fill rgba 255,158,194 @ 0.55).
        octx.fillStyle = 'rgba(255, 158, 194, 0.55)';
        octx.beginPath();
        octx.ellipse(0, 0, p.r * 1.35, p.r * 0.75, 0, 0, Math.PI * 2);
        octx.fill();
      }
      octx.restore();
    }
  }

  // Satu frame animasi. now = performance.now(). Dipanggil dari rAF internal
  // (saat efek aktif) atau manual (pengujian headless).
  function tick(now) {
    var cfgv = cfg();
    if (!cfgv.enabled) return;
    var a = headAnchor();
    if (!a) return;
    if (octx) syncCanvasBox();
    updateParticles(now, a, cfgv);
    // spawn lanjutan selama efek masih aktif
    if (current && now < current.until) {
      var def = EFFECTS[current.key];
      if (def.spawnEvery > 0 && now - current.lastSpawn >= def.spawnEvery) {
        current.lastSpawn = now;
        spawnParticle(current.key, Math.floor(Math.random() * 4), cfgv);
      }
    }
    draw();
    // selesai: semua partikel mati & window habis
    if (!particles.length && (!current || now >= current.until + 400)) {
      current = null;
      clearCanvas();
      if (rafId) { cancelAnimationFrame(rafId); rafId = null; }
    }
  }

  function loop() {
    tick(performance.now());
    if (rafId !== null) rafId = requestAnimationFrame(loop);
  }

  // ── API publik ──────────────────────────────────────────────────
  window.__emotionOverlay = {
    // Dipanggil app.js di titik ekspresi dipasang (semua mode: universal,
    // .exp3 native, synthetic) — nama apa pun, yang tak cocok diabaikan.
    onExpression: function (name) {
      var cfgv = cfg();
      if (!cfgv.enabled) return;
      var fx = resolveEmotionFx(name);
      var now = performance.now();
      if (!fx) { this.clear(); return; }
      if (!ensureCanvas()) return;
      var def = EFFECTS[fx.key];
      if (current && current.key === fx.key) {
        current.until = now + def.dur;      // perpanjang, jangan numpuk
        return;
      }
      current = { key: fx.key, until: now + def.dur, lastSpawn: now - def.spawnEvery, dur: def.dur };
      var counts = { heart: 3, blush: 2, sparkle: 4, tear: 1, sweat: 1, dizzy: 3, anger: 1, shock: 1 };
      var n = counts[fx.key] || 1;
      for (var i = 0; i < n; i++) spawnParticle(fx.key, i, cfgv);
      if (rafId === null) rafId = requestAnimationFrame(loop);
    },
    clear: function () {
      current = null;
      particles = [];
      clearCanvas();
      if (rafId) { cancelAnimationFrame(rafId); rafId = null; }
    },
    _status: function () {
      return { active: !!current, key: current ? current.key : null, particles: particles.length, attached: !!octx };
    },
    _tick: function (now) { tick(now || performance.now()); },
    _resolve: resolveEmotionFx,          // ekspos untuk guard test
  };

  console.log('emotion-overlay: siap (Canvas 2D — efek app-level untuk ekspresi yang rig-nya tidak mengikat art)');
})();



