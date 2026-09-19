/**
 * shared/speech-timing.ts — Estimasi durasi bicara TTS dari panjang teks.
 *
 * Dipakai dua pihak: MotionRuntime klien (melar playback [MOTION:id] supaya
 * mengisi seluruh omongan) dan scheduler VTuber server (menahan active slot
 * sampai balasan selesai dibicarakan klien). Heuristik sama dengan yang
 * dulu hidup di brain.ts — dipindah ke shared supaya server tidak menduplikasi rumus.
 */

/** Estimasi durasi bicara satu segmen (heuristik: ±16 karakter/detik plus
 *  lead-in; dibatasi 12 dtk supaya angka liar tidak menjebak slot bicara).
 *  Tidak bisa eksak: durasi TTS sebenarnya baru diketahui saat audio selesai. */
export function estimateSpeechMs(text: string): number {
  const t = String(text || "").trim();
  if (!t) return 0;
  return Math.min(12000, Math.round(500 + t.length * 62));
}
