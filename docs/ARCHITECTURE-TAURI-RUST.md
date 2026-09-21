# ARCHITECTURE — Migrasi ke Tauri + Rust Core (arah, kontrak, roadmap)

> **Dokumen arah arsitektur.** Ditulis untuk AI agent & manusia yang mengerjakan
> migrasi backend `live2d-agent` dari TypeScript/Bun ke **Rust core di dalam
> Tauri**, tanpa merusak renderer Live2D/PixiJS yang sudah jalan.
>
> **Status: RENCANA (belum dieksekusi).** Kode belum berubah. Dokumen ini
> adalah kontrak yang mengikat saat eksekusi dimulai. Jika dokumen & kode
> bertentangan setelah eksekusi dimulai: **kode yang benar — perbaiki dokumen.**
>
> Sumber niat: arah "Tauri + Rust Application Core + TypeScript/Live2D Frontend"
> yang disetujui user. Prinsip inti:
> **Rust memiliki aplikasi. TypeScript memiliki WebView & rendering. LLM tetap
> provider eksternal yang bisa diganti.**

---

## 1. Bukan tujuan (Non-Goals)

Jangan tafsirkan arah ini sebagai "semua harus jadi Rust". Yang **eksplisit
BUKAN** tujuan:

1. Menulis ulang renderer Live2D di Rust.
2. Mengganti / memindahkan PixiJS 8 ke Rust.
3. Memindahkan render loop Live2D lewat Tauri IPC.
4. **Menaruh IPC di jalur per-frame** (setParameter 30/60/120 FPS lewat IPC).
5. Menanam runtime inferensi LLM + model ke dalam exe utama.
6. Menghapus TypeScript hanya karena Rust masuk.
7. Rewrite big-bang.
8. Memakai migrasi Rust sebagai "obat" masalah performa render.

Performa render diselesaikan di pipeline render (Cubism → adapter → Pixi 8 →
WebGL → compositing), **bukan** dengan memindah logika backend ke Rust.

---

## 2. Empat keputusan yang dikunci (user)

1. **Transport: Tauri IPC-first.** Tiap modul yang pindah ke Rust langsung
   diganti `invoke()` / event Tauri. BUKAN strategi "HTTP-parity" (server HTTP
   yang identik). Konsekuensi: frontend berubah per langkah, guard legacy ikut
   di-update, klien HTTP mandiri (CLI/OBS/pet) diurus lewat lapisan kompat.
2. **Desktop-first; HTTP = kompatibilitas opsional.** Server HTTP TIDAK menjadi
   arsitektur inti. Sebuah *compat adapter* (opsional, dari service yang sama)
   tetap menyajikan `/api/*` untuk: CLI agent terminal, overlay OBS
   (`vtuber.html` = OBS browser source, wajib HTTP), akses dari browser/HP.
   Ini antarmuka kompat — bukan pusat arsitektur.
3. **engine/ diabsorb in-process.** Sidecar Whisper + SuperTonic (crate
   `engine/`) jadi **library** yang dipanggil langsung dari Rust core. Hilang:
   port 8330, spawn/health/`ensureSidecarHasStt` restart-dance, satu proses.
4. **Renderer tetap TypeScript.** `src/live2d/*`, `src/client/*`,
   `static/js/app.js`, MotionRuntime, ParameterArbiter — tidak pindah.

---

## 3. Arsitektur target

