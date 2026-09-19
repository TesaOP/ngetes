# GAP AUDIT — Target Arsitektur vs Kode Aktual

> Audit Phase A (View) / B (Behavior) / C (Engine) terhadap
> [`ARSITEKTUR-TARGET.md`](ARSITEKTUR-TARGET.md), dilakukan 2026-09-19 di
> branch `migration/pixi8-cubism` @ `174a8e7`. Dokumen hidup: coret item
> saat fase terkait selesai + terverifikasi runtime. Verdict:
> ✅ ADA PENUH · ⚠️ SEBAGIAN · ❌ TIDAK ADA.

## Keputusan terkunci (2026-09-19, user)

1. **Rework arsitektur dulu**, stabilisasi fitur pasca-migrasi menyusul —
   regresi runtime ditangani di fase yang memilikinya (mis. vtuber.html di
   fase VTuber).
2. Daftar "fitur belum work" belum pasti dari user (belum diuji langsung) —
   temukan lewat verifikasi runtime tiap fase, jangan ditebak.
3. `MODES.md` (teardown penuh pindah mode) vs target §35–36 (context switch
   tanpa destruction) — **direkonsiliasi saat fase terkait**, eksplisit di
   commit-nya.

## Sudah terpenuhi — jangan dikerjakan ulang

| Target | Bukti |
|---|---|
| Role space ±30-ref → range model, ADD vs SET, roleDefault | `src/client/engine/role-mapping.ts`; wrapper `app.js` (`pokeRoleNorm/pokeRoleRef/roleDefault`) |
| Motion semantik verb → klip native per-model; LLM hanya pilih id yang di-sanitize | `src/client/engine/motion-taxonomy.ts`; directive-parser; director server memvalidasi ke daftar (`index.ts:1286-1327`) |
| Urutan update dua fase + updater ber-gate (blink/breath/gaze single-owner, order 200–900, AppWriteUpdater 900) | `src/live2d/Live2DUserModel.ts:219-232`; `icubismupdater.ts:23-32` |
| Satu engine/renderer untuk semua mode; renderer & model selamat pindah mode (§35 sebagian) | facade `src/live2d/view/Live2DView.ts`; `mode-runtime.js` hanya teardown panel/poll klien |
| Speech ≠ execution — lifecycle memang terpisah (§15) | `brain.ts:468-471`; `assistant.ts:139-150` |
| Harness dua lane tanpa IntentRouter (§13) | chat dock → `/api/chat`; `#as-input` → `/api/assistant/*` |
| Capability engine-safe sampai ke LLM (angka param tidak pernah dikirim) | `app.js` `getCapabilityProfile()`; prompt di bundle + director sanitize |

## Gap lapisan behavior (inti rework)

