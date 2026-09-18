/**
 * framing.ts — matematika transform facade view (murni, tanpa dependensi
 * framework/pixi/GC) supaya bisa diuji bun test tanpa memuat core WASM.
 *
 * mvp paritas framing pixi-live2d 0.4.0 (column-major, p' = M·p; layout sama
 * dengan CubismMatrix44._tr):
 *   P·RES·T(x,y)·R·S(sx,sy)·T(-ax·ow,-ay·oh)·T(ow/2,oh/2)·S(1,-1)·S(ow/cw,oh/ch)
 * Titik model (unit canvas, y-up, terpusat) → box lokal y-down (0..ow, 0..oh,
 * kepala di atas) → transform display CSS (padanan pixi: position·rotate·
 * scale·anchor) → clip space ortho.
 */

type F32 = Float32Array;

export function mat4Mul(a: F32, b: F32): F32 {
  const o = new Float32Array(16);
  for (let c = 0; c < 4; c++) {
    for (let r = 0; r < 4; r++) {
      o[c * 4 + r] =
        a[0 * 4 + r] * b[c * 4 + 0] +
        a[1 * 4 + r] * b[c * 4 + 1] +
        a[2 * 4 + r] * b[c * 4 + 2] +
        a[3 * 4 + r] * b[c * 4 + 3];
    }
  }
  return o;
}

export function mat4Identity(): F32 {
  const m = new Float32Array(16);
  m[0] = m[5] = m[10] = m[15] = 1;
  return m;
}

export function mat4Scale(x: number, y: number, z = 1): F32 {
  const m = mat4Identity();
  m[0] = x;
  m[5] = y;
  m[10] = z;
  return m;
}

export function mat4Translate(x: number, y: number): F32 {
  const m = mat4Identity();
  m[12] = x;
  m[13] = y;
  return m;
}

export function mat4Rotate(rad: number): F32 {
  const m = mat4Identity();
  const c = Math.cos(rad), s = Math.sin(rad);
  m[0] = c; m[1] = s; m[4] = -s; m[5] = c;
  return m;
}

/** ortho y-down (ruang CSS: kiri-atas (0,0)) → clip [-1,1]. */
export function mat4OrthoScreen(w: number, h: number): F32 {
  const m = mat4Identity();
  m[0] = 2 / w;
  m[5] = -2 / h;
  m[12] = -1;
  m[13] = 1;
  return m;
}

export interface FacadeTransform {
  x: number;
  y: number;
  scaleX: number;
  scaleY: number;
  rotation: number;
  anchorX: number;
  anchorY: number;
  /** ukuran box lokal (px CSS) — pixi-live2d: originalWidth/Height */
  origW: number;
  origH: number;
  /** ruang canvas model (unit Cubism, mis. ren ≈ 1.0 × 1.346) */
  canvasW: number;
  canvasH: number;
  /** ukuran stage CSS */
  stageW: number;
  stageH: number;
  resolution: number;
}

/** Matriks display untuk titik BOX LOKAL (0..ow, 0..oh, y-down):
 * P·RES·T(x,y)·R·S(sx,sy)·T(-ax·ow,-ay·oh) — tanpa tahap canvas→box. */
export function computeDisplayMvp(t: FacadeTransform): Float32Array {
  const anchor = mat4Translate(-t.anchorX * t.origW, -t.anchorY * t.origH);
  const scale = mat4Scale(t.scaleX, t.scaleY);
  const rot = mat4Rotate(t.rotation);
  const pos = mat4Translate(t.x, t.y);
  const res = mat4Scale(t.resolution, t.resolution);
  const proj = mat4OrthoScreen(t.stageW * t.resolution, t.stageH * t.resolution);
  let m = mat4Mul(proj, res);
  m = mat4Mul(m, pos);
  m = mat4Mul(m, rot);
  m = mat4Mul(m, scale);
  m = mat4Mul(m, anchor);
  return m;
}

/** mvp penuh untuk titik CANVAS MODEL (unit Cubism, y-up, terpusat). */
export function computeModelMvp(t: FacadeTransform): Float32Array {
  const display = computeDisplayMvp(t);
  const unit = mat4Scale(t.origW / t.canvasW, t.origH / t.canvasH);
  const flipY = mat4Scale(1, -1);
  const center = mat4Translate(t.origW / 2, t.origH / 2);
  // kolom-vektor: kanan diterapkan duluan
  let m = mat4Mul(display, center);
  m = mat4Mul(m, flipY);
  m = mat4Mul(m, unit);
  return m;
}

/** p' = P·RES·T·R·S·A·p → kembalikan piksel CSS (padanan toGlobal pixi
 * untuk titik box lokal; clip → CSS: (clip+1)/2·stage). */
export function localToScreen(t: FacadeTransform, px: number, py: number): { x: number; y: number } {
  const m = computeDisplayMvp(t);
  const cx = m[0] * px + m[4] * py + m[12];
  const cy = m[1] * px + m[5] * py + m[13];
  return { x: ((cx + 1) / 2) * t.stageW, y: ((1 - cy) / 2) * t.stageH };
}