```
Companion.exe (Tauri)
├─ WebView (TypeScript) — presentation & rendering
│    UI · Companion presentation · MotionRuntime · ParameterArbiter
│    · Live2D/Cubism · PixiJS 8 · WebGL
│    └─ src/client/transport/  ← SEAM TUNGGAL (baru)
│         api.call(scope, action, payload)   — request/response
│         api.stream(scope, action, payload) — streaming (delta token, dsb)
│         api.subscribe(topic, cb)           — event push (ganti polling)
│         backend:
│           • TauriTransport  = invoke + ipc::Channel + event listen  (primer)
│           • HttpTransport    = fetch / poll / SSE                   (kompat)
│
├─ Tauri IPC (coarse-grained — JANGAN per-frame):
│    commands per domain · streaming via ipc::Channel · event push per topik
│    custom protocol  lumi://  untuk aset model (/model/*) & payload biner
│
└─ Rust Core (cargo workspace)
     config/persistence · llm-provider (trait) · agent (loop+21 tool+gate)
     · modes (vtuber/assistant/pet) · media (tts/stt — engine absorbed)
     · browser CDP · model/sheet/motion file manager
     compat adapter (axum, OPSIONAL, di belakang flag) → mirror /api/*

LLM = provider eksternal (trait LlmProvider): Cloud · Local · OpenAI-compat
      · Custom — bisa diganti tanpa mengubah arsitektur companion.
```

### Frame Loop Rule (KRITIS)

