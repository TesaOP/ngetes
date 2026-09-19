/**
 * client/speech/speech-policy.ts — Kepemilikan & policy speech (boundary
 * ARSITEKTUR-TARGET §15–16).
 *
 * Speech adalah RESOURCE TUNGGAL: satu pembicara pada satu waktu. Producer
 * menyebut KELAS-nya saat request; controller memutuskan ALLOW / PREEMPT /
 * QUEUE / SUPPRESS / CANCEL. Eksekusi TTS tetap di app.js (executor yang
 * didaftarkan lewat setExecutor) — modul ini murni keputusan + kepemilikan
 * claim, tanpa DOM/audio, supaya bisa di-unit-test langsung (pola lip-sync).
 *
 * Aturan terkunci (ARSITEKTUR-TARGET §16 + §6):
 *   1. Input user companion > speech worker → companion boleh PREEMPT.
 *   2. Worker TIDAK boleh memotong companion yang sedang bicara → SUPPRESS.
 *   3. Worker vs worker diserialisasi → QUEUE (bukan last-claim-wins).
 *   4. (§6) SPEAKING + input user baru → PREEMPT. Speech yang dipotong
 *      TIDAK dianggap completed: onDone tidak dipanggil — producer menerima
 *      onPreempted untuk cleanup-nya sendiri (mis. unlockAI di brain).
 *
 * Default konservatif untuk kombinasi yang belum dikunci dokumen:
 *   - Tier lebih tinggi menang; tier sama → QUEUE; tier lebih rendah →
 *     SUPPRESS. VTuber/narasi/proactive (tier 1) tidak saling memotong.
 *   - Antrean FIFO tunggal cap SPEECH_QUEUE_CAP (cap 20 milik antrean event
 *     VTuber, Fase 5). Penuh → item BARU dibuang + warning; item lama tidak
 *     pernah di-silent-evict.
 *   - Policy hanya menguasai AUDIO. Bubble/chat log tetap tanggung jawab
 *     producer (teks tetap masuk log saat SUPPRESS).
 */

/** Kelas produser speech — tier menentukan hak preempt (lihat SPEECH_TIERS). */
export type SpeechClass =
  | "companion" // t2 — balasan/chain companion dari input user (brain)
  | "direct" // t2 — aksi user langsung: echo brain-off
  | "vtuber" // t1 — balasan VTuber app utama (mode-runtime)
  | "worker_narration" // t1 — narasi hasil akhir task (SSE speak)
  | "companion_proactive" // t1 — siap untuk gate Fase 4 (reactEvent)
  | "worker_actor"; // t0 — quip/filler dekoratif (actor)

/** Tier: 2 = user-driven (boleh menggulingkan siapa pun), 1 = konten, 0 = dekoratif. */
const SPEECH_TIERS: Record<SpeechClass, number> = {
  companion: 2,
  direct: 2,
  vtuber: 1,
  worker_narration: 1,
  companion_proactive: 1,
  worker_actor: 0,
};

/** Kelas hasil aksi user eksplisit — sesama kelas ini saling PREEMPT (§6). */
const USER_DRIVEN: ReadonlySet<string> = new Set(["companion", "direct"]);
/** Kelas worker — sesama worker wajib diserialisasi, tidak saling potong (§16). */
const WORKER_CLASSES: ReadonlySet<string> = new Set([
  "worker_actor",
  "worker_narration",
]);

export const SPEECH_QUEUE_CAP = 4;

export type SpeechJob = {
  text: string;
  cls: string; // dinormalisasi di request(); longgar supaya caller legacy aman
  onDone?: () => void;
  /** Dipanggil (bukan onDone) saat job ini digulingkan sebelum selesai. */
  onPreempted?: () => void;
};

type StoredJob = SpeechJob & { cls: SpeechClass };

export type SpeechAction = "ALLOW" | "PREEMPT" | "QUEUE" | "SUPPRESS";

/**
 * Matriks keputusan murni request-vs-holder. Holder null = slot bicara kosong.
 * Urutan cek: tier → pasangan user-driven (§6) → pasangan worker (§16) →
 * tier sama → sisanya SUPPRESS.
 */
export function decide(
  requester: SpeechClass,
  holder: SpeechClass | null,
): SpeechAction {
  if (!holder) return "ALLOW";
  // Serialisasi worker dicek SEBELUM tier: §16-3 "worker speech diserialisasi"
  // berlaku dua arah — narasi tidak boleh memotong quip actor yang sedang
  // bunyi, walaupun tier-nya lebih tinggi.
  if (WORKER_CLASSES.has(requester) && WORKER_CLASSES.has(holder))
    return "QUEUE";
  if (SPEECH_TIERS[requester] > SPEECH_TIERS[holder]) return "PREEMPT";
  if (USER_DRIVEN.has(requester) && USER_DRIVEN.has(holder)) return "PREEMPT";
  if (SPEECH_TIERS[requester] === SPEECH_TIERS[holder]) return "QUEUE";
  return "SUPPRESS";
}

/** Kelas tak dikenal dianggap aksi user (default aman app.js), dengan warning. */
export function normalizeSpeechClass(
  cls: string | null | undefined,
): SpeechClass {
  const c = String(cls || "");
  if (Object.prototype.hasOwnProperty.call(SPEECH_TIERS, c))
    return c as SpeechClass;
  console.warn("[speech] kelas tidak dikenal:", c, "→ dianggap 'direct'");
  return "direct";
}

