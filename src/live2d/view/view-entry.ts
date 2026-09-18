/**
 * view-entry — jembatan window untuk integrasi view Pixi 8.
 * Dimuat sebagai module script (index.html) SETELAH skrip klasik app.js —
 * lihat catatan timing di Live2DView. app.js menandai
 * window.__live2dRendererRequested saat ?renderer=pixi8; modul ini
 * mengambil alih canvas panggung, lalu menyelesaikan janji
 * __live2dViewWait yang ditunggu jalur init model app.js.
 */
import { Live2DView } from "./Live2DView";

const view = new Live2DView();
const w = window as any;

w.__live2dView = {
  ready: false,
  view,
  loadModel: (p: string) => view.loadModel(p),
  resize: (cw: number, ch: number) => view.resize(cw, ch),
  setBackground: (hex: number) => view.setBackground(hex),
  // Flip kepemilikan blink (#1), breath (#2), gaze (#3): app.js tetap
  // pemegang pintu konfigurasi, framework yang memutar.
  setBlinkGate: (fn: (() => boolean) | null) => view.setBlinkGate(fn),
  setBreathGate: (fn: (() => boolean) | null) => view.setBreathGate(fn),
  setLookGate: (fn: (() => boolean) | null) => view.setLookGate(fn),
  setLookTarget: (x: number, y: number) => view.setLookTarget(x, y),
  // Flip kepemilikan lipsync (#4): app.js memasang penyedia 0..1 (sumber
  // audio tetap di driver), framework menulis param mulut.
  setLipsyncProvider: (fn: (() => number | null) | null) =>
    view.setLipsyncProvider(fn),
};

if (w.__live2dRendererRequested) {
  const canvas = document.querySelector("#live2d-canvas") as HTMLCanvasElement | null;
  const size = w.__live2dStageSize?.() ?? {
    w: canvas?.clientWidth || 800,
    h: canvas?.clientHeight || 600,
  };
  if (canvas) {
    void view.init(canvas, size.w, size.h).then(() => {
      w.__live2dView.ready = true;
      w.__live2dViewReadyResolve?.();
    });
  } else {
    console.error("[view] #live2d-canvas tidak ditemukan — pixi8 view batal");
  }
} else {
  // tanpa toggle: modul tidak melakukan apa pun (stack lama jalan normal)
  w.__live2dViewReadyResolve?.();
}

export {};
