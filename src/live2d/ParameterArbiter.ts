/** Fase 13 — Parameter Arbiter: cegah dua sumber override param sama. */
export type SourceId = 'blink' | 'gaze' | 'emotion' | 'lipsync' | 'motion' | 'raw' | 'physics' | 'manual';

const PRIORITY: Record<SourceId, number> = {
  blink: 90,
  gaze: 70,
  emotion: 80,
  lipsync: 85,
  motion: 60,
  raw: 50,
  physics: 40,
  manual: 100, // manual setParameter prioritas tertinggi untuk test
};

interface Pending {
  source: SourceId;
  prio: number;
  value: number;
  seq: number;
}

export class ParameterArbiter {
  private byParam = new Map<string, Map<SourceId, Pending>>();
  private seq = 0;

  set(id: string, value: number, source: SourceId): void {
    const prio = PRIORITY[source] ?? 0;
    let m = this.byParam.get(id);
    if (!m) { m = new Map(); this.byParam.set(id, m); }
    m.set(source, { source, prio, value, seq: ++this.seq });
  }

  clearSource(source: SourceId): void {
    for (const [id, m] of this.byParam) {
      m.delete(source);
      if (!m.size) this.byParam.delete(id);
    }
  }

  /** Nilai final per param = sumber prioritas tertinggi (seq terbaru menang bila seri). */
  resolve(): Map<string, number> {
    const out = new Map<string, number>();
    for (const [id, m] of this.byParam) {
      let best: Pending | null = null;
      for (const p of m.values()) {
        if (!best || p.prio > best.prio || (p.prio === best.prio && p.seq > best.seq)) best = p;
      }
      if (best) out.set(id, best.value);
    }
    return out;
  }

  /** True bila >1 sumber mencoba tulis param sama (konflik yang di-arbiter). */
  hasConflict(id: string): boolean {
    const m = this.byParam.get(id);
    return !!m && m.size > 1;
  }
}