export type SpeechClaim = {
  readonly id: number;
  readonly cls: SpeechClass;
  /** False = claim sudah digulingkan; executor wajib jadi no-op state global. */
  isActive(): boolean;
  /** Executor (app.js) mendaftarkan pembersih audio/timer per-claim. */
  onPreempted(fn: () => void): void;
};

export type SpeechRequestResult =
  | { status: "ALLOW"; claim: SpeechClaim }
  | { status: "QUEUED" }
  | { status: "SUPPRESS" };

/** Dipanggil controller saat slot bebas dan ada antrean — satu-satunya jalur eksekusi. */
export type SpeechExecutor = (job: SpeechJob, claim: SpeechClaim) => void;

class ClaimImpl implements SpeechClaim {
  readonly id: number;
  readonly cls: SpeechClass;
  alive = true;
  cleanups: Array<() => void> = [];

  constructor(id: number, cls: SpeechClass) {
    this.id = id;
    this.cls = cls;
  }

  isActive(): boolean {
    return this.alive;
  }

  onPreempted(fn: () => void): void {
    if (typeof fn === "function") this.cleanups.push(fn);
  }
}

/**
 * Controller speech — satu instance per halaman (window.__speech).
 * Semua metode aman dipanggil sinkron dari executor/producer mana pun.
 */
export class SpeechController {
  private nextId = 1;
  private active: { claim: ClaimImpl; job: StoredJob } | null = null;
  private queue: Array<{ id: number; job: StoredJob }> = [];
  private executor: SpeechExecutor | null = null;
  /** True selama cleanup preempt berjalan — request sinkron dari cleanup
   *  masuk antrean, tidak boleh ikut menimbang PREEMPT (claim baru yang
   *  belum sempat dieksekusi bisa terguling oleh cleanup claim lama). */
  private preempting = false;

  /** Executor (app.js runSpeech) — mendaftar sekali; langsung drain bila ada antrean. */
  setExecutor(fn: SpeechExecutor): void {
    this.executor = fn;
    this.drain();
  }

  request(job: SpeechJob): SpeechRequestResult {
    const cls = normalizeSpeechClass(job.cls);
    const stored: StoredJob = { ...job, cls };
    const holder = this.active ? this.active.claim.cls : null;
    // Selama transisi preempt: jangan ambil keputusan PREEMPT — antre saja.
    const action = this.preempting ? "QUEUE" : decide(cls, holder);

    if (action === "SUPPRESS") return { status: "SUPPRESS" };

    if (action === "QUEUE") {
      if (this.queue.length >= SPEECH_QUEUE_CAP) {
        console.warn(
          "[speech] antrean penuh (" +
            SPEECH_QUEUE_CAP +
            "), buang request kelas",
          cls,
        );
        return { status: "SUPPRESS" };
      }
      this.queue.push({ id: this.nextId++, job: stored });
      return { status: "QUEUED" };
    }

    // ALLOW / PREEMPT — claim BARU dipasang duluan supaya cleanup claim lama
    // yang tidak sengaja memanggil speak() sinkron melihat holder baru
    // (queue/suppress), bukan merebut slot.
    const claim = new ClaimImpl(this.nextId++, cls);
    const old = this.active;
    this.active = { claim, job: stored };
    if (action === "PREEMPT" && old) this.preempt(old);
    return { status: "ALLOW", claim };
  }

  /** Eksekusi selesai (natural) — kosongkan slot lalu drain antrean. */
  release(claimId: number): void {
    if (!this.active || this.active.claim.id !== claimId) return;
    this.active = null;
    this.drain();
  }

  /** CANCEL (§15): gulingkan holder + kosongkan antrean — dipakai teardown. */
  stopAll(reason?: string): void {
    const old = this.active;
    this.active = null;
    this.queue.length = 0;
    if (old) this.preempt(old, reason);
  }

  current(): { id: number; cls: SpeechClass } | null {
    return this.active
      ? { id: this.active.claim.id, cls: this.active.claim.cls }
      : null;
  }

  queued(): Array<{ id: number; cls: SpeechClass }> {
    return this.queue.map((q) => ({ id: q.id, cls: q.job.cls }));
  }

  private preempt(
    entry: { claim: ClaimImpl; job: StoredJob },
    reason?: string,
  ): void {
    const { claim, job } = entry;
    claim.alive = false;
    const cleanups = claim.cleanups;
    claim.cleanups = [];
    this.preempting = true;
    try {
      for (const fn of cleanups) {
        try {
          fn();
        } catch (e) {
          console.warn("[speech] cleanup preempt gagal:", e);
        }
      }
      if (job.onPreempted) {
        try {
          job.onPreempted();
        } catch (e) {
          console.warn("[speech] onPreempted producer gagal:", e);
        }
      }
    } finally {
      this.preempting = false;
    }
    if (reason) console.log("[speech] claim #" + claim.id, "dibatalkan:", reason);
  }

  private drain(): void {
    if (this.active || !this.queue.length || !this.executor) return;
    const next = this.queue.shift()!;
    const claim = new ClaimImpl(this.nextId++, next.job.cls);
    this.active = { claim, job: next.job };
    try {
      this.executor(next.job, claim);
    } catch (e) {
      console.error("[speech] executor gagal:", e);
      this.active = null;
      this.drain();
    }
  }
}

export function createSpeechPolicy(): SpeechController {
  return new SpeechController();
}
