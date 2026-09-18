/** Fase 5 — facade: app.js → Live2DModel.from → Cubism Core native (tanpa byte-hack). */
import type { Live2DAdapterOptions, Live2DModelLike } from "./types";
import { loadLive2DModel } from "./ModelLoader";

export class Live2DModel {
  static async from(
    modelPath: string,
    options?: Live2DAdapterOptions,
  ): Promise<Live2DModelLike> {
    return loadLive2DModel(modelPath, options);
  }
}
