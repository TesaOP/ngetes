/** Fase 4 — adapter kosong. Belum logic; hanya kontrak facade agar app.js tidak lagi import PIXI.live2d langsung. */

export type Live2DModelId = string;

export interface Live2DAdapterOptions {
  autoInteract?: boolean;
}

export interface ParameterInfo {
  id: string;
  min: number;
  max: number;
  defaultValue: number;
}

export interface Live2DModelLike {
  /** Pixi Container yang bisa di-add ke stage (placeholder Fase 4). */
  container: import("pixi.js").Container;
  destroy(): void;
}