| Target | Kode aktual | Bukti kunci |
|---|---|---|
| ~~§6 Companion MERGE saat thinking~~ **✅ Fase 3 (2026-09-19)** | Pesan baru saat mikir tidak diabaikan lagi: request lama di-abort, kedua teks sudah di history, satu fetch baru menjawab keduanya (merge stateless via history). Input user otomatis menggulingkan `reactEvent` yang sedang mikir (§18) | `brain.think()` gen + `ctrl.abort()`; `test/companion-concurrency.test.ts` |
| ~~§6 Companion PREEMPT saat speaking~~ **✅ Fase 2 (2026-09-19)** | Speech policy memotong chain lama; `onPreempted` ≠ `onDone`; `unlockAI` sekali-saja; rantai zombi/orphan markDone mati | `speech-policy.ts` + `brain.playSegments` + runtime verify |
| ~~§32–34 Stale/generation protection + model-load epoch~~ **✅ Fase 3 (2026-09-19)** | Generasi per request (reply telat dibuang sebelum memutar apa pun, termasuk setelah director pass); `busy` sinkron sebelum await (race double-entry mati); timeout 90 dtk via `REQUEST_TIMEOUT_MS` (§32); epoch model — profil model lama yang telat tidak menimpa model baru; finally hanya generasi terbaru yang reset state | `brain.ts` gen/ctrl/modelEpoch |
| ~~§7 VTuber dedup + cooldown audience~~ **✅ Fase 5 (2026-09-19)** | **Terimplementasi di server** (`vtuber-scheduler.ts`): dedup user+teks 30 dtk → DROP; cooldown global → DROP; audience = suppression (tidak antre). Behavior pindah dari dua klien ke SATU scheduler server — race app-vs-overlay mati dari akarnya | `vtuber-scheduler.ts ingestChat` |
| ~~§7 Donation FIFO-20 tolak eksplisit~~ **✅ Fase 5 (2026-09-19)** | Antrean FIFO cap 20; penuh → item BARU ditolak + feedback system di feed; item lama tidak pernah di-silent-evict. Slot ditahan selama estimasi bicara (`speech-timing` + buffer 500ms) | `vtuber-scheduler.ts enqueue` + runtime verify (3 donasi terserialisasi) |
| ~~§7 Operator queue + precedence donation>operator + active slot~~ **✅ Fase 5 (2026-09-19)** | Operator = kelas sendiri (masuk walau respond mati), antrean FIFO-20; satu active slot dibagi donation+operator, item aktif tidak dipreempt; slot bebas → donation dulu. Intake: `POST /api/vtuber/operator` + input UI; echo feed "Operator: …". Precedence terverifikasi live (3 balasan donasi diproses sebelum operator) | `vtuber-scheduler.ts drain` + `vtuber.ts vtuberOperatorSay` |
| ~~§9 Worker `activeTask/taskId/parkedTasks`~~ **✅ Fase 6 (2026-09-19)** | **Terimplementasi**: `WorkerTask` (`state.ts`) — slot aktif tunggal + antrean FIFO cap 20 (penuh → item baru DITOLAK eksplisit, tidak silent-evict); task baru saat sibuk **di-park** (prompt tidak menyentuh history sebelum jalan — pencampuran history dua task mati); drain otomatis saat slot kosong; status mengekspos activeTask/parkedTasks | `state.ts`, `assistant.ts runTask/drainNext` |
| ~~§10 Approval pause mempertahankan ownership~~ **✅ Fase 6 (2026-09-19)** | Loop return `paused` + **tidak melepas busy**; slot tetap milik task saat menunggu izin — ask baru di-park, resume (`assistantResolveApproval` → `runTask` task yang sama) tidak lewat gerbang park; approval yatim (task dibatalkan saat pause) ditolak eksplisit | `loop.ts paused/finally`, `assistant.ts` |
| ~~§11 `cancel(taskId)` kooperatif~~ **✅ Fase 6 (2026-09-19)** | `assistantCancel(taskId?)`: running → kooperatif; paused → terminal LANGSUNG (loop tidak jalan); parked → dikeluarkan spesifik (FIFO utuh); tanpa id → task aktif (kompat panel/CLI) | `assistant.ts assistantCancel`, route `/api/assistant/cancel` |
| ~~§12 Modify = replacement mewarisi posisi~~ **✅ Fase 6 (2026-09-19)** | `assistantModify(taskId, text)` route baru: running → cancel kooperatif + `pendingReplacement` (warisi slot sebelum antrean: A→A'→B→C); paused → terminal + replacement langsung; parked → diganti in-place (posisi tetap) | `assistant.ts assistantModify` |
| ~~§17–18 Proactive gate per mode/worker/brain-off~~ **✅ Fase 4 (2026-09-19)** | **Terimplementasi** di `reactEvent()` (satu muara semua event proaktif — idle/away/return/mood; jalur `setPresence` ikut): gate `proactiveAllowed()` SEBELUM LLM/director/speech — (1) toggle Mode Otak dibaca dari DOM (elemen sama dengan jalur chat, tak bisa desinkron), (2) mode aktif ≠ stage → ditekan (VTuber/pet punya perilaku sendiri; karakter dobel dicegah), (3) `assistant.busy` dari `/api/mode` → ditekan (task hidup walau panel ditutup), (4) fetch gagal → fail-open. Slot diklaim sebelum await gate → think yang datang menang via gen-guard (Fase 3). Kelas speech `companion_proactive` (tier 1) — tidak bisa memotong bicara user/narasi/VTuber. `expressEventEmotion` (visual) + `setUserMood` (tracking) sengaja tidak digate | `brain.ts proactiveAllowed`; 6 test baru di `companion-concurrency.test.ts` |
| ~~§15–16 Speech policy layer~~ **✅ Fase 2 (2026-09-19)** | **Terimplementasi**: `src/client/speech/speech-policy.ts` (murni, 19 unit test) dipasang sebagai `window.__speech`; app.js `speak()` = satu pintu audio sadar-claim (executor `runSpeech`); 6 kelas produser: companion/direct (t2), vtuber/worker_narration/companion_proactive (t1), worker_actor (t0). Keputusan terimplementasi: serialisasi worker DUA ARAH menang atas tier (§16-3); antrean FIFO cap 4, overflow buang item baru; policy menguasai AUDIO saja (bubble/chat tetap ditulis producer saat SUPPRESS); preempt memanggil `onPreempted` (bukan onDone — §6 dipotong ≠ completed); unlockAI sekali-saja via onPreempted; `stopSpeaking()` publik (teardown pindah mode + ganti model); request sinkron dari dalam cleanup preempt → antrean (flag transisi); degradasi legacy penuh tanpa bundle. Bug lama yang ikut mati: markDone telat menginjak state claim baru, rantai segmen zombi, bocor blob URL, `aiLock` nyangkut saat chain dipotong | `speech-policy.ts`; `app.js` `speak/runSpeech/stopSpeaking`; `brain.ts playSegments`; `panel.ts speakAsCharacter`; `mode-runtime.js switchMode` |

## Gap lapisan engine (sisa kecil)

- **Expression semantik → native by-name saja** (tanpa alias): "happy"
  tidak akan cocok dengan exp3 "smile" — `app.js:1937-1940`. (§21)
- **Speech adapter tanpa cancel/preempt eksplisit**: `doRemoteTTS`
  (`app.js:2361-2465`) hanya guard timeout internal; tidak ada
  `stopSpeaking()` publik; prefetch segmen tetap jalan saat dipotong. (§15)
- **`CapabilityAnalyzer.ts` orphan** (hanya dipakai unit test) — kandidat
  engine-boundary yang belum di-tap ke produksi; produksi pakai
  `detectModelCapabilities` + `getCapabilityProfile` di app.js.
- **Dua kanal tulis manual order 900** hidup berdampingan:
  `AppWriteUpdater` (jalur app.js) dan `ArbiterUpdater` (jalur
  renderer/proof, `ParameterArbiter` — praktis dormant di integrasi view).
  coreModel bocor sebagai pintu tulis legacy BY DESIGN (tampung aman).

## Regresi migrasi (lapisan runtime)

- ~~**`vtuber.html:70` masih memuat `js/pixi-live2d-0.4.0.js` yang sudah
  dihapus dari repo**~~ **✅ Fase 5 (2026-09-19)** — overlay OBS dimigrasikan
  ke stack baru (pola pet.html: importmap pixi8.mjs + live2d-view.mjs +
  `__live2dView.loadModel`); bibir browser-TTS via `setLipsyncProvider`
  (role-resolved, menggantikan `mouthPulse` yang hardcode ParamMouthOpenY);
  behavior LLM/antrean dilepas dari overlay (scheduler server). Runtime
  verify: model termuat, HUD hidup, tanpa `PIXI.live2d`.
- ~~Race ganda app-utama vs overlay~~ **✅ Fase 5 (2026-09-19)** — akar
  masalah dibongkar: tidak ada lagi dua klien yang masing-masing memutuskan
  kapan membalas; SATU scheduler server yang memutuskan, heartbeat overlay
  hanya menentukan SIAPA yang membicarakan balasan.

## Rencana fase

| # | Fase | Status |
|---|---|---|
| 0 | Dokumen target + gap audit masuk repo | ✅ selesai |
| 1 | Stabilisasi migrasi | ditunda (keputusan #1) |
| 2 | **Speech boundary** — lapisan kepemilikan speech + policy + API cancel/preempt eksplisit | ✅ selesai 2026-09-19 (gate hijau: 476 unit + 416 guard; runtime verify menyusul) |
| 3 | Companion concurrency — MERGE thinking, PREEMPT speaking, generation/stale, epoch model-load | ✅ selesai 2026-09-19 (482 unit + 416 guard; runtime verify merge+preempt hijau) |
| 4 | Proactive gate — suppress VTuber aktif / Worker jalan / brain off | ✅ selesai 2026-09-19 (488 unit + 416 guard; runtime verify 4 skenario hijau) |
| 5 | VTuber — dedup+cooldown audience, donation FIFO-20 tolak eksplisit, operator queue + precedence; migrasikan vtuber.html ke stack baru | ✅ selesai 2026-09-19 (501 unit + 416 guard; runtime verify: balasan agen live, precedence donasi→operator, overlay OBS hidup) |
| 6 | Worker task identity — taskId/activeTask/parked, cancel(taskId), approval ownership, modify-replacement | ✅ selesai 2026-09-19 (514 unit + 416 guard; runtime verify menyusul) |

**REWORK LENGKAP (Fase 2–6) — semua gap behavior §6–12, §15–18, §32–34
tertutup 2026-09-19.** Sisa opsional (UI daftar antrean task, cancel per-task
dari UI, ekspresi/motion balasan VTuber §7) **✅ diselesaikan 2026-09-19**
(STATUS entri 48, klien-aditif). SISA follow-up: ekspresi VTuber di overlay
OBS (`vtuber.html`) masih no-op (perlu wiring `window.__live2dView`) +
verifikasi runtime visual (model+TTS+OBS) belum dijalankan.

Aturan per fase (dari ARSITEKTUR-TARGET §46): satu boundary per waktu,
guard/test diperbarui di commit yang sama, verifikasi runtime setelah
perubahan, jangan perbaiki masalah satu layer lewat layer lain.
