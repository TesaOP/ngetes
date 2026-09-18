/** Fase 11 — bridge MotionRuntime → Live2DRenderer (Cubism). */
import type { RuntimeBridge } from "../client/animation/motion-runtime";
import type { Live2DRenderer } from "./Live2DRenderer";

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
      (this.renderer as any).setRole(role, v, 'motion');
    }
  }

  clearPoseDelta(): void {
    // reset all roles to default
    const map = this.renderer.getRoleMap();
    if (!map) return;
    for (const role in map) this.renderer.setRole(role, 0);
  }

  applyParamDrive(params: Record<string, number>): void {
    for (const id in params) {
      (this.renderer as any).setParameter(id, params[id], 'motion');
      this.ownedParams.add(id);
    }
  }

  releaseParamDrive(paramIds: string[]): void {
    for (const id of paramIds) {
      this.ownedParams.delete(id);
      const info = this.renderer.getParameterInfo(id);
      if (info) this.renderer.setParameter(id, info.default);
    }
  }

  readParam(id: string): number {
    return this.renderer.getParameter(id) ?? 0;
  }

  getSupports(): Set<string> {
    const map = this.renderer.getRoleMap();
    const caps = new Set<string>();
    if (!map) return caps;
    // field ax/ay -> head, ex/ey -> eyes, mouthForm -> mouth, body* -> body
    if (map.angleX || map.angleY) caps.add("head");
    if (map.eyeBallX || map.eyeBallY) caps.add("eyes");
    if (map.mouthForm || map.mouthOpenY) caps.add("mouth");
    if (map.bodyAngleX || map.bodyAngleY || map.bodyAngleZ) caps.add("body");
    // always allow at least head/eyes/mouth for test
    if (caps.size === 0) caps.add("head");
    return caps;
  }

  getOwnedParams(): Set<string> {
    return new Set(this.ownedParams);
  }

  playNative(_group: string): void {
    // Fase 11: native motion playback via Cubism motion manager — belum, stub
    console.log(`[MotionBridge] playNative ${_group} (stub Fase 11)`);
  }

  now(): number {
    return typeof performance !== "undefined" ? performance.now() : Date.now();
  }
}
