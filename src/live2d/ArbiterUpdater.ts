/**
 * ArbiterUpdater — tulisan arbiter masuk pipeline lewat ICubismUpdater
 * order 900 (setelah semua efek: blink 200 … pose 800), sesuai pola resmi
 * 5-r.5 ("inject manual overrides through an updater, bukan tulis bebas
 * setelah model.update()"). Nilai source bersifat sticky di arbiter dan
 * ditulis ulang tiap frame setelah loadParameters() membuang efek frame
 * sebelumnya — sumber drive wajib terus menulis selama masih menguasai.
 */
import { ICubismUpdater } from "./cubism/motion/icubismupdater";
import type { CubismModel } from "./cubism/model/cubismmodel";
import { ParameterController } from "./ParameterController";
import type { ParameterArbiter } from "./ParameterArbiter";

export class ArbiterUpdater extends ICubismUpdater {
  constructor(private arbiter: ParameterArbiter) {
    super(900);
  }

  onLateUpdate(model: CubismModel, _deltaTimeSeconds: number): void {
    const resolved = this.arbiter.resolve();
    if (!resolved.size) return;
    const pc = new ParameterController(model);
    for (const [id, val] of resolved) pc.setParameter(id, val);
  }
}
