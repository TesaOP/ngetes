/**
 * Fase 4 — facade kosong. Tujuan: app.js memanggil API ini, bukan PIXI.live2d langsung.
 * Isi masih stub — Fase 5 akan isi dengan load nyata via Cubism SDK.
 */
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
