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
  // Flip kepemilikan blink (Fase B): app.js tetap pemegang pintu
  // konfigurasi kedip (blinkEnabled sheet + frozen), framework memutar.
  setBlinkGate: (fn: (() => boolean) | null) => view.setBlinkGate(fn),
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
