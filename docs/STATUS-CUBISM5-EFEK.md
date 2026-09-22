# STATUS SESI — Dukungan Cubism 5 & Efek Model (Handoff)

> Dokumen handoff sesi kerja. Tulis ulang/tambah sesuai perkembangan; jangan
> hapus keputusan yang masih berlaku. Kode yang dirujuk: sudah ter-commit di
> master (lihat daftar commit di bawah).

## ARSITEKTUR SAAT INI (post-migrasi Pixi 8 — entri 27–40 selesai)

Satu jalur render untuk SEMUA permukaan (app utama & pet.html):

```text
LLM → Motion/Behavior Director → app.js komposisi jiwa (ADITIF)
      → framework Cubism 5-r.5 (motion/ekspresi/physics/pose
        + blink/breath/gaze/lipsync updater ber-gate)
      → Pixi 8 renderer → model
```

- **Renderer**: `src/live2d/view/` (Live2DView facade + framing) di-bundle ke
  `static/js/live2d-view.mjs` (gitignored — `bun run build`). Framework
  Cubism 5-r.5 vendored di `src/live2d/cubism/` dengan Core 6.0.1 — tanpa
  MOC-version-hack (moc3 v4.2/v5/v6 native).
- **Kepemilikan**: framework memutar motion/ekspresi/physics/pose +
  blink/breath/gaze/lipsync (updater ber-gate); app.js menyumbang liveliness
  & emosi secara ADITIF lewat backend `coreModel` (SET utk aiLock/motion-
  layer/mouthForm, ADD untuk komposisi). Gaze kursor & pet via
  `setLookTarget` (CubismTargetPoint). Slider keekspresivan per grup sendi
  (kepala/mata/badan, 0–2×) di panel konfigurasi, persist per-model.
- **Stack lama (Pixi 6 + pixi-live2d + MOC-hack) sudah DIHAPUS** — git
  history menyimpan semuanya. PixiJS 6 global tersisa hanya sebagai utilitas
  renderer overlay efek emosi (emotion-overlay.js, canvas terpisah).
- **Gate**: 457 unit test + 416 guard (10 suite) + tsc — hijau penuh.

### Belum teruji (konsolidasi — catatan untuk sesi berikutnya)

1. **Pet klik-tembus di shell Tauri nyata** — halaman pet.html terverifikasi
   di browser, tapi `setIgnoreCursorEvents`/destroy jendela hanya bisa
   diuji dengan build `bun run build:pet` (butuh toolchain Rust).
2. **Uji rasa user jangka panjang** — zoom/drag dalam pemakaian nyata,
   VTuber Twitch/YouTube asli (baru mock), TTS berkala, Motion Studio UI
   visual penuh (baru pipeline dasarnya yang teruji).
3. Konversi guard override ke bun test atas AppWriteUpdater (opsional —
   perilaku sticky-override kini diuji lewat verifikasi browser).

Dokumentasi publik (README/AGENTS/MODES/TROUBLESHOOTING/MODEL-AGNOSTIC)
sudah selaras dengan arsitektur ini.

**Antrean stabilisasi (sesi berikutnya mulai dari sini):**
1. **Fitur pasca-migrasi yang belum teruji** (entri 41–42): pet klik-tembus
   di shell Tauri nyata (butuh `bun run build:pet` / toolchain Rust); Motion
   Studio UI visual penuh (pipeline dasar teruji); uji rasa jangka panjang
   VTuber/chat/TTS. User: "memang ada yang salah di beberapa hal, belum
   diuji langsung" — temukan lewat pemakaian nyata, jangan ditebak.
2. ~~**Sisa opsional rework**: UI daftar antrean task, cancel per-task dari
   panel, ekspresi/motion balasan VTuber~~ **✅ entri (48) 2026-09-19** —
   ketiganya diimplementasi + gate hijau; fitur-fiturnya lolos audit visual
   entri (49). SISA: jalur preset-param di overlay OBS (entri 49 — jalur
   native sudah terpasang, no-op anggun untuk model tanpa aset semantik).
3. **Server dev user harus distart ulang** — instance lama 8310/8312 sudah
   dimatikan (permintaan user, entri 49); jalankan ulang `bun run dev`/
   `bun run src/server/index.ts` untuk memakai semua fix. Branch
   `migration/pixi8-cubism` di-commit tapi BELUM di-push. Provider LLM
   aktif (apinex) sesekali 402/429 — bukan regresi.
4. Keputusan terbuka lama: branch usang `tes`/`feat/cubism-official-renderer`
   (user bilang hapus manual saat sudah stabil) + blocker Core di SHA lama
   GitHub (risiko sudah diterima user — jangan dibuka lagi).

## UPDATE 2026-09-22 (66) — Stage 2 mulai: penyajian statis Rust (fondasi single-exe)

Arah dikunci (dari syarat user "satu exe"): backend disajikan **Rust
in-process (axum) di dalam Tauri**, bukan Tauri IPC. Detail §2 ARCHITECTURE.

Increment pertama Stage 2 (commit menyusul): `core` kini bisa **menyajikan
frontend & aset statis** dari Rust — fondasi Tauri-serve-frontend (single-exe):
- `core/src/paths.rs` — AppPaths (root/static/data/model/sheets/motions) +
  detect() padanan appRoot() TS.
- `core/src/static_serve.rs` — port `serveStatic`/`safeJoinStatic`: guard
  traversal ".." (mentah + ter-encode), block `config.json`(+.bak), route
  `model/*`→`data/`, MIME suffix-terpanjang (.model3.json dst).
- `core/src/lib.rs` — router: `/health`, `/api/version`, fallback statis + SPA
  (path tanpa ekstensi → index.html; `/api/*` tak dikenal → 404 JSON, sama TS).
  `serve(port, paths)` in-process loopback.
- 9 test cargo hijau (traversal 403, config forbidden, model→data, MIME, api
  404 JSON, health, version).

Grup rute BACA config juga sudah diport:
- `core/src/config.rs` — load() byte-compat (merge {...DEFAULT, ...base}, nilai
  user tak ditimpa), mask_key (6+8bullet+4), normalize_roles (LLM_ROLES),
  api_config_response (GET /api/config: apiKey koneksi & tts dimask, roles
  dinormalisasi). Route GET /api/config di router.
- **Smoke terverifikasi** (`cargo run -p live2d-core --example serve` di root
  repo, port 8340): GET / → index.html asli (74 KB, text/html), /js/app.js asli
  (319 KB), /config.json → 403, /api/config → config asli dgn apiKey TERMASK
  (`sk-xt-••••••••0be6`) sama format TS. 13 test cargo hijau.

Rute model READ-ONLY juga diport:
- `core/src/model.rs` — find_model3 (DFS depth≤6), list_models, model_path_rel.
  Route GET /api/models + /api/model/path.
- **Paritas LIVE vs server Bun** (core:8341 vs bun:8312): `/api/models` =
  `["lumine","ren_official_cubism","神宫白子模型"]` IDENTIK (termasuk nama CJK);
  `/api/model/path?name=lumine` = `model/lumine/lumine/lumine.model3.json`
  identik. Gap sengaja: Auto-Rescue belum diport (Bun masih pemilik selama transisi).

Rute sheet juga diport:
- `core/src/sheet.rs` — sanitize_key (huruf/angka Unicode dipertahankan: CJK/kana
  utuh), get_sheet (tandai `_stale` bila scannerVersion≠2), save_sheet (stamp +
  tulis atomik tmp+rename). Route GET/POST /api/sheet. Smoke OK: POST → path,
  GET → {scannerVersion:2,...}, 404 bila tak ada. 16 test cargo hijau.

Config TULIS juga diport (user OK: config.json gitignored + key di-reroll):
- `core/src/config.rs` +save_connections/events/tts/i18n (atomic tmp+rename) +
  handle_config_post (add/update/delete/setActive/saveEvents/saveTTS/saveI18n/
  save) — persis handleConfigPost TS: id `conn_<base36>`, roles dinormalisasi,
  update tanpa apiKey → key lama dipertahankan, cleanStr apiKey, saveTTS balas
  termask tapi FILE simpan key asli. Route POST /api/config. 18 test cargo hijau
  (add→aktif+roles, update key-preserve, setActive 404, delete→activeId null,
  saveTTS mask+merge).

Ekspresi juga diport:
- `core/src/expressions.rs` — discover (walk .exp3.json depth≤6, params=Id
  rigger utk gate overlay, declared dari model3, sort), adoption GET/POST
  (sanitize_model_folder_name). Route GET /api/model/expressions +
  GET/POST expressions-adoption. **Paritas urutan LIVE vs Bun: 19 ekspresi
  lumine IDENTIK** (fix bug: localeCompare TS → banding lowercase, bukan byte
  cmp). 20 test cargo hijau. Gap: rescue-only folder belum ditangani.

Motions (read/delete) + model files/avatar juga diport:
- `core/src/motions.rs` — list/get/delete (validasi id [A-Za-z0-9_-]{1,60},
  sanitize_key modelKey). `core/src/model.rs` +list_model_files (walk) +
  find_avatar (AVATAR_PREFERRED lalu walk depth≤2, skip aset model3/moc3) +
  avatar_mime. Routes: GET /api/model/files, GET /api/model/avatar (biner),
  GET /api/motions, GET+DELETE /api/motions/{id}. Paritas: files lumine 28=28,
  avatar 200 image/png 104 KB, motions []. 21 test cargo hijau.
  Gap: POST/PUT motions (butuh sanitizeMotionAsset client) belum diport.

Auto-Rescue juga diport:
- `core/src/rescue.rs` — port buildRescueBlueprint (natural_compare, walk_files,
  scan .moc3/tekstur/motion/exp, idle dari vtube.json/pola nama, dedup ekspresi,
  manifest virtual di memori). Terintegrasi: list_models & model_path_rel &
  route virtual `model/<folder>/__rescue__.model3.json`. 23 test cargo hijau;
  paritas /api/models tetap identik.

Model delete + upload (base64) juga diport:
- `core/src/model.rs` +delete_model (rmdir rekursif, guard traversal) +
  upload_model (valid_model_name, tulis base64 per file, wajib *.model3.json,
  guard traversal). Routes DELETE /api/model/{name}, POST /api/model/upload.
  Smoke end-to-end (temp root): upload testmdl → /api/models=[testmdl] → delete
  → []; tanpa route-conflict (GET /api/model/path tetap jalan). 23 test hijau.

Import-zip juga diport (crate `zip`, bukan shell unzip):
- `core/src/model.rs` +import_zip — decode base64, ekstrak via `zip` crate
  (enclosed_name menolak traversal), sanitize_model_folder_name, wajib
  *.model3.json. Route POST /api/model/import-zip. 24 test hijau (uji rakit
  zip in-memory → ekstrak → temukan model3, sanitize '!'→'_').

**Stage 2 file/config/model/sheet surface: SELESAI diport** (static, config r/w,
models, model/path, sheet r/w, expressions+adoption, motions r/d, files, avatar,
auto-rescue, delete, upload, import-zip) — semua paritas/uji vs Bun, 24 test.

## Stage 3 MULAI (batch 3a) — LLM client + /api/chat
- `core/src/llm.rs` — port llm-client (jalur non-stream): call_llm multi-provider
  (openai-compatible/groq/openai via reqwest, gemini, anthropic, mock),
  classify_error (ERROR_RULES), conn_has_role/order_for_role (eksplisit dulu),
  llm_for_role = routing + fallback/cooldown + persist status ke config
  (rateLimitedUntil ISO8601 via chrono, byte-compat). Route POST /api/chat.
  Deps: reqwest(rustls), chrono. 28 test cargo hijau; smoke /api/chat mock →
  {reply,used} end-to-end. BELUM: streaming SSE (assistant), agent loop+tools.
- **batch 3b (helper)**: `core/src/jsonx.rs` — port extractJSON (SSE-merge +
  iris objek seimbang string-aware), salvageJSONArrayOfObjects (array terpotong),
  extractJSONArrayLoose (tahan echo model kecil: ambil blok TERAKHIR + objek
  pembungkus), extractJSONObjectLoose. 33 test cargo hijau (5 jsonx). Fondasi
  endpoint LLM-role JSON (animate-text/classify-params/analyze-sheet/motions)
  yang menyusul.

BELUM diport (coupling ke kode lain): mode manager (runtime modes), POST/PUT
motions + motion-taxonomy (client motion-dsl/classifier), LLM/agent/vtuber/media/
browser (Stage 3-4). Shell belum menjalankan server core (masih Bun).

## UPDATE 2026-09-22 (65) — EKSEKUSI Stage 0-1 migrasi Tauri + TEMUAN IPC penting

Lanjutan (64). User: "gas implementasikan hingga akhir" + "kamu tes sendiri".
Dikerjakan Stage 0 & 1 (fondasi), lalu spike IPC membongkar satu asumsi kunci.

**SELESAI & gate hijau:**
- **Stage 0** (commit 77913fa): cargo **workspace** (agent-shell + engine +
  core), `engine/` jadi **lib+bin**, crate `core` skeleton. Profil release di
  root (shell opt-level=s, engine=3). `cargo check --workspace` + test hijau.
- **Stage 1a/1b** (commit 1cf2234): seam `src/client/transport/`
  (apiBase/apiUrl/apiFetch/getJson/postJson + hasTauri/tauriInvoke/appInfo,
  window.__transport, 9 unit test) + command Tauri `app_info` + invoke_handler +
  agent-shell depend live2d-core. Additive, call-site lama belum diubah.

**TEMUAN SPIKE IPC (dibuktikan headless — mengubah rencana):**
`window.__TAURI__.core.invoke` ada & command ter-registrasi, TAPI ditolak:
`"app_info not allowed. Plugin not found"`. Akar (authority.rs::resolve_access):
otorisasi difilter per-origin. Shell load frontend via `WebviewUrl::External`
(http://127.0.0.1:8310) = origin **Remote**; command app default konteks
**Local**; capability `remote.urls` tak menolong karena command app tak punya
identifier permission. → **Selama frontend disajikan server HTTP (remote),
Tauri IPC command app TIDAK bisa.** frontendDist/origin-lokal jadi PRASYARAT
IPC-first (bukan langkah akhir). Detail + 2 opsi keputusan (A: pindah frontend
ke Tauri-served lebih dulu; B: pertahankan HTTP transport) di
docs/ARCHITECTURE-TAURI-RUST.md §6b.

**Yang terbukti jalan:** frontend penuh (Live2D+Pixi+TTS/STT) mulus di jendela
Tauri via HTTP ("model muncul, suara keluar"). HttpTransport valid; hanya IPC
yang menuntut origin lokal.

**Belum:** Stage 1c (3 spike) & Stage 2-5 tertahan menunggu keputusan A/B.
Computer-use MCP TIDAK tersedia di sesi ini (Tool not found) — verifikasi GUI
dilakukan headless (launch exe + baca log diag). Catatan: pet click-through
(core:window dari pet.html remote) kemungkinan kena isu origin yang sama —
belum pernah terverifikasi di shell nyata.

## UPDATE 2026-09-22 (64) — RENCANA MIGRASI Tauri + Rust core (dokumen saja, belum eksekusi)

User: "sekarang mungkin memigrasikan backend ke rust dan frontend ke tauri.
Planning aja dulu". Hasil: dokumen rencana di-commit; **kode belum berubah.**

**Deliverable:** `docs/ARCHITECTURE-TAURI-RUST.md` — kontrak arah migrasi
backend TypeScript/Bun → Rust core di dalam Tauri, frontend Live2D/Pixi TETAP
TypeScript. Baca dokumen itu sebelum menyentuh apa pun terkait migrasi.

**4 keputusan yang dikunci user (via AskUserQuestion):**
1. Transport **Tauri IPC-first** (bukan HTTP-parity) — tiap modul pindah →
   invoke()/event; frontend berubah per langkah, guard ikut update.
2. **Desktop-first; HTTP = kompatibilitas opsional** (compat adapter axum untuk
   CLI agent + OBS overlay vtuber.html + akses browser/HP). HTTP bukan inti.
3. Sidecar **engine/ (Whisper+SuperTonic) diabsorb in-process** jadi lib Rust —
   hilang port 8330 + spawn/health/restart dance.
4. Renderer (live2d/pixi/MotionRuntime/ParameterArbiter) tetap TypeScript.

**Roadmap Stage 0–5** (detail di dokumen): 0 kontrak+cargo workspace →
1 transport seam `src/client/transport/` + IPC shell + 3 spike (blob IPC,
custom protocol `lumi://` aset model, Channel streaming) → 2 core infra
(config/paths/mode/file-mgr + port motion-dsl/taxonomy ke Rust) → 3 AI
(LlmProvider trait + agent loop/21 tool/gate + bus push + vtuber; compat adapter
paralel) → 4 media+browser+engine absorb → 5 packaging 1 exe + pangkas Bun.

**Invarian yang dijaga** (§5 dokumen): format file data byte-compatible
(config.json/sessions/.agent-memory), replay kursor `since=0`, kosakata SSE
panel, mask apiKey, Model-Agnostic (TANPA id param di Rust), aturan sheet
user>ai, teardown mode, frame-loop TIDAK lewat IPC, klien mandiri
(CLI/OBS/pet), guard legacy ikut update.

**Catatan eksplorasi kunci:** frontend tanpa WebSocket, 1 SSE (panel assistant),
sisanya polling berkursor; ±60 pemanggil `/api/*` di 6+ file (5 pola origin);
`appRoot()` choke point path data; kopling server→client (index.ts import
motion-dsl + motion-taxonomy); klien HTTP mandiri = vtuber.html/pet.html/CLI.

**Belum dikerjakan:** semua Stage. Eksekusi mulai Stage 0 di sesi berikutnya
setelah user membaca rencana.

## UPDATE 2026-09-21 (63) — STT & TTS NATIVE: sidecar Rust (Whisper + SuperTonic) jadi default

User: "supertonic sama whisper bakal jadi default projek ini" + "cloud tetap
diperbolehkan". Dikerjakan penuh, gate hijau.

**Apa yang berubah.** STT & TTS punya engine NATIVE lokal sebagai default,
menggantikan (a) Whisper transformers.js in-browser dan (b) server Python
SuperTonic terpisah (`F:\backup\supertonic-tts`, kini dipensiunkan — tak
dihapus). Provider cloud/browser lama TETAP bisa dipilih; hanya *default*
install baru yang native.

**Crate baru `engine/`** — sidecar HTTP loopback (127.0.0.1) satu proses:
- `POST /tts` SuperTonic (via `ort` 2.0-rc.10 + ONNX Runtime prebuilt) — port
  1:1 dari `supertonic/core.py` (UnicodeProcessor NFKD+regex+`<lang>` +
  lookup unicode_indexer; 4 ONNX: duration→text_encoder→vector_estimator loop
  ×8→vocoder; voice style JSON bersarang di-flatten). Output WAV 44.1kHz.
- `POST /stt` Whisper (via `whisper-rs` 0.16 → whisper.cpp) — PCM 16k → teks.
  Di belakang feature `stt` (butuh cmake+libclang saat build; TTS tidak).
- `GET /health` `/voices`. Modul: `text/wav/tts/stt/main`. 6 unit test crate.
- Verifikasi: TTS "Halo…Lumine…" → 4.46 dtk audio, **RTF 0.32** (3× realtime);
  STT round-trip transkrip terkenali (uji pakai model tiny).

**Integrasi server** (`src/server/engine.ts` baru + `index.ts`):
- Server auto-spawn sidecar saat boot (log `[engine] sidecar native siap`),
  kill-on-exit (SIGINT/SIGTERM/exit). Degrade anggun bila exe belum di-build.
- TTS: cabang provider `"supertonic"` di `ttsAudioFor` (proxy → sidecar).
  `handleTTSOptions` +katalog voice F1..F5/M1..M5. `format`/`lang` masuk
  `TTSConfig` + `ttsCacheKey`.
- STT: endpoint baru `POST /api/stt` — dispatch per-provider (`local`→sidecar,
  `openai`→`/v1/audio/transcriptions` multipart).
- **Model on-demand**: TTS reuse `~/.cache/supertonic3` bila ada, else unduh
  dari HF (SHA dipin, ~385MB); STT unduh `ggml-<model>.bin`. TIDAK dibundel ke
  installer — diunduh saat provider native pertama dipakai. `dist.ts` menyalin
  `engines/live2d-engine.exe` (installer wildcard ikut otomatis).

**Klien** (`voice-input.js` + `app.js` + `index.html` + i18n):
- `voice-input.js` provider-aware: `browser`→Whisper in-tab; selain itu →
  encode WAV16k + `POST /api/stt`. getUserMedia+VAD+anti-echo dipertahankan.
- Default config: `tts.provider="supertonic"`, `stt.provider="local"`
  (`engineModel:"base"`). Dropdown TTS +opsi SuperTonic (i18n id+en parity).

**Privasi (aturan direvisi).** Audio mic: default `local` = loopback 127.0.0.1
(proses lokal, BUKAN cloud); cloud hanya bila user memilih sadar. Frame webcam
tetap tak pernah keluar (tak berubah). Lihat AGENTS.md §5 + header voice-input.js.

**Toolchain (mesin build saja).** Rust+MSVC ada; dipasang cmake 4.4.3 + LLVM
23.1.1 (winget) untuk kompilasi whisper.cpp. User akhir TIDAK pasang apa pun —
dapat exe jadi. `whisper-rs` 0.14 gagal (bindgen size assert) → 0.16. `ort`
butuh feature `std` untuk `commit_from_file`. Nama tensor ONNX asli:
`duration`/`text_emb`/`denoised_latent`/`wav_tts`. Voice style dims `[1,50,256]`
& `[1,8,16]` (data JSON bersarang, bukan `[1,1,256]`).

**Utang teknis (kecil).** Loop request sidecar single-thread (cukup 1 user
lokal). STT kualitas produksi pakai `base` (uji pakai `tiny`). UI dropdown
provider STT belum ada (config via config.json; TTS sudah ada dropdown).

**Gate:** `bun run build` + `tsc` bersih + 536 unit + 416 guard + i18n 11 —
HIJAU. Sidecar crate: `cd engine && cargo build --release --features stt`
(butuh PATH cmake+LLVM, `LIBCLANG_PATH`).

## UPDATE 2026-09-21 (62) — MOTION BUATAN MANUAL UNTUK LUMINE: malu_menoleh + penasaran_miring

User: "suruh kamu aja yang bikinin motion untuk lumine" — bikin langsung,
tanpa LLM.

- **Dua Motion Asset** disimpan via `POST /api/motions` (model-key
  `model_lumine_lumine_lumine_model3_json`) mengikuti MOTION-SYSTEM-SPEC:
  target track role kanonik (`ax/ay/ex/ey/mouthForm/bodyZ`), pulang ke 0 di
  keyframe terakhir, easing halus (ease-out attack, ease-in-out release),
  amplitudo realistis (kepala ≤14°, mata ≤0.5, mouthForm ≥-0.35).
  1. `malu_menoleh` (2.6 dtk) — contoh flagship spesifikasi §33: menoleh
     kiri, tatapan menghindar bawah, mulut terkatup; emo malu 0.95.
  2. `penasaran_miring` (2.2 dtk) — bodyZ+ax miring kanan, mendongak sedikit,
     mata ke atas kanan; emo normal 0.6 / senang 0.45.
- **Verifikasi runtime**: GET library mengembalikan keduanya (5 track/aset);
  registry klien live (`__live2dAgent.listRegistryMotions()`) = 11 entri
  (9 builtin + 2 user) → otomatis masuk katalog LLM (bisa dipilih director
  saat konteks cocok) dan tampil di library Motion Studio. Belum diverifikasi
  visual (pane uji rAF beku) — preview via Motion Studio di browser user;
  file di `data/motions/model_lumine_…/` bisa diedit/tuning.

## UPDATE 2026-09-21 (61) — MOTION STUDIO: ANALISA & GENERATE TAHAN ECHO JUGA

User: motion studio juga ada analisa AI (maksudnya: terapkan perlakuan sama).

- **Dua alur diperkuat** (`handleMotionsAnalyze`, `handleMotionsGenerate`):
  helper baru `extractJSONObjectLoose` (`llm-client.ts`) — pindai semua blok
  `{…}` seimbang (string-aware), kandidat TERAKHIR yang valid menang; template
  instruksi pada echo (`"<emosi>": 0.0-1.0` — bukan JSON valid) otomatis
  terlewati. Parse regex `\{[\s\S]*\}` lama (greedy first-to-last) diganti.
  Echo (marker "balas JSON"/"Kamu menganalisa"/"Permintaan user") → retry
  sekali dengan koreksi eksplisit; gagal dua kali → warning/error menyebut
  "model mengulang instruksi dua kali". Prompt diperkuat: "MULAI balasanmu
  langsung dengan { dan AKHIRI dengan }".
- **Uji**: 5 unit test baru `extractJSONObjectLoose` (sehat, prosa+markdown,
  echo+answer → jawaban menang, echo murni → null, sampah → null). End-to-end
  stub echo→jawaban di server port 8311: `motions/analyze` → deskripsi+tag+
  emotionCompatibility utuh; `motions/generate` → motion asli lolos
  `sanitizeMotionAsset` (track role `ay`, durasi, emosi dinormalisasi) —
  jawaban stub tanpa track ditolak sanitizer sesuai desain. Config
  dikembalikan (aktif: zyloai); stub dihapus; server uji dimatikan.
- Gate: tsc + 536 unit + 416 guard hijau.

## UPDATE 2026-09-21 (60) — "GA ADA SARAN?": LUBANG TRANSPARANSI TERTUTUP — PENYEBAB NYATA KUOTA 429

User: AI ga ngasih saran — bingung itu beneran habis sarannya, salah input,
atau AI ga nyaranin apa-apa.

- **Reproduksi** (endpoint analyze-sheet, data lumine asli, koneksi zyloai/
  muse-glimmer-30b): HTTP 200, 0 preset, TANPA warning — tiga kemungkinan
  tak bisa dibedakan user. Dua sumber lubang: (1) sanitizer membuang saran
  (nama bentrok preset user — lumine punya 33! / id param tak ada) secara
  senyap; (2) kegagalan provider tak selalu sampai ke status.
- **Penyebab nyata kali ini**: zyloai **HTTP 429 — daily token cap 200.000
  token (Basic plan)**, reset tengah malam UTC (±07:00 WIB). Bukan "habis
  saran". Catatan: prompt analisa besar (223 param) — kuota cepat habis.
- **Fix server** (`handleAnalyzeSheet`): sanitizer kini melacak ALASAN —
  duplikat nama (dgn sampel nama) vs id/struktur tidak valid. 0 saran →
  warning eksplisit: "model tidak mengusulkan apa pun (array kosong)" ATAU
  "model mengusulkan N preset tapi SEMUANYA dibuang: X nama sudah dipakai
  (mis. …), Y ditolak…". Saat ada yang lolos → respons membawa `stats`
  (raw/kept/droppedDup/droppedInvalid/dupNames).
- **Fix klien** (`analyzeSheetPresets` + handler Analisa AI): stats disampaikan
  di status — "N saran preset (saran AI)" + "X usulan AI dibuang (…)".
- **Bukti mekanisme entri 59 bekerja di dunia nyata**: log server menunjukkan
  muse-glimmer ECHO lagi → terdeteksi (echo=true) → retry koreksi jalan →
  retry kena 429 → alasannya tampil. Persis rantai yang didesain.
- Gate: tsc + 531 unit + 416 guard hijau. Server user harus RESTART untuk
  kode server baru; halaman di-reload untuk app.js baru.

## UPDATE 2026-09-21 (59) — BALASAN LLM TAK TERSTRUKTUR (MODEL KECIL ECHO PROMPT): PARSER TANGGUH + RETRY KOREKSI

User: beberapa model apalagi model kecil kadang gagal bikin response
terstruktur — bukti: "awalan balasan: Kamu pakar rigging Live2D Cubism..."
= model MENGULANG prompt, bukan menjawab. (Provider kini zyloai — user ganti
dari apinex yang 402, dan saran AI mulai mengalir.)

- **Helper baru `extractJSONArrayLoose`** (`src/shared/llm-client.ts`):
  parse langsung → objek pembungkus ({presets:[…]} dst.) → pindai SEMUA blok
  `[…]` seimbang (string-aware) dan ambil kandidat TERAKHIR yang isinya
  array-of-objects — array pertama pada kasus echo adalah CONTOH FORMAT dari
  prompt, jawaban asli ada setelahnya → salvage terpotong. Dipakai 3 handler:
  analyze-sheet, classify-params, animate-text (pengganti parse regex rapuh).
- **Kebijakan anti-saran-palsu di analyze-sheet**: balasan yang mengandung
  penanda prompt ("PARAMETER TERSEDIA" / awal "Kamu pakar") = echo → array
  contoh dari prompt TIDAK disajikan sebagai saran; handler retry SEKALI
  dengan koreksi eksplisit ("balas HANYA array JSON, mulai dengan ["), hasil
  retry yang dipakai. Retry gagal + echo → warning "model mengulang instruksi
  dua kali"; tanpa echo → warning truncation seperti dulu.
- **Prompt diperkuat** di ekornya: "MULAI balasanmu langsung dengan [ dan
  AKHIRI dengan ] — JANGAN mengulang instruksi".
