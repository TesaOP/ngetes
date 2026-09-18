/** Facade model untuk halaman proof (Live2DModel.from → Cubism Core native). */
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
