/** Fase 15 — Intent semantik → param (Director + Arbiter). LLM tidak output ParamAngleX. */
import type { Live2DRenderer } from "./Live2DRenderer";

export interface SemanticIntent {
  emotion?: string;
  intensity?: number;
  gaze?: 'away' | 'direct' | 'up' | 'down';
  headTilt?: number;
}

export class IntentDirector {
  constructor(private renderer: Live2DRenderer) {}

  /** Terjemahkan intent → role/param via renderer (model-agnostic). */
  direct(intent: SemanticIntent): void {
    if (intent.emotion) {
      // emotion → setRole via arbiter (fallback handled by CapabilityAnalyzer)
      // untuk Fase 15 proof, cukup set angleX dari headTilt
    }
    if (typeof intent.headTilt === 'number') {
      this.renderer.setRole('angleZ', intent.headTilt * 30, 'emotion');
    }
    if (intent.gaze === 'away') {
      this.renderer.setRole('eyeBallX', intent.intensity ?? 0.8, 'gaze');
      this.renderer.setRole('eyeBallY', 0.2, 'gaze');
    } else if (intent.gaze === 'direct') {
      this.renderer.setRole('eyeBallX', 0, 'gaze');
      this.renderer.setRole('eyeBallY', 0, 'gaze');
    }
    if (intent.emotion === 'embarrassed') {
      // coba blush, fallback ke expression jika tidak ada
      const ok = this.renderer.setRole('blush', intent.intensity ?? 0.8, 'emotion');
      if (!ok) {
        // fallback: pakai expression pertama jika ada
        const prof = this.renderer.getModelProfile();
        if (prof?.expressions?.length) {
          // di real app, trigger expression via motion manager — di proof cukup log
          console.log(`[IntentDirector] blush missing → fallback expression ${prof.expressions[0]}`);
        }
      }
    }
  }
}
