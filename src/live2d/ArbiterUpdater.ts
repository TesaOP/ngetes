/**
 * ArbiterUpdater — tulisan arbiter masuk pipeline lewat ICubismUpdater
 * order 900 (setelah semua efek: blink 200 … pose 800), sesuai pola resmi
 * 5-r.5 ("inject manual overrides through an updater, bukan tulis bebas
 * setelah model.update()"). Nilai source bersifat sticky di arbiter dan
 * ditulis ulang tiap frame setelah loadParameters() membuang efek frame
 * sebelumnya — sumber drive wajib terus menulis selama masih menguasai.
 *
 * Untuk param INPUT physics dipasang instance kedua di order < 600 (opsi
 * `onlyIds`): physics (600) MEMBACA nilai param input, jadi override manual
 * harus sudah ditulis sebelum evaluate() supaya pendulum bereaksi. Nilai
 * arbiter absolut (SET) → menulisnya lagi di pass 900 idempoten, aman.
 */
import { ICubismUpdater } from "./cubism/motion/icubismupdater";
import type { CubismModel } from "./cubism/model/cubismmodel";
import { ParameterController } from "./ParameterController";
import type { ParameterArbiter } from "./ParameterArbiter";

export class ArbiterUpdater extends ICubismUpdater {
  /**
   * @param arbiter sumber nilai sticky.
   * @param order execution order (default 900, setelah semua efek framework).
   * @param onlyIds bila diisi, hanya id ini yang ditulis — dipakai pass
   *   pra-physics untuk param INPUT physics. null = semua id.
   */
  constructor(
    private arbiter: ParameterArbiter,
    order = 900,
    private onlyIds: Set<string> | null = null,
  ) {
    super(order);
  }

  onLateUpdate(model: CubismModel, _deltaTimeSeconds: number): void {
    const resolved = this.arbiter.resolve();
    if (!resolved.size) return;
    const pc = new ParameterController(model);
    for (const [id, val] of resolved) {
      if (this.onlyIds && !this.onlyIds.has(id)) continue;
      pc.setParameter(id, val);
    }
  }
}
