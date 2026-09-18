/**
 * Unit test framing facade view Pixi 8 (src/live2d/view/Live2DView.ts) —
 * murni matematika, tanpa GL/DOM.
 */
import { describe, expect, it } from "bun:test";
import { computeModelMvp, localToScreen, type FacadeTransform } from "../src/live2d/view/framing";

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
