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
      // coba blush; kalau model tidak punya param blush, fallback NYATA ke
      // ekspresi pertama dari manifest (bukan sekadar log)
      const ok = this.renderer.setRole('blush', intent.intensity ?? 0.8, 'emotion');
      if (!ok) {
        const prof = this.renderer.getModelProfile();
        const fallbackName = prof?.expressions?.[0];
        if (fallbackName) {
          void this.renderer.playExpression(fallbackName)
            .then((played) => console.log(`[IntentDirector] blush missing → fallback expression ${fallbackName} ${played ? "diputar" : "gagal"}`));
        } else {
          console.warn("[IntentDirector] blush missing, model tanpa ekspresi — intent diabaikan");
        }
      }
    }
  }
}
