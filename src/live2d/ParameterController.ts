/** Fase 8 — Parameter API model-agnostic (tanpa AI). Bungkus CubismModel. */
import { CubismFramework } from "./cubism/live2dcubismframework";
import type { CubismModel } from "./cubism/model/cubismmodel";

export interface ParamInfo {
  id: string;
  min: number;
  max: number;
  default: number;
}

export class ParameterController {
  constructor(private model: CubismModel) {}

  private idHandle(id: string) {
    return CubismFramework.getIdManager().getId(id);
  }

  getParameters(): string[] {
    const n = this.model.getParameterCount();
    const out: string[] = [];
    for (let i = 0; i < n; i++) out.push(this.model.getParameterId(i).getString());
    return out;
  }

  private isExistIndex(idx: number): boolean {
    return idx >= 0 && idx < this.model.getParameterCount();
  }

  getParameterInfo(id: string): ParamInfo | null {
    const h = this.idHandle(id);
    const idx = this.model.getParameterIndex(h);
    if (!this.isExistIndex(idx)) return null;
    return {
      id,
      min: this.model.getParameterMinimumValue(idx),
      max: this.model.getParameterMaximumValue(idx),
      default: this.model.getParameterDefaultValue(idx),
    };
  }

  getParameter(id: string): number | null {
    const h = this.idHandle(id);
    const idx = this.model.getParameterIndex(h);
    if (!this.isExistIndex(idx)) return null;
    return this.model.getParameterValueByIndex(idx);
  }

  setParameter(id: string, value: number): boolean {
    const h = this.idHandle(id);
    const idx = this.model.getParameterIndex(h);
    if (!this.isExistIndex(idx)) return false;
    this.model.setParameterValueById(h, value, 1.0);
    return true;
  }
}
