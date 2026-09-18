/** Fase 11 — bridge MotionRuntime → Live2DRenderer (Cubism). */
import type { RuntimeBridge } from "../client/animation/motion-runtime";
import type { Live2DRenderer } from "./Live2DRenderer";
import { MotionPriority } from "./Live2DUserModel";

export class MotionBridge implements RuntimeBridge {
  private poseBase: Record<string, number> = {};
  private ownedParams = new Set<string>();

  constructor(private renderer: Live2DRenderer) {}

  getPoseBase(): Record<string, number> {
    return { ...this.poseBase };
  }

  applyPoseDelta(delta: Record<string, number>): void {
    const alias: Record<string, string> = { ax: "angleX", ay: "angleY", ex: "eyeBallX", ey: "eyeBallY", bodyX: "bodyAngleX", bodyY: "bodyAngleY", bodyZ: "bodyAngleZ", mouthForm: "mouthForm" };
    for (const k in delta) {
      const role = alias[k] || k;
      const v = delta[k] ?? 0;
      this.renderer.setRole(role, v, 'motion');
    }
  }

  clearPoseDelta(): void {
    // Reset semua role ke default ASLI model (bukan 0-ref — eyeOpen dsb.
    // default ≠ 0) lewat arbiter source 'motion': clear adalah tulisan
    // motion, jadi tulisan motion berikutnya menimpanya secara alami
    // (sama source, seq terbaru menang) — tidak mematikan motion seperti
    // kalau dipakai source 'manual' yang prioritasnya lebih tinggi.
    const map = this.renderer.getRoleMap();
    if (!map) return;
    for (const role in map) this.renderer.setRoleDefault(role, 'motion');
  }

  applyParamDrive(params: Record<string, number>): void {
    for (const id in params) {
      this.renderer.setParameter(id, params[id], 'motion');
      this.ownedParams.add(id);
    }
  }

  releaseParamDrive(paramIds: string[]): void {
    for (const id of paramIds) {
      this.ownedParams.delete(id);
      const info = this.renderer.getParameterInfo(id);
      // kembali ke default via source 'motion' — kepemilikan layer sudah
      // dilepas, tulisan motion berikutnya boleh menimpa
      if (info) this.renderer.setParameter(id, info.default, 'motion');
    }
  }

  readParam(id: string): number {
    return this.renderer.getParameter(id) ?? 0;
  }

  getSupports(): Set<string> {
    const map = this.renderer.getRoleMap();
    const caps = new Set<string>();
    if (!map) return caps;
    // field ax/ay -> head, ex/ey -> eyes, mouthForm -> mouth, body* -> body.
    // Kapabilitas hanya klaim dari role map nyata — model tanpa kepala
    // tidak mengaku punya kepala (jangan tambah fallback sintetis).
    if (map.angleX || map.angleY) caps.add("head");
    if (map.eyeBallX || map.eyeBallY) caps.add("eyes");
    if (map.mouthForm || map.mouthOpenY) caps.add("mouth");
    if (map.bodyAngleX || map.bodyAngleY || map.bodyAngleZ) caps.add("body");
    return caps;
  }

  getOwnedParams(): Set<string> {
    return new Set(this.ownedParams);
  }

  playNative(group: string): void {
    // motion native via Cubism motion manager — grup divalidasi dari
    // manifest di dalam model; -1 = kalah prioritas / grup tak ada.
    void this.renderer.playNativeMotion(group, 0, MotionPriority.Normal)
      .then((h) => console.log(`[MotionBridge] playNative ${group} → handle ${h}`))
      .catch((e) => console.warn(`[MotionBridge] playNative ${group} gagal`, e));
  }

  now(): number {
    return typeof performance !== "undefined" ? performance.now() : Date.now();
  }
}
