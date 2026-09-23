# Troubleshooting

Masalah umum dan solusinya. Detail arsitektur: [`AGENTS.md`](../AGENTS.md)
untuk agent · README untuk ringkasan produk.

- **Chat diam total / panggung kosong?** Belum `bun run build` — `static/js/bundle.js` dan `static/js/live2d-view.mjs` tidak ada (dua-duanya di-gitignore), jadi `window.__agent` tidak terpasang dan renderer tidak termuat. Jalankan build, refresh.
- **Karakter kurang/terlalu lebay mengikuti mouse?** Panel konfigurasi → slider **Ekspresif kepala/mata/badan (gaze)** — per-model, live-apply, persist lewat Simpan.
- **Diam 30 menit?** Tab ⚙️ AI → **🎚️ Kelakuan** → **⚡ Hidup** → Simpan. Otak membaca `quietMs` langsung dari `window.__appEvents` (live, tanpa restart).
- **0 emosi?** Console `[exp3] adopted N` — kalau 0, model memang tanpa `.exp3`; bikin preset `emosi` di tab Sheet.
- **Fetch gagal?** Cek `location.origin` — jangan hardcode `127.0.0.1:8310`.
- **Model CJK 404?** `safeJoin` decode `%E7%A5%9E` → `神宫白子` di-handle `src/server/index.ts`.
- **413 saat upload?** Body melebihi cap endpoint (sheet 5 MB, upload 200 MB, import-zip 500 MB).
- **Akses dari HP/LAN?** `HOST=0.0.0.0` — sadari semua orang di jaringan bisa membaca server.
- **Model blank di headless?** Normal — swiftshader tidak render WebGL ke framebuffer; model tetap load (console `[Live2D] Model loaded`).
- **TTS 429 / suara browser terus?** Kuota provider TTS habis (mis. Gemini free tier) — sistem otomatis jatuh ke suara browser; tunggu reset kuota atau isi billing. Detail provider: ⚙️ → Mesin Suara.
- **Suara panjang terpotong/berjeda?** Pipeline per-kalimat menunggu latensi provider (Gemini ±12–16 s/request); segmen berikutnya di-prefetch — pastikan jaringan stabil. Cache server 30 mnt membuat kalimat sama instan.
- **VTuber Twitch feed kosong padahal "Terhubung"?** Sebagian ISP/proxy memblokir TMI chat Twitch — coba VPN/hotspot, atau pakai provider YouTube/mock.
- **Assistant menolak menjalankan perintah?** Itu fitur — `write_file`/`run_command` menunggu persetujuanmu di panel Assistant (kartu ⚠️).
- **STT tidak mulai merekam?** Karakter sedang bicara TTS — push-to-talk sengaja ditolak saat itu (anti-echo: tanpa itu dia mengobrol dengan dirinya sendiri).
- **TTS/STT native diam / "engine native tidak tersedia"?** Engine native kini
  library in-process (bukan sidecar exe — `engines/live2d-engine.exe` sudah
  dihapus). Build: STT butuh `cargo build --release -p companion --features engine-stt`
  (cmake + LLVM/libclang di PATH + `LIBCLANG_PATH`); TTS jalan tanpa itu. Tanpa
  build STT, pilih provider TTS/STT lain di ⚙️ — degrade anggun.
- **Native pertama kali lama / "mengunduh model…"?** Model TIDAK dibundel — diunduh on-demand sekali (SuperTonic ~385 MB, Whisper GGML ~40–150 MB) dari HuggingFace lalu di-cache (`~/.cache/supertonic3` + `engines/models/`). Butuh online sekali; offline sebelum terunduh → error jelas, fitur degrade.
- **STT native kurang akurat?** Default model `base`. Ganti `stt.engineModel` ke `small`/`medium` di `data/config.json` (lebih akurat, lebih berat + unduhan lebih besar). Model `tiny` paling ringan tapi paling sering salah kata.