- **Uji**: 7 unit test baru di `test/llm-client-parse.test.ts` (sehat, prosa/
  markdown, objek pembungkus, echo+answer → jawaban terakhir menang, echo
  tanpa jawaban, terpotong, sampah). End-to-end dengan stub yang PANGGILAN
  PERTAMA meng-echo prompt dan setelah koreksi menjawab array → endpoint
  mengembalikan preset ASLI stub ("Senyum Sedikit"/"Merenggis"), bukan
  "Senang"/"Sedih" dari contoh prompt. Config dikembalikan (aktif: zyloai);
  stub dihapus; server uji dimatikan.
- Gate: tsc + 531 unit + 416 guard hijau.

## UPDATE 2026-09-21 (58) — UJI "APA AI BENER-BENER MEMBERI SARAN?": PIPELINE BENAR, PROVIDER 402

User: coba cari tahu apakah AI benar-benar memberikan saran preset ekspresi/
aksesoris — uji di model lumine dan yang Jepang (神宫白子/面饼0).

- **Fakta provider**: koneksi aktif SATU-SATUNYA (apinex, `free/claude-sonnet-
  4.6`) menolak SEMUA panggilan LLM dengan **HTTP 402** ("model ini hanya untuk
  pelanggan berlangganan") — dua-duanya: `analyze-sheet` maupun
  `classify-params`. Jadi 0 saran masuk, dan yang terlihat user cuma
  "tidak ada saran preset baru" — PENYEBABNYA disembunyikan. Ini penyebab
  nyata "AI tidak pernah memberi saran".
- **UX fix**: `analyzeSheetPresets` kini melempar `data.warning` saat 0 saran
  (handler sudah menampilkan "preset gagal: <penyebab>") — kegagalan provider/
  parse tidak lagi tersamar jadi "tidak ada saran baru".
- **Pembuktian pipeline** (stub openai-compatible lokal port 8399 yang membalas
  12 preset dari parameter NYATA di prompt, dipasang lewat `POST /api/config
  {action:add,setActive}`): lumine → **11 saran (5 emosi, 3 properti, 3
  aksesoris)**; 面饼0 → **12 saran (6 emosi, 3 properti, 3 aksesoris)**; semua
  id valid, sanitizer membuang saran ber-id yang tak ada di model (kerja
  benar), hasil tampil di UI "Saran AI" per kategori. Pipeline server→LLM→
  sanitize→sheet→UI BENAR; kemacetannya murni provider.
- **Bersih-bersih**: koneksi stub dihapus + activeId dikembalikan ke apinex;
  cache `presets.ai` sisa stub dikosongkan di localStorage + disk (cache ini
  memang dibangun ulang tiap Analisa AI; preset USER utuh — lumine 33,
  mianbing 0); `tmp-stub-llm.ts` dihapus; kedua server uji dimatikan.
- Gate: tsc + 524 unit + 416 guard hijau.
- **Untuk user**: supaya Analisa AI benar-benar jalan, ganti provider —
  apinex free/claude-sonnet-4.6 sekarang meminta langganan (402). Tambah
  koneksi baru (Gemini/Groq/OpenAI-compatible lain) di Stream/Connections →
  jadikan aktif, lalu tekan Analisa AI lagi.

## UPDATE 2026-09-21 (57) — DAFTAR PRESET DIPERSIMPel: GRUP + SARAN AI YANG DIPAKAI HILANG DARI DAFTAR

User: daftar preset ribet & membingungkan — saran AI yang sudah dipakai tetap
menumpuk, tak terbaca sudah dipakai atau belum.

- **Akar keruwetan**: `applyAISuggestion` menyalin saran ke `presets.user`
  tanpa menyentuh cache `presets.ai`, dan `resolvePresets` menandai saran
  yang namanya sudah diambil user sebagai `suggestion:true` → baris
  "tertutup AI" menumpuk selamanya sebagai duplikat preset milikmu. Badge
  per-baris ("milikmu"/"saran AI"/"tertutup AI") menambah kebisingan, dan
  tab kategori menghitung baris tersembunyi ikut saran tertutup.
- **Redesign `paintPresetList`**: tiga kelompok — header "Preset kamu" →
  baris user (Terap · Edit · Hapus), header "Saran AI" → saran terbuka
  (Pakai), lalu saran yang SUDAH dipakai diringkas jadi satu baris toggle
  "N saran AI sudah dipakai — jadi preset milikmu" (expand = nama saja,
  dim, tanpa tombol — Pakai di sana akan menimpa preset user dengan nama
  sama, jadi sengaja tak ada). Badge per-baris dihapus (header grup yang
  bicara); hasil Pakai langsung HILANG dari daftar saran + count toggle
  naik. Tab kategori kini menghitung baris TERLIHAT saja.
- **Tombol "Coba" dihapus**: implementasinya identik "Terap" (dua-duanya
  `releasePresetPreview` + `applyPreset`; tooltip "pratinjau tanpa
  mengunci" tidak pernah ada implementasinya) — satu aksi satu tombol.
- **i18n**: kunci baru `sheet.group.mine`, `sheet.group.ai`,
  `sheet.closedSummary` di kedua kamus (keys badge lama dipertahankan —
  tak lagi dipakai render).
- **Verifikasi runtime** (Browser Use, round-trip tanpa jejak): struktur
  grup + toggle "4 saran AI sudah dipakai" tampil; Pakai "Jijik" → pindah
  ke "Preset kamu" (17→18, saran 5→4, toggle 4→5, hilang dari saran);
  Hapus preset uji (confirm di-override untuk uji) → pulih 17/5/4, saran
  "Jijik" terbuka kembali. Gate: build + tsc + 524 unit + 416 guard hijau
  (guard Reset Pose `textContent` tetap utuh).

## UPDATE 2026-09-21 (56) — PANEL PARAM: PILIHAN MODE POSE + FIX HITUNG MUNDUR 1000→10 DTK

User: countdown di panel parameter slider — bisa dipilih tidak? "balik gerak
otomatis setelah 10 dtk tanpa perubahan" vs "bener-bener tidak boleh gerak".

- **Bug tertangkap**: hitung mundur freeze ternyata `remaining = 1000` (≈16,7
  mnt) padahal label & i18n `pn.sub` menjanjikan 10 dtk — setelah geser slider,
  model beku belasan menit, bukan 10 detik. Diperbaiki jadi konstanta
  `PN_FREEZE_SECONDS = 10`.
- **Pilihan mode pose** (permintaan user): checkbox "Mode pose — model tetap
  diam selama panel ini terbuka" di popup param (`#pn-pose-mode`). Dicentang →
  freeze persisten (teks "❄ mode pose"), tanpa hitung mundur; panel ditutup →
  unfreeze (jalur `closeParamNotesPopup` sudah ada). Tidak dicentang → perilaku
  lama yang benar: balik gerak 10 dtk setelah geseran terakhir. Preferensi
  persist di localStorage `l2d_pn_pose_mode` (preferensi UI, bukan config
  karakter). Ganti mode saat sedang beku langsung mengalihkan teks/hitung
  mundur (`pnFreeze()` re-freeze); slider onInput & unfreezeMaybe kini lewat
  `pnFreeze()` yang membaca mode.
- **i18n**: teks freeze yang hardcoded di-i18n-kan — kunci baru `pn.poseMode`,
  `pn.freezePose`, `pn.freezeAuto` ({t} detik), `pn.freezeRestore` di KEDUA
  kamus.
- **Verifikasi runtime** (Browser Use, server sendiri — server user sedang
  mati): auto mode "10 dtk"→"8 dtk"→"✓ gerak idle aktif kembali" tepat 10 dtk;
  pose mode tetap beku setelah 12 dtk + persist "1"; uncheck saat beku →
  langsung hitung mundur; tutup panel → unfreeze. Preferensi user dikembalikan
  ke "0" (auto) setelah uji; tak ada Simpan yang ditekan. Gate: build + tsc +
  524 unit + 416 guard hijau.

## UPDATE 2026-09-21 (55) — PANEL KELAKUAN → INISIATIF: PROFIL "MATI" + FRAMING COMPANION PROAKTIF

User: panel kelakuan terlalu ribet, bingung cara mematikan, dan semantiknya
salah — seharusnya "kapan dia jadi proaktif companion", bukan "dia nanya tiap
N menit".

- **Redesign UI** (`static/index.html` + `app.js` initBehaviourPanel + kamus
  id/en): judul "Kelakuan" → "Inisiatif"; profil kini 4 tingkat berurutan
  **Mati · Tenang · Sedang · Hidup** — Mati = ketiga boolean false (brain
  sudah menolak idle/away/return lewat `getEvents()`; tak ada perubahan
  runtime). Hint/label direframe ke semantik inisiatif ("Bicara sendiri saat
  kamu diam", "Pamit saat kamu pergi", "Jeda antar inisiatif", dsb.); tombol
  Simpan dipendekkan jadi "Simpan".
- **Sorotan profil kini dari state form** (`updateProfileHighlight(src)`,
  dipisah dari `paintForm`): uncheck ketiga checkbox secara manual langsung
  menyorot Mati. Listener `change` checkbox memanggil highlight dari
  `readForm()` — BUKAN paintForm (yang menimpa form balik ke config live dan
  membatalkan edit manual — bug versi pertama yang tertangkap saat uji).
- **Countdown jujur**: semua inisiatif off → "Inisiatif mati — karakter hanya
  merespons kalau kamu bicara dulu." (bukan hitung mundur yang menyesatkan).
  Countdown tetap membaca config LIVE (perilaku berjalan), bukan form belum
  disimpan.
- **Bug bonus tertangkap**: max slider `beh-quietMs/idleMs/idleRepeatMs`
  600000 (10 mnt) sementara profil Tenang memakai 1800000 (30 mnt) — nilai
  prefill ter-clamp slider, jadi Simpan dari slider akan MERUSAK config 30
  menit jadi 10 menit dan sorotan profil tak pernah cocok. Max dinaikkan ke
  3600000.
- **Verifikasi runtime** (Browser Use, lewat server dev user yang sedang
  jalan — EADDRINUSE, tak ada server kedua): 4 profil tampil berurut; Mati →
  semua checkbox off + tersorot; check/uncheck manual → sorotan mengikuti
  (Tenang/Mati); slider menampilkan "30 mnt" tanpa clamp; countdown off
  branch tampil & pulih. Gate: tsc bersih, 524 unit + 416 guard hijau.
  Config user TIDAK diubah (uji tanpa menekan Simpan).

## UPDATE 2026-09-20 (54) — SLIDER PANEL PARAM HAMPIR TAK BERPENGARUH: RANGE SHEET ESTIMASI SEJAK MIGRASI

User: geser slider di panel parameter, perubahan karakter tidak sebesar yang
seharusnya.

- **Akar masalah.** Slider panel param membaca `min`/`max` dari
  `sheet.params`. Sejak migrasi Pixi 8, `inspectModel()` GAGAL mengukur range
  secara diam-diam → sheet jatuh ke rentang tebakan (`rangeSource: estimated`;
  id yang tak persis cocok PARAM_META dapet netral -1..1). Slider -1..1 yang
  menulis ke param rig ber-range lebat (mis. ±30, ±10) hanya menggerakkan
  3–10% jangkauan → "perubahan kecil". Lumine (223 param) & 神宫白子/面饼0
  (214 param) kena penuh — semuanya estimated; ren lolos karena sheet-nya
  terukur 2026-09-18 (era stack lama, `core-arrays`). Jalur role/emosi TIDAK
  terdampak — `detectModelCapabilities` mengukur live lewat facade.
- **Kenapa pengukuran gagal.** Dua cabang pengukur `inspectModel()` mati
  berdua di stack baru: cabang "core-arrays" butuh `.parameters` mentah
  (tidak di-expose wrapper), cabang "wrapper-accessors" mengecek accessor
  (`getParameterCount`/`getParameterMinimumValue` dsb.) pada backend
  coreModel — padahal backend hanya pintu tulis; accessor range ada di
  wrapper hasil `coreModel.getModel()`. Kedua kondisi false → rawParams
  kosong → fallback estimasi tanpa jejak di UI.
- **Fix.** (1) `src/live2d/view/Live2DView.ts`: wrapper `getModel()` kini
  expose `getParameterIds()` (plural, string[]). (2) `static/js/app.js`
  `inspectModel()`: cabang "wrapper-accessors" membaca dari `gm` (hasil
  `getModel()`) dengan fallback `cm` (kompatibel stack lama), plus fallback
  id per-indeks bila plural tak ada — kegagalan ukur tak lagi senyap.
- **Verifikasi runtime** (Browser Use): re-inspeksi 面饼0 & lumine dengan
  build baru → `rangeSource: wrapper-accessors`, 0 param estimated,
  ParamAngleX -30..30, Param77 面饼0 -10..10, ParamEyeLOpen lumine 0..2
  (estimasi lama 0..1 — slider hanya mencakup separuh jangkauan). Panel
  param menampilkan slider [-30..30] dst. Sheet keduanya sudah re-scan di
  disk — user tidak perlu apa-apa. userNote aman (merge carried notes
  dijaga guard).
- **Catatan lingkungan uji:** pane Browser Use IAB punya rAF beku
  (0 tick/600 ms) — render loop Live2D tidak jalan & loader tak tersembunyi
  di pane tersebut (loader disembunyikan callback ticker rAF). Verifikasi
  gerak visual penuh tetap di browser user; jalur tulis param
  (AppWriteUpdater order 900) tidak berubah dan sudah terbukti dipakai
  emosi/liveliness/lipsync.
- Gate: `bunx tsc --noEmit` bersih, 524 unit + 416 guard hijau, build sukses.

## UPDATE 2026-09-20 (53) — OVERLAY OBS PUTIH BURAM: preserveDrawingBuffer (koreksi entri 52)

User: di OBS overlay tetap putih, tidak transparan. Entri (52) mengklaim
"dikonfirmasi" — TAPI ujinya cuma on-screen (jalur composite on-frame). OBS
Browser Source (CEF) & alat capture membaca drawing buffer **DI LUAR** siklus
frame → jalur berbeda, hasil berbeda. Itu sebab (52) lolos padahal OBS putih.

- **Akar masalah.** Pixi diinit `preserveDrawingBuffer: false`. Saat konsumen
  membaca backbuffer di luar frame, WebGL mengembalikan buffer yang di-auto-
  clear ke `gl.clearColor` **saat itu** — dan nilainya tertinggal PUTIH
  `[1,1,1,1]` dari clear buffer mask Cubism (`cubismrenderer_webgl.ts:220`,
  `_clearedMaskBufferFlags`). On-screen (composite on-frame) transparan; OBS
  dapat putih.
- **Diagnosis terukur (Browser Use + decode PNG).** Latar body magenta
  `#ff00ff`. Canvas TAMPIL → screenshot **90% putih, 0% magenta**. Canvas
  `display:none` → **100% magenta** (screenshot IAB jujur; CSS transparan
  bekerja). Canvas WebGL transparan buatan tangan (`clearColor 0,0,0,0`) di
  atas magenta → **95% magenta + biru** (readback off-frame jujur pada canvas
  transparan). `draw()` manual + readPixels in-frame → 91/100 sampel alpha≈0 +
  9 piksel warna karakter alpha 255 (output WebGL SEBENARNYA transparan+model).
  `COLOR_CLEAR_VALUE` = `[1,1,1,1]`. Paksa runtime `gl.clearColor(0,0,0,0)` →
  screenshot berubah **90% putih → 89.7% magenta+model**. Terbukti.
- **Fix `src/live2d/view/Live2DView.ts`** (2 lapis): (1) init Pixi
  `preserveDrawingBuffer: true` — buffer yang dibaca OBS = frame yang digambar.
  (2) `pixiStateReset` reset `gl.clearColor(0,0,0,0)` tiap frame (setelah
  Cubism menggambar) — auto-clear off-frame jadi transparan, bukan putih.
- **Verifikasi build baru** (tanpa patch runtime): body magenta + model penuh
  → screenshot **89.8% magenta tembus + ~10% piksel model**. `bunx tsc
  --noEmit` bersih, `bun run build` sukses (live2d-view.mjs regenerate).
- **Aksi user di OBS:** refresh cache Browser Source (Properties → Refresh
  cache of current page) atau hapus+tambah ulang, supaya OBS memuat
  live2d-view.mjs baru — kalau tidak, masih pakai versi lama yang putih.
- CATATAN pelajaran: uji transparansi overlay HARUS lewat readback off-frame
  (screenshot/OBS), bukan cuma mata di layar app. Keduanya jalur berbeda.

## UPDATE 2026-09-20 (52) — OVERLAY OBS: TRANSPARANSI DIKONFIRMASI + TAMPILAN MURNI KARAKTER

User tanya: overlay OBS bisa transparan? dan minta overlay TIDAK menampilkan
donasi/livechat (ditangani sumber/layar lain).

- **Transparansi — sudah benar, dikonfirmasi.** Stack view (`Live2DView`)
  init Pixi `backgroundAlpha: 0` + `renderer.setClear(null)`; canvas WebGL
  `getContextAttributes().alpha === true`. Uji browser: latar body diberi
  warna solid (#ff0033 / #0a5) → tembus di area kosong, karakter di atasnya.
  Hitam yang terlihat user = kanvas OBS kosong (belum ada source di bawah
  Browser Source), BUKAN bug overlay.
- **Tampilan murni karakter (default).** `vtuber.html`: bubble ucapan &
  alert donasi kini **opt-in** — default disembunyikan (early-return di
  `bubble()`/`donationAlert()`), aktifkan per elemen dengan `?bubble=1` /
  `?alerts=1`. Teks status debug (`#status`) hanya tampil di `?hud=1`
  (`body.hud`). Suara TTS + lipsync TIDAK terpengaruh — karakter tetap
  bicara & mulut bergerak walau teks disembunyikan. URL OBS = `/vtuber.html`
  polos → hanya karakter transparan.
- Verifikasi visual: URL bersih + donasi diinject → bubble/alert/status
  semua `display:none`, hanya karakter (latar uji tembus); `?bubble=1&
  alerts=1` → `donationAlert` jalan (teks alert terisi tiap donasi mock).
- vtuber.html statis (tak lewat bundle) — tak perlu build. Gate: tsc bersih,
  524 unit + 416 guard 0 gagal. Config user di-backup+restore (uji start
  mock menulis section vtuber → dipulihkan bersih).

## UPDATE 2026-09-20 (51) — KONEKSI STREAM VTUBER TERSIMPAN (API KEY YT DSB. TAK DIKETIK ULANG)

Laporan user: API key YouTube + video ID tidak kesimpen tiap sesi (video ID
memang ganti tiap stream, tapi key/persona harus diketik ulang). Betul —
`vtuberStart` dulu hanya menyimpan cfg di memori runtime; form mulai kosong
tiap reload/restart. Ditambah persistensi bergaya `connections`/`tts`:

- **`config.ts` `saveVtuberConn(conn)`** (+ tipe `Config.vtuber` opsional):
  merge per-field ke `config.json` section `vtuber`. Start provider lain
  (mock) TIDAK menghapus channel/key YouTube tersimpan. apiKey masked/kosong/
  placeholder ditolak (pertahankan yang asli) — sama seperti connections.
- **Server**: `handleVtuberStart` mengisi apiKey asli tersimpan saat body
  masked/kosong (start YT setelah reload tak gagal "key wajib"), lalu
  `saveVtuberConn(body)`; `handleVtuberConfig` juga persist perubahan live.
  Route baru **`GET /api/vtuber/conn`** mengembalikan koneksi tersimpan
  dengan **apiKey DIMASK** (`config.maskKey`) — key asli tak pernah balik ke
  HTTP (pola sama dengan `/api/config`).
- **Klien** (`mode-runtime.js`): saat panel VTuber nyala, prefill form dari
  `GET /api/vtuber/conn` — provider/channel/videoId/persona/cooldown/respond;
  apiKey ditaruh sebagai **placeholder masked** (field value kosong, bukti key
  ada). Start dengan field kosong → server pakai key tersimpan.
- Verifikasi (server uji 8317, config user di-backup lalu di-restore):
  API-level — start YT key dummy → `config.json` simpan key ASLI, `GET
  /conn` mengembalikan `AIzaSy••••••••fXYZ`; start ulang field kosong →
  `{ok:true}` + key disk tetap utuh. Visual — reload → Stream Settings
  terisi: provider youtube, Video ID `testVID123`, API Key placeholder
  masked, persona/cooldown/respond kembali. Config user diverifikasi bersih
  (tak ada section vtuber) setelah restore.
- Gate: tsc bersih, build OK, 524 unit + 416 guard 0 gagal.

CATATAN: config.json menyimpan apiKey plaintext (sama seperti connections &
tts yang sudah ada) — bind loopback default + config.json tak pernah
disajikan via HTTP tetap berlaku.

## UPDATE 2026-09-20 (50) — LIPSYNC KADANG BEKU DI TENGAH BICARA: 2 AKAR MASALAH DIPERBAIKI

Laporan user: "kadang harusnya masih berbicara tapi mulutnya ga gerak, mungkin
ketahan ekspresi". Dugaan user BENAR sebagian — ada dua mekanisme, dua-duanya
diperbaiki di `app.js`:

1. **Timer estimasi memotong `state.talking` lebih awal.** Gerbang semua
   lipsync adalah `state.talking`, yang dimatikan timer estimasi
   `panjangTeks × 75ms` tanpa mekanisme perpanjangan. Audio yang lebih lambat
   dari estimasi (suara lambat, `ttsRate` < 1, jeda antar-segmen remote,
   `hematRequest` bundel besar) → mulut beku di tengah suara. Fix:
   - provider lipsync kini memprioritaskan **amplitudo audio nyata**
     (AudioLipSync) selama elemen `<audio>` benar-benar diputar — tidak
     lagi digantungkan pada `state.talking`;
   - timer mulut kini punya `state.extendMouth(ms)` (re-arm) dan di-re-arm
     per segmen remote (`durasi segmen + margin`) serta dari TTS browser
     `u.onstart`/`u.onboundary` (progress nyata); `onend`→markDone tetap
     penutup sebenarnya.
   Verifikasi runtime (browser, TTS browser): teks 317 karakter (estimasi
   lama 23,8 dtk) — jejak `ParamMouthOpenY` (150 ms/sample) bergerak terus
   TANPA celah beku sampai 27,8 dtk (~4 dtk melewati batas lama), lalu menutup
   bersih; screenshot mid-speech menunjukkan mulut terbuka.
2. **Template emosi sintetis menahan `mouthOpenY`.** `EMOTION_ROLE_TEMPLATES`
   ("senang" 0.35, "kaget" 0.8) masuk `state.emoTarget` yang ditulis-SET tiap
   frame DI SETELAH framework lipsync (AppWriteUpdater urutan 900) → mulut
   terpaku mendekati nilai emosi selama emosi aktif. Fix: lapisan emosi
   melewatkan sumbu buka-mulut (`mouthOpenY`/`mouthOpenX`) selama bicara /
   audio diputar — bentuk (mouthForm) tetap milik emosi, buka-tutup milik
   lipsync. (Kasus ini jarang di model user karena preset sheet menang atas
   sintetis, tapi tetap jebakan untuk model tanpa preset.)

Catatan: server TTS user (127.0.0.1:7788) mati saat pengujian — jalur yang
diverifikasi adalah TTS browser; jalur amplitudo-nyata (remote) dapat
perbaikan yang sama di provider dan butuh verifikasi saat server 7788 hidup.
Gate: tsc bersih, 524 unit + 416 guard 0 gagal.

## UPDATE 2026-09-20 (49) — AUDIT UI PAKAI BROWSER USE: 5 BUG DITEMUKAN & DIPERBAIKI (VERIFIKASI VISUAL)

Sesi ini menjalankan audit UI menyeluruh lewat browser automation (klik satu per
satu + verifikasi screenshot) di instance terpisah port 8317, merespons laporan
user "fitur dipencet tidak menunjukkan perubahan". Server dev user 8310/8312
(kode lama) dimatikan atas permintaan user.

**Hasil audit — fitur yang TERBUKTI JALAN (bukti visual):** pindah 4 mode +
teardown VTuber; VTuber start/stop/feed mock/balasan+bubble+lipsync; operator
say saat runtime jalan; overlay OBS render+bicara; pet open/close + pet.html;
chat LLM live, quick-reply, Clear, Brain Mode, Mic (listening); rail projek
(new/delete/switch sesi, dialog confirm); panel kontrol 4 tab; Inspeksi Model
→ sheet tersimpan; preset Try (senang vs sedih beda visual nyata); tes ekspresi
exp3 (collar biru→hijau); daftar motion + Try (probe playMotion result:true);
drag/zoom/dobel-klik reset panggung; profil perilaku; toggle kamera; Advanced
mode; ganti bahasa id↔en (sweep i18n penuh); loop agent assistant live (tool
call + jawaban).

**Bug diperbaiki (semua diverifikasi ulang visual):**
1. **Desync mode chat→stage (mode-runtime.js)** — klien POST `/api/mode`
   `{mode:"chat"}` yang tidak dikenal server (hanya "stage") → 400 ditelan →
   `active` server nyangkut di mode lama (pelanggaran invarian MODES.md).
   Fix: petakan chat→stage saat POST. Terverifikasi: kembali dari VTuber kini
   `active: stage` + runtime berhenti.
2. **Title tombol rail Assistant tertimpa status (projek.ts)** — tooltip nama
   mode ("AI Assistant — Local Agent") ditimpa kata status ("off"/"ready")
   sehingga accessible name kehilangan nama mode. Fix: gabungan
   `top.tabTip.assistant + " · " + status`.
3. **String hardcoded Indonesia di UI English (app.js)** — (a) status profil
   perilaku `'Profil "Hidup" dipakai — tekan Simpan.'` → kunci baru
   `beh.profil.applied` (id+en); (b) `fmtMs` "detik" → `__t("live.sec")`.
   Terverifikasi visual: `Profile "Lively" loaded — press Save.` + `15 s`.
4. **Send Operator no-op senyap (mode-runtime.js)** — runtime mati → error
   ditelan, tanpa umpan balik apa pun. Fix: elemen `#vt-op-status` di popup
   Stream Settings + kunci `vt.operatorSent` / `vt.operatorNotRunning`
   (id+en), auto-clear 4 dtk. Terverifikasi visual kedua jalurnya.
5. **Ekspresi balasan overlay OBS (brain.ts + vtuber.html)** — entri (48)
   meninggalkan `expressReply` no-op di overlay. Kini `expressReply` punya
   fallback stack-overlay: aset NATIVE model dibaca dari facade saat runtime
   (`.exp3` cocok fuzzy kata emosi, grup motion cocok kata gesture) — tanpa
   id bernomor, tidak menulis param di luar pintu resmi. **BATAS JUJUR**:
   ketiga model user TIDAK punya aset native bernama semantik (lumine:
   grup motions kosong, exp3 hanya collar_*) → jalur ini no-op anggun untuk
   mereka; jalur preset-param user di overlay masih follow-up (butuh
   kebijakan updater/pemulihan nilai default di stack view). Overlay
   terverifikasi tetap utuh (bubble+lipsync, panggilan tidak melempar).

**Temuan lain yang dikonfirmasi BUKAN bug:** siluet hitam sesaat di pet.html =
frame pertama sebelum tekstur (normal, hilang setelah load); preset Try terasa
mati = klik pertama salah baris (container match terlalu luas) — dengan
locator presisi per-baris terbukti bekerja; riwayat VTuber tampil di log
Stage+Chat = persist riwayat, runtime benar berhenti.

**Sisa follow-up:** jalur preset-param overlay (di atas); uji pet klik-tembus
Tauri nyata (butuh build:pet); koneksi Twitch/YouTube asli; server dev user
WAJIB direstart untuk mendapat semua fix sisi klien+server.

Gate: tsc bersih, build OK, **524 unit test + 416 guard — 0 gagal** (2 fail
"pra-eksis" entri 48 ternyata butuh `data/model` yang ada di mesin ini — hijau
juga). i18n parity hijau (kunci baru ada di dua kamus).

## UPDATE 2026-09-19 (48) — SISA OPSIONAL REWORK: UI ANTREAN TASK + CANCEL PER-TASK + EKSPRESI VTUBER

Tiga item opsional terakhir `ARSITEKTUR-GAP.md` (§7 ekspresi/motion balasan,
UI daftar antrean task, cancel per-task) diselesaikan. Semua sisi server sudah
ada sejak Fase 5/6 — kerja sesi ini menyambungkan klien.

- **UI daftar antrean task (§9)** — `assistantStatus` sudah mengekspos
  `activeTask`/`parkedTasks` (Fase 6); klien belum membacanya. Ditambah:
  `AssistantStatus` (`panel/api.ts`) kini punya field itu; `view.renderQueue`
  (`panel/view.ts`) — kotak `.as-queue` baru (di bawah TASK) menampilkan
  parked FIFO + posisi; `refreshStatus` (`panel/panel.ts`) memanggilnya tiap
  poll. Kotak hilang saat antrean kosong.
- **Cancel per-task (§11)** — server `assistantCancel(taskId)` + route
  `/api/assistant/cancel` (body `{taskId}`) sudah siap (Fase 6). Ditambah:
  tombol ✕ per-baris antrean → `view` deps `onCancelTask` →
  `panel.cancelTaskById(taskId)` POST `{taskId}` → target `parked` dikeluarkan
  spesifik tanpa mengganggu slot aktif. i18n baru (id+en): `as.queue`,
  `as.queue.count`, `as.queue.cancel`, `as.queue.cancelSent`.
- **Ekspresi/motion balasan VTuber (§7)** — balasan VTuber tadinya teks-murni
  (`__debugSpeak` = audio saja). Ditambah PURE `deriveReplyActions(text)`
  (`directive-parser.ts`): teks polos → emosi+gesture generik model-agnostik
  (`guessEmotion` + `EMOTION_GESTURE_FALLBACK`); directive eksplisit LLM
  dihormati (gabung jadi satu set). Brain dapat method publik `expressReply`
  (visual-SAJA: reuse `applyActions` untuk resolusi ekspresi model-agnostik
  "param/native/clip"; TIDAK bicara/tulis chat/sentuh aiLock → tak bentrok
  jalur audio). `mode-runtime.js` vtuber `speak()` memanggil
  `window.__agent.expressReply(text)` sebelum `__debugSpeak`. Scheduler server
  TIDAK diubah (prompt tetap; `deriveReplyActions` yang menebak) — perubahan
  murni klien-aditif, satu layer.
- **CATATAN JUJUR — belum diverifikasi runtime**: efek visual VTuber
  (model + TTS + gerak nyata) belum diuji live sesi ini. Overlay OBS
  (`vtuber.html`) memuat bundle → `window.__agent` ada, TAPI `expressReply`
  jadi **no-op** di sana karena `l2d()` = `window.__live2dAgent` (driver
  app.js) tidak dimuat di stack overlay minimal — jadi saat OBS on-air (app
  utama tidak bicara), ekspresi belum tampil. Wiring overlay ke API
  `window.__live2dView` (ekspresi/motion stack baru) + verifikasi OBS =
  follow-up. UI antrean juga baru diuji lewat tsc/build/unit, belum visual.
- Gate: **tsc bersih, build OK, +5 unit test (`test/reply-actions.test.ts`)**.
  `bun run test` = 518 pass / 2 fail; **2 fail PRA-EKSIS** (server-integration:
  motion-taxonomy standalone + static CJK fallback — butuh model di `data/`
  yang di-gitignore; diverifikasi gagal juga di baseline tanpa perubahan
  sesi ini). i18n parity + panel + worker-tasks + directive-parser hijau.

## UPDATE 2026-09-19 (47) — FASE 6 + REWORK ARSITEKTUR LENGKAP: WORKER TASK IDENTITY (COMMIT)

§9–12 terimplementasi — **seluruh rework arsitektur behavior (Fase 2–6)
selesai.**

- **`state.ts`**: `WorkerTask {taskId, prompt, status, createdAt}` +
  `activeTask`/`parkedTasks` (FIFO cap 20)/`pendingReplacement`/`nextTaskSeq`.
- **`loop.ts`**: `AskResult.paused` — pause approval TIDAK melepas busy
  (finally bersyarat); slot tetap milik task (§10).
- **`assistant.ts`** (facade pemilik lifecycle): `assistantAsk` saat slot
  sibuk → **PARK** (bukan ditolak) — prompt tidak menyentuh history sebelum
  jalan (pencampuran dua task dalam satu history mati); penuh (20) → tolak
  eksplisit. `runTask`/`finalizeTask`/`drainNext` — slot kosong → replacement
  (§12) dulu, lalu antrean FIFO, otomatis. `assistantCancel(taskId?)` (§11):
  running kooperatif, paused terminal langsung, parked dikeluarkan spesifik;
  tanpa id = task aktif (kompat). `assistantModify` (§12, route baru):
  replacement mewarisi posisi. `assistantResolveApproval` melanjutkan task
  yang SAMA (tanpa gerbang park); approval yatim ditolak eksplisit. Reset
  history saat busy kini ditolak; pindah sesi saat pause ikut ditolak (§14).
- **Klien**: `done` SSE membawa `parked/paused/taskId/position` — panel
  menampilkan status "diantrekan" (i18n `as.task.parked` id+en), CLI satu
  baris info antrean; tombol cancel panel tetek task aktif (kini juga saat
  menunggu izin). Route: cancel menerima `{taskId}`, baru `POST
  /api/assistant/modify`.
- Test: `test/worker-tasks.test.ts` (13 kasus: park+isolasi history, cap-20
  tolak eksplisit, pause ownership, cancel per-task 3 kelas, modify 3 kelas
  + replacement running, drain E2E mock berurutan, status). Test lama
  (agent-cancel/sessions/panel/parity) hijau tanpa perubahan kontrak yang
  tidak memang berubah.
- Gate: tsc bersih, build OK, **515 unit (+14) + 416 guard, 0 fail**.
- Runtime verify DIJALANKAN (instance terpisah 8312, panel Assistant + API,
  LLM asli user yang ternyata jalan di sela 429): (a) task minta write_file
  → loop pause + **slot tetap dipegang** (awaiting_approval, busy) §10;
  (b) task kedua dikirim dari PANEL saat pause → **ter-park**, panel
  menampilkan "Queued — another task is running (position 1)" §9;
  (c) approve → t_1 lanjut → file tertulis + self-verify → **t_2 nyala
  otomatis** dan terjawab benar (drain FIFO) — VERIFIKASI INI MENANGKAP BUG:
  resume lupa reset status ke "running" sehingga slot nyangkut + antrean
  tak di-drain; diperbaiki + test regresi (14 kasus); (d) cancel task
  paused → `{target:"paused"}` → slot lepas seketika + "Dibatalkan oleh
  user." §11.

**INSIDEN DATA (jujur dicatat):** test worker-tasks/agent-cancel (dan lama:
agent-undo via `agentRunApproved`→`pushMsg`) menulis history test ke
`data/assistant-sessions.json` milik USER — sesi aktif user (33 pesan)
tertimpa dan HILANG PERMANEN (percobaan pemulihan via memori server 8310
gagal karena server itu menjalankan kode lama tanpa saveSession-dulu).
Perbaikan: store sesi kini LAZY + test mengisolasi lewat
`LIVE2D_TEST_SESSION_ROOT` (state.ts + assistant.ts + 3 file test) —
hash file sesi user identik sebelum/sesudah `bun run test` (terverifikasi).
Residu test di file sesi user sudah dibersihkan; user mendapat sesi fresh
(workDir kini appRoot, bukan temp polusi lama). Pelajaran: jalur persist
apa pun yang tersentuh test WAJIB diisolasi — bukan hanya config.json.

**Penutup rework**: Fase 2 (speech policy) + 3 (companion concurrency) +
4 (proactive gate) + 5 (VTuber behavior engine + overlay OBS) + 6 (worker
task identity) — semua baris gap behavior `ARSITEKTUR-GAP.md` ✅. Sisa
opsional: ekspresi/motion balasan VTuber, UI daftar antrean task, cancel
per-task dari UI panel. REGRESI runtime 8310 milik user: server harus
direstart untuk mendapat semua fase (kode klien sudah ter-build).

## UPDATE 2026-09-19 (46) — FASE 5 REWORK ARSITEKTUR: VTUBER BEHAVIOR ENGINE + OVERLAY OBS (COMMIT)

§7 terimplementasi + regresi overlay OBS diperbaiki. Keputusan arsitektur:
behavior VTuber pindah dari dua klien ke **SATU scheduler di server** —
race app-vs-overlay mati dari akarnya, antrean tahan reconnect.

- **`src/server/vtuber-scheduler.ts`** (baru, deps di-inject): audience =
  suppression (dedup user+teks 30 dtk + cooldown, tidak antre); donation =
  FIFO cap 20, penuh → item BARU ditolak + feedback system feed (tidak
  silent-evict); operator = kelas sendiri FIFO-20 (masuk walau respond
  mati); satu active slot donation+operator, item aktif tidak dipreempt,
  slot bebas → donation dulu (precedence §7); LLM role "chat" server-side;
  slot ditahan estimasi bicara (`src/shared/speech-timing.ts` baru —
  `estimateSpeechMs` pindah shared, brain re-export).
- **`vtuber.ts`**: intake semua provider + mock-event → scheduler; cfg
  {persona, cooldownMs, respondChat, respondDonation} default MATI (test
  lama bebas jaringan); `vtuberSetConfig` live, `vtuberOperatorSay`
  (echo + antrean), status + antrian via /api/mode. Route baru:
  POST /api/vtuber/config, /api/vtuber/operator.
- **Klien jadi render+speech**: mode-runtime vtuber dibersihkan
  (maybeRespond/askLLM/persona-local/speakQueue/vtuberAgentSay dihapus);
  start mengirim config behavior; perubahan form → POST config live;
  input Operator baru di drawer (i18n vt.operatorPh/Send, id+en; key
  vt.donatePrompt/chatPrompt/aiFail dihapus dari kedua kamus — prompt kini
  protokol server berbahasa Indonesia).
- **`vtuber.html` MIGRASI STACK BARU** (fitur OBS yang rusak sejak
  pixi-live2d dihapus): pola pet.html (importmap pixi8.mjs +
  live2d-view.mjs + __live2dView.loadModel); HUD sisa tombol Salin URL;
  bibir browser-TTS via setLipsyncProvider (role-resolved) menggantikan
  mouthPulse hardcode ParamMouthOpenY; overlay bicara event agent on-air.
- Test: `test/vtuber-scheduler.test.ts` (12 kasus: suppression/dedup/
  cooldown, FIFO cap + tolak eksplisit, precedence, no-preempt, respond
  flags, LLM gagal, balasan kosong, stop, persona). Gate: tsc bersih,
  build OK, **501 unit (+13) + 416 guard, 0 fail**; vtuber-inject lama
  hijau tanpa perubahan.
- Runtime verify (instance kedua port 8312, server 8310 user tak
  disentuh): balasan agen ASLI masuk feed ("Wah, Ayu! Makasih banyak
  donasinya 50 ribu…" — LLM user sukses di sela 429); precedence live —
  3 balasan donasi diproses sebelum operator; error LLM (402/429) jadi
  feedback system + slot tidak pernah nyangkut; overlay /vtuber.html
  muat model di stack baru, HUD hidup, heartbeat → overlay:true (app
  yield). Catatan: provider LLM aktif user sedang kena 402 (daily
  check-in) / 429 rate-limit — itu state akun mereka, bukan regresi.

## UPDATE 2026-09-19 (45) — FASE 4 REWORK ARSITEKTUR: PROACTIVE POLICY GATE (COMMIT)

§17–18 terimplementasi — semua di `brain.ts`, tanpa sentuhan app.js/server:

- **Gate `proactiveAllowed()` di `reactEvent()`** (satu muara SEMUA event
  proaktif: idle/user_left/user_returned/mood:* — jalur away/return di
  `setPresence` ikut tertangkap), SEBELUM LLM/director/speech/side-effect:
  1. Toggle Mode Otak dibaca dari DOM (`#toggle-brain` — elemen yang sama
     dengan jalur chat, tidak bisa desinkron; absen → tidak menggate).
  2. Mode aktif ≠ stage (dari `/api/mode`) → ditekan: VTuber punya event
     model sendiri, Pet punya idle-chatter sendiri — bicara proaktif app
     utama akan bikin karakter dobel.
  3. `assistant.busy` → ditekan (task Worker hidup di server walau panel
     ditutup).
  4. Fetch gagal → **fail-open** + warning.
- Slot (`gen`/`busy`) diklaim SEBELUM await gate → think yang datang saat
  gate menunggu tetap menang via gen-guard Fase 3 (tanpa race baru);
  `setThinking` dipindah setelah gate lolos (indikator tidak berkedip
  untuk event yang ditolak).
- **Kelas speech `companion_proactive`** (tier 1): `playSegments(segments,
  cls)` — event proaktif kini tidak bisa memotong bicara user/narasi
  worker/VTuber (matriks policy Fase 2). `think` tetap `companion`.
- `expressEventEmotion` (reaksi visual) + `setUserMood` (tracking mood)
- sengaja TIDAK digate — hanya reaksi proaktif (LLM+speech) yang berhenti.
- MODES.md aturan 3 diperluas: otak proaktif ikut "tidak diproses" saat
  mode non-aktif (baca `/api/mode`), bukan hanya runtime panel.
- Test: +6 kasus di `companion-concurrency.test.ts` (vtuber ditekan,
  worker busy ditekan, brain off ditekan, lolos+kelas proactive, fail-open,
  think menggulingkan gate). Gate: tsc bersih, build OK,
  **488 unit (+6) + 416 guard, 0 fail**.
- Runtime verify DIJALANKAN (browser, shim fetch/DOM — tanpa ubah state
  server, tanpa biaya LLM; `quietMs` dipendekkan supaya event sampai ke
  gate): (a) mode vtuber → "proactive idle ditekan: mode aktif vtuber",
  0 fetch LLM; (b) stage + worker busy → ditekan; (c) semua lolos →
  bicara jalan dengan holder speech `companion_proactive`; (d) toggle
  brain off → "ditekan: mode otak mati", tanpa fetch baru.

## UPDATE 2026-09-19 (44) — FASE 3 REWORK ARSITEKTUR: COMPANION CONCURRENCY (COMMIT)

§6 MERGE/PREEMPT + §32–34 stale/epoch terimplementasi — semua di
`src/client/agent/brain.ts`, **app.js tidak disentuh**:

- **MERGE (§6)**: pesan baru saat masih MIKIR tidak diabaikan lagi (bug
  lama: `if (this.busy) return` membuang pesan tanpa jejak) — request lama
  di-abort (`AbortController`), kedua teks sudah ada di history, satu
  fetch baru menjawab keduanya (server `/api/chat` stateless). Input user
  otomatis menggulingkan `reactEvent` yang sedang mikir (§18 precedence).
- **Generasi (§33)**: setiap think/reactEvent = `++gen`; reply telat
  dibuang SEBELUM memutar segmen (cek gen setelah profile-load, setelah
  fetch+json, dan setelah director pass); `finally` hanya generasi
  TERBARU yang boleh me-reset busy/thinking — abort karena merge senyap,
  bukan pesan error.
- **busy sinkron** sebelum await pertama → race double-entry (dulu diset
  setelah `await loadProfile()`) mati.
- **Timeout (§32)**: `REQUEST_TIMEOUT_MS` 90 dtk (statis, bisa dipendekkan
  test) lewat abort yang sama — request menggantung kini jatuh ke fallback
  "Maaf…" dan busy lepas, bukan membekukan brain.
- **Epoch model (§34)**: `invalidateCapabilityProfile()` menaikkan
  `modelEpoch`; `loadProfile` membuang profil model lama yang selesai
  setelah ganti model (profil model A tidak lagi menetap untuk model B).
- `_reactiveState()` kini membawa `busy` + `gen` (hook QA/verifikasi).
- **PREEMPT saat speaking**: tanpa kode baru — speech policy Fase 2 sudah
  memotong chain (`onPreempted` → unlockAI sekali-saja).
- Test: `test/companion-concurrency.test.ts` (6 kasus: merge+stale,
  double-entry, precedence proactive, timeout, epoch, selesai alami)
  dengan harness mock window/document/fetch (dipulihkan afterAll).
- Gate: tsc bersih, build OK, **482 unit (+6) + 416 guard, 0 fail**.
- Runtime verify DIJALANKAN (browser, halaman dengan bundle baru; /api/chat
  di-shim 1,5 dtk supaya jendela merge deterministik — tanpa biaya LLM):
  (a) think(A) saat mikir → think(B) → log "merge: pesan baru saat masih
  mikir", TEPAT 2 fetch (fetch kedua memuat A+B berurutan), SATU balasan
  diputar, unlock sekali, tanpa fallback error, gen=2, busy lepas — reply
  telat fetch#1 dibuang senyap (§33 live); (b) think(C) saat C BICARA →
  think(D) → log "chain preempted by speech policy, AI lock released",
  tidak ada merge baru (bicara = preempt, bukan merge), gen=4 sesuai
  hitungan, bubble menampilkan chain baru.

## UPDATE 2026-09-19 (43) — FASE 2 REWORK ARSITEKTUR: SPEECH BOUNDARY (COMMIT)

Rework arsitektur behavior dimulai (urutan fase di
`docs/ARSITEKTUR-GAP.md`, target di `docs/ARSITEKTUR-TARGET.md`; keputusan
user: rework dulu, regresi migrasi ditangani di fase pemiliknya).

**Speech boundary §15–16 terimplementasi** — speech jadi resource dengan
kepemilikan, bukan last-wins:

- **`src/client/speech/speech-policy.ts`** (baru, murni tanpa DOM): 6 kelas
  produser bertier (companion/direct t2 user-driven; vtuber/worker_narration/
  companion_proactive t1; worker_actor t0 dekoratif), matriks
  ALLOW/PREEMPT/QUEUE/SUPPRESS, antrean FIFO cap 4 (overflow buang item
  baru), `stopAll` (CANCEL), claim dengan `onPreempted` hook. Serialisasi
  worker dua arah menang atas tier (§16-3). Request sinkron dari dalam
  cleanup preempt masuk antrean (flag transisi). Terpasang sebagai
  `window.__speech` via bundle-entry; 19 unit test `test/speech-policy.test.ts`.
- **app.js**: `speak(text,onDone,opts)` → policy → `runSpeech(job,claim)`
  executor. Claim-aware `markDone` (claim mati = no-op state global — bug
  "markDone lama menginjak speak baru" mati), cleanup preempt per-sesi
  (stop audio, revoke blob URL — bocor lama mati, resolve loop segmen,
  abort fetch TTS in-flight, settle visual). API baru
  `__live2dAgent.stopSpeaking()` dipanggil teardown pindah mode
  (`mode-runtime.js`) + ganti model (`loadModel`). `__debugSpeak` default
  kelas "vtuber"; echo brain-off = "direct". Degrade legacy penuh tanpa
  bundle.
- **Produser**: brain `playSegments` preempt-flag + unlock sekali-saja via
  `onPreempted` (bug aiLock nyangkut mati; dipotong ≠ completed — §6);
  narasi worker = `worker_narration`, quip/filler actor = `worker_actor`
  (panel `speakAsCharacter(text,cls)`).
- **Docs ikut**: MODES.md (stopSpeaking di teardown), MOTION-SYSTEM-SPEC
  (unlock-once lockAI), ARSITEKTUR-GAP (baris policy ✅ + keputusan default).
- Gate: tsc bersih, build OK, **476 unit (+19 baru) + 416 guard, 0 fail**.
- Runtime verify DIJALANKAN (browser, dev server user jalan, TTS jalur
  browser): (a) ALLOW + selesai alami → onDone terpanggil + slot lepas;
  (b) preempt companion-vs-companion saat bicara → `onPreempted` jalan,
  onDone TIDAK, holder pindah claim baru, bubble ikut, tanpa segmen zombi;
  (c) worker narration/actor saat companion bicara → SUPPRESS (holder
  utuh, log keputusan); worker vs worker → QUEUE lalu drain FIFO setelah
  narasi selesai (quip berakhir `done`, bukan preempted);
  (d) `stopSpeaking()` saat bicara → holder null, claim dibatalkan,
  bubble tersembunyi. Console bersih — hanya log keputusan policy.
  Catatan: jalur brain→LLM→chain tidak diuji ulang di sini (mekanisme
  preempt di boundary sama-sama terverifikasi; E2E chat sudah entri 41).

## UPDATE 2026-09-19 (42) — AUDIT PURGE CORE: LUBANG BARU DI BRANCH `tes` + KEPUTUSAN USER (TANPA KODE)

Audit pasca-purge (read-only) menemukan celah yang tidak tercatat entri
sebelumnya:

- **Branch `tes` TIDAK ikut ditulis ulang filter-branch** (refs/original
  hanya menyimpan 3 branch: master, migration, feat/cubism-official-
  renderer) — masih memuat core di path lama dan bisa diunduh publik:
  `raw.githubusercontent.com/…/live2d-agent/tes/js/live2dcubismcore.min.js`
  → HTTP 200 (diuji 2026-09-19). Bukan SHA obscure: branch tampil di
  daftar branch repo publik.
- SHA pra-rewrite master `090ba3f3…` juga masih menyajikan core di path
  `static/js/…` → HTTP 200 — objek unreachable yang belum di-GC GitHub.
- Yang sudah bersih: tree master/migration/feat-cubism-official-renderer
  = 0 file core; tidak ada tag lokal/remote; repo publik (`private: false`).

**Keputusan user (final — jangan dibuka lagi tanpa perintah user):**

1. **Risiko diterima** — tidak ada aksi purge lanjutan (hapus repo, tiket
   support, dsb.) untuk akses SHA-lama maupun branch `tes`.
2. Branch usang `tes` & `feat/cubism-official-renderer` **akan dihapus
   manual oleh user setelah projek stabil** — agent tidak menyentuh.

Tanpa perubahan kode; gate tidak dijalankan ulang. refs/original tetap
dipertahankan lokal sebagai arsip riwayat pra-rewrite.

## UPDATE 2026-09-19 (41) — MATRIKS VERIFIKASI SEMUA MODE DI STACK BARU (COMMIT)

Sweep verifikasi lintas mode setelah pensiunan penuh (tanpa perubahan kode):

- **Panggung** ✅: load/framing/blink/breath/gaze/lipsync/slider/zoom/bg DOM.
- **Chat → brain → aksi** ✅: pesan nyata → balasan multi-bubble + aiLock
  pose + TTS (teruji entri 34, diulang saat flip).
- **VTuber** ✅: mode pindah via activity bar, provider mock jalan, feed
  ter-render (50 node; donasi masuk), AI MEMBALAS di feed ("Siap, Kevin!"),
  teardown aman. Catatan: perpindahan mode via API saja TIDAK mengaktifkan
  runtime klien — harus klik tombol activity bar (perilaku lama, bukan
  regresi).
- **Assistant** ✅: mode pindah, panel wide + composer, 21 tool terdaftar,
  agent loop jalan nyata (LLM → tool list_dir → transcript). Run contoh
  berakhir "Selesai dengan error" karena workDir sesi menunjuk folder temp
  — perilaku agent, bukan regresi renderer.
- **Pet** ✅ (browser): pet aktif, gaze kursor, i18n; ⚠️ klik-tembus Tauri
  belum diuji (butuh build Rust `bun run build:pet`).
- **Motion Studio** ✅ dasar: Registry/Runtime hidup, playEmotionClip
  memutar klip (swing angleY 9,1°) — UI studio visual belum diuji penuh.
- Gate: 457 unit + 416 guard + tsc — hijau penuh (tanpa perubahan kode di
  entri ini).

## UPDATE 2026-09-19 (40) — PENSIUNAN PENUH STACK LAMA (COMMIT)

`?renderer=legacy` tidak ada lagi: SATU jalur render — adapter
src/live2d/view (Pixi 8 + Cubism 5-r.5, efek framework penuh). Migrasi
Pixi 8 SELESAI (Fase 0–B sejak entri 27).

- **app.js** (−741 baris): flag RENDERER_PIXI8 + semua cabang kondisional
  dilucuti; tickBlink, tulis breath, gaze look-state (state.look.t*),
  installOverrideGuard dihapus — penggantinya updater framework (order
  200–900) + gate; liveliness selalu aditif; `PIXI.Point` → objek polos;
  `installOverrideGuard` digantikan AppWriteUpdater order 900 (jaminan
  sama: sticky override menang setelah physics/blink/breath).
- **Latar panggung pindah ke DOM**: `#stage-bg` (img object-fit cover) +
  `#stage-dim` overlay — dulu sprite PIXI yang TIDAK ter-render di stack
  view (gap senyap yang ditemukan saat pensiunan); bgColor kini CSS
  #stage (dulu background pixi alpha 0 — tidak terlihat).
- **index.html**: script pixi-live2d-0.4.0 dihapus; pixi.6.5.10
  DIPERTAHANKAN sebagai utilitas renderer overlay efek emosi (canvas
  terpisah — bukan renderer Live2D).
- **Guard**: test-multiply-color.js + test-override-guard.js dipensiunkan
  (subjeknya mekanisme lib lama / beforeModelUpdate yang digantikan
  AppWriteUpdater — konversi bun test menyusul bila perlu);
  test-core6-compat section patch lib pensiun, probe kini core-only
  (moc v5/v6 + model hidup), section D (shim stamp) tetap.
- Verifikasi (browser): `/` → model via facade TANPA error console, gaze
  vertikal −30 terukur, blink 47 dip/12 dtk (min 0), bg DOM bekerja,
  slider gaze tampil. Gate: 457 unit + 416 guard hijau penuh, tsc bersih
  (c237aae).
- **Sisa (opsional/lanjutan)**: uji pet klik-tembus di shell Tauri nyata
  (build butuh Rust); konversi guard override ke bun test atas
  AppWriteUpdater; pantauu uji rasa user beberapa hari — regresi tercatat
  di git (branch ini menyimpan seluruh sejarah stack lama).

## UPDATE 2026-09-19 (39) — FLIP DEFAULT: STACK BARU JADI UTAMA (COMMIT)

Fase B akhir: **tanpa parameter, app jalan di stack baru** (adapter
src/live2d/view + efek framework penuh + slider gaze). Kill-switch dibalik:
`?renderer=legacy` = balik ke stack lama (Pixi 6 + pixi-live2d) selama masa
transisi; `?renderer=pixi8` lama jadi no-op. Flag `RENDERER_PIXI8` tetap
menjadi satu-satunya kunci semua cabang (load/tickBlink/breath/gaze/gate/
slider) sehingga legacy tetap utuh 100%.

Verifikasi (browser): `/` → view ready + model via facade; `/?renderer=
legacy` → model via PIXI.live2d (jalan, view idle). Gate: 457 unit + 461
guard hijau penuh, tsc bersih (c502e04).

**Sisa (pensiunan penuh — commit terpisah, SETELAH user puas dengan uji
rasa)**: hapus pixi.6.5.10 + pixi-live2d-0.4.0 (dan semua cabang legacy di
app.js: Live2DModel.from, tickBlink, tulis breath/gaze lama,
installOverrideGuard), konversi/hapus guard yang membaca lib lama
(test-core6-compat section A/B/C membaca pixi-live2d-0.4.0.js — section D
string-check tetap relevan), hapus `?renderer=legacy` itu sendiri.
CATATAN uji pet: klik-tembus hanya teruji lewat polling /api/pet/state di
shell Tauri nyata — build `bun run build:pet` butuh toolchain Rust.

## UPDATE 2026-09-19 (38) — PET.HTML DIMIGRASI KE STACK BARU (COMMIT)

Item terakhir yang bisa dikerjakan sebelum flip default: jendela Pet
(pet.html, halaman mandiri untuk shell Tauri/Chrome) pindah dari
Pixi 6 + pixi-live2d ke adapter view (importmap pixi8.mjs +
js/live2d-view.mjs — pola index.html). Dengan ini TIDAK ADA lagi halaman
yang bergantung stack lama.

- Driver pet: tandai `__live2dRendererRequested` + `__live2dStageSize`,
  tunggu `__live2dViewWait`, `loadModel(path)` lewat adapter; framing via
  facade getBounds/scale/x/y (math lama); goyangan rotasi halus tetap;
  tombol/ganti model/integrasi Tauri (klik-tembus/close/Esc) tak berubah.
- Gaze kursor kini `setLookTarget(±1)` — framework look menggerakkan SEMUA
  sendi ter-skala range model; hardcode ParamAngleX/Y & ParamEyeBallX/Y
  (pelanggaran model-agnostic di pet) dihapus. Blink/breath framework
  aktif (dulu tidak ada di pet).
- **Dua bug lama pet ikut ketemu & diperbaiki**:
  1. i18n-entry tidak pernah mengekspos `window.__i18n` — SEMUA `t()` di
     pet.html sejak awal selalu mengembalikan kunci mentah (i18n pet mati
     senyap). i18n.js kini mengekspos runtime `{t,getLang,setLang}` —
     kontrak sama dengan bundle.js.
  2. startIdleLife: `let t = 0` menimpa fungsi penerjemah `t()` — sapaan
     berkala selalu melempar "t is not a function" sejak stack lama;
     variabel diganti `tm`. boot catch kini fail-loud (`__petBootError` +
     console.error).
- Verifikasi (/pet.html, browser): status "pet aktif", i18n ter-translate,
  gaze kiri −26°/kanan +24° terukur, blink/breath hidup, screenshot model
  utuh (神宫白子 — model pertama di list, perilaku default sama).
- Gate: 457 unit + 461 guard hijau penuh, tsc bersih (98ea4b5).
- **Belum (menuju flip default)**: uji rasa user via A/B berdampingan
  (zoom/drag/kelembutan + slider gaze), uji pet di shell Tauri nyata
  (klik-tembus), lalu flip default & pensiunkan stack lama (satu commit,
  setelah user setuju).

## UPDATE 2026-09-19 (37) — SLIDER KEEKSPRESIVAN GAZE PER GRUP SENDI (COMMIT)

Lanjutan keluhan user (entri 36): "keekspresivan sendinya harusnya bisa
disetting". Tiga slider (0–2×; 1 = netral paritas stack lama) di panel
konfigurasi per-model: **Kepala** (angleX/Y/Z), **Mata** (eyeBallX/Y),
**Badan** (bodyAngleX/Y). Gain mengalikan faktor look per grup saat build —
`renderer.setGazeGain()` membangun ulang data look via
`userModel.setLookParameters()` TANPA membuat ulang updater. Config:
`gazeHead/gazeEyes/gazeBody` di model config (normalize clamp 0..2),
live-apply saat slider digeser, persist per-model via sheet config
(readConfigForm → saveModelConfig — pola bg/pitch yang sama). Section
disembunyikan di stack lama (look framework hanya ada di pixi8). i18n
id+en: cfg.gazeHead/Eyes/Body.

Verifikasi (?renderer=pixi8, ren): slider badan 2× → faktor bodyAngleX/Y
10/7.5 → 20/15; kepala & mata 0.5× → 15 & 0.5; output live; Simpan →
sheet config tersimpan (live2d_sheet_*), kembali terpasang saat apply.
Config uji dikembalikan ke netral 1/1/1. Gate: 457 unit + 461 guard hijau
penuh, tsc bersih (976dda4).

## UPDATE 2026-09-19 (36) — GAZE VERTIKAL BADAN HILANG SAAT FLIP — DIPULIHKAN (COMMIT)

Laporan user: karakter mengikuti mouse hanya kiri-kanan, sendi atas-bawah
mati ("harusnya kek stack lama semua persendian aktif"). Akar: SPEC look
framework (buildLookData) tidak punya **bodyAngleY** — stack lama menulisnya
via `tby = ny·REF_HALF·0.25` (±0.75 half-range) dan flip gaze (entri 33 #3)
menghapus jalur itu tanpa pengganti; kepala (angleY) & mata (eyeBallY)
sebenarnya sudah terdaftar benar (fy 30/1) dan terukur bergerak, tapi sendi
badan vertikal benar-benar mati. Perbaikan: SPEC += bodyAngleY
{ stdHalf: 10, fy: 7.5 } — diskalakan role-space (ren ±10 → ±7.5°, paritas
eksak tby lama). Verifikasi: enam sendi × lima arah — angleY ±28, eyeBallY
±0.99, bodyAngleY ±7.45, angleX ±30, eyeBallX ±1, bodyAngleX +10/−7.8.
Gate: 457 unit + 461 guard hijau penuh, tsc bersih (ca3931b).

## UPDATE 2026-09-19 (35) — VERIFIKASI MULTI-MODEL + SUITE HIJAU PENUH (COMMIT)

User menambah model ke mesin backup (lumine + 神宫白子/面饼0 di layout
`data/model/_/神宫白子模型/`; "tesmodel" = duplikat ren yang dihapus).
Verifikasi multi-model dijalankan dan MENEMUKAN dua kegagalan senyap —

- **Gap 1 — blink mati pada rig tanpa grup EyeBlink (8be1dee)**:
  `CubismEyeBlink.create(manifest-tanpa-grup)` → instance ber-0 id; driver
  app.js sudah dilewati di pixi8 → 神宫白子 tidak pernah berkedip.
  Perbaikan: `ensureEyeBlink` mengisi id mata dari ROLE MAPPING (by-name);
  deklarasi rigger yang punya id selalu menang. Terverifikasi kedip
  role-fallback jalan.
- **Gap 2 — adopsi .exp3 yatim tidak diteruskan ke stack baru (8be1dee)**:
  buildModelSettings hanya dipakai jalur lama; stack baru mem-fetch
  manifest mentah → ekspresi yatim mati tanpa error (kelas kegagalan
  MODEL-AGNOSTIC #7 persis). Perbaikan: manifest hasil adopsi diteruskan
  `loadModel(modelPath, settings)` → renderer (in-memory; aturan user>ai +
  opt-out tetap di app.js; tidak pernah ke disk). Terverifikasi: lumine
  0→19, 神宫白子 0→8 ekspresi.
- **Invarian lain lolos di kedua model**: roleMap 16/14 role (termasuk
  browL/browR/eyeRSmile), paramRange 223/214 terukur, caps head/eyes/body
  (body absent → degrade graceful), gaze ter-skala range rig (lumine
  2,2°→28,2°), lipsync spread 0,99, zoom anchored 0 px. Regresi ren: nihil.
- **Suite hijau penuh pertama kali di mesin ini (5bc82f3)**: 3 test
  integrasi + probe guard ternyata mengejar path data mesin asli
  (神宫白子/, tesmodel/runtime/ren.moc3) — kini layout-agnostic:
  findCjkModel() dari data yang ada; probe core6 mengumpulkan .moc3
  rekursif + bucket versi dari byte ke-4 header moc3; SKIP jujur bila
  bootstrap emscripten gagal di env non-browser (bunEval kini via file
  temporer — `bun -e` tidak stabil lintas versi Bun untuk init emscripten);
  `csmGetMocVersion` di Live2DRenderer dikoreksi dua-argumen sesuai
  wrapper core 6.0.1. Hasil: **457 unit + 461 guard + tsc — SEMUA hijau,
  tanpa satu pun kegagalan env**.
- Pelajaran pengukuran (catat!): tool-call layer meng-unescape backslash —
  regex dengan `\\` di dalam template literal rawan salah-cook saat
  ditulis lewat skrip; probe kini bebas regex (indexOf murni).
- **Belum (menuju flip default)**: uji rasa user via A/B berdampingan
  (zoom/drag/kelembutan), migrasi pet.html ke adapter view (jendela Pet
  masih stack lama — mati diam-diam bila stack lama dipensiunkan tanpa
  ini), lalu flip default & pensiunkan stack lama (satu commit, setelah
  user setuju).

## UPDATE 2026-09-19 (34) — FLIP LIPSYNC + UJI CHAT END-TO-END + PEMBERSIHAN (COMMIT)

Lanjutan entri (33). Tiga hal selesai:

- **Flip lipsync (a8bac75, Fase B #4)**: LipsyncUpdater (order 450) menulis
  param mulut role-resolved dari provider nilai 0..1 — diskalakan range
  aktual (role-space); model tanpa role mulut tidak mendaftar apa pun.
  Sumber nilai tetap di app.js (`setLipsyncProvider`: analisis audio TTS
  lokal / pola sintetis — input jiwa tidak pindah). Blok overrides lipsync
  app.js dilewati di pixi8; pembersihan saat bicara selesai tetap jalan
  (no-op). Verifikasi: provider bicara = ParamMouthOpenY osilasi 0,02→1,0;
  diam = base 0. **Dengan ini SEMUA efek standar diputar framework**
  (blink/breath/gaze/lipsync/motion/ekspresi/physics/pose); app.js
  menyumbang komposisi jiwa aditif (liveliness/emosi) + pintu konfigurasi.
- **Uji chat/brain end-to-end di stack baru (browser nyata, LLM koneksi
  user)**: pesan "lambaikan tangan + senyum" dijawab LLM dengan balasan
  multi-bubble + aksi; `aiLock: true` menggerakkan pose (angleX 8,6°),
  gelembung ucapan tampil, lipsync/TTS jalan — jalur brain→directive→
  actor→AppWriteUpdater→model bekerja penuh di pixi8. Tidak ada regresi
  arah lama.
- **Pembersihan (a2eb7ea)**: loader hilang 200ms setelah model siap +
  fade 300ms (dulu ~1,25 dtk menghalangi); trace `__loadSteps` dihapus
  dari Live2DRenderer (diagnostik Fase A selesai). **`__l2dDebug`
  DIPERTAHANKAN** — itu interface nyata (voice-input.js & emotion-overlay.js
  membacanya), bukan sampah debug. `rolePoke` dari catatan Fase A tidak
  pernah ada di kode — artefak perintah konsol sesi verifikasi lama.
- Gate: tsc bersih; 454 unit (3 gagal env data backup — pre-existing);
  guard 450/451 (1 gagal env probe — pre-existing).
- **Belum (menuju flip default)**: verifikasi multi-model (Cubism 4/5
  lain — TERBLOKIR di mesin backup ini: hanya ren di data/model; uji di
  mesin data lengkap), uji manual rasa user (zoom/drag/kelembutan) via
  A/B berdampingan, lalu flip default & pensiunkan stack lama (satu
  commit terpisah, setelah user setuju).

## UPDATE 2026-09-19 (33) — FASE B: FLIP KEPEMILIKAN BLINK → BREATH → GAZE SELESAI (3 COMMIT)

Tiga flip "satu fitur satu pemilik" selesai (permintaan user: "lanjut").
Kini framework memutar SEMUA efek standar (blink/breath/gaze/motion/
ekspresi/physics/pose) dan app.js menyumbang komposisi jiwa (liveliness,
emosi, lipsync) secara ADITIF di stack baru — pola komposisi resmi official
sample. Stack lama tidak tersentuh (semua cabang RENDERER_PIXI8).

- **#1 blink (928b909)**: CubismEyeBlinkUpdater order 200; driver tickBlink
  dilewati di pixi8. Gate: `setBlinkGate(() => blinkEnabled && !frozen)`
  menumpang callback `_motionUpdated` resmi — kedip mundur otomatis saat
  motion menganimasikan mata (wink) dan saat gate ditutup (tulisan efek
  frame-transien, tanpa restore manual). Verifikasi: gate ON = 2 kedipan/
  8 dtk (min 0 = tertutup penuh); gate OFF = kurva mata motion tampil
  (min 0,8) — kepemilikan berpindah bersih.
- **#2 breath (235c648)**: CubismBreath (addParameterValueById — aditif
  resmi) dalam GatedUpdater (class pembungkus gate + order). Driver breath
  app.js dilewati di pixi8. Gate: hasBreath + frozen + motion-layer (kurva
  klip yang membawa breath sendiri tetap berdaulat). Verifikasi: gate ON
  osilasi 0→0,499 (~2,5 siklus/8 dtk); gate OFF datar di base (0).
- **#3 gaze (257c778)**: CubismLook (aditif, di-ease CubismTargetPoint)
  order Drag; mousemove app.js di pixi8 meneruskan target ±1 ke
  `setLookTarget` (renderer sudah punya). Liveliness jadi tulisan ADITIF:
  `pokeAddParam` (backend coreModel dapat `addParameterValueById`;
  AppWriteUpdater flush SET dulu lalu ADD) di luar aiLock, tanpa term gaze
  (bAx = A1·0.5 saja) — lerp absolut lama akan mengikis kontribusi gaze.
  mouthForm tetap SET-lerp. Cabang aiLock/motion-layer tetap SET (pose
  dimiliki otak/motion); `setLookGate(() => !aiLock && !frozen &&
  !motionLayer)` meredam gaze framework agar tidak dobel. dblclick reset
  ikut mereset target gaze framework. Verifikasi: gaze kanan/kiri = angleX
  median +23,1/−23,9 (range ±30, terkomposisi sway); sway hidup di atas
  gaze penuh (spread 4,2°); gate OFF = kembali sway (−2,6°).
- **Keputusan komposisi yang terkunci (jangan dibalik tanpa alasan)**:
  tulisan pose app.js di stack baru ADITIF di luar aiLock (offset sway di
  atas motion+gaze+breath), SET absolut hanya untuk aiLock/motion-layer/
  mouthForm/emosi/rawDrive. Komponen angle framework breath kini hidup
  (tidak lagi tertutup liveliness).
- Gate per flip: tsc bersih; 454 unit (3 gagal env data backup —
  pre-existing); guard 450/451 (1 gagal env probe — pre-existing).
- **Belum (Fase B lanjutan)**: lipsync updater (IParameterProvider TTS —
  saat ini lipsync app.js tulis overrides di 900, masih jalan), uji jalur
  chat/brain end-to-end di stack baru (aiLock + ekspresi per teks),
  bersihkan instrumentasi debug (`__loadSteps`, `__l2dDebug`), fade-out
  loader (kosmetik), rolePoke gagal (`undefined setParameter` — periksa),
  verifikasi multi-model (Cubism 4/5 lain), baru flip default & pensiunkan
  stack lama.

## UPDATE 2026-09-18 (32) — CATATAN USER + STATUS DEFAULT & MODEL-AGNOSTIC (COMMIT)

Feedback user setelah mencoba hasil entri (30)+(31): **stack baru masih
terasa berat, tapi sudah cukup oke** — diterima untuk sekarang, TIDAK
ditindaklanjuti lebih jauh di sesi ini. Catatan pengukuran: A/B selang-
seling menghasilkan 5,9–7,3 ms/frame (di bawah budget vsync 16,6 ms), jadi
"berat" yang masih terasa kemungkinan berasal dari (a) jendela interferensi
host yang juga mengena stack lama, atau (b) beban GPU di mesin user (dpr /
ukuran stage / iGPU) yang belum terukur — kalau keluhan berlanjut, ukur
live rAF berdampingan dengan stack lama di menit yang sama dulu, jangan
micro-benchmark.

Status dua pertanyaan user (dicatat agar sesi berikutnya tidak menebak):
- **Default masih stack lama** — `?renderer=pixi8` sengaja opt-in/kill-switch
  (app.js `RENDERER_PIXI8`). Ini desain migrasi: flip default BARU boleh
  setelah Fase B selesai (flip kepemilikan blink→breath→gaze ke framework,
  lipsync updater TTS, uji chat/brain end-to-end, verifikasi multi-model),
  lalu pensiunkan stack lama. Jangan flip duluan.
- **Model-agnostic: arsitekturnya ya, buktinya baru satu model.** Sudah
  benar: tulis param hanya lewat role space (RoleController/roleInfo —
  look & breath diskalakan dari range aktual model), grup EyeBlink/LipSync
  by-name dari manifest, kemampuan diukur dari disk (ModelInspector),
  tanpa id bernomor (hardcode ParamEarL/R sudah dihapus entri (28) +
  guard). Framing memakai canvasW/H model, MODEL_UNIT_PX bebas (terkompensasi
  frameModel). YANG BELUM: diverifikasi visual/fungsional hanya di ren
  (moc v6) — uji model Cubism 4/5 lain adalah bagian fase lanjutan sebelum
  flip default.

## UPDATE 2026-09-18 (31) — ENTEng LANJUT: CACHE BUFFER INDEKS QUAD + KALIBRASI PENGUKURAN (COMMIT)

Lanjutan permintaan user "buat lebih enteng lagi" setelah entri (30). Dua
hasil: satu optimasi nyata, dan pemahaman penting soal pengukuran di mesin
ini (penting untuk sesi berikutnya).

- **Cache buffer indeks quad render-target** (`cubismrenderer_webgl.ts`):
  dua jalur dulu melakukan `createBuffer`+`bufferData`+`deleteBuffer` tiap
  panggilan — `drawOffscreenWebGL` (per offscreen per frame; ren 24×) dan
  `afterDrawModelRenderTarget` (per frame) = 25 siklus alloc GPU per frame
  untuk konten yang statis (`s_renderTargetIndexArray`). Kini satu buffer
  di-cache (`getRenderTargetIndexBuffer()`), dihapus di `release()`. Aman
  thd. VAO Pixi (binding ELEMENT_ARRAY_BUFFER dengan default-VAO tidak
  menyentuh VAO milik Pixi).
- **Kalibrasi pengukuran (penting!)**: loop mikro di mesin ini SANGAT
  bimodal — jendela cepat (drawModel 2–7 ms) dan jendela lambat (23–33 ms)
  bergantian antar-menit, kena SEMUA pipeline termasuk kontrol. Buktinya:
  kontrol stack lama di-DOM-detach terukur 1,3 ms di jendela cepat;
  A/B selang-seling dalam jendela yang sama menghasilkan rasio stabil
  **baru 5,9–7,3 ms vs lama 2,9–3,9 ms** (drain `gl.finish` per frame,
  termasuk GPU). Jangan pernah menyimpulkan dari satu jendela pengukuran —
  selama-selang-seling atau pakai live rAF. Angka 30 ms sebelumnya pada
  entri (30) adalah kombinasi stall glGet asli (sudah dihapus, efek nyata:
  live 26 FPS dulu) + jendela interferensi host.
- **Fakta pipeline yang terkonfirmasi**: histogram panggilan GL per frame
  baru ≈ lama (705 vs 568 bindBuffer; 626 vs 564 bufferData) — biaya ekstra
  stack baru berasal dari jalur v6 (pass offscreen + uniform 641 vs 204),
  bukan pemborosan panggilan; render Pixi stage kosong hanya **0,2 ms**
  (ide "skip render Pixi" tidak jalan — biarkan); `gl.getError()` bersih;
  readPixels konten model opaque.
- Gate: tsc bersih; 454 unit (3 gagal env data backup — pre-existing);
  guard 450/451 (1 gagal env probe — pre-existing).
- **Belum (Fase B lanjutan)**: sama seperti entri (30) — flip kepemilikan
  blink→breath→gaze ke framework, lipsync updater, uji chat/brain end-to-end,
  bersihkan instrumentasi debug, fade-out loader, rolePoke, mode
  Assistant/Pet, pensiunkan stack lama.

## UPDATE 2026-09-18 (30) — FASE B: PERF + ZOOM/DRAG — STALL glGet DIHABISKAN (26→60 FPS) (COMMIT)

Feedback user setelah Fase A: **stack baru terasa lebih berat**, **zoom
kurang lancar**, **drag kadang loncat-loncat**. Diukur di browser nyata
(server produk, ren, webview IAB): stack lama **60 FPS stabil** (p50 16,7 ms,
0 long frame) vs stack baru **26 FPS** (p50 33,4 ms, 103/240 long frame,
`draw()` CPU 34,5 ms/frame).

- **Akar "berat" = sinkronisasi CPU–GPU paksa tiap frame.** Jalur draw
  memanggil `gl.getParameter`/`getVertexAttrib` SETELAH draw submission:
  (1) `saveProfile()` ±15 glGet (framework, di Live2DRenderer.draw),
  (2) `CubismRenderer_WebGL.doDrawModel` — lastFbo/lastViewport,
  (3) `beforeDrawModelRenderTarget` → `_modelRenderTargets[0].beginDraw()`
  tanpa argumen (baca FBO aktif). Satu glGet pun setelah antrean GPU penuh
  (198 drawable + 24 offscreen) memaksa CPU menunggu GPU mengosongkan
  antrean → CPU dan GPU SERI (terukur: blok save+setRenderState 41,8 ms
  di loop; gl.finish setelah draw hanya +0,03 ms — GPU bukan bottleneck).
  Stack lama tak punya glGet di jalur panas → CPU/GPU overlap → 60 FPS.
- **Fix (3 lapis, semua GL-set murni tanpa glGet):**
  1. `Live2DView.init` memasang **jembatan state reset Pixi** via
     `renderer.setPixiStateReset()` — setelah drawModel: colorMask/depthMask/
     frontFace/activeTexture di-set, cache Pixi 8 di-invalidate
     (`state.stateId=0` memaksa `state.set()` berikutnya menerbitkan ulang
     semua enable/disable; `blendMode=""` memaksa blendFunc; `_blendEq`
     memaksa blendEquation; `_glFrontFace=false` senada CCW yang baru
     di-set). Mesin state Pixi 8 terbukti murni cache (tidak pernah glGet
     saat render) — satu-satunya risiko bersama context adalah cache basi,
     dan itu dibatalkan di sini.
  2. `CubismRenderer_WebGL.setRenderState` kini **menyimpan**
     fbo/viewport (`_renderingFrameBuffer/_renderingViewport/_renderStateValid`);
     `doDrawModel` & `beforeDrawModelRenderTarget` memakai nilai tersimpan
     itu (fallback glGet hanya bila setRenderState belum dipanggil — di luar
     kontrak resmi LAppView). `beginDraw` menerima sentinel baru
     `CUBISM_DEFAULT_FRAMEBUFFER` (cubismrendertarget_webgl.ts) untuk FBO
     pemulih = framebuffer default TANPA membaca GL (null sungguhan tetap
     dibaca via glGet sebagai jalur lama). `release()` mereset flag.
  3. `Live2DRenderer.draw`: jalur view memakai jembatan (1); jalur lama
     saveProfile/restoreProfile DIPERTAHANKAN untuk halaman proof (mereka
     memanggil draw() sendiri tanpa bridge).
- **Mikro:** `ParameterArbiter.resolveFinal()` — cache resolve (dirty flag)
  untuk baca param app.js ±10×/frame (dulu alokasi Map per baca);
  `AppWriteUpdater` cache id handle; matriks MVP dipakai ulang (dulu `new`
  per frame); ParameterController di-cache per model (null-kan saat loadModel).
- **Akar "zoom loncat/lancar" = facade tak punya `toLocal`.**
  `setScaleAroundPoint` app.js bergantung `m.toLocal(cursor)` untuk zoom
  anchored; tanpa itu ia jatuh ke jalur fallback `m.texture?…` yang
  **center-kan seluruh box ke kursor tiap tick** interpolasi lerp 0.25/frame
  (+ bucket) → posisi tersnap ulang berkali-kali dan melawan drag. Fix:
  `framing.ts` — `localToScreen` diubah ke **bentuk tertutup tanpa alokasi**
  (rantai RES·ortho saling meniadakan; afine 2D R·S·A·p + posisi — hasil
  identik, 8 test lama tetap hijau; dulu ±7 Float32Array + 5 matmul per
  mousemove gaze) + `screenToLocal` (kebalikan eksak) + facade `toLocal`.
  Uji semantik di browser: titik di bawah kursor **tetap persis di bawah
  kursor** setelah reposition ala setScaleAroundPoint (error 0 px).
- **Bukti (browser nyata, server produk, ?renderer=pixi8):** batas atas
  frame (draw + gl.finish) **13,8 ms** — muat budget vsync 16,6 ms; bedah
  fase bersih: `um.update` median 5,8 ms (core 5,1 — Core 6 moc v6),
  `drawModel` 2,2 ms, prep 0,07 ms → total CPU ±8–10 ms/frame ≈ 60 FPS
  (dari 26). **Piksel terverifikasi** via readPixels (tengah = warna kulit
  alpha 255, pojok transparan) dan **screenshot**: ren utuh di panggung
  (jaket putih trim oranye, mask rambut-wajah & blend jaket benar, chat UI
  hidup). Overlay biru di dada = bagian motion model (konfirmasi user,
  bukan bug render). Catatan metode: loop draw() manual TANPA present
  menghasilkan 40+ ms artifak (luap antrean perintah) — instrumen sah
  adalah finish-per-frame atau loop rAF.
- **Keterbatasan lingkungan verifikasi:** compositor IAB membekukan rAF
  saat jendela host ter-oklusi → FPS rAF langsung tak selalu terukur;
  pengukuran memakai bedah fase + finish (batas atas) + readPixels.
- Gate: tsc bersih; **454 unit (3 gagal env data backup — pre-existing) +
  guard 450/451 (1 gagal env probe — pre-existing)**; 19 test framing baru
  (roundtrip screenToLocal dua arah × 9 kasus + semantik anchored).
- **Belum (Fase B lanjutan):** flip kepemilikan blink→breath→gaze ke
  framework (satu commit per fitur), lipsync updater (IParameterProvider
  TTS), uji jalur interaksi penuh dari chat/brain, bersihkan instrumentasi
  debug (`__loadSteps`, `__l2dDebug`), fade-out loader (kosmetik),
  `rolePoke` gagal (`undefined setParameter` — cek regresi vs stack lama),
  mode Assistant/Pet, terakhir pensiunkan stack lama.

## UPDATE 2026-09-18 (29) — FASE A INTEGRASI VIEW: PANGGUNG + CHAT DI STACK BARU (?renderer=pixi8) (COMMIT)

Strategi **TRANSPLANT JIWA** — akar kegagalan integrasi-view berulang user
(v2 stack baru selalu terasa lebih berat & fitur tidak pernah ikut): app.js
tidak perlu ditulis ulang; SEMUA tulisan karakter bermuara di satu titik
mati `coreModel()` (app.js:137), dan urutan tulis stack baru (updater order
900 setelah framework) = semantik `beforeModelUpdate` lama. Jadi:
- `src/live2d/view/Live2DView.ts` — facade `state.model` (transform
  x/y/scale/anchor/rotation, `motion()`/`expression()`, `toGlobal`,
  `getBounds`, `getParameterIds`) + backend `coreModel`
  (setParameterValueById berbobot/getParameterValueById/setPartOpacityById/
  getModel() parts+parameter min-max-def) + shim `internalModel`
  (motionManager.definitions, expressionManager.definitions, settings,
  eyeBlinkIds/lipSyncIds, focusController; TANPA `on` — override guard
  dilewati, padanannya AppWriteUpdater order 900 flush per frame).
- Framing paritas pixi-live2d dipisah murni di `view/framing.ts`
  (column-major, layout CubismMatrix44._tr): P·RES·T·R·S·A·C·F·S(cw,ch) —
  8 unit test `test/live2d-view.test.ts` (anchor/scale/rotasi/resolution/
  canvas non-persegi 1.0×1.346). `origW = MODEL_UNIT_PX·canvasW` —
  konstanta bebas, terkompensasi frameModel.
- `?renderer=pixi8` = kill-switch: default TETAP stack lama; index.html
  menambah importmap pixi.js→./js/pixi8.mjs + module
  js/live2d-view.mjs (build entry ke-3, gitignored; instance ESM
  terpisah dari global PIXI v6). app.js: shim `app` sempit
  (screen/stage/renderer/ticker, ±30 baris) + cabang init model;
  sisanya TIDAK disentuh — tickBlink/liveliness/gaze kursor/lipsync/
  emosi/preset/MotionRuntime+bridge lama jalan apa adanya.
- **Kepemilikan efek (satu fitur satu pemilik)**: view mematikan
  blink/look/breath framework (`loadModel effects` opt) + `autoIdle=false`
  (app.js startIdleMotion pemilik idle); physics/pose/ekspresi tetap
  framework. Arbiter tetap terpasang (gagal-aman).
- **Dua bug senyap ditemukan & diperbaiki saat verifikasi**: (1)
  `img.decode()` diantre **94 detik** di webview sibuk-GPU (decoder
  starved loop render 198-drawable) → pemuat tekstur kini
  `fetch→blob→createImageBitmap` (fallback <img>) — bukti jejak
  `__loadSteps`: 598ms→1270ms total; (2) draw Cubism diregistrasi ticker
  prioritas default (0) sedangkan render Pixi di LOW(-25) → **Pixi
  membersihkan hasil Cubism tiap frame** (canvas kosong) → prioritas -50
  (setelah render Pixi). rAF fallback aman otomatis.
- **Bukti (browser, server produk, ?renderer=pixi8)**: ren berdiri di
  panggung asli (screenshot: jaket putih trim oranye, boots hitam),
  loader hilang, framing jalan (natW/natH=400×538, scale 1.36), roleMap
  21 role + paramRange 73 + 5 ekspresi native, tick app.js hidup
  (angleX liveliness mengalir ke model), **motion Idle native mengubah
  pose + ekspresi exp_04 terputar lewat jalur facade yang dipakai
  brain**, sapaan proaktif + Mode Otak tampil. Stack lama tanpa toggle
  tetap jalan (pixi lama + model termuat) — tanpa regresi guard.
- Gate: tsc bersih; **438 unit (3 gagal env data backup) + guard 451
  (1 gagal env probe)** — editan app.js tidak mematahkan guard apa pun.
- **Belum (Fase B berikutnya)**: obrolan penuh end-to-end (brain→ekspresi
  nyata per teks; sapaan kini statis), overlay biru di dada ren
  (diduga gizmo/hit-area; cek `emotion-overlay.js`), pointer drag/zoom di
  facade, flip kepemilikan blink/breath/gaze → framework (hapus driver
  app.js satu per commit), lipsync updater (IParameterProvider TTS),
  mode Assistant/Pet, terakhir hapus stack lama.

## UPDATE 2026-09-18 (28) — REVIEW MIGRASI F0–15 + KOREKSI RUNTIME SESUAI SKILL cubism-web (COMMIT)

Review penuh branch `migration/pixi8-cubism` (16 fase, di-commit 08:34–09:51
hari yang sama), lalu perbaikan berbasis skill `cubism-web` (sumber resmi
5-r.5/Core 6.0.1 — persis versi vendored di `src/live2d/cubism/`).

- **Temuan review (utama)**: (1) guard core6 section D masih menguji shim yang
  dihapus Fase 7 → gate merah; (2) `draw()` hanya `model.update()` mentah —
  tanpa pipeline dua fase, physics/pose di-load tapi TIDAK PERNAH
  dievaluasi, blink/look/breath tidak terpasang; (3) `playNative` stub;
  (4) `clearPoseDelta`/`releaseParamDrive` menulis via source 'manual'
  (prio 100) → mematikan motion selamanya + reset ke 0-ref salah untuk
  eyeOpen; (5) `CapabilityAnalyzer` hardcode `ParamEarL/ParamEarR`
  (pelanggaran model-agnostic); (6) `getSupports()` klaim "head" palsan;
  (7) **MIME `.mjs` = application/octet-stream** → browser menolak modul
  ESM ("Failed to fetch dynamically imported module") → SEMUA halaman proof
  pipeline Pixi8 **tidak pernah bisa jalan di server produk** selama ini —
  bukti fase 8–15 sebelumnya tidak terverifikasi di server asli.
- **Koreksi runtime (pola resmi LAppModel 5-r.5)**: baru
  `src/live2d/Live2DUserModel.ts` — `update(dt)` dua fase
  (loadParameters → motion → saveParameters → scheduler.onLateUpdate →
  model.update), dt DETIK + clamp; updater efek terdaftar di
  `CubismUpdateScheduler`: EyeBlink(200, dengan callback motionUpdated),
  Expression(300), Look(400, faktor std ±30/±10/±1 **diskalakan ke range
  aktual model lewat roleInfo** — role-space, bukan angka literal),
  Breath(500, idem), Physics(600 + `stabilization()` sekali), Pose(800);
  arbiter kini `ArbiterUpdater` order 900 (titik injeksi resmi "manual
  overrides after all effects"). `Live2DRenderer`: `saveProfile/restoreProfile`
  di sekitar `drawModel` (drawModel tidak mengembalikan state host),
  `CubismWebGLOffscreenManager.beginFrameProcess/endFrameProcess` (ren:
  blend-enabled, 24 offscreen), FBO di-capture sekali untuk `setRenderState`,
  mipmap tekstur, polling shader 4 dtk **dihapus** (pola resmi: terus menggambar,
  frame awal hitam by design), `dispose()` menghapus cache motion/ekspresi
  (`ACubismMotion.delete`) + scheduler sebelum `release()`.
- **Motion native & ekspresi nyata**: `startMotionGroup(group, idx, prio)` —
  validasi grup by-name dari manifest, lazy-fetch + cache, protokol
  `reserveMotion`/`startMotionPriority`/reset reservasi saat gagal,
  `setEffectIds(blinkIds, lipSyncIds)`; kontrak return diratakan jadi angka
  (-1 gagal / 1 jalan) karena port vendored mengembalikan objek
  `CubismMotionQueueEntry`. `playExpression(name)` — 5-r.5 ekspresi TANPA
  prioritas; fallback blush IntentDirector kini benar-benar memutar
  ekspresi pertama manifest, bukan `console.log`.
- **Arbiter & bridge**: `clearPoseDelta`/`releaseParamDrive` menulis default
  ASLI (`roleDefaultOf`, via `RoleController.roleDefaultActual`) dengan
  source `'motion'` — anti-starvation & benar untuk rig default ≠ 0.
  Unit test baru `test/live2d-pipeline.test.ts` (16 test): prioritas/tie/
  konflik/purity arbiter, kapabilitas via role, role `ear` baru dengan uji
  anti-positif-palsu (`pearl` ⊅ ear — substring `earl` sengaja dihindari).
- **Infra**: `src/build-proof.ts` + `npm run build:proof` — `live2d-render.mjs`
  kini reproducible (sebelumnya command manual tak terekam, artefak pasti
  drift); MIME `.mjs` → `text/javascript`; guard core6 section D dikunci
  ulang (shim hilang total & tidak boleh kembali).
- **Bukti (browser nyata, server produk)**: `static/pipeline-proof.html`
  `ok:true` — ren moc v6 (198 drawable/24 offscreen), role map 21 role,
  `setRole(angleX,15)→15`, motion `Idle` native jalan, `exp_01` diputar,
  grup tak dikenal ditolak (-1), reset default → 0, **pixel berubah antar
  frame (idle+blink+physics hidup)**; screenshot: karakter ren utuh
  (jaket putih trim hitam aksen oranye) ter-render lewat pipeline baru;
  8 halaman proof fase 8–15 semuanya hijau. **Catatan**: canvas tampil
  hitam bila rAF berhenti (context Pixi `preserveDrawingBuffer:false`) —
  bukti visual wajib diambil saat loop masih menggambar; loop proof kini
  tidak berhenti.
- **Belum disentuh (sengaja)**: **integrasi view** — `app.js:542` masih
  `PIXI.live2d.Live2DModel.from` stack lama; ini fase berikutnya (ganti titik
  masuk model ke adapter baru, satu mode dulu, patuhi teardown
  `docs/MODES.md`; UI di atas model = DOM karena Cubism menggambar setelah
  Pixi). File untracked `agent-prompt-migrasi-pixi8-cubism.md` +
  `live2d-agent-2.0.0.tgz` dibiarkan tidak di-commit.
- Gate: **tsc bersih; 430 unit (3 gagal = data model 神宫白子/lumine/tesmodel
  tidak ada di F:\backup\lumi — hijau di mesin data lengkap); guard 451
  (1 gagal = probe butuh lumine/tesmodel — idem)**. Yang bisa hijau di
  mesin backup ini semuanya hijau.

## UPDATE 2026-09-18 (27) — FASE PENDAHULUAN: RISET CUBISM 5-r.5 + PIXI 8 + SPIKE TERISOLASI (BELUM COMMIT)


Riset wajib sebelum Fase 0. SDK `F:\CubismSdkForWeb-5-r.5.zip` diekstrak ke temp
(`CubismSdkForWeb-5-r.5`), Pixi 8 diverifikasi dari registry, dan spike read-only
dijalankan di env terisolasi (bukan di repo) — **tidak menyentuh kode aplikasi**,
tidak ada branch/commit baru.

- **Versi terkunci**: SDK Web **5-r.5** (2026-04-02) + Core **6.0.1** (`0x06000001`,
  `Core/CHANGELOG.md` 2026-01-08) + Pixi **8.20.1** exact (registry shasum
  `233621f…`). Catatan lengkap di `docs/knowledge/cubism5-pixi8-notes.md`.
- **Keputusan adapter**: bukan `pixi-live2d-display` (Pixi 6 only) dan bukan fork
  pihak ketiga — adapter tipis di atas **Cubism Framework renderer resmi**
  (`CubismRenderer_WebGL`) di atas GL context milik Pixi 8. Boundary:
  `createRenderer(w,h)` → `startUp(gl)` → `loadShaders()` → `bindTexture()` →
  `setRenderState(null,viewport)` → `setMvpMatrix()` → `drawModel()`.
- **Spike**: model SDK `Ren` (moc **v6**, 198 drawable / 24 offscreen,
  `blendModeEnabled=true`, `masking=true`) dimuat **tanpa MOC hack** —
  `C.Moc.fromArrayBuffer` mengenali v6 native. Bukti:
  `C:\Users\Admin\AppData\Local\Temp\opencode\lumi-cubism5r5-research-20260917\spike-log.txt`
  → `core 6000001`, `moc v6`, `drawable 198 offscreen 24`, `physics ok`,
  `texture bound`, `grid 5x5 #####` (semua piksel tergambar, bukan latar);
  `spike-proof.png` mengonfirmasi ada konten non-latar yang digambar.
- **Temuan penting**: (1) `CubismFramework.startUp()` tanpa `logFunction` membungkam
  semua log (`getLoggingLevel()=Off`) — spike sempat silent-fail; (2) shader load
  asinkron (`fetch` 13 vert/frag) — `isShaderLoaded=false` sampai selesai, frame
  awal hitam; (3) `app.ticker` tidak ada di build ESM Pixi 8 minimal — render
  manual `app.render()` atau rAF sendiri; (4) headless Playwright screenshot
  hang — bukti via `canvas.toDataURL` + `readPixels`.
- **Belum disentuh (sengaja)**: branch `migration/pixi8-cubism` belum dibuat
  (Fase 0), hack MOC lama belum dihapus (Fase 7), audit dependency lama belum
  ditulis ke `audit-old-live2d.md`, model bundel `data/model/*` belum diuji.
  Gate repo tetap hijau — verifikasi di bawah.

## UPDATE 2026-09-11 (26) — REFACTOR CLIENT: ROLE-SPACE, LIFECYCLE, KONTRAK BRIDGE, EKSPRESI TS (BELUM COMMIT)

Audit maintainability client dilanjutkan dengan tranche kecil, tanpa rewrite
`app.js` atau perubahan renderer/motion pipeline:
- **Inferensi emosi AgentBrain dikunci di role-space referensi**: pecahan
  semantik memakai `refHalfFor(role)` dan baru dipetakan sekali oleh render
  loop melalui `toActual`. Ini mencegah scaling ganda pada rig 0..100 atau
  asimetris; role yang tidak dimiliki model tetap menghasilkan nol.
- **Lifecycle UI eksplisit dan idempotent** (`src/client/lifecycle.ts`): timer,
  listener, dan AbortController dimiliki satu scope. Panel Assistant tidak
  dapat memasang poll setelah teardown, PanelView membersihkan timer status,
  dan rail projek melepas seluruh listener splitter serta menolak hasil draw
  async yang sudah basi. Start panel/rail kedua membongkar instance lama.
- **Boundary client lebih jelas**: API status/history/events Assistant dipisah
  ke modul typed, kontrak global `Window` dipusatkan, dan tipe wire browser
  dipindah ke `src/shared/browser-types.ts` dengan re-export kompatibilitas.
- **Kolektor ekspresi native dipindah ke TS murni**: union stabil dari
  `model.expressions`, `expressionManager.definitions`, dan
  `settings.expressions`; nama asli rigger dipertahankan dan getter rusak
  tidak menjatuhkan sumber lain. `app.js` sekarang adapter tipis dan fail-loud
  bila bundle belum dibangun, bukan diam-diam mengosongkan ekspresi.
- Guard source yang rapuh karena batas jarak karakter diperbarui agar tetap
  mengunci urutan perilaku fungsi lengkap; test baru mencakup role-space,
  lifecycle race, kolektor ekspresi, dan urutan bootstrap bridge.

Gate: **414 unit + 463 guard, 0 gagal**; build & `tsc --noEmit` bersih.

## UPDATE 2026-09-10 (25) — AUDIT KODE + PERBAIKAN: KOMIT RENDER, SHIM FAIL-LOUD, KEDIP rAF, ROLE-MAPPING TS (COMMIT)


Sesi audit implementasi Cubism dari kode (bukan dokumen). Seluruh WIP entri
(13)-(24) di-commit dalam 5 commit logis (27b95f8 render/core6, 1474b75
server TTS/bahasa/SSE, 9556c84 panel/akting, a1aa0ef app.js/UI, 80fd9a0 docs)
— tree bersih, gate penuh hijau sebelum tiap fase.

Perbaikan (semua sudah di-commit):
- **Shim core fail-loud** (app.js): moc3 v6+ TIDAK di-stamp lagi — log error
  eksplisit suruh update core; v5 masih di-stamp dengan warning. Stamp buta
  = kelas kegagalan senyap entri (19) tidak bisa terulang.
- **Kedip pindah ke tick rAF** (`tickBlink`, dt-based): setInterval 3 dtk +
  random-gate dihapus; interval natural, fase close/closed/open selalu
  dipulihkan saat freeze/model ganti, DIJEDA saat klip emosi memutar (kurva
  wink tidak ditimpa).
- **Breath sadar-motion**: `pokeRoleNorm("breath")` dilewati saat
  `motionLayersActive` — kurva motion3 yang membawa breath tidak ditimpa.
- **BUG EKSPRESI DITEMUKAN & DIPERBAIKI** (b88b825): `state.modelExpressions`
  dari `settings.expressions` DITIMPA baris berikutnya + `em.deferred` tidak
  ada di pixi-live2d 0.4.0 (yang benar `em.definitions`) → ekspresi native
  SELALU kosong untuk semua model (ren punya 5 .exp3 deklaratif, tak pernah
  muncul di vocabulary). Urutan prioritas ekspresi kini benar-benar sampai
  cabang native.
- **Diagnostik runtime**: `window.__live2dAgent.diagnostics()` — versi core
  vs moc (+peringatan core basi), status 4 patch lib, peta role + rentang
  param, probe sumber caps, flag sheet basi.
- **Stamp scannerVersion sheet** (server+client): simpan = stamp, GET =
  tandai `_stale` bila versi beda; client warning + flag `state.sheetStale`.
- **detectModelCapabilities**: warning eksplisit saat jalur resmi habis dan
  pencarian menyelam ke private field framework.
- **ROLE-MAPPING PORT KE TS** (`src/client/engine/role-mapping.ts`, sumber
  kebenaran tunggal): ROLE_KEYWORDS/GROUP_PATTERNS/mapRoles/pickFromGroup +
  role-space math (toActual/roleClampActual/normToRange/roleDefaultOf/
  writeRef/detectAccessories) — murni, diuji bun test. app.js jadi delegasi
  tipis via `window.__roleMapping` (fallback inline utk harness vm).
  Guard duplikat test-role-mapping.js & test-param-scaling.js (salinan port
  "keep in sync" manual) DIHAPUS — diganti test/role-mapping.test.ts (25
  test, termasuk rename-invariance & sheet-nyata). Guard app.js tetap:
  tabel bebas id bernomor kini menguji app.js TIDAK memuat tabel sendiri.
- **Belum dikerjakan (dicatat)**: pipeline offscreen Porter-Duff penuh
  (Atop iso-group masih aproksimasi) — tunggu bukti visual yang mengganggu;
  sheet lama perlu re-scan manual (GET kini menandai basi, belum auto).
Gate: **402 unit + 463 guard, 0 gagal**; build & tsc bersih.

## UPDATE 2026-09-09 (24) — GURATAN MERAH KELOPAK: TEKSTUR DI-PREMULTIPLY (BELUM COMMIT)

User kirim screenshot close-up: dua guratan cokelat-merah simetris di atas
kedua mata. Diagnosis: ren punya artmesh Atop (Face/Hairline, mis. ArtMesh82/
188/189) yang teksturnya menyimpan shade sebagai **RGB berwarna dengan ALPHA
0** (sample PNG di UV-nya: (116,46,46,0), (85,28,28,0)). Pixi 6 default
alphaMode NPM=0 — texel itu masuk GPU apa adanya; kapan pun artmesh digambar
(bahkan sebagai Over di antialias edge), RGB bocor → garis merah. Viewer
resmi mem-premultiply saat upload sehingga texel itu menjadi (0,0,0,0).

**PATCH 6:** opsi muat tekstur model di vendored lib (`Texture.fromURL`)
ditambah `alphaMode: 1` (PREMULTIPLY_ON_UPLOAD) — satu properti, memperbaiki
semua artmesh sekaligus tanpa menyentuh shader. Cache-bust lib ke `?v=3` di
3 HTML (pelajaran entri 23: subresource JS bisa basi walau HTML fresh).
Verifikasi (Browser Use, tab baru — capture clip di tab lama macet berulang,
tab baru normal): guratan merah hilang di pose netral/menoleh kanan/kiri;
model lain tak berubah (lumine tanpa texel semacam ini — premultiply
identitas untuk texel opaque). Gate: **377 unit + 550 guard, 0 gagal**;
tsc bersih. PATCH 3/4/5/6 + core 6.0.1 semuanya working tree (belum commit).

## UPDATE 2026-09-09 (23) — KEPALA REN: ATOP/OUT VIA CANVAS ALPHA + BLENDFUNC BENAR (BELUM COMMIT)

User: "bagian kepalanya masih aneh" — saat kepala menoleh muncul patch putih
(di akar: artmesh alpha Atop/Out digambar Over), dan dua percobaan perbaikan
pertamaku justru **wajah hitam bolong**. Diagnosis tuntas lewat instrumen:
`preserveDrawingBuffer` sementara + `readPixels` → area "hitam" ternyata
**RGBA(0,0,0,0) = wajah DIBOLONGKAN**, bukan dilukis hitam: blendFunc Out
versi lama memakai faktor dst ZERO sehingga operator Porter-Duff Out MENGANTI
seluruh dst — artmesh "Hair Shadow Out" (ArtMesh197, menutupi mulut-dagu)
menghapus pixel wajah.

Fix final (PATCH 4 lengkap):
- **Canvas transparan**: app.js `backgroundAlpha: 0` (warna latar tetap dari
  CSS #stage) → dst-alpha canvas = cakupan artmesh, prasyarat Atop/Out.
- **PATCH 4c**: `getDrawableBlendMode` membaca `blendModes` — keluarga warna
  → 3 mode lama (seperti 4a), byte alpha: Atop → case 3
  `(DST_ALPHA, ONE_MINUS_SRC_ALPHA, ZERO, ONE)`; Out → case 4
  `(ONE_MINUS_DST_ALPHA, ONE, ONE_MINUS_DST_ALPHA, ONE)` = "gambar hanya di
  area kosong, dst DIPERTAHANKAN" — bukan mengganti dst (bolong!). Atop
  Multiply (leher) → Multiplicative (butuh dst-RGB, blendFunc tak kuat).
- Deteksi alpha canvas **LAZY** `window.__l2dCanvasAlpha()` — panggilan
  getContext saat load-time MENCIPTAKAN context dan bisa merusak init pixi.
- Cache-bust: tag script lib di 3 HTML di-bump `?v=2` (JS subresource bisa
  basi di cache walau HTML fresh — bikin dua kesimpulan bisect palsu).
PELAJARAN BISECT: matikan-part via setPartOpacity TIDAK valid utk model v6
(cache faktor PATCH 5 beku + part opacity tak mengalir) — pakai
__l2dCanvasAlphaResult=false + readPixels, bukan tebakan part.
Hasil (Browser Use): kepala bersih di pose netral/menoleh kanan-kiri, hitam
hilang, kalung terlihat, lengan tetap transparan. readPixels wajah
(58,55,50,255) opaque, latar (22,18,12,0) transparan sesuai desain.
Sisa batas: Atop iso-group belum sempurna (gloss bisa bocor ke artmesh
non-group yang tumpang tindih), multiply-atop = aproksimasi.
Gate: **377 unit + 548 guard, 0 gagal**; tsc bersih.

## UPDATE 2026-09-09 (22) — LENGAN TRANSPARAN REN: PATCH 5 OPACITY GROUP OFFSCREEN (BELUM COMMIT)

User menunjukkan referensi tampilan resmi: **lengan jaket harusnya transparan**
(bahan bening, tangan terlihat menembus). Render kita masih solid. Peta akar
dari probe + cdi3: 24 offscreen group moc3 v6 — group 9/10 = PartJacketArmL/R
**op 0.50** (lengan transparan), 17/18 = ArmL/RDrawOrder **op 0.00** (layer
tersembunyi by design), 4/5 = Display/See-through op 0.6/0.3, 13/14/19/20/21
berisi artmesh Out (tabung gloss). Core 6 TIDAK membake opacity group itu ke
drawable opacity, dan parts.opacity ↔ offscreens.opacity tidak sinkron dua
arah via update() — framework lama menggambar semua full-opacity.

**PATCH 5:** `getDrawableOpacity` di vendored lib mengalikan opacity drawable
dengan faktor opacity group offscreen part-nya; peta part→faktor dibangun
sekali di `patchCore6Compat` (`model.__offGroupFactor`, cache Float32Array
per drawable). Group op 0 otomatis tak tergambar; lengan ×0.5 → transparan.
**Jebakan yang kena (terdokumentasi guard A3):** peta dipasang di CORE model
tapi versi pertama patch membacanya lewat `this.__offGroupFactor` (`this` =
wrapper framework, SELALU undefined → faktor 1, visual tak berubah); koreksi
`this._model.__offGroupFactor`. Verifikasi: Browser Use — lengan transparan
sesuai referensi, tangan terlihat menembus kain.
**Sisa batas (belum dikerjakan):** 16 artmesh alpha Atop/Out tetap digambar
Over (tabung gloss sedikit beda dari viewer resmi); pipeline resmi
5-r.5 = copy-buffer + frag shader Porter-Duff per group offscreen — port
besar, baru layak kalau bedanya masih terasa.
Gate: **377 unit + 544 guard, 0 gagal**; tsc bersih. Core 6.0.1 + PATCH 3/4a/5
di working tree (belum commit).

## UPDATE 2026-09-09 (21) — REN v6: BLENDMODES BARU + PELAJARAN PATCH YANG GAGAL (BELUM COMMIT)

Laporan user setelah entri (20): "masih aneh" (render sudah tampil tapi ada
artefak), lalu versi patch pertamaku justru bikin **wajah & kaos dalam jadi
hitam solid** — regresi yang kupicu sendiri, kubuktikan sendiri via browser
(Browser Use), kurevert sendiri.

Fakta rig ren (moc3 v6) dari probe core:
- `drawables.blendModes` (BARU di core 6, ada juga utk moc lama): low byte =
  ColorBlendType (Normal=0, AddCompatible=1, MultiplyCompatible=2, Add=3,
  AddGlow=4, Darken=5, Multiply=6, ColorBurn=7, Lighten=9, Screen=10,
  HardLight=14, Color=17, dst.), high byte = AlphaBlendType (Over=0, Atop=1,
  Out=2). Ren: 178 Normal, 5 Multiply, 4+1+2+1 glow/light/hardlight, 9 Atop,
  7 Out. Semua constantFlags blend bits v6 = 0 → framework lama menggambar
  SEMUA sebagai Normal (multiply tidak menggelapkan, glow tidak menyala).
- 24 offscreen per-part compositing; 81/198 drawable anak part ber-offscreen;
  offscreen opacity (0.6/0.3) TIDAK dibake ke drawable opacity oleh core, dan
  parts.opacity ↔ offscreen.opacity TIDAK sinkron dua arah via update().

**PATCH 4 (berlaku):** `getDrawableBlendMode` di vendored lib membaca
`blendModes` lebih dulu dan memetakan HANYA keluarga warna → 3 mode lama
(glow: Add/AddGlow/Lighten/Screen/ColorDodge/AddCompatible → Additive;
darken: MultiplyCompatible/Darken/Multiply/ColorBurn/LinearBurn →
Multiplicative; sisanya Normal). Fallback constantFlags utk core lama tetap.
**PELAJARAN (jangan diulang):** versi pertamaku juga memetakan byte alpha
Atop/Out ke blendFunc baru langsung di framebuffer utama — HASILNYA HITAM
(wajah/kaos). Renderer resmi 5-r.5 menggambar Atop/Out di dalam offscreen
render-target per group (isBlendModeEnabled → _modelRenderTargets) — tanpa
pipeline itu, Atop/Out HARUS digambar Over biasa. Guard A2 di
test-core6-compat.js sekarang MENOLAK kehadiran string Atop/Out blendFunc.
**Batas arsitektur yang diketahui (belum dikerjakan):** (1) alpha Atop/Out 16
artmesh digambar Over — highlight/gloss moc v6 bisa tampak sedikit beda dari
viewer resmi; (2) offscreen compositing + opacity part (0.6/0.3) belum
diterapkan; (3) ekspresi model ren: getExpressibleEmotions() kosong (belum
diinvestigasi, terpisah dari render). Fidelity penuh = renderer 5-r.5.
Verifikasi visual (Browser Use, bukan laporan user): model tampil benar,
wajah normal, mask utuh saat ParamAngleX=30 + physics, console bersih.
Gate: **377 unit + 538 guard, 0 gagal**; tsc bersih. Core 6.0.1 + PATCH 3 +
PATCH 4a semuanya di working tree (belum commit).

## UPDATE 2026-09-09 (20) — REGRESI CORE 6: renderOrders OBJECTS-UNION + PATCH 3 LIB (BELUM COMMIT)

Lanjutan entri (19): setelah swap core 6.0.1, **karakter tak tergambar sama
sekali** (blank, tanpa error console). Akar: core 6.0.1 menghapus
`drawables.renderOrders` (dipindah ke `Model.getRenderOrders()`) DAN di moc v6
nilai itu adalah **objects-union** — order gabungan drawable (0..count-1) +
offscreens (count..count+offscreen-1); ren = 198 drawable + 24 offscreen =
222 entri. Framework vendored era core 4 (pixi-live2d-0.4.0.js) membaca
`drawables.renderOrders` → undefined → `_sortedDrawableIndexList` kosong →
semua drawable dianggap tak terlihat. Bahkan delegasi naif salah: framework
lama mengiterasi slot `0..count-1` pada array (bukan peta), nilai order sparse
(dibumbui offset offscreen) membuat slot kosong = drawable terlewat senyap.

Fix — **PATCH 3** di `static/js/pixi-live2d-0.4.0.js` (pola patch existing:
#1 doDrawModel/__mcDraw, #2 setupShaderProgram multiplyColor): IIFE
`patchCore6Compat` membungkus `Live2DCubismCore.Model.fromMoc`; saat
`drawables.renderOrders === undefined` (core 6+) menempel getter yang
mengembalikan **permutasi padat 0..n-1** drawable — sort drawable by order
asli, rem tie by indeks; offscreen diabaikan (framework lama tak mengenalnya).
Core lama 5.1.0 (moc v5 orders length === count) tak tersentuh — jalur
passthrough. Perubahan user-terlihat: karakter tampil kembali + model moc3 v6
(tesmodel/ren) render dengan mask/clip/physics benar.
Guard baru **test-core6-compat.js** (10 assertion; suites 11→12): string-match
level sumber PATCH 3 + **eksekusi nyata via Bun** — eval core+patch, `Model.
fromMoc` moc v5 (lumine) & v6 (ren), wajib Int32Array sepanjang drawable count
(jebakan v6: delegasi naif mengembalikan 222 entri untuk 198 drawable — guard
menangkapnya). Urutan eval penting di probe: core dulu TANPA window (emscripten
pilih env node), baru `globalThis.window=globalThis` sebelum eval patch.
Gate: **377 unit + 533 guard, 0 gagal**; tsc bersih. Verifikasi user: Ctrl+F5.

## UPDATE 2026-09-09 (19) — MODEL MOC3 v6: CORE DI-SWAP KE 6.0.1 (SDK WEB 5-r.5) (BELUM COMMIT)

Laporan user: model baru ("tesmodel"/ren) **masking texture, physics, dan
clipping berantakan**. Akar masalah TERBUKTI via probe Bun eksekusi sungguhan:
`ren.moc3` = moc3 **versi 6** (byte ke-4 = 0x06, Cubism 5.1 export), sedangkan
core terpasang **5.1.0** hanya mengenal sampai `MocVersion_50 = 5` —
`Moc.fromArrayBuffer` mengembalikan NULL → shim `patchCubismCore` (app.js)
men-stamp 6→4 buta → moc v6 dibaca dengan layout v4 → **mask/clip/texture dan
physics rusak senyap**, persis pola kegagalan entri lama (shim stamp v5→v4).
Inventaris: lumine = v5, 神宫白子 = v4, ren = v6 — hanya v6 yang korban.

Fix: `static/js/live2dcubismcore.min.js` di-swap ke **Core 6.0.1** dari zip
resmi `CubismSdkForWeb-5-r.5.zip` (cubism.live2d.com/sdk-web/bin/ — CDN core
masih menyajikan 5.1.0 identik hash `25ae938c…`, jadi ambil dari zip SDK).
Bukti: `MocVersion_53 = 6` ada di core baru; probe env node — ren v6 load **OK
non-null** (getMocVersion 6), lumine v5 OK, 神宫白子 v4 OK (backward-compat
terjaga). Changelog 5-r.5 (2026-04-02) bahkan menyebut fix "unnecessary
multiply color and screen color settings in mask drawing". Wasm ter-embed
base64 dalam satu file — tanpa fetch eksternal. Guard `test-multiply-color.js`
tetap hijau (API `csmGetDrawableMultiplyColors` masih ada). Shim stamp 6→4 di
app.js tidak diubah: moc v6 kini genuine-load jadi shim tak terpicu; kalau
kelak ada moc v8+, shim itu akan salah stamp lagi (catatan kesadaran, bukan
bengkel sekarang).
Gate: **377 unit + 523 guard, 0 gagal**; tsc bersih. Verifikasi user: hard
reload (Ctrl+F5) — core client-side, tak perlu restart server.

## UPDATE 2026-09-09 (18) — UCAPAN DETERMINISTIK: TERJEMAHAN PINDAH KE SERVER (BELUM COMMIT)

Laporan user: "kadang ngikutin teks yang ditulis user, kadang ngikutin
dropdown" — suara tidak konsisten. Akar masalah: terjemahan ucapan dulu
DILAKUKAN DI CLIENT (fetch /api/tts/translate sebelum TTS) — kalau LLM
role "chat" lambat/gagal/timeout 30 dtk, client diam-diam jatuh ke teks
asli → kadang audio bahasa user, kadang bahasa dropdown. Ditambah kebocoran
tampilan: pipeline doRemoteTTS menampilkan kalimat yang dibacakan di bubble,
jadi teks terjemahan ikut tampil berbahasa asing.

Redesain — SATU titik keputusan di server:
- **/api/tts menerima `ttsLang` opsional**: bila "Bahasa suara" tetap,
  handleTTS menerjemahkan teks DULU (translateForSpeech — LLM role chat,
  cache LRU) lalu menyintesis. Semua request TTS remote kini melalui logika
  yang sama → ucapan TIDAK bisa berganti bahasa sendiri.
- **Skip cerdas**: teks yang sudah berbahasa target TIDAK diterjemahkan
  (detectSpeechLangBase — heuristic skrip+kata layak Indonesia, cermin
  detectTextLang di app.js; dikunci test). User menulis Jepang + suara
  ja-JP = nol panggilan LLM.
- **Bubble tetap bahasa user**: client mengirim kalimat ASLI per segmen;
  terjemahan hanya di sisi audio. (Dulu bubble ikut menampilkan terjemahan.)
- speak() app.js: jalur remote TANPA pre-translate (cukup meneruskan
  ttsLang); jalur suara browser tetap pre-translate via /api/tts/translate
  (Web Speech butuh teks lokal). fetchTTSAudio: timeout 45 dtk saat
  ttsLang aktif (menampung latensi LLM terjemahan). window.__debugSpeak
  kini terpasang apa pun provider — jalur VTuber/mode-runtime selalu masuk
  pipeline ini.
Gate: **377 unit + 523 guard, 0 gagal**; build & tsc bersih. Entri (13)-(18)
masih di working tree; perlu restart server + reload halaman.

## UPDATE 2026-09-09 (17) — "BAHASA SUARA" TETAP = UCAPAN DITERJEMAHKAN, TEKS TETAP BAHASA USER (BELUM COMMIT)

Pertanyaan user: dari dua setelan bahasa, mana yang mengurus ucapan narator?
Dan bisa nggak: user menulis Indonesia → teks balasan tetap Indonesia tapi
suara berbahasa lain (mis. Jepang)? Jawaban lama: "Bahasa" (UI) = UI +
baseline balasan; "Bahasa suara" (ttsLang) HANYA pemilih voice TTS — teks
tidak pernah diterjemahkan, voice ja-JP membaca teks Indonesia mentah-mentah
(hasil aneh).

Sekarang "Bahasa suara" punya makna penuh, dua mode:
- **"Ikuti bahasa teks" (nilai `auto`, DEFAULT baru)** — suara mengikuti
  bahasa balasan. Deteksi bahasa teks heuristic script tanpa jaringan
  (`detectTextLang`: kana→ja, hanzi→zh, hangul→ko, kata layak Indonesia≥20%
  →id, selainnya→en). browserTTS memakai `ttsLangForText()` untuk u.lang +
  pemilihan voice; voice "Suara Sistem" yang dipin user tetap menang (itu
  fungsinya — pilih "(otomatis)" bila mau suara mengikuti bahasa).
- **Bahasa tetap (id-ID/ja-JP/en-US/zh-CN)** — teks yang DIBACAKAN adalah
  TERJEMAHANNYA ke bahasa itu; teks di bubble & chat log tetap bahasa
  balasan (cermin user). Terjemahan via route baru **POST /api/tts/translate**
  (LLM role "chat", prompt bahasa target dari map locale, cache LRU 200 di
  `src/server/persona/speech-lang.ts`, gagal = teks asli — TTS tetap jalan).
  speak() app.js memanggilnya sebelum splitSpeechSegments; fallbackTimer
  di-re-arm setelah terjemahan agar audio tidak terpotong.

Perubahan file: speech-lang.ts (baru, murni + injectable LLM call),
index.ts (route+handler), app.js (speak(), browserTTS, detectTextLang,
ttsLangForText, MODEL_CONFIG_DEFAULTS ttsLang:"auto", normalisasi menerima
"auto"), index.html (option auto + hint), dict-id/en (+2 kunci).
Guard: config lama bersave id-ID/ja-JP tetap berlaku (mode tetap); yang
belum pernah di-set dapat auto. Gate: **375 unit + 515 guard, 0 gagal**;
build & tsc bersih. Entri (13)-(17) masih di working tree.

## UPDATE 2026-09-09 (16) — KARAKTER CERMINKAN BAHASA USER (BELUM COMMIT)

Laporan user: karakter selalu membalas bahasa Indonesia meski user menulis
bahasa lain. Penyebab: (a) buildSystem di agent loop menulis literal
"Jawab user dalam bahasa Indonesia." di kedua varian prompt; (b) prompt
brain (mode chat) sepenuhnya Indonesia tanpa aturan bahasa sama sekali;
(c) narrator memaksa bahasa config.i18n.

Perubahan (aturan cermin, konsisten di 4 titik):
- loop.ts `buildSystem` — rule final id: "Balas dalam bahasa yang SAMA
  dengan bahasa yang dipakai user… (campuran → dominan; istilah teknis
  tetap apa adanya)"; varian en dibuat mirror juga (bukan "Reply in
  English"). Varian prompt id/en tetap ada — yang berubah hanya aturan
  bahasa jawabannya.
- brain.ts `buildSystemPrompt` — block "=== BAHASA ===" mirror-bahasa
  ditambahkan untuk SEMUA lang, dengan penegasan kata kunci directive
  tetap kosakata Indonesia (protokol). Block "=== LANGUAGE ===" (UI
  pilihan en) ditambahkan SESUDAHNYA sehingga tetap menang bila user
  eksplisit pilih English di UI.
- narrator.ts — simpulan akhir meniru bahasa teks hasil kerja agent.
- quip (/api/assistant/quip) SENGAJA TIDAK diubah: label event itu
  teknis (nama tool Inggris) — mencerminkan label membuat komentar
  sampingan berubah bahasa sendiri di UI Indonesia; quip tetap mengikuti
  bahasa UI (config.i18n).
Gate: **369 unit + 515 guard, 0 gagal**; build & tsc bersih. Entri (13)-
(16) semua masih di working tree belum commit.

## UPDATE 2026-09-09 (15) — EKSPRESI MODEL ASLI MENANG ATAS HARDCODE (BELUM COMMIT)

Laporan user: ekspresi karakter ketimpa emosi hardcode engine padahal model
punya .exp3/emote sendiri. Akar masalahnya TIGA lapis: (a)
`refreshRoleEmotions()` mengisi `supportedEmotions` dari
`EMOTION_ROLE_TEMPLATES` (hardcode senang/sedih/... di app.js); (b)
`applyExpression()` memeriksa vocab itu SEBELUM mencoba `.exp3` bawaan model
(cabang native hanya untuk nama yang TIDAK ada di vocab); (c)
`getCapabilityProfile().emotions` mengiklankan emosi hardcode sebagai
kemampuan model ke prompt LLM.

Urutan prioritas BARU (app.js + brain.ts applyActions):
1. **Preset emosi USER** (sheet.presets.user kategori emosi — aturan sheet
   user > segalanya).
2. **Ekspresi bawaan model (.exp3)** — pencocokan case-insensitive ke
   `state.modelExpressions`.
3. **Klip emosi terukur** dari disk (MotionTaxonomy EMOTION_VERBS →
   playEmotionClip).
4. **Template sintetis role-space** (state.roleEmotions dari
   EMOTION_ROLE_TEMPLATES) — HANYA untuk model yang tidak punya apa-apa
   sendiri; nama template tidak lagi masuk supportedEmotions sheet/state.

Perubahan detail:
- `refreshRoleEmotions()` — supportedEmotions = {} + klip emosi (nilai null
  = "mainkan klip"); roleEmotions tetap dibangun sebagai fallback runtime.
  Dipanggil ulang setelah loadMotionTaxonomy (dulu klip kosong karena
  taxonomy async datang setelah detectModelCapabilities).
- `projectEmotionPresets(sheet)` — sheet.supportedEmotions hanya berisi
  preset user; emosi sintetis builtin TIDAK lagi diproyeksikan ke sheet.
- `inspectModel()` — sheet baru dibuat dengan supportedEmotions = {} (dulu
  menanam hasil buildRoleEmotions()).
- `applyExpression()` ditulis ulang: user-preset → .exp3 (nativeName) →
  klip (userEntry===null) → sintetis (state.roleEmotions) → fireOverlay
  untuk nama asing. Log label baru (User emotion preset / Native
  expression / Emotion clip / Synthetic emotion (fallback)).
- `getExpressibleEmotions()` — via jujur: param(preset user) → native(.exp3)
  → clip; emosi sintetis tidak diiklankan.
- `getCapabilityProfile().emotions` — vocab model dulu, nama template
  sintetis dicantumkan hanya sebagai cadangan kosakata prompt (runtime tetap
  prioritaskan yang asli).
- brain.ts `applyActions`: `emotionVia` dari getExpressibleEmotions — pose
  inferensi & gesture fallback (EMOTION_GESTURE_FALLBACK) DILEWATI bila
  emosi dimainkan dari native/clip (aset model membawa face+body sendiri;
  menumpuk pose tebakan merusak ekspresi asli). [GESTURE:] eksplisit LLM
  tetap selalu dipakai.
Guard: test-emotion-overlay.js diperbarui (3 assertion baru: nativeName
case-insensitive, Object.assign roleEmotions dihapus, inspectModel tak
menanam sintetis) — 515 guard. Gate: **369 unit + 515 guard, 0 gagal**;
build & tsc bersih. Sesi (13)(14)(15) semua masih di working tree.

## UPDATE 2026-09-09 (14) — SSE AGENT TERPOTONG 10 DTK OLEH BUN idleTimeout (BELUM COMMIT)

Laporan user: stream agent "kadang berhenti, nggak streaming terus, atau
reconnect ulang". Diagnosis terukur: **Bun 1.4.0 memutus koneksi yang senyap
>10 detik** (default `idleTimeout: 10`) — direproduksi dengan server uji:
stream SSE yang tidak mengirim apa pun terpotong ECONNRESET tepat 10 dtk,
Bun mencetak "timed out a request after 10 seconds. Pass `idleTimeout` to
configure." Korban di app: ask-stream/approve-stream senyap saat tool
berjalan lama, narrate di akhir tugas, atau menunggu approval → panel masuk
jalur follow-bus (terlihat "berhenti/reconnect"); route non-SSE yang handler-
nya menunggu LLM >10 dtk juga berpotensi terpotong.

Perbaikan `src/server/index.ts`:
- `Bun.serve({ idleTimeout: 255 })` — nilai maksimum yang diizinkan (detik).
- Helper `makeSseStream()` (DRY untuk ask-stream & approve-stream):
  **heartbeat** komentar SSE `": ka\n\n"` tiap 5 dtk (parser klien drainSse
  melewatkan frame tanpa `data:` — dikunci test baru). Heartbeat menutupi
  senyap >255 dtk (menunggu approval ber menit) dan menjaga proxy/browser
  tak menganggap koneksi mati.
- Perilaku `clientGone` (enqueue ke stream mati tak menggagalkan tugas)
  dipertahankan persis.
Test: `drainSse` komentar heartbeat → 0 event (test/agent-panel.test.ts).
Gate: **369 unit + 512 guard, 0 gagal**; build & tsc bersih. CATATAN:
perlu **restart server** (`bun run start`) supaya idleTimeout & heartbeat
aktif; perubahan (13) dan (14) keduanya masih di working tree.

## UPDATE 2026-09-09 (13) — AKTING AGENT: LLM-DULU, TEMPLATE JADI JARING PENGAMAN (BELUM COMMIT)

Laporan user: di mode agent karakter cuma mengucap kata template
(`as.actor.*`: "Oke, aku pantauin ya…", "Hmm, dia lagi periksa berkas…")
dan tidak memantau/melaporkan/menyimpulkan pekerjaan agent. Diagnosis:
LLM role "chat" sehat (probe /api/assistant/quip merespons berkarakter),
tapi actor.ts (a) mengucapkan fallback template LANGSUNG lalu quip LLM
menyusul di belakang, (b) mayoritas event di-hardcode `allowLLM: false`
(tool_call_start, final_answer, plan_revised, verification_result,
subagent_spawned — alias seluruh momen kerja), dan (c) cooldown quip
berarti template dikucurkan tanpa henti.

Perubahan `src/client/agent/panel/actor.ts`:
- **Urutan dibalik**: quip LLM diminta DULU; template hanya diucapkan bila
  quip gagal (post reject → segera) atau tak datang dalam `fallbackMs`
  (default 4500 ms, injectable untuk test). Quip datang → template
  DIBATALKAN (timer fallback diclear). Quip datang setelah template
  terlanjur → dibuang (tidak dobel bicara).
- **Semua event kerja kini boleh minta quip** (allowLLM true): tool_call_start,
  final_answer, plan_revised, verification_result gagal, subagent_spawned,
  error, thinking_start. `tool_call_end` tanpa komentar (reaksi pandang saja).
- **Label event asli dikirim ke prompt quip** (`event: ev.label` — mis.
  "read_file package.json") supaya komentar spesifik, bukan generik.
- **Cooldown quip 25s → 15s, dan dalam cooldown = DIAM** (bukan template
  kaleng). Cooldown berarti "baru bicara", bukan izin spam.
- `speak` (bus) kini menclear fallback timer — komentar persona server tidak
  tertimpa template yang menggantung.
Test: test/agent-panel.test.ts ditulis ulang untuk semantik baru (9 test
actor: quip menang, fallback timer, reject → template segera, cooldown =
diam, label ke prompt, dsb). Gate: **368 unit + 512 guard, 0 gagal**;
build & tsc bersih. CATATAN: perubahan ini ADA DI WORKING TREE BELUM
DI-COMMIT — bercampur WIP lain (brain timer, panel draft-clear, stage-hint,
vtuber inject "agent", dsb.) dari sesi sebelumnya.

## UPDATE 2026-09-08 (12) — MODE AGENT: LEBAR DEFAULT + PANGGUNG BERSIH PER-MODE (a2ef22e)

Finalisasi permintaan user (hanya mode Agent; mode lain TANPA perubahan):

- **Lebar workspace agent**: `.agent-wide` 860px →
  `clamp(650px, 100vw - 470px, 1200px)` — mengikuti viewport (1280 CSS ≈
  810; 1920 mentok 1200); stage min-width 340 tetap terlindungi.
  Preferensi drag tersimpan (inline basis) tetap menang di atas default.
  `.agent-wide.tech-collapsed 558px !important` dicabut — bentrok clamp.
- **Panggung bersih per-mode**: `body.mode-agent` (toggle di
  mode-runtime `setPanel`) menyembunyikan `#hint`, `#btn-fullbody`,
  `#live-state` via CSS `display:none !important`. HUD DIKEMBALIKAN ke
  index.html — commit paralel f27868a sempat menghapusnya permanen
  (hilang di semua mode, melanggar batasan user); entri (12) lama yang
  mendaftar penghapusan permanen dicabut. Mode lain: HUD normal kembali.
  app.js null-safe untuk ketiganya (initLiveStateIndicator, fbBtn).
Gate: **362 unit + 512 guard, 0 gagal**; build & tsc bersih; i18n OK.

## UPDATE 2026-09-08 (11) — VTUBER: FEED LIVE UTAMA, KONFIG POPUP (f8b4dae)

Permintaan user: panel kanan mode VTuber diutamakan Feed Live; sisanya
popup. Sekarang #mode-vtuber hanya: baris Mulai/Berhenti/status + tombol
**Pengaturan**, lalu Feed Live mengisi sisa tinggi. Form konfigurasi
(platform/channel YT-Twitch/gaya jawab/toggle AI-balas & donasi/jeda/
Overlay OBS + hint) pindah ke popup **#vt-config** (fixed kanan, pola
controls-panel; ✕ / Escape menutup — Escape menutup popup dulu sebelum
controls-panel).

Kunci desain: SEMUA ID tetap (vt-provider/channel/video-id/yt-key/persona/
respond/donate-respond/cooldown/overlay-open/start/stop/status/feed/
alert) → mode-runtime.js (poll 2.5 dtk, provider change row-toggle,
overlay OBS) tanpa adapter; vt-alert tetap di panel utama. i18n +3 kunci
(vt.cfgOpen/cfgOpenTip/cfgTitle) di id+en. Mulai/Berhenti bisa dipakai
tanpa membuka popup (setting terakhir dipakai).
Gate: **362 unit + 512 guard, 0 gagal**; build & tsc bersih.

## UPDATE 2026-09-08 (10) — RAIL PROJEK POPUP MELAYANG (c5ab550)

Permintaan user: panel projek boleh muncul di posisi sama tapi JANGAN
mengecilkan panel Live2D. `#projek-rail` dari kolom flex (mendorong
layout) → **popup absolute**: anchor `#left-workspace` (kini
`position: relative`), menempel samping activity bar (`left: 66px`),
melayang di atas stage — bayangan + border `--line-strong`, lebar 264px,
z-index 30. Stage tak tersentuh: `measureOccupied` di projek.ts otomatis
benar karena offsetWidth left-workspace tetap 56 saat rail melayang.

Default auto-open ≥1600px DILEPAS — popup menutupi stage, jadi tak
membuka sendiri tanpa pilihan eksplisit (localStorage `projekRail.open`
tetap dihormati; komentar di projek.ts menjelaskan).
Gate: **362 unit + 512 guard, 0 gagal**; build & tsc bersih.

## UPDATE 2026-09-08 (9) — IKON DESAIN SENDIRI UNTUK VTUBER/ASSISTANT/PET (b3082a9)

Masukan user: Video (kamera) generik kurang pas untuk VTuber; Sparkles
'AI magic' kurang mewakili agent. Tiga SVG dirancang sendiri (stroke 1.6,
currentColor, 24×24 — koheren dengan set Reicon yang bergaris):

- **VTuber**: kepala+bahu avatar + dot LIVE ber-arc gelombang siaran di
  sudut — "orang yang sedang live", bukan kamera.
- **Assistant**: kepala robot — antena dot, dua mata, stub kuping, mulut.
- **Pet**: paw versi stroke terbuka (4 jari ellipse + telapak outline),
  bukan blob fill — supaya senada gaya garis.
- Chat tetap Reicon Message. Karan gaya: 2 ikon stroke → paw fill ikut
  dikonversi stroke demi koherensi.
Gate: **362 unit + 512 guard, 0 gagal**; build & tsc bersih.

## UPDATE 2026-09-08 (8) — IKON SET REICON DI ACTIVITY BAR (4027ea4)

User minta pelajari reicon.dev dan terapkan + selesaikan overlap tombol
mode (CHAT/VTUBER/ASSISTANT/PET overflow di bar sempit). Riset: Reicon =
library ikon SVG open-source **MIT** (2676+ ikon, Outline & Filled, paket
npm `reicon` + ikonify). Konsumsi dipilih **inline SVG copy** — cocok
prinsip zero-dep client (tanpa npm/CDN runtime), pattern sama dengan
Feather yang sudah ada.

- Mode switcher: Chat→Message, VTuber→Video, Assistant→Sparkles,
  Pet→Paw. `data-mode` & tooltip i18n dipertahankan → wiring
  mode-runtime.js (switcher bergantung tombol data-mode) tak tersentuh.
  Label teks keluar dari DOM; kunci `top.tab.*` tinggal di dict (guard
  i18n hanya parity dict ↔ dict + coverage HTML→dict, jadi aman).
- Gear ⚙ dan folder rail diganti path Reicon (Settings/Folder) — satu
  gaya ikon di activity bar.
- CSS: tombol mode 40×34 → 38×38 persegi; `::before
  attr(data-mode)` (singkatan teks) dihapus; dot status agent tetap.
- Kalau nanti butuh ikon lain: ambil dari `reicon` npm (tar paket →
  icons/*.js punya path `O:` outline), tempel sebagai inline SVG
  `fill="none"` + path `fill="currentColor"` — JANGAN tambah dependensi.
- Gate: **362 unit + 512 guard, 0 gagal**; build & tsc bersih.

## UPDATE 2026-09-08 (7) — GEAR GANDA + GRIP SPLITTER (bdfe04f)

Laporan user: di sebelah Clear masih ada tombol lagi (⚙). Akar: kelalaian
di 2422809 — ⚙ baru ditambah ke activity bar tapi ⚙ header lama tak
dihapus → dua tombol, dan yang di header MATI karena ID kembar
(`$("#btn-toggle-controls")` menangkap yang pertama di DOM). Header kini
avatar + nama + Clear saja.

Splitter dipertegas sesuai permintaan: grip 3×56 → 4×96px warna `--muted`
(lebih kontras), hover/drag 5×160px `--lamp`; hit area 8→10px.
Gate: **362 unit + 512 guard, 0 gagal**; build & tsc bersih.

## UPDATE 2026-09-08 (6) — KOREKSI: REVERT b42e6aa + PERMINTAAN ASLI (2422809)

User mengoreksi b42e6aa: yang diminta HANYA (1) hapus ☰ overlay stage,
(2) pindah ⚙ ke panel kiri paling bawah — bukan memindahkan composer chat.
Pelajaran: instruksi beranotasi dieksekusi apa adanya; jangan improvisasi
di luar anotasi meski terlihat "cocok secara desain".

- `git revert b42e6aa` (38178c0) — composer kembali ke sidebar, overlay
  stage kembali semula; lalu di atasnya: ☰ stage dihapus, `⚙
  btn-toggle-controls` (ID tetap) jadi `.act-btn` ber-svg gear di dasar
  `#activity` (spacer `.act-flex`); gaya tombol header lama dilepas;
  wiring `stageBtn` null-guard.
- **Jebakan revert yang perlu diingat**: `git revert` menulis ulang 3 file
  static dengan **CRLF** (repo memakai LF). Guard legacy yang menghitung
  jarak karakter antar pola (test-param-notes-ui.js `releasePresetPose
  total`) melewati ambang hanya karena `\r` tambahan per baris
  (d3: 1973→2023, ambang 2000). Solusi: normalisasi `sed -i 's/\r$//'`
  kembali ke LF — jangan ubah ambang guard untuk itu.
- Gate: **362 unit + 512 guard, 0 gagal**; build & tsc bersih.

## UPDATE 2026-09-08 (5) — BAR CHAT DI BAWAH STAGE (b42e6aa) — DIREVERT

Anotasi user pada screenshot: **Hapus** Full Body + ☰ overlay pojok stage
dan ⚙ di header chat; **Pindah** composer chat (Mic/ketik/Kirim/frasa/Mode
Otak) ke bar di bawah stage.

- `#stage-chat-bar` baru di dalam `#stage`: Mic + input + Kirim + toggle
  Mode Otak + ☰ (satu-satunya pintu `#controls-panel` sekarang; ⚙ header
  dihapus) + Full Body kontekstual (muncul setelah zoom manual — perilaku
  `state._showFullBtn` tak berubah). SEMUA ID dipertahankan → wiring app.js,
  voice-input, i18n tak tersentuh; composer lama keluar dari `#mode-chat`,
  quick phrases tetap di sidebar.
- CSS: `.stage-ctl` & `#btn-fullbody` dari absolute → static di bar;
  `#live-state` naik ke `bottom: 58px`; bar `flex-wrap` di <1280px (Mode
  Otak turun baris). `frameModel` + `ResizeObserver` (175c6a9/57eb5a0)
  otomatis mem-frame ulang karakter karena stage menyusut ±52px.
- Catatan port: pemindahan ini menyentuh UI legacy app.js — hanya wiring
  tombol (null-guard), logic tetap di app.js; quick phrases & log chat
  belum dipindah (menunggu arahan).
- Gate: **362 unit + 512 guard, 0 gagal**; build & tsc bersih.

## UPDATE 2026-09-08 (4) — SPLITTER AKTIF DI 1025–1499PX (fa3f8ed)

User: panel tetap tak bisa digeser (screenshot assistant, tech pane di bawah
percakapan). Akar terverifikasi dari screenshot: mesin user 1920 fisik @
scaling 150% → viewport CSS ±1280 saat maximize → CSS lama menyembunyikan
`#sb-gutter` di ≤1499px, jadi splitter TIDAK PERNAH ada di rentang itu
(kesalahan dugaan "125%" pada entri (2) dikoreksi: sebenarnya 150%).

- CSS: gutter disembunyikan hanya ≤1024px (shell kolom penuh). 1025–1499px:
  .app tetap flex row → flex-basis tetap berarti LEBAR; default 372/540px
  dipertahankan untuk kondisi tanpa ukuran tersimpan (inline menimpa).
- projek.ts: MIN_DESKTOP_W 1025px; `measureOccupied` mengukur anak .app
  nyata (gutter display:none → 0, gap hanya kolom terlihat); floor
  kontekstual — 722px hanya jika tech di samping percakapan (≥1500px +
  agent-wide), bertumpuk 372px. Rentang rasio 20/80 … 50/50 kini dicapai
  (batas bawah stage = min-width 340px ≈ 27% di 1280).
- Regresi: test 1280px (drag 700 → stage 466; 50/50 = 583; tame 372;
  anti-gepeng upper stage 715↔466 skala identik).
- Gate: **362 unit + 512 guard, 0 gagal**; build & tsc bersih.

## UPDATE 2026-09-08 (3) — SPLITTER EKSPLISIT + FRAMING ANTI-GEPENG (175c6a9)

Panduan user perbaikan berikutnya (poin 1–2 dieksekusi; poin 3 menunggu
arahan): (1) panel Live2D resize dengan splitter vertikal yang jelas;
(2) panel kiri diperkecil tidak boleh membuat karakter "gepeng" — canvas
jaga rasio, zoom/pan yang menyesuaikan; (3) panel kanan responsif per mode.

- **Anti-gepeng** (`engine/framing.ts` baru, murni → `window.__framing`):
  akar masalah — frameModel legacy membatasi skala dengan lebar stage
  (`min(stageW/natW, H/natH)`) sehingga tiap panel menyempit karakter ikut
  mengecil. Kini `computeFrame` untuk mode `upper`/`full` hanya fungsi
  TINGGI (105%/82% H); lebar hanya menggeser pan (x center) & memotong sisi.
  Hanya `fit` (dobel-klik/reset) yang menyesuaikan lebar. frameModel legacy
  memakai computeFrame dengan fallback rumus lama (degrade manis). Perhatian:
  port "port saat disentuh" — logika framing kini sumber-kebenaran TS,
  app.js tipis; guard = test/framing.test.ts (invarian skala identik untuk
  lebar 840↔372px, tinggi sama).
- **Splitter eksplisit** (CSS `#sb-gutter`): grip bar vertikal 3×56px selalu
  terlihat (`--line-strong`), membesar 96px & menyala `--lamp` saat
  hover/drag; hit area 8px. Drag tetap lewat projek.ts (floor kontekstual,
  state tersimpan — db9d0f2).
- **Panel kanan per mode**: chat & VTuber sudah full-height (sidebar flex
  column, chat-log flex). Assistant menunggu penjelasan user — jangan
  berimprovisasi.
- Gate: **359 unit + 512 guard, 0 gagal**; build & tsc bersih.

## UPDATE 2026-09-08 (2) — PANEL LIVE2D RESIZE + STATE TERSIMPAN (db9d0f2)

Lanjutan permintaan user: panel Live2D harus bisa diresize dan ukuran terakhir
tersimpan. Temuan: gutter sudah ada, tapi floor workspace 650px membatasi
stage maksimal ±530px di layar 1536px, dan nilai tersimpan warisan migrasi
(±1040px) otomatis menjadikan stage strip tiap maximize (floor clamp lama
hanya menjamin stage ≥340px — tetap strip).

Fix: floor drag kontekstual — 372px (chat/vtuber) atau 722px (assistant =
tech 340 + gap + percakapan min) di `shell/workspace.ts::workspaceFloor`;
`clampWorkspaceBasis` (drag, stage ≥340) dan `tameStoredWorkspaceBasis`
(restore: stage ≥45% ruang panel, hanya memangkas — preferensi kecil sadar
tetap dihormati). `MIN_DESKTOP_W` 1280→1500: di bawahnya layout bertumpuk dan
inline basis berarti HEIGHT (quirk lama ikut dibersihkan). Mouseup menyimpan
nilai terlihat (hasil clamp), bukan keinginan mentah. Stage = sisa ruang,
di-frame ulang otomatis oleh ResizeObserver `57eb5a0` saat drag.

Konsekuensi user: setelah update, nilai lama 1040 dibaca menjadi ±649 saat
maximize 1536 (stage ±531). Drag gutter ke kiri → stage hingga ±808 (chat
mode). Dobel-klik gutter = reset ke default CSS. Catatan terbuka: default
`.agent-wide` 860px di mode assistant pada layar 1536px tetap menyisakan
stage ±326px (pre-existing, belum disentuh — kandidat perbaikan berikutnya).
Gate: **351 unit + 512 guard, 0 gagal**; build & tsc bersih.

## UPDATE 2026-09-08 — STAGE TENGGELAM SAAT MAXIMIZE (FIX 57eb5a0)

Laporan user: restored (gambar 1) stage normal; maximized (gambar 2) stage jadi
strip ±460px dan model Live2D cuma terlihat potongan kiri-bawah. Dua lapis,
satu akar:

1. **Basis workspace tanpa batas.** `applyWorkspaceW` (shell/projek.ts)
   menerapkan preferensi tersimpan `live2d.agentWorkspace.w` (sering ±1040,
   warisan migrasi `live2d.sidebar.w`+340) mentah-mentah begitu
   `innerWidth ≥ 1280`. Workspace `flex-shrink: 0` → stage tergencet sampai
   `min-width: 340px`. Jendela restored (±1260px CSS @125% scaling) justru di
   bawah ambang → default 372px → stage lega. Itulah inkonsistensi yang
   dilaporkan: jendela kecil dapat stage besar, maximize menenggelamkannya.
2. **Canvas/framing tidak mengejar.** `fitCanvas`+`frameModel` (app.js) hanya
   di listener `resize` window; lebar stage berubah belakangan lewat
   transisi `flex-basis 0.18s` → canvas & framing tersangkut di ukuran lama,
   model "tengah canvas lebar" jatuh di luar stage sempit, terpotong
   `overflow: hidden`.

Fix (commit `57eb5a0`): `ResizeObserver` di `#stage` (rAF-debounced) →
`fitCanvas` + `frameModel` mengikuti ukuran akhir elemen apa pun pemicunya
(juga memperbaiki drag gutter yang selama ini tanpa re-frame); fungsi murni
`clampWorkspaceBasis` di **shell/workspace.ts** (modul baru, aman diimpor
test — projek.ts menyentuh `location` di level modul) membatasi basis agar
stage tak pernah di bawah 340px, dipakai `applyWorkspaceW` + `setOpen`.
Regresi: `test/workspace-clamp.test.ts` memakai angka nyata 1260/1536px.
Gate: **344 unit + 512 guard, 0 gagal**; build & tsc bersih.

## UPDATE 2026-09-07 (6) — 4 KOLOM + BROWSER NYATA YANG DIKONTROL AGENT

Target final: `[projek/history][Live2D][conversation][technical pane]` dan browser
harus dapat dilihat serta dimanipulasi oleh user **dan** model. Implementasi
memakai Edge/Chrome headed dengan profil terisolasi + CDP (`ws` yang sudah ada),
bukan iframe/proxy palsu:

- **Keamanan lebih dulu** (`68c6fe9`): API localhost privileged menolak Origin
  asing; same-origin loopback dan CLI/native tanpa Origin tetap berjalan.
  Browser policy hanya HTTP(S), menolak scheme lokal, private/link-local/cloud
  metadata, mengecek DNS, dan mewajibkan grant eksplisit per-origin untuk
  localhost/LAN.
- **Engine CDP** (`bf23263`): discovery Edge/Chrome bersama (fallback Pet ikut
  reuse), launch browser terlihat dengan profil `data/browser/profile`, satu
  tab, navigate/history/focus/close, AX-tree inspect, ref opaque+TTL+stale
  guard, trusted click/type, screenshot, redaksi password/secret.
- **9 tool browser agent** (`9deddc3`): `browser_status/open/navigate/inspect/
  click/type/history/close/grant_private`. Semua aksi mutating lewat approval.
  `browser_type.text` tidak pernah masuk bus/SSE/history/status; hanya panjang
  karakter yang publik, sedangkan nilai asli hidup sementara di approval map.
- **Shell 4 kolom** (`5eecdfc`): conversation tetap di `#sidebar`; Review,
  Terminal, Browser pindah ke `#agent-tech`; Browser punya mount nyata
  `#as-browser-root`. Combined workspace resizable 650–1200px; <1280px panel
  teknis ditumpuk di bawah conversation; Chat tab teknis redundan dihapus.
- **Control plane Browser** (`6b47740`): address/back/forward/reload,
  engine/koneksi/grant, preview screenshot bertimestamp, klik preview trusted,
  focus browser live, close, dan grant origin privat eksplisit. Poll/screenshot
  hanya saat tab terlihat; object URL dibersihkan. API POST wajib JSON 64 KiB;
  screenshot `no-store`; type response tidak mengulang teks.
- Model saat ini text-only, jadi agent melihat halaman lewat accessibility/DOM
  semantic snapshot; screenshot adalah preview user. Tidak ada klaim multimodal
  palsu. User dan agent tetap memakai tab browser nyata yang sama.
- Gate: **336 unit + 512 guard, 0 gagal**; build dan tsc bersih. Manual smoke
  browser terpasang tetap wajib saat merakit release Windows.


## UPDATE 2026-09-07 (4) — GAP AUDIT UI AGENT: STATUS GLOBAL, BADGE LEVEL, CANCEL, QUICK ACTIONS

User memberi checklist "elemen ideal UI agent" (6 kelompok). Hasil audit:
mayoritas sudah ada (streaming, tool card, plan live, approval+diff,
Review/Terminal tab, multi-session, memory, verifikasi ✓/✗, subagent chip,
akting per-event). Empat gap utama ditutup dalam 4 commit:

1. **Status global** — pill panel dapat state `approval` (dulu saat loop
   pause untuk izin, `busy=false` → pill keliru "siap"; kini
   `pendingApprovals` yang menentukan). Tombol Assistant di activity bar
   diberi dot status (`data-agent`: off/idle/busy/approval, poll 4 dtk di
   `projek.ts`) — status agent terlihat dari mana pun. Kartu approval
   dapat pulse rail amber (keyframe `appr-pulse`, opacity-only — patuh
   manifesto).
2. **Badge level tool** — `/status` + field additive `tools [{name,level}]`
   dari registry `TOOLS`; panel menampilkan badge "auto" (mint, safe) /
   "izin" (amber, mutating) di header kartu tool & approval → user paham
   kenapa sesuatu auto-jalan vs diminta izin.
3. **Cancel per-task** — `Runtime.cancelRequested` dicek loop antar-langkah
   (awal turn + setelah `execTool`); tool yang jalan selesai dulu (run_command
   ≤30 dtk — cancel kooperatif, di-dokumentasikan MODES), reply
   "Dibatalkan oleh user.", runtime TIDAK dimatikan. Route
   `POST /api/assistant/cancel` (accepted hanya saat busy); tombol panel
   "Stop Task" (`#as-cancel`) terpisah dari "Matikan Agent" (nuke).
   `ask` baru membersihkan flag sisa — cancel tak bocor antar tugas.
4. **Quick actions + resizable** — 4 chip task umum di composer (teks i18n
   = prompt, dikirim textContent — satu sumber); `#sb-gutter` 6px antara
   stage & sidebar untuk drag lebar panel (320–900px, persist localStorage,
   dobel-klik reset), handler di `projek.ts`.
- i18n: 10 key baru (`as.status.approval`, `as.lvl.*`, `as.cancel*`,
  `as.quick.*`) di KEDUA kamus.
- Gate: **305 unit + 512 guard, 0 gagal**; build & tsc bersih.
- **Sisa gap yang disadari (belum dikerjakan)**: indikator koneksi persisten
  (sekarang masih status line sekali saat drop — bisa jadi dot di pill);
  subagent parallel count (chip ada, hitungan "n/4" belum); rename/pin sesi;
  halaman Review tanpa diff penuh per file.

## UPDATE 2026-09-07 (3) — SHELL 3 KOLOM ala ZCode + PROJEK MULTI-SESSION

User sketsa layout baru (kiblat tetap ZCode desktop): activity bar kiri,
panggung tengah, side panel kanan — dan rail "Projek" berisi riwayat sesi.
Dikerjakan 3 commit (shell / server / rail UI):

- **Shell 3 kolom**: `.app` kini `[activity 56px][projek rail 240px toggle]
  [stage flex:1][sidebar 372px/600px agent-wide]`. `mode-switch` PINDAH utuh
  ke `<nav id="activity">` — id & `data-mode` dipertahankan sehingga
  `mode-runtime.js` TIDAK diubah sama sekali (setPanel/switchMode/boot).
  Tombol mode kini gaya ikon (label disembunyikan, singkatan `data-mode`
  via `::before`, tooltip `top.tabTip.*`). Breakpoint <1024px direvisi.
- **Rail projek** (`#projek-rail`, isi dibangun `src/client/shell/projek.ts`
  → `window.__shellProjek`, start di boot bundle): kartu project (basename
  workdir dari /status, klik = salin path) + daftar sesi (poll 8 dtk saat
  terbuka) dengan switch/new/delete; state buka-tutup di localStorage.
  Aturan AGENTS.md dipatuhi: logic UI baru = TS, bukan legacy JS.
- **Multi-session server** (`src/server/agent/sessions.ts`): store
  `data/assistant-sessions.json` `{active, sessions[]}` — cap 20 sesi FIFO
  (bukan yang aktif), auto-nama sesi = pesan user pertama (40 char),
  migrasi SEKALI dari `assistant-history.json` (+ arsip `.bak`), tulis
  atomic tmp→rename. `state.loadSession/saveSession` jadi wrapper store →
  CLI `bun run agent` otomatis ikut sesi aktif. `Runtime` + `sessionId`.
- **API**: `GET /api/assistant/sessions`, `POST …/sessions/new {workDir?}`,
  `POST …/sessions/switch {id}`, `POST …/sessions/delete {id}` — semua
  409 saat busy. **Pindah sesi TIDAK mematikan runtime** (kontrak MODES
  utuh): facade ganti `rt.history/workDir/sessionId` + persist.
- **Sinkron panel**: `projek.ts` melempar event DOM `agent:session-changed`
  → panel.ts reset transcript/registry/termLog + hydrate ulang dari
  /history + refreshStatus (listener dibersihkan di destroyPanel).
- i18n: 13 key baru (`shell.projek.*`, `as.sess.*`) di KEDUA kamus.
- Guard yang dijaga & hijau: `#pn-search` proximity (area popup tak
  disentuh), urutan script voice-input < emotion-overlay, i18n coverage
  (data-i18n tanpa anak elemen — tombol ikon memakai data-i18n-title).
- Gate: **302 unit + 512 guard, 0 gagal**; build & tsc bersih.
- **Prioritas berikutnya (dicatat)**: rename sesi (sekarang auto-nama
  saja), pin/urutkan sesi, interrupt tugas berjalan per-task (tetap),
  review tab menampilkan diff penuh per file (tetap).

## UPDATE 2026-09-07 (2) — PANEL AGENT "POWERFUL VIBECODING" (diff, markdown, tab, undo)

User menilai UI agent masih kurang powerful untuk vibecoding dibanding kiblat
coding-agent modern; keempat gap diisi dalam 4 commit:

1. **Diff & ringkasan perubahan (client-only)** — `panel/diff.ts` (LCS baris,
   cap 1000 baris, trim prefix/suffix, hunks berkonteks) menghitung diff di
   CLIENT dari argumen `write_file`/`edit_file`/`delete_file` yang SUDAH lewat
   SSE — nol perubahan server. Kartu tool mutasi kini menampilkan diff
   (bukan JSON), kartu approval menampilkan PRATINJAU diff terbuka (keputusan
   Allow/Deny berbasis isi), dan tiap akhir giliran (`done`) memunculkan kartu
   ringkasan "N file berubah +a −r" ala ZCode. Tracking per giliran: reset
   saat `appendUser`, tool ERROR membuang change, pause approval (⏳) menunda
   ringkasan tanpa membuang tracking, `resolveApprovalVisual(klien lain)`
   tetap masuk hitungan.
2. **Markdown + grup langkah** — `panel/md.ts`: parser mini zero-dep → token
   data (heading/paragraf/fenced code/quote/list/inline code+bold+italic+link
   http-saja); view merender via createElement/textContent — TIDAK pernah
   innerHTML konten model (XSS-safe by design). Blok `final` dirender
   markdown; streaming tetap plain+caret. Run ≥2 kartu tool berurutan
   dibungkus grup `.as-step` collapsible ("Bekerja — N langkah", status ikut
   child terakhir, signature rebuild: id+status+rev — state expand child
   terjaga).
3. **Tab Obrolan/Review/Terminal** — `panel/registry.ts` (murni): 
   `ChangeRegistry` (perubahan file lintas giliran; `mergeTouched` menyatukan
   `notes.filesTouched` dari /status — path sesi CLI tampil "touched") &
   `TermLog` (riwayat run_command, cap 80). Server ADDITIVE:
   `assistantStatus()` kini menyertakan `notes: {filesTouched}`. Panel poling
   `view.activeTab()` untuk menggambar halaman aktif.
4. **Undo/revert (server)** — snapshot isi file SEBELUM eksekusi
   write/edit/delete di `execTool` (path lewat `safePath`; hanya bila tool
   SUKSES; SATU rekaman per path = kondisi asli; jalur approval panel/CLI
   ikut karena `agentRunApproved` lewat `execTool`). `Runtime.undo`
   (cap 20 FIFO, in-memory — sengaja TIDAK dipersist, seperti approval).
   Route: `GET /api/assistant/undo`, `POST /api/assistant/revert {id}`
   (error 404 bila id asing; revert ganda ditolak). Panel: tombol Revert per
   file di tab Review → lookup by path → revert → status line di transcript.
- Gate: **295 unit + 512 guard, 0 gagal**; build & tsc bersih. Test baru:
  diff (8), transcript-change (5), md (6), registry (7), undo (6, eksekusi
  sungguhan di workDir mkdtemp), parity status (1).
- **Prioritas berikutnya (dicatat, belum dikerjakan)**: interrupt tugas
  berjalan (cancel 1 task) masih jadi gap — "Matikan Agent" tetap blunt
  instrument; halaman Review belum menampilkan diff penuh per file (baru
  stat), bisa naik level dengan menautkan ke kartu diff di transcript.

## UPDATE 2026-09-07 — REMAKE TAMPILAN AGENT (panel Assistant ala ZCode, port ke TS)

User minta tampilan agent di-remake total — kiblatnya coding-agent seperti
ZCode. Panel lama cuma log teks polos di rail sempit (372px), pertanyaan via
`POST /ask` blocking (user menatap "Memproses…"), aktivitas tool tidak tampil.
Kini:

- **Panel port ke TS** — aturan lama "panel DOM tidak direncanakan di-port"
  DIREVISI (AGENTS.md + MODES.md + README sudah diubah): panel kini di
  `src/client/agent/panel/{stream,transcript,actor,view,panel}.ts`,
  di-bundle sebagai `window.__agentPanel`; `mode-runtime.js` tinggal bridge
  `start()` ±10 baris. Tidak ada guard legacy yang menunjuk mode-runtime
  (dicek grep) — logic teruji lewat `test/agent-panel.test.ts` (32 test baru).
- **Transcript live ala ZCode**: rail melebar `#sidebar.agent-wide`
  (372→600px, karakter tetap terlihat); streaming delta via SSE
  `/api/assistant/ask-stream` yang selama ini hanya dipakai CLI; kartu tool
  collapsible (args pretty + hasil, dot status); kartu approval menonjol;
  plan widget dengan progres x/y; chip subagent; status pill
  (siap/bekerja/bekerja-klien-lain/mati); textarea auto-grow (Enter kirim,
  Shift+Enter baris baru).
- **Server additive** (kontrak lama utuh): `assistantResolveApproval` menerima
  `onEvent` opsional; `agentRunApproved` meng-emit SSE `tool_result` (hasil
  tool yang disetujui kini sampai ke transcript); bus event
  `permission_resolved` (yang selama ini cuma deklarasi) kini benar-benar
  di-emit saat approve/deny; route baru `POST /api/assistant/approve-stream`
  (SSE, pola sama dengan ask-stream); `clipToolResult` line-boundary-aware
  (diff tak putus di tengah baris) + chip "…dipotong" di panel.
- **Dedup eksplisit antar 3 channel** (SSE + poll status 2dtk + poll bus
  1,5dtk): kartu approval satu-satunya sumber = `pendingApprovals` dari
  `/status` keyed by `ap.id` (event SSE/bus hanya pemicu refresh); plan satu
  sumber = `status.plan`; transcript punya mode `live` (SSE) vs `follow`
  (bus — pantau CLI/klien lain) dengan supresi
  `thinking/tool_call/permission/final/error` saat live;
  `verification/subagent` selalu dirender (tidak ada di SSE).
- **Protokol putus-koneksi dua-kasus** (`decideFallback`, teruji): SSE gagal
  sebelum event pertama → cek `/status` fresh, resend `POST /ask` sekali bila
  tidak busy; putus setelah ≥1 event → TIDAK PERNAH resend, lanjut follow
  dari bus + hydrate jawaban final dari history saat busy→false (dedupe by
  teks). Server juga sudah menolak ask kedua saat busy (defense-in-depth).
- **Kontinuitas approve→lanjutan**: kartu approval bermetamorfosis jadi kartu
  tool "menjalankan…" → diisi `tool_result` dari approve-stream; lanjutan
  delta menempel tanpa bubble/header baru; prompt internal "Lanjutkan tugas…"
  difilter dari hydrate.
- **Direktur akting dipertahankan + diperluas per-event** (port ke
  `actor.ts`, cooldown 2,5dtk/25dtk utuh): verification gagal→prihatin,
  lolos→ringan tanpa komentar; subagent_completed→senang; plan_revised→gaze
  think + fallback `as.actor.revised`; permission_resolved disetujui→lega,
  ditolak→tanpa reaksi.
- **Hydrate saat buka panel**: `/api/assistant/history` (user→bubble,
  assistant→bubble final, tool→kartu hasil, `MENUNGGU PERSETUJUAN:`→kartu
  running), bus TIDAK direplay (baseline seq terkini), workdir dicerminkan
  dari `/status`.
- i18n: 21 key baru `as.*` di KEDUA kamus (id/en, parity hijau); key mati
  lama dibiarkan (tak berbahaya). Tombol Reset (`/api/assistant/reset`, sudah
  ada dari dulu) kini diekspos di panel.
- Gate: **260 unit + 512 guard, 0 gagal**; build & tsc bersih.
- **Prioritas berikutnya (dicatat, belum dikerjakan)**: interrupt tugas
  berjalan (cancel 1 task, bukan "Matikan Agent" yang mematikan runtime) —
  `run_command` 30 dtk & subagent paralel maks 4 bisa nyangkut, dan satu-
  satunya jalan keluar sekarang blunt instrument.

## UPDATE 2026-09-02 (4) — jalur saran preset AI diperbaiki & diuji end-to-end

Keluhan "analisis AI untuk sheet gagal mulu" dianamis sampai akar:

1. **Role `sheet` cuma dipegang satu koneksi** (gemini "germini") yang sering
   HTTP 503 high demand — tak ada kandidat fallback ber-role sheet. Kini
   ada koneksi kedua **"Tf-mimo"** (tokenfaucet, model `mimo-v2.5`, role
   sheet, stream:true) lewat `POST /api/config` — chat user (gpt-5.6-terra)
   tidak tersentuh. Germini tetap urutan pertama; 503 → jatuh ke Tf-mimo.
2. **Gateway openai-compatible memutus non-stream dgn HTTP 524 ~15 dtk.**
   conn.stream=true wajib untuk faucet ini. Kelemahan lamanya: postJson
   memakai timeout ABSOLUT 60 dtk — mimo dengan prompt 50k char butuh
   36-59 dtk → bisa ter-abort acak. Kini timeout jadi **idle timeout**
   (di-reset tiap chunk stream) saat conn.stream=true.
3. **Truncation senyap**: gemini-2.5 (thinking model) memakan budget output
   utk reasoning — maxTokens 2048 membuat JSON 12 preset terpotong → parse
   gagal → `presets:[]` tanpa penjelasan. maxTokens germini dinaikkan ke
   8192 (via API), balasan terpotong kini di-**salvage**
   (`salvageJSONArrayOfObjects` — N-1 objek utuh diselamatkan, string-aware)
   dan kalau nihil, warning eksplisit + awalan balasan dikirim ke user.

Hasil end-to-end lewat endpoint asli (payload 223 param + 128 catatan):
**12 preset valid** (emosi/properti) dalam 36 dtk, tersimpan ke
`presets.ai` sheet lumine. Guard: unit test salvage (13) — total **217 unit
+ 505 guard, 0 gagal**. Commit ffd57c3.

## UPDATE 2026-09-02 (3) — peta param 神宫白子 + popup param (cari/grup/label cdi3) + grup di payload AI

**Peta param model kedua (神宫白子 / 面饼0, Cubism 5.0):** seluruh 214 param
dirender MIN vs MAX (freeze persistent + override-per-param + physics hidup +
**settle 45 frame sebelum tiap render**): **213 hidup, 1 mati** (`Param5` =
高光/highlight), 13 halus (31–115 px; highlight mata/pupil + aksesori kecil —
pita, tombol controller). Kontras lumine (83/223 mati) — perbedaan murni rig,
bukan runtime. PELAJARAN PENTING pengukuran: spring physics rig ini punya
konstanta waktu panjang — tanpa settle panjang, sisa getar antar render
menghasilkan noise 5–27 ribu px yang MENUTUPI param mati (hasil "0 mati"
pertama tanpa settle itu palsu). Nilai param setia + render deterministik
(tes MIN-vs-MIN dan render-ganda-0-px dipakai sebagai noise floor).

**Popup "📝 Penjelasan Parameter" diperluas:**
- Kotak pencarian (`#pn-search`): filter live id/label/grup + isi catatan;
  header grup yang semua barisnya tersaring ikut disembunyikan; tanpa
  re-render (nilai slider & fokus textarea tidak rusak).
- Header grup (`appendGroupHeader`): param dikelompokkan via
  `resolveParamGroup` (user > ai > heuristik), urutan kemunculan pertama;
  "Bagian (Parts)" tetap seksi terpisah.
- **Label + grup ASLI rigger dari cdi3.json** (`prefetchCdiInfo`, fire-and-
  forget saat loadModel): Name ("heart eye", "eyelashes shake4", 猫猫贴纸)
  menggantikan id mentah sebagai label; GroupId jadi judul grup
  `Rig: <label0> +N` (lihat `cdiGroupTitle`). Sheet yang sudah ada di-patch
  in place saat data tiba (label/grup bukan field user); popup terbuka
  di-render ulang lewat jembatan `window.__pnRefreshIfOpen` (scope wireUI).

**Saran preset AI:** payload analyze-sheet kini menyertakan `group` per param
(dikirim client, ditulis server ke prompt sebagai `[grup: …]`) supaya LLM
tahu param mana yang sekeluarga. Ditambah pembersihan data: **95 catatan
userNote palsu sisa scanner lama** (template "Tidak ada efek visual
terdeteksi…" — termasuk klaim keliru "ParamAngleX tak dipakai rig" yang
terbantahkan scan 2026-09-02) dihapus dari sheet lumine; catatan analisis
asli (128) dipertahankan. Berkas sheet git-ignored (data user).

Guard baru: `test-param-notes-ui.js` (18 assertion). Suite kini **212 unit +
505 guard, 0 gagal**.

## UPDATE 2026-09-02 (2) — fitur "🧪 Kalibrasi Efek" DIHAPUS (keputusan user)

User memutuskan menghapus fitur kalibrasi efek: hasil ukurnya sering tidak
cocok dengan yang terlihat (param yang bergerak halus terukur "mati" di
beberapa model), dan param yatim yang memang tidak bergerak justru membuat
badge-nya terasa tak berguna. Yang dihapus:

- Tombol `#btn-visfx-calibrate` + span `#visfx-status` (index.html) + CSS
  `.dead-param`/`.pn-dead-badge` (app.css).
- `runVisualCalibration()` + `VISFX_SETTLE_FRAMES` + badge di
  `buildParamSliderRow` + status popup (app.js).
- `visfxIsDead`/`visfxSummarize`/`filterVisfxDead`/`visfxSave` — saran preset
  AI kembali mengirim SEMUA param (data yang tidak dipercaya justru bisa
  menyembunyikan param yang benar-benar hidup dari LLM).
- Guard `test-legacy/test-visual-calibration.js` dihapus (suites 11 → 10;
  total kini 212 unit + 487 guard, 0 gagal).

Yang DIPERTAHANKAN: gate overlay-vs-native (`overlayGateSuppress`) tetap
membaca `state.visfxMap` — kini hanya sebagai cache LEGACY dari localStorage
v2 warisan scan lama (tidak ada scanner baru; tidak pernah ditulis ulang).
Tanpa data → fail-open (overlay jalan seperti dulu; terverifikasi di
browser: lumine tanpa entri visfx → overlay heart tetap menyala, bindings
`exp_heart` → ParamEX04/05/08/09/11 tetap terbaca dari server). Penjelasan
param yang tidak bergerak kini manual: `ParamEyePhysics18` ("eyelashes
shake4") TIDAK ADA sama sekali di `lumine.physics3.json` (outputs hanya
EyePhysics1–16) dan tidak terikat art — param yatim rig distribusi; tidak
ada kode apa pun yang bisa menggerakkannya. Kelas yang sama: RX1_1, EX02-11.

## UPDATE 2026-09-02 — freeze edit TIDAK menol physics lagi

Lanjutan akar masalah yang sama: bukan cuma scanner — **freeze edit manual**
(`freezeModelForEdit`, dipakai popup Penjelasan Parameter + Sheet preset editor
+ Motion Studio) juga me-nol-kan `im.physics`, sehingga geser slider pada param
INPUT physics (ParamAngleX/BodyAngle*/Breath) saat model dibekukan tampak MATI
total: 0 piksel, persis keluhan user. Diperbaiki dengan menyamakan freeze
dengan scan:

- `freezeModelForEdit` kini hanya membungkam **motion (stopAllMotions) +
  expression reset + eyeBlink + breath + fidget/mouse-follow (aiLock)**.
  Physics dibiarkan hidup — slider pada param input langsung terlihat
  (terukur di browser, lumine, freeze aktif: ParamAngleX -30 vs +30 =
  **23.994 px berubah**, dulu 0 px).
- Nilai slider pada param OUTPUT physics tetap menang lewat override guard
  (re-assert di beforeModelUpdate = SETELAH physics.evaluate), jadi kedua
  arah input/output sama-sama bekerja saat frozen.
- `unfreezeModelForEdit` tetap memulihkan physics/eyeBlink/breath dari
  `state._frozenRefs` (same-ref, no-op untuk physics).
- Guard: `test-visual-calibration.js` section 8 +3 assertion (freeze tidak
  menulis `im.physics = null`, blink/breath tetap dibungkam, unfreeze
  memulihkan refs). Suite total: **212 unit + 525 guard, 0 gagal**.
- Tombol "🧪 Kalibrasi Efek" TETAP DIPERTAHANKAN (keputusan sesi ini):
  fungsinya menghasilkan data badge "🚫 tanpa efek" per param (cache
  localStorage v2 per model), menandai param yang GENUINE tidak terikat art
  (lumine: 20/223 — rantai _1/_4 VBridger dsb.), dan memfilter param mati
  dari saran preset AI (`filterVisfxDead` di analyze-sheet) supaya LLM tidak
  mengusulkan preset yang pasti tampak rusak. Setelah fix scan 2026-09-01 (3),
  datanya jujur; sebelum fix, badge banyak yang salah menandai param hidup.

## UPDATE 2026-09-01 (3) — scan kalibrasi diperbaiki + cache lama di-invalidasi

Investigasi keluhan user "param physics (contoh `ParamBodyPhysicsRX1_1`) tidak
bereaksi ke slider" menemukan TIGA titik buta scanner sekaligus. Hasil akhir
setelah semua diperbaiki (scan asli via tombol, lumine): **mati turun
84 → 20/223**, `ParamAngleX` 0 → ±22.000 px, `ParamBodyAngleX` 0 → ±124.000,
`ParamSkirtX1` kembali ±63.000 (sempat 430 saat physics-hidup-tanpa-re-assert).
`ParamBodyPhysicsRX1_1` terukur ±0 piksel (±396 px pada JPEG, 0.07% layar) —
badge "🚫 tanpa efek" untuk param itu kini JUJUR dan konsisten dengan
pengalaman user menggesernya (tak terlihat mata).

Tiga gangguan yang kini dibungkam selama scan (`runVisualCalibration`, app.js):

1. **Overrides user di-stash; param yang DISCAN dipasang sebagai override
   sementara.** Dulu: sticky overrides (slider yang pernah digeser,
   eye-follow, aksesoris) + rawDrive di-re-assert guard pada
   `beforeModelUpdate` dan menimpa tulisan MIN/MAX scan → param itu terukur
   "mati" palsu. Kini `state.overrides` diganti map kosong + `state.rawDrive`
   null selama scan, dan param yang discan dipasang sebagai override — guard
   me-re-assert MIN/MAX pada titik yang benar (setelah physics.evaluate,
   sebelum o.update yang dirender), lalu semuanya dipulihkan di finally.
2. **Grup Idle di-stash** (`mm.groups.idle = null`) — `motionManager.update`
   me-restart idle saat queue kosong; evaluasi CubismMotion dimulai dengan
   `loadParameters()` yang MENGHAPUS tulisan scan. Param yang dianimasikan
   idle terukur mati palsu di model yang mendeklarasikan grup Idle (lumine
   tidak punya — model lain ada).
3. **PHYSICS TETAP HIDUP selama scan** — freeze meng-nol-kan `im.physics`,
   padahal param INPUT physics (AngleX/BodyAngle*/Breath, dst.) hanya
   berdampak piksel MELALUI rantai physics: physics mati → MIN vs MAX
   dirender identik → mati palsu (terukur: AngleX 0 px physics-mati vs
   ±24.000 px physics-hidup). Konsekuensinya physics menimpa param output
   tiap frame — ditangani oleh (1): guard menulis ulang MIN/MAX SETELAH
   physics.evaluate. Settle frames (`VISFX_SETTLE_FRAMES` = 4) memberi
   spring waktu konvergen sebelum render dibandingkan.

Lainnya:
- **Cache kalibrasi dibump ke v2** (`visfxStoreKey` → `l2d_visfx_v2_<key>`) —
  data scan lama (ternoda titik buta 1/2 dan/atau diambil saat shim stamp
  masih buta) otomatis tak terbaca. Scan ulang SEKALI untuk badge akurat;
  data lama tinggal yatim di localStorage (bukan data loss).
- Guard: `test-visual-calibration.js` dirombak ke kontrak baru (stash
  overrides + override-per-param + physics hidup + settle + prefix v2).
  Suite total kini: **212 unit + 522 guard, 0 gagal** (11 suite).
- Catatan verifikasi: mengukur dari tab latar belakang browser menyesatkan
  bila lupa rAF/ticker app tak jalan di sana — `internalModel.update` baru
  dievaluasi saat RENDER (`_render`), jadi pompa uji harus mencakup
  `renderer.render(stage)`, bukan cuma `Ticker.shared.update()`.

## UPDATE 2026-09-01 (2) — gate overlay vs efek native (dobel-gambar) SELESAI

Risiko "overlay emosi bisa dobel dengan efek native di rig v5" (disebut di
bagian konsekuensi fix shim di bawah) kini ditangani dengan data terukur,
bukan toggle manual:

- **Server**: `discoverExpressions()` (`src/server/index.ts`) kini menyertakan
  `params` per ekspresi — Id yang ditulis ISI file `.exp3.json` (baca disk,
  dedupe, file rusak → `[]` bukan error). Backward-compatible: field lama
  `Name/File/declared` tetap.
- **Client** (`static/js/app.js`): `fireOverlay()` melewati gate
  `overlayShouldSuppress()` sebelum menyalakan overlay. Gate = fungsi murni
  `overlayGateSuppress(name, bindings, visfx, resolveFx)`: overlay DITEKAN
  hanya bila ekspresi itu memetakan ke efek overlay (via `_resolve`), namanya
  ada di bindings `.exp3` native, dan minimal SATU param bindings-nya terukur
  HIDUP di kalibrasi (`changed > 0`). Semua keadaan tanpa bukti (belum
  dikalibrasi, fetch gagal, alias emosi universal seperti `sedih` yang bukan
  nama `.exp3`) → **fail-open**: overlay jalan seperti sebelumnya. Kegagalan
  paling parah = dobel-gambar seperti sebelum fix shim, bukan efek hilang.
- Bindings di-prefetch saat model dimuat (`prefetchOverlayGate()`, di samping
  `detectModelCapabilities()`), cache dibuang saat model ganti.
- Guard baru: `test/legacy/test-overlay-gate.ts` (28 assertion — server
  in-process, keputusan murni via vm-extract, wiring level sumber).
  Suite total kini: **212 unit + 514 guard, 0 gagal** (11 suite).
- Catatan: kalibrasi yang tersimpan SEBELUM fix shim (masa stamp v4) akan
  membuat gate fail-open untuk semua efek — aman (dobel seperti perilaku lama),
  tapi re-scan kalibrasi tetap dianjurkan agar gate aktif dan badge akurat.

## UPDATE 2026-09-01 — AKAR MASALAH SEBENARNYA KETEMU (shim stamp v5→v4)

Kesimpulan lama "butuh core ≥5.2" **TERBUKTI SALAH**. Akar masalah sesungguhnya:
**`patchCubismCore()` di `static/js/app.js`** — shim kompatibilitas warisan core
4.2.2 yang menurunkan stamp versi moc3 `5 → 4` secara MEMBUTA sebelum
`Moc.fromArrayBuffer`. Akibatnya moc3 v5 (lumine rig 5.x, hash e07d58a8) di-revive
sebagai moc **v4** → tabel keyform BlendShape (ParameterType_BlendShape=1) tidak
pernah diinisialisasi → EX02–05/08–11 tidak pernah terevaluasi walau nilai
parameternya tertulis benar (terverifikasi: `ex05:1` di buffer, opacity
ArtMesh44/225 tetap 0).

Bukti A/B hari ini (core 5.1.0 yang sama, bytes moc3 yang sama, terverifikasi sha):
- **Tanpa shim** (halaman minimal HTML + probe Bun): keyform BlendShape
  **DIEVALUASI** — mata spiral dizzy tampil, opacity ArtMesh44/225 0→1.
- **Dengan shim** (app): nol perubahan drawable (`dVtx=0 dOp=0 dMul=0 dScr=0`).

**Fix (TER-COMMIT — `096175e`):** shim diubah menjadi *try-genuine-first* —
coba `orig(ab)` dengan stamp asli; hanya kalau core mengembalikan null, stamp
diturunkan ke 4 lalu dicoba lagi. Moc v5 asli → jalan penuh (fitur BlendShape
hidup); moc "stamped-5-tapi-layout-v4" → tetap selamat lewat fallback lama.

Konsekuensi & catatan lanjutan:
- **Kesimpulan B di bawah (BlendShape butuh core ≥5.2) TIDAK LAGI BERLAKU** —
  core 5.1.0 mengevaluasi blendshape dengan baik selama stamp moc tidak dipalsukan.
  Opsi #1 di bawah (unduh core ≥5.2) tidak diperlukan untuk efek ini.
- **Overlay emosi kini bisa dobel dengan efek native** di rig v5 (mis. exp_heart
  menggambar hati via rig + overlay menggambar hati lagi) — **SUDAH DI-GATE**
  dengan data kalibrasi; lihat "UPDATE 2026-09-01 (2)" di atas. Kalau tetap
  terlihat dobel di rig tertentu, cek dulu apakah kalibrasi sudah di-scan
  ulang pasca-fix shim, atau matikan `overlay.enabled`.
- Kalibrasi 🧪 "94/223 tanpa efek" untuk MyModel kini basi — di-scan saat shim
  masih men-stamp v4. Re-scan kalau mau badge yang akurat.

## Ringkasan satu paragraf (historis — dibuat sebelum akar masalah ketemu)

Dukungan model Cubism 5.x dan efek rig dituntaskan sebagian besar: (1)
**override guard** membuat nilai slider/pose bertahan 100% frame (dulu 0%),
(2) patch **multiplyColor** membuat efek ganti warna rig 4.2+ (collar) tampil,
(3) **kalibrasi efek visual** mengukur param mana yang benar-benar mengubah
piksel, (4) **overlay efek emosi** menggambar hati/blush/kilau/air mata untuk
efek yang rig-nya tidak memuat, (5) **Auto-Rescue** merakit manifest untuk
folder model tanpa .model3.json, (6) **core resmi 5.1.0** terpasang. SATU
HAL yang belum tuntas: efek **BlendShape** rig lumine v5.0 (heart eye, blush,
tear, sparkling, sweat, dizzy — EX02-05/08-11) belum tampil karena butuh
core ≥5.2; overlay manual kini yang menutupi kebutuhan visualnya.

## Urutan perbaikan & commit

| Commit | Isi |
|---|---|
| 5292c86 | Override guard: re-assert overrides+rawDrive di event `beforeModelUpdate` (app.js `installOverrideGuard`); guard test-override-guard.js |
| 97e8ba5 | Patch multiplyColor 2 titik di lib vendored + core resmi 5.1.0 (207KB); guard test-multiply-color.js |
| 5f51c87 | Kalibrasi efek visual (🧪 tombol di popup Penjelasan Parameter) + badge "🚫 tanpa efek" + filterVisfxDead di analyze-sheet; guard test-visual-calibration.js |
| 05fab80 | Overlay efek emosi app-level (js/emotion-overlay.js, 8 efek, anchor kepala diukur dari framebuffer); guard test-emotion-overlay.js |
| 15a5298 | Auto-Rescue: src/server/rescue.ts + jalur virtual `model/<f>/__rescue__.model3.json`; guard test-auto-rescue.js |

Suite: **212 unit + 498 guard, 0 gagal** (10 suite). Working tree bersih
(`data/` dan log server tetap untracked sesuai desain).

## Akar masalah teknis (penting untuk sesi berikutnya)

### A. Urutan update pixi-live2d-display 0.4.0 (lib vendored)
```
per frame: [PIXI ticker] internalModel.update():
  motionManager.update → o.saveParameters() → expressionManager →
  eyeBlink → updateFocus (ADD ke EyeBall/Angle) → breath →
  physics.evaluate → pose → emit("beforeModelUpdate") →
  o.update()  ← DEFORMER + BLEND COLOR dievaluasi/dirender DI SINI
  o.loadParameters()  ← nilai param kembali ke snapshot
[rAF app.js] tick(): target() easing → emotion ease → applyOverrides()
  → applyRawDrive()
```
Akibat: tulisan app selalu "telat satu frame" terhadap sistem internal —
inilah akar dua bug pertama (slider kalah, warna collar tak muncul).
Titik injeksi yang benar: event **`beforeModelUpdate`** (dipancarkan tepat
sebelum `o.update()`).

### B. Efek BlendShape lumine (BELUM TUNTAS di web runtime)
- `ParamEX01–EX11` bertipe **`ParameterType_BlendShape = 1`** (171 dari 223
  param rig v5.0 bertipe ini). Rig diekspor dari Cubism 5.3.
- Bukti binding ADA: kombinasi EX04+EX08+EX11 (= exp_heart) menyalakan flag
  **`BLEND_COLOR_DID_CHANGE` (bit6, 0x40)** di 32 artmesh pada
  `drawableDynamicFlags` — arti bit terkonfirmasi dari class constants
  Cubism 5.3 resmi (`CubismDrawableFlag$DynamicFlag` di
  `app/lib/Live2DCubismCore.jar` milik Editor 5.3 yang terinstal).
- TAPI nilai akhirnya tidak berubah (opacity/blend color/vertex tetap) —
  **core Web 5.1.0** (yang tersedia di CDN publik
  `cubism.live2d.com/sdk-web/cubismcore/live2dcubismcore.min.js`,
  terverifikasi unduh fresh = 5.1.0) menandai perubahan tapi tidak
  mengevaluasi keyform blendshape sampai selesai. Viewer resmi
  **5.3.03 (core 5.2/5.3)** menampilkan penuh (screenshot user: heart eye).
- Formula resmi screen/multiply keyform sudah diperoleh dari
  `Shaders/WebGL/fragshadersrccolorblend.frag` + `cubismshader_webgl.ts`
  (framework 5-r.5): shader premultiplied menerapkan
  `tex.rgb *= multiply.rgb; tex.rgb += screen.rgb - tex.rgb*screen.rgb;`
  lalu `tex *= baseColor`.

### C. Kesimpulan lama yang SUDAH DIKOREKSI
"Binding efek tidak ada di moc3" — SALAH untuk rig v5.0 (binding ada,
terbukti via flag). Yang benar: binding ada, evaluasinya butuh core
≥5.2. Untuk rig v4.2 lumine: efek memang tidak terikat (export lama).

## State model & data

- `data/model/lumine` — moc3 **Cubism 4.2** (hash f458de1d2a). Efek
  BlendShape memang tidak terikat di ekspor ini.
- `data/model/MyModel/lumine` — moc3 **Cubism 5.0**, hash identik dengan
  rig sumber `F:/lumine_l2d` (e07d58a858). SALINAN LANGSUNG dari sumber
  (import web ternyata sudah membawa moc3 yang benar — bukan itu masalahnya).
- 94/223 param lumine v4.2 "mati" di scan; daftar mati MyModel v5.0
  identik. 7 param yang tampak mati (AngleX/Y/Z, BodyAngleX/Y/Z,
  BodyPotisionZ) sebenarnya INPUT physics — hidup di produksi.
- `ParamAnime01/2/3` (guruguru + tetesan air mata) dianimasikan oleh
  `idle.motion3.json` (direferensikan `lumine.vtube.json`
  `IdleAnimation`); keyform-nya juga tidak dievaluasi core 5.1.
- Kalibrasi tersimpan di **localStorage** per model (`l2d_visfx_v2_<key>` —
  v2 sejak fix scan 2026-09-01, data kunci lama basi; lihat UPDATE (3)) —
  sengaja bukan sheet (skema v4 tak tersentuh); hilangnya bukan data loss.

## Yang belum tuntas + opsi

1. **Core ≥5.2 untuk web** (jalur "efek native tampil"): unduh Cubism SDK
   for Web terbaru dari live2d.com (gratis, klik lisensi — link CDN publik
   masih menyajikan 5.1.0) → ambil `live2dcubismcore.min.js` → taruh di
   proyek → swap → verifikasi EX08/EX11 menyalakan BLEND_COLOR pada
   multiply/screen arrays (probe RGBA penuh — jangan lupa channel a) →
   patch screen color (formula sudah ada di bawah) bila diperlukan.
2. **Patch screen color** (opsional, future-proof): perluas patch lib
   titik (A): stash juga `screenColors[4i..4i+4]`, dan di titik (B) banding
   `h.rgb += (1-h.rgb)*screen.rgb` bila keyform menganimasikannya. Saat ini
   hanya multiply yang diterapkan — cukup untuk collar, belum tentu untuk
   efek berbasis screen.
3. **Overlay vs native**: overlay tetap ON default (config
   `overlay.enabled`). Gate dengan data kalibrasi kini TERPASANG (lihat
   "UPDATE 2026-09-01 (2)") — efek yang terukur hidup tidak lagi digambar
   dobel oleh overlay. Overlay tetap relevan sebagai kompensasi untuk ekspresi
   yang rig-nya memang tidak mengikat art (rig v4.2, rig distribusi).
4. Jangan migrasi pixi 8 hanya demi ini: rendering pixi 6 sudah benar;
   migrasi = menulis ulang seluruh integrasi (app.js, motion editor,
   overlay, guard semuanya terikat internal 0.4.0).

## Berkas kunci

- `static/js/app.js` — installOverrideGuard, visfx helpers
  (visfxStoreKey/visfxIsDead/visfxSummarize/filterVisfxDead),
  runVisualCalibration, fireOverlay di 3 jalur + else synthetic-unknown,
  resetEmotion memadamkan overlay, label cdi3 di popup.
- `static/js/emotion-overlay.js` — modul overlay (EFFECTS/ALIASES/
  resolveEmotionFx/_tick/_status; anchor `measureHead()` ukur framebuffer,
  cache 5 dtk, fallback bounds).
- `static/js/pixi-live2d-0.4.0.js` — 2 patch: doDrawModel stash
  `__mcDraw`, setupShaderProgram kalikan `t.__mcDraw` ke u_baseColor
  (t = renderer! `this` di sana = shader manager — jangan dibalik lagi).
- `static/js/live2dcubismcore.min.js` — core resmi 5.1.0 dari CDN
  (mampu memuat moc3 v5.0; mengevaluasi param biasa + multiply color;
  blendshape keyform v5.2+ BELUM).
- `src/server/rescue.ts` — scanRescueFolder/buildRescueBlueprint.
- `src/shared/types.ts` + `config.ts` — Config.overlay.
- Guard: test/legacy/{test-override-guard,test-multiply-color,
  test-visual-calibration,test-emotion-overlay,test-auto-rescue}.js

## Cara verifikasi cepat (kalau sesi baru ragu)

1. `bun run test` → harus 212 unit + 487 guard, 0 gagal.
2. `bun run src/server/index.ts` → buka 127.0.0.1:8310 → Load lumine →
   geser ParamCollarChange → warna ikat pinggang berubah.
3. Load MyModel (Cubism 5.0) → termuat ±300ms.
4. `window.__live2dAgent.setExpression('exp_heart',1)` → hati melayang
   di atas kepala (overlay) — lumine fresh TIDAK punya data visfx di
   localStorage → gate fail-open → overlay jalan.
5. Popup Penjelasan Parameter → TIDAK ada tombol 🧪 (fitur dihapus
   2026-09-02 (2)) dan TIDAK ada badge "🚫 tanpa efek".

## Peta param lumine (ukur 2026-09-02, model fresh dari distribusi)

Model dibandingkan hash MD5 (zip distribusi = folder fresh = copy project,
byte-identik) lalu SEMUA 223 param dirender MIN vs MAX (freeze persistent +
override-per-param + physics hidup + pump `im.update` + render ke
RenderTexture): **140 hidup, 83 mati (0 px), 11 halus (<60 px)**. Pola mati
= fakta rig VBridger, bukan bug runtime:

- Rantai physics `*Physics(R|L|)X/Y#_{1,4}`: segmen `_1`/`_4` mati, `_2`/`_3`
  hidup (contoh RX2_3 = 23.512 px vs RX2_1 = 0) — rigger hanya memakai
  segmen tengah tiap rantai.
- `ParamEyePhysics8–14, 17, 18` (grup kedua fisika mata: "Pupil Physics2",
  "eyelashes shake3/4", dst.) mati semua; 17/18 bahkan TIDAK ADA di
  physics3.json. Grup pertama (1–7, 15–16) hidup tapi halus (eyelash
  shake1/2 = 11–25 px).
- Param mulut rigger `MouthFunnel/Shrug/PressLipOpen/PuckerWiden` mati;
  `EX01/04/06/07/12` mati; `EX02/03/05/10` hidup tapi halus (14–52 px).
- Implikasi: slider/preset pada param mati memang tidak akan pernah
  berbuat apa-apa — bukan sesuatu yang bisa diperbaiki app.
