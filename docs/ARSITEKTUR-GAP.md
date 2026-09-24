# GAP AUDIT: sisa follow-up

> Audit Phase A/B/C terhadap [`ARSITEKTUR-TARGET.md`](ARSITEKTUR-TARGET.md), dibuat
> 2026-09-19 di branch `migration/pixi8-cubism`. **Rework perilaku (Fase 2 sampai 6,
> gap §6-12 / §15-18 / §32-34) sudah tertutup 2026-09-19.** Catatan tiap fase yang
> sudah selesai tidak lagi disimpan di sini; jejaknya ada di
> [`STATUS-CUBISM5-EFEK.md`](STATUS-CUBISM5-EFEK.md) (entri Speech boundary sampai
> Worker task identity). Yang tersisa di bawah ini hanya item terbuka, dengan status
> diverifikasi terhadap kode saat kondensasi (2026-09-24).

## Sisa follow-up

- **Verifikasi runtime visual VTuber overlay (OBS).** Wiring `window.__live2dView`
  sudah ada di `static/vtuber.html` (loadModel + setLipsyncProvider; ekspresi native
  aman no-op bila model tak punya yang cocok). Yang belum tercatat: menjalankan
  verifikasi runtime penuh model + TTS + overlay OBS bersamaan. (§7)
- **`CapabilityAnalyzer.ts` masih orphan.** Hanya dipakai unit test
  (`test/live2d-pipeline.test.ts`), belum di-tap ke produksi; produksi memakai
  `detectModelCapabilities` + `getCapabilityProfile` di `app.js`. Putuskan: jadikan
  boundary produksi, atau buang.
- **Expression semantik cocok native by-name saja (tanpa alias).** Nama emosi
  seperti "happy" tidak otomatis cocok dengan `.exp3` bernama "smile". (§21)

## Known by-design (bukan TODO, jangan "diperbaiki" tanpa alasan)

- **Dua kanal tulis manual di order 900 hidup berdampingan:** `AppWriteUpdater`
  (jalur app.js) dan `ArbiterUpdater` (`ParameterArbiter`, praktis dormant di
  integrasi view). `coreModel` sengaja bocor sebagai pintu tulis legacy yang
  ditampung aman.

Aturan per fase (ARSITEKTUR-TARGET §46): satu boundary per waktu; guard/test
diperbarui di commit yang sama; verifikasi runtime setelah perubahan; jangan
memperbaiki masalah satu layer lewat layer lain.
