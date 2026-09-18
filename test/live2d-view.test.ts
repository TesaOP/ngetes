/**
 * Unit test framing facade view Pixi 8 (src/live2d/view/Live2DView.ts) —
 * murni matematika, tanpa GL/DOM.
 */
import { describe, expect, it } from "bun:test";
import { computeModelMvp, localToScreen, screenToLocal, type FacadeTransform } from "../src/live2d/view/framing";

function t(over: Partial<FacadeTransform> = {}): FacadeTransform {
  return {
    x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0,
    anchorX: 0, anchorY: 0,
    origW: 400, origH: 400,
    canvasW: 1, canvasH: 1,
    stageW: 800, stageH: 600,
    resolution: 1,
    ...over,
  };
}

function apply(m: Float32Array, px: number, py: number): { x: number; y: number } {
  return { x: m[0] * px + m[4] * py + m[12], y: m[1] * px + m[5] * py + m[13] };
}

describe("computeModelMvp — paritas framing pixi-live2d", () => {
  it("pusat canvas model (0,0 y-up) → pusat box lokal (origW/2, origH/2)", () => {
    const m = computeModelMvp(t());
    const p = apply(m, 0, 0);
    // ortho 800×600: x_css = (x_clip+1)/2·800 ; y_css = (1−y_clip)/2·600
    const sx = (p.x + 1) / 2 * 800;
    const sy = (1 - p.y) / 2 * 600;
    expect(sx).toBeCloseTo(200, 3);
    expect(sy).toBeCloseTo(200, 3);
  });

  it("model y-up: puncak kepala (canvasH/2) jatuh di ATAS box (y lokal ≈ 0)", () => {
    const m = computeModelMvp(t());
    const p = apply(m, 0, 0.5);
    const sy = (1 - p.y) / 2 * 600;
    expect(sy).toBeCloseTo(0, 3);
    const bawah = apply(m, 0, -0.5);
    expect((1 - bawah.y) / 2 * 600).toBeCloseTo(400, 3);
  });

  it("anchor(0,0): pojok kiri-atas box = (x,y)", () => {
    const m = computeModelMvp(t({ x: 120, y: 80 }));
    const p = apply(m, -0.5, 0.5); // pojok kiri-atas model (canvas -cw/2, +ch/2)
    expect((p.x + 1) / 2 * 800).toBeCloseTo(120, 3);
    expect((1 - p.y) / 2 * 600).toBeCloseTo(80, 3);
  });

  it("anchor(0.5,0.5): pusat box dipindah ke posisi (x,y)", () => {
    const s = localToScreen(t({ anchorX: 0.5, anchorY: 0.5, x: 300, y: 200 }), 200, 200);
    expect(s.x).toBeCloseTo(300, 3);
    expect(s.y).toBeCloseTo(200, 3);
  });

  it("scale 2 menggandakan box dari titik anchor", () => {
    const s = localToScreen(t({ scaleX: 2, scaleY: 2 }), 200, 200);
    expect(s.x).toBeCloseTo(400, 3);
    expect(s.y).toBeCloseTo(400, 3);
    // scale 2 + anchor tengah = box membesar di sekeliling posisi
    const c = localToScreen(t({ scaleX: 2, scaleY: 2, anchorX: 0.5, anchorY: 0.5 }), 200, 200);
    expect(c.x).toBeCloseTo(0, 3);
    expect(c.y).toBeCloseTo(0, 3);
  });

  it("resolution 2: hasil CSS tetap — mvp bekerja di piksel fisik", () => {
    const s = localToScreen(t({ resolution: 2, stageW: 800, stageH: 600 }), 200, 200);
    expect(s.x).toBeCloseTo(200, 3);
    expect(s.y).toBeCloseTo(200, 3);
  });

  it("canvas non-persegi (ren 1.0 × 1.346): box mengikuti rasio canvas", () => {
    const s = localToScreen(t({ canvasW: 1, canvasH: 1.346, origW: 400, origH: 400 * 1.346 }), 200, 538.46);
    // titik bawah-tengah box
    expect(s.x).toBeCloseTo(200, 4);
    expect(s.y).toBeCloseTo(538.46, 3);
  });

  it("rotasi memutar box di sekitar posisi+anchor", () => {
    const s = localToScreen(t({ rotation: Math.PI, anchorX: 0.5, anchorY: 0.5, x: 300, y: 200 }), 200, 200);
    expect(s.x).toBeCloseTo(300, 3);
    expect(s.y).toBeCloseTo(200, 3);
  });
});

describe("screenToLocal — kebalikan localToScreen (zoom-anchored app.js)", () => {
  const CASES: Partial<FacadeTransform>[] = [
    {},
    { x: 120, y: 80 },
    { scaleX: 2, scaleY: 2 },
    { scaleX: 1.36, scaleY: 1.36, x: 50, y: 30 },
    { anchorX: 0.5, anchorY: 0.5, x: 300, y: 200 },
    { rotation: 0.7, anchorX: 0.5, anchorY: 0.5, x: 300, y: 200 },
    { rotation: Math.PI },
    { resolution: 2, stageW: 800, stageH: 600 },
    { canvasW: 1, canvasH: 1.346, origW: 400, origH: 400 * 1.346 },
  ];

  for (const over of CASES) {
    const label = Object.entries(over).map(([k, v]) => `${k}=${v}`).join(", ") || "default";
    it(`roundtrip lokal→layar→lokal identitas (${label})`, () => {
      const tf = t(over);
      const p = { x: 173.2, y: 88.7 };
      const s = localToScreen(tf, p.x, p.y);
      const back = screenToLocal(tf, s.x, s.y);
      expect(back.x).toBeCloseTo(p.x, 4);
      expect(back.y).toBeCloseTo(p.y, 4);
    });
    it(`roundtrip layar→lokal→layar identitas (${label})`, () => {
      const tf = t(over);
      const s = { x: 321.7, y: 254.3 };
      const l = screenToLocal(tf, s.x, s.y);
      const back = localToScreen(tf, l.x, l.y);
      expect(back.x).toBeCloseTo(s.x, 4);
      expect(back.y).toBeCloseTo(s.y, 4);
    });
  }

  it("titik di bawah kursor tetap di bawah kursor saat scale berubah (semantik setScaleAroundPoint)", () => {
    // skenario app.js: local = toLocal(cursor) di scale lama, lalu
    // x = cursor - local*scaleBaru — titik lokal itu HARUS tetap jatuh di kursor.
    const tf = t({ scaleX: 1.2, scaleY: 1.2, x: 100, y: 60, anchorX: 0, anchorY: 0 });
    const cursor = { x: 350, y: 210 };
    const local = screenToLocal(tf, cursor.x, cursor.y);
    const next: FacadeTransform = { ...tf, scaleX: 2.4, scaleY: 2.4, x: cursor.x - local.x * 2.4, y: cursor.y - local.y * 2.4 };
    const after = localToScreen(next, local.x, local.y);
    expect(after.x).toBeCloseTo(cursor.x, 3);
    expect(after.y).toBeCloseTo(cursor.y, 3);
  });
});
