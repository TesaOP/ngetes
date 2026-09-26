/**
 * Regresi urutan eksekusi updater: tulisan manual ke param INPUT physics
 * WAJIB di-flush sebelum Physics(600) supaya CubismPhysics.evaluate() —
 * yang MEMBACA nilai param input — bereaksi di frame yang sama. Bug asalnya:
 * semua tulisan manual (AppWriteUpdater/ArbiterUpdater) di order 900, jadi
 * physics selalu membaca nilai lama → memindah param input tak berayun.
 *
 * Test ini mengunci invariansi urutan lewat scheduler asli, tanpa memuat
 * Core: updater pra-physics (Physics-10) harus jalan setelah Breath(500) dan
 * sebelum Physics(600).
 */
import { expect, test, describe } from "bun:test";
import { CubismUpdateScheduler } from "../src/live2d/cubism/motion/cubismupdatescheduler";
import {
  ICubismUpdater,
  CubismUpdateOrder,
} from "../src/live2d/cubism/motion/icubismupdater";

class RecordingUpdater extends ICubismUpdater {
  constructor(order: number, private label: string, private log: string[]) {
    super(order);
  }
  onLateUpdate(): void {
    this.log.push(this.label);
  }
}

describe("urutan flush param input physics", () => {
  test("pass pra-physics jalan setelah breath, sebelum physics", () => {
    const log: string[] = [];
    const sched = new CubismUpdateScheduler();
    // Sengaja didaftarkan acak — scheduler yang mengurutkan.
    sched.addUpdatableList(
      new RecordingUpdater(CubismUpdateOrder.CubismUpdateOrder_Physics, "physics", log),
    );
    sched.addUpdatableList(
      new RecordingUpdater(CubismUpdateOrder.CubismUpdateOrder_Breath, "breath", log),
    );
    sched.addUpdatableList(
      new RecordingUpdater(
        CubismUpdateOrder.CubismUpdateOrder_Physics - 10,
        "input-flush",
        log,
      ),
    );
    sched.addUpdatableList(new RecordingUpdater(900, "manual-900", log));

    sched.onLateUpdate({} as any, 0.016);

    expect(log).toEqual(["breath", "input-flush", "physics", "manual-900"]);
    // Invariansi inti: input-flush mendahului physics.
    expect(log.indexOf("input-flush")).toBeLessThan(log.indexOf("physics"));
  });
});
