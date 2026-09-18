/** Fase 10 — hubungkan role-mapping (model-agnostic) ke ParameterController. */
import { mapRoles, writeRef, roleDefaultOf, type ParamRange } from "../client/engine/role-mapping";
import { ParameterController } from "./ParameterController";
import type { CubismModel } from "./cubism/model/cubismmodel";

export class RoleController {
  private roleToId: Record<string, string> = {};
  private rangeByRole: Record<string, ParamRange | null> = {};
  private paramCtrl: ParameterController;

  constructor(
    private model: CubismModel,
    paramSet: Set<string>,
    official?: { eyeBlinkIds: string[]; lipSyncIds: string[] },
  ) {
    this.paramCtrl = new ParameterController(model);
    this.roleToId = mapRoles(paramSet, official);
    // bangun range per role
    for (const role in this.roleToId) {
      const id = this.roleToId[role];
      const info = this.paramCtrl.getParameterInfo(id);
      this.rangeByRole[role] = info ? { min: info.min, max: info.max, def: info.default } : null;
    }
  }

  getRoleMap(): Record<string, string> {
    return { ...this.roleToId };
  }

  /** Set role via skala referensi (±30 atau 0..1), dipetakan ke range model. */
  setRole(role: string, vRef: number): boolean {
    const id = this.roleToId[role];
    if (!id) return false;
    const r = this.rangeByRole[role];
    const actual = writeRef(role, vRef, r);
    return this.paramCtrl.setParameter(id, actual);
  }

  getRole(role: string): number | null {
    const id = this.roleToId[role];
    if (!id) return null;
    return this.paramCtrl.getParameter(id);
  }

  resetRole(role: string): boolean {
    const id = this.roleToId[role];
    if (!id) return false;
    const r = this.rangeByRole[role];
    return this.paramCtrl.setParameter(id, roleDefaultOf(r));
  }
}
