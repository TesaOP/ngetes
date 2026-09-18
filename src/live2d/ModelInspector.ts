/** Fase 9 — Model Inspector: jawab "model ini punya apa saja?" tanpa hardcode. */
import type { CubismModel } from "./cubism/model/cubismmodel";
import type { CubismModelSettingJson } from "./cubism/cubismmodelsettingjson";

export interface ModelProfile {
  parameters: { id: string; min: number; max: number; default: number }[];
  parts: { id: string }[];
  motions: { group: string; count: number }[];
  expressions: string[];
  physics: boolean;
  pose: boolean;
  canvas: { width: number; height: number };
  drawable: number;
  offscreen: number;
}

export function inspectModel(
  model: CubismModel,
  setting: CubismModelSettingJson | null,
): ModelProfile {
  const parameters: ModelProfile["parameters"] = [];
  const n = model.getParameterCount();
  for (let i = 0; i < n; i++) {
    const id = model.getParameterId(i).getString();
    parameters.push({
      id,
      min: model.getParameterMinimumValue(i),
      max: model.getParameterMaximumValue(i),
      default: model.getParameterDefaultValue(i),
    });
  }
  const parts: ModelProfile["parts"] = [];
  const pc = (model as any).getPartCount?.() ?? 0;
  for (let i = 0; i < pc; i++) {
    try {
      const pid = (model as any).getPartId?.(i)?.getString?.() ?? `Part${i}`;
      parts.push({ id: pid });
    } catch { parts.push({ id: `Part${i}` }); }
  }
  const motions: ModelProfile["motions"] = [];
  let expressions: string[] = [];
  let physics = false;
  let pose = false;
  if (setting) {
    const anySetting: any = setting;
    if (anySetting.getMotionGroupCount) {
      const gc = anySetting.getMotionGroupCount();
      for (let i = 0; i < gc; i++) {
        const g = anySetting.getMotionGroupName?.(i) ?? `Group${i}`;
        const count = anySetting.getMotionCount?.(g) ?? 0;
        motions.push({ group: g, count });
      }
    }
    expressions = [];
    const ec = anySetting.getExpressionCount?.() ?? 0;
    for (let i = 0; i < ec; i++) expressions.push(anySetting.getExpressionName?.(i) ?? `exp_${i}`);
    physics = !!anySetting.getPhysicsFileName?.() && anySetting.getPhysicsFileName?.() !== "";
    pose = !!anySetting.getPoseFileName?.() && anySetting.getPoseFileName?.() !== "";
  }
  return {
    parameters,
    parts,
    motions,
    expressions,
    physics,
    pose,
    canvas: { width: model.getCanvasWidth(), height: model.getCanvasHeight() },
    drawable: (model as any).getDrawableCount?.() ?? (model as any).drawables?.count ?? 0,
    offscreen: (model as any).getOffscreenCount?.() ?? 0,
  };
}