Rust hanya mengeluarkan **directive semantik** ("emotion: happy, gaze {x,y},
motion: idle"). Frontend yang menerjemahkan ke animasi/parameter/blend/render,
seluruhnya lokal. Tidak pernah `setAngleX()`/`setParameter()` lewat IPC.

```
LLM → Agent/Decision (Rust) → Companion Directive → IPC → Frontend
    → CompanionPolicy → MotionRuntime → ParameterArbiter → Live2D → Pixi → WebGL
```

---

## 4. Kondisi awal (hasil eksplorasi — dasar rencana)

- **Frontend:** TANPA WebSocket. Hanya **1** konsumen SSE (panel assistant:
  `ask-stream`/`approve-stream`; event `delta / tool_call / tool_result /
  approval / speak / done`). Sisanya `fetch` + **polling berkursor**:
  assistant events 1500ms, status 2000ms, vtuber events 2500ms, koneksi LLM
  4000ms, badge 4000ms, rail 8000ms, pet 5000ms, browser status 2000ms +
  screenshot 3000ms.
- **±60 pemanggilan `/api/*`** tersebar di 6+ file dengan **5 pola derivasi
  origin berbeda** (`location.origin`+fallback 8310, `location.origin` polos,
  path relatif). Guard `test/legacy/test-api-origin.js` mengunci ini.
- **Shell Tauri sekarang** (`agent-shell/`): 0 `#[tauri::command]`, 0
  invoke_handler, `bundle.active=false`, load `WebviewUrl::External` ke server
  Bun. Click-through pet via `withGlobalTauri` + `setIgnoreCursorEvents`, state
  di server (`/api/pet/*`).
- **Biner over HTTP:** TTS audio blob (`/api/tts`), screenshot JPEG
  (`/api/browser/screenshot`); aset model Live2D di-load `fetch("/model/...")`
  relatif (loader Cubism di `live2d-view.mjs`).
- **Klien HTTP mandiri (mudah terlupa):** `vtuber.html` (OBS overlay),
  `pet.html`, `src/cli/agent.ts` (SSE + fallback JSON).
- **Kopling server→client:** `src/server/index.ts` meng-import
  `sanitizeMotionAsset` (`../client/animation/motion-dsl`) & `MotionTaxonomy`
  (`../client/engine/motion-taxonomy`). Handler motions yang pindah ke Rust
  harus mem-port keduanya.
- **`appRoot()`** (`src/shared/paths.ts`) = choke point tunggal semua path data:
  `data/config.json`, `data/assistant-sessions.json`,
  `.agent-memory/memory.json` (di akar app, BUKAN `data/`), `data/sheets/*`,
  `data/motions/*`, `data/browser/profile`, model engine.
- Sudah native/Rust: `agent-shell/` (Tauri) + `engine/` (crate live2d-engine,
  TTS `ort` + STT `whisper-rs`, port 8330).

---

## 5. Kontrak yang WAJIB utuh lintas migrasi (invarian)

Setiap stage harus mempertahankan semuanya; kalau berubah, itu regresi.

- **Format file data byte-compatible:** `data/config.json` (atomic tmp+rename,
  merge per-id koneksi, `runtimeOverrides`, apiKey plaintext di disk & di-MASK
  ke antarmuka dengan placeholder `MASUKKAN…` / `••••`),
  `data/assistant-sessions.json` (`{active, sessions:[{id,name,workDir,ts,
  messages}]}`, cap 20×60), `.agent-memory/memory.json` (di akar app, cap
  100×1200 char), cache `data/sheets/*` (stamp `scannerVersion`).
- **Replay ber-kursor:** events assistant & vtuber bisa diminta `since=0` untuk
  replay dari ring buffer (assistant bus 120 event ber-seq). Migrasi
  polling→push WAJIB hybrid: replay awal via kursor + push live.
- **Kosakata event stream panel** persis: `delta / tool_call / tool_result /
  approval / speak / done{ok,reply,error,parked,taskId,position,paused}` +
  semantik fallback `decideFallback` dua-kasus (`src/client/agent/panel/
  stream.ts`).
- **Model-Agnostic** (docs/MODEL-AGNOSTIC-RULES.md): TIDAK ADA id param
  bernomor / nama model / range spesifik di Rust core; makna param hanya lewat
  role space. Logika penyimpulan harus tetap benar setelah semua nama diganti.
- **Sistem sheet** (docs/SHEET-SYSTEM.md): `user > ai` mutlak; `paramGroups` ≠
  `presets`; angka hanya dari engine.
- **Mode** (docs/MODES.md): satu mode aktif; teardown dulu saat pindah mode.
- **Motion** (docs/MOTION-SYSTEM-SPEC.md): LLM hanya memilih id semantik; sanitize
  satu pintu; runtime satu pemutar. **Frame loop tidak lewat IPC.**
- **Keamanan/privasi** (AGENTS.md §5): loopback default; body cap; guard
  traversal; frame webcam tak pernah keluar; audio mic default loopback lokal,
  cloud hanya bila user pilih sadar.
- **Guard legacy** ikut di-update di commit yang sama saat kontrak berubah
  (guard menguji kode asli via `vm` — bukan salinan).

---

## 6. Roadmap bertahap

Aturan: **tiap stage meninggalkan aplikasi tetap fungsional + gate hijau**
(`bun run build` + `bunx tsc --noEmit` + `bun run test` + `cargo test` untuk
crate yang tersentuh). Tiap stage punya definisi "selesai" sendiri dan boleh
berhenti aman di situ.

### Stage 0 — Kontrak & fondasi
- Commit dokumen ini + entri STATUS + rujukan AGENTS.md. **(sesi ini)**
- Ubah ke **cargo workspace**: root `Cargo.toml` `members = ["agent-shell",
  "engine", "core"]`; `engine/` jadi **lib + bin** (bin tetap untuk debug
  standalone; lib untuk diabsorb Stage 4).
- Selesai = workspace build + semua gate hijau, perilaku aplikasi tak berubah.

### Stage 1 — Transport seam + infra IPC shell
- Buat `src/client/transport/` (abstraksi `api` + `TauriTransport` &
  `HttpTransport`). Migrasi ±60 situs panggilan **mekanis 1:1** — perilaku HTTP
  identik, backend Tauri belum aktif (feature-detect `window.__TAURI__`).
- `agent-shell/`: tambah `invoke_handler` + capabilities; command minimal
  (`app_info`, pet click-through → command, lepas polling `/api/pet/state`).
- **Spike teknis wajib** (buktikan sebelum lanjut): (a) blob biner via IPC
  (TTS WAV beberapa MB) — IPC vs custom protocol; (b) custom protocol `lumi://`
  untuk aset model + loader Cubism tetap resolve; (c) `ipc::Channel` untuk
  streaming event SSE.
- Guard `test-api-origin.js` → guard transport (perbarui, jangan hapus).
- Selesai = jendela Tauri load frontend, semua fitur jalan via HttpTransport;
  TauriTransport siap tapi belum jadi jalur utama.

### Stage 2 — Rust core: infrastruktur
- Crate `core` (tokio). Pindah low-risk: config manager (byte-compatible),
  paths (portable exe-dir), mode manager, model/sheet/motions file manager.
  **Port `sanitizeMotionAsset` + `motion-taxonomy` ke Rust** (memecah kopling
  server→client).
- IPC command per domain; transport backend Tauri **aktif** untuk scope ini.
- **Parity harness**: diff respons TS vs Rust per scope (ikuti pola test
  server-parity yang sudah ada). Bun server tetap ada untuk scope belum pindah.

### Stage 3 — AI infrastruktur
- **3a** trait `LlmProvider` (Cloud/Local/OpenAI-compat/Custom) + role routing
  (chat/motion/sheet/assistant) + streaming (idle-timeout per chunk) +
  fallback/cooldown + persist status ke config.
- **3b** agent loop + 21 tool + permission gate (event `approval` push
  menggantikan SSE) + bus → event push (ganti poll 1500ms; **replay `since=0`
  tetap ada** — hybrid) + sessions/memory/undo/subagent. Refactor singleton →
  owned state; catat pola runtime-swap subagent (`subagent.ts:47-58`) yang
  perlu didesain ulang.
- **3c** vtuber (IRC via tokio-tungstenite, YouTube poll, scheduler) +
  persona/narrator.
- **Compat adapter (axum) dibangun paralel** dari service yang sama → CLI agent
  (`src/cli/agent.ts`) tetap hidup; kosakata SSE dipertahankan.

### Stage 4 — Media + browser + engine absorb
- TTS multi-provider + cache + pcmToWav; STT. **engine lib dipanggil
  in-process** (downloader model on-demand pindah ke core; port 8330 hilang).
- Browser CDP (tokio-tungstenite + panggilan Win32 langsung menggantikan
  PowerShell focus); zip import via crate `zip` (ganti unzip.exe/Expand-Archive).
- `vtuber.html`/OBS overlay dilayani compat adapter; `pet.html` (jendela Tauri)
  via IPC (`withGlobalTauri`).

### Stage 5 — Packaging + pangkas
- `frontendDist` Tauri meng-embed `static/`; hapus `live2d-agent.exe` Bun dari
  dist (`src/dist.ts` disederhanakan; `installer.iss` di-update — kontrak
  `data/` di samping exe dipertahankan).
- Compat adapter di belakang flag config (default ON — OBS butuh). Hapus server
  Bun lama dari jalur produksi (tetap dipakai test/dev).
- Update AGENTS.md/README/TROUBLESHOOTING; guard final. **`bun` tetap alat
  dev/build, bukan dependensi runtime produksi.**

---

## 6b. TEMUAN IPC (spike Stage 1 — mengubah urutan migrasi)

Diverifikasi headless (server + jendela Tauri nyata + diagnostik):

- `window.__TAURI__.core.invoke` ADA, command `app_info` ter-registrasi
  (`generate_handler!`), tapi invoke **ditolak**: `"app_info not allowed.
  Plugin not found"`.
- Akar masalah (dibaca dari `tauri-2.11.6/src/ipc/authority.rs::resolve_access`):
  otorisasi command difilter `origin.matches(&cmd.context)`. Shell memuat
  frontend via `WebviewUrl::External("http://127.0.0.1:8310")` → origin
  **Remote**. Command app default konteksnya **Local**, dan capability yang
  cuma berisi permission `core:*` tidak memberi command app ke origin remote.
  Menambah `remote.urls` ke capability pun tak menolong: command app **tidak
  punya identifier permission** (dicek: tak ada di `gen/schemas`), jadi tak bisa
  didaftarkan sebagai permission bercakupan-remote.

**Konsekuensi (load-bearing):** selama frontend disajikan oleh server HTTP
(origin remote), **Tauri IPC untuk command app tidak bisa dipakai.** Ini
membalik urutan rencana: **frontend harus disajikan LOKAL oleh Tauri**
(`frontendDist` → origin `tauri://`/`http://tauri.localhost`) SEBELUM IPC-first
bisa jalan. Artinya "packaging frontendDist" (dulu Stage 5) menjadi
**prasyarat Stage 1**, bukan langkah akhir.

**Dampak ke arsitektur "shell load External URL":** model saat ini (shell =
`WebviewUrl::External` ke server Bun) harus berubah jadi Tauri menyajikan aset
frontend lokal, dan server HTTP turun peran jadi *compat adapter* (untuk CLI
agent + OBS overlay + akses HP) — persis niat "HTTP = kompatibilitas opsional",
tapi transisinya harus lebih awal.

**Yang SUDAH terbukti jalan:** frontend penuh (Live2D + Pixi + TTS/STT) berjalan
mulus di dalam jendela Tauri via HTTP (origin remote) — "model muncul, suara
keluar". Jadi HttpTransport (mode kompatibilitas) valid; hanya jalur IPC yang
menuntut origin lokal.

**Keputusan terbuka untuk user** (belum diputus):
- **A. Pindah frontend ke Tauri-served (local origin) lebih dulu** → buka jalan
  IPC-first sesuai rencana; perubahan besar di shell (`frontendDist`, resolusi
  aset model lewat protocol lokal — spike custom protocol jadi wajib duluan).
- **B. Pertahankan HTTP transport** (sudah terbukti jalan), Tauri tetap sekadar
  cangkang jendela; tunda/urungkan IPC-first. Jauh lebih sederhana, tapi
  menyimpang dari pilihan "Tauri IPC-first".

## 7. Risiko utama & mitigasi

| Risiko | Mitigasi |
|---|---|
| Blob besar via IPC (TTS WAV) | Spike Stage 1: bandingkan IPC vs custom protocol `lumi://`; pilih yang tak menyalin ganda. |
| Loader Cubism resolve URL relatif di bawah `lumi://` | Spike Stage 1: uji load model3/moc/texture/physics; siapkan protocol handler streaming aset. |
| Semantik replay polling→push | Hybrid: poll/replay awal `since=0` lalu subscribe push; jangan buang kursor. |
| Refactor singleton agent/subagent (runtime-swap) | Desain owned-state + handle; port bertahap dengan parity harness. |
| Profil Cargo campur (`opt-level` shell "s" vs engine 3) | Cargo **workspace** dengan profil per-package / feature. |
| Skop besar tak tuntas satu sesi | Tiap stage berhenti aman + gate hijau; jangan gabung dua stage. |
| Guard legacy pecah | Update guard di commit yang sama; guard menguji kode asli. |

---

## 8. Definisi sukses

```
Development : Bun + Tauri + Rust + TypeScript  → jalan normal
Production  : Companion.exe                      → pengalaman lengkap, 1 exe
Internal    : Rust = core/backend/native
              TypeScript = UI + Live2D + Pixi + presentation
              LLM = layanan eksternal yang bisa diganti
```

Dan rantai ini tetap bersih & terpisah:

```
LLM → Agent/Decision → Semantic Directive → Tauri IPC → Frontend
    → MotionRuntime → ParameterArbiter → Live2D → PixiJS 8 → WebGL
```

Proyek **bukan** menjadi "renderer Rust". Proyek menjadi *aplikasi desktop
Tauri yang core native-nya Rust, lapisan presentasi/render-nya tetap TypeScript
+ PixiJS 8 + Live2D, dan LLM-nya layanan eksternal yang bisa diganti* — satu
exe utama dari sisi user, fleksibel di provider LLM, dengan render Live2D
real-time yang sepenuhnya lokal di frontend.
