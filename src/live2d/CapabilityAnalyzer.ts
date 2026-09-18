/** Fase 14 — Semantic Analyzer: model → capability profile + fallback. */
import type { ModelProfile } from "./ModelInspector";

export interface CapabilityProfile {
  has: Record<string, boolean>;
  missing: string[];
  fallback: Record<string, string>; // capability → fallback action
}

const CAPS: Record<string, { params?: string[]; roles?: string[]; expressions?: string[] }> = {
  headRotation: { roles: ['angleX', 'angleY', 'angleZ'] },
  eyeBlink: { roles: ['eyeLOpen', 'eyeROpen'] },
  mouth: { roles: ['mouthOpenY', 'mouthForm'] },
  gaze: { roles: ['eyeBallX', 'eyeBallY'] },
  bodyRotation: { roles: ['bodyAngleX', 'bodyAngleY', 'bodyAngleZ'] },
  physics: { params: ['__physics'] }, // special
  blush: { roles: ['blush'] },
  earMovement: { roles: ['ear'] },
};

export function analyzeCapabilities(profile: ModelProfile, roleMap: Record<string, string>): CapabilityProfile {
  const has: Record<string, boolean> = {};
  const missing: string[] = [];
  const fallback: Record<string, string> = {};

  for (const cap in CAPS) {
    const need = CAPS[cap];
    let ok = false;
    if (need.roles) ok = need.roles.some(r => !!roleMap[r]);
    else if (need.params) {
      if (need.params[0] === '__physics') ok = profile.physics;
      else ok = need.params.some(p => profile.parameters.some(pr => pr.id === p));
    }
    if (need.expressions) ok = need.expressions.some(e => profile.expressions.includes(e));
    has[cap] = ok;
    if (!ok) {
      missing.push(cap);
      // fallback: blush → expression, ear → none
      if (cap === 'blush') fallback[cap] = profile.expressions.length ? `expression:${profile.expressions[0]}` : 'ignore';
      else if (cap === 'earMovement') fallback[cap] = 'ignore';
      else fallback[cap] = 'ignore';
    }
  }
  return { has, missing, fallback };
}
