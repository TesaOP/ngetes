/**
 * client/i18n-entry — entrypoint bundle TERPISAH → static/js/i18n.js.
 *
 * Dipakai static/pet.html yang tidak memuat bundle.js penuh (pet tidak butuh
 * MotionRuntime/brain). self-executing: init() langsung menyweep DOM pet.
 * index.html TIDAK memuat file ini — i18n sudah ikut di dalam bundle.js.
 *
 * Ekspos window.__i18n = { t, getLang, setLang, ... } — kontrak yang sama
 * dengan bundle.js (bundle-entry.ts): driver pet memanggil window.__i18n.t()
 * saat runtime. Dulu entry ini hanya init() tanpa ekspos runtime — SEMUA
 * t() di pet.html selalu mengembalikan kunci mentah (i18n pet mati senyap).
 */
import * as i18n from "./i18n/index";

if (typeof window !== "undefined") {
  i18n.init();
  window.__i18n = i18n;
}
