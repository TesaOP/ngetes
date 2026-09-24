# Troubleshooting

Masalah umum dan solusinya. Detail arsitektur: [`AGENTS.md`](../AGENTS.md) untuk
kontributor, README untuk ringkasan produk.

- **Chat diam total atau panggung kosong?** Belum `bun run build`: `static/js/bundle.js` dan `static/js/live2d-view.mjs` tidak ada (dua-duanya di-gitignore), jadi `window.__agent` tidak terpasang dan renderer tidak termuat. Jalankan build, lalu refresh.
- **Karakter kurang atau terlalu lebay mengikuti mouse?** Panel konfigurasi, slider **Ekspresif kepala/mata/badan (gaze)**: per-model, live-apply, persist lewat Simpan.
- **Diam 30 menit?** Tab AI, **Kelakuan**, **Hidup**, Simpan. Otak membaca `quietMs` langsung dari `window.__appEvents` (live, tanpa restart).
- **0 emosi?** Cek console `[exp3] adopted N`. Kalau 0, model memang tanpa `.exp3`; bikin preset `emosi` di tab Sheet.
- **Fetch gagal?** Cek `location.origin`, jangan hardcode `127.0.0.1:8310`.
- **Model CJK 404?** `safeJoin` men-decode `%E7%A5%9E` jadi `神宫白子`, ditangani `core/src/static_serve.rs`.
- **Upload model gagal?** Model dikirim sebagai JSON base64 ke `/api/upload` atau `/api/import-zip`, dan folder wajib memuat `*.model3.json`. Body yang tidak valid ditolak dengan 400.
- **Akses dari HP atau LAN?** `HOST=0.0.0.0`, tapi sadari semua orang di jaringan bisa membaca server.
- **Model blank di headless?** Normal: swiftshader tidak render WebGL ke framebuffer; model tetap load (console `[Live2D] Model loaded`).
- **TTS 429 atau suara browser terus?** Kuota provider TTS habis (mis. Gemini free tier); sistem otomatis jatuh ke suara browser. Tunggu reset kuota atau isi billing. Detail provider di menu Mesin Suara.
- **Suara panjang terpotong atau berjeda?** Pipeline per-kalimat menunggu latensi provider (Gemini sekitar 12 sampai 16 detik per request); segmen berikutnya di-prefetch, jadi pastikan jaringan stabil.
- **VTuber Twitch feed kosong padahal "Terhubung"?** Sebagian ISP atau proxy memblokir TMI chat Twitch; coba VPN atau hotspot, atau pakai provider YouTube/mock.
- **Assistant menolak menjalankan perintah?** Itu fitur: `write_file` dan `run_command` menunggu persetujuanmu di kartu approval panel Assistant.
- **STT tidak mulai merekam?** Karakter sedang bicara TTS; push-to-talk sengaja ditolak saat itu (anti-echo, supaya dia tidak mengobrol dengan dirinya sendiri).
- **TTS/STT native diam, atau "engine native tidak tersedia"?** Engine native sekarang library in-process (bukan lagi sidecar exe). STT butuh `cargo build --release -p lumimi --features engine-stt` (cmake plus LLVM/libclang di PATH plus `LIBCLANG_PATH`); TTS jalan tanpa itu. Tanpa build STT, pilih provider TTS/STT lain, dan fitur degrade dengan anggun.
- **Native pertama kali lama, atau "mengunduh model…"?** Model tidak dibundel: diunduh sekali on-demand (SuperTonic sekitar 385 MB, Whisper GGML sekitar 40 sampai 150 MB) dari HuggingFace lalu di-cache (`~/.cache/supertonic3` plus `engines/models/`). Butuh online sekali; offline sebelum terunduh memunculkan error jelas dan fitur degrade.
- **STT native kurang akurat?** Default model `base`. Ganti `stt.engineModel` ke `small` atau `medium` di `data/config.json` (lebih akurat, lebih berat, unduhan lebih besar). Model `tiny` paling ringan tapi paling sering salah kata.
