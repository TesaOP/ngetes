/**
 * server/vtuber-scheduler.ts — Otak behavior VTuber (§7 ARSITEKTUR-TARGET).
 *
 * SATU scheduler di server; dua klien (app utama + overlay OBS) hanya
 * render feed + memutar balasan. Ini mematikan race app-vs-overlay dari
 * akarnya dan membuat antrean tahan reconnect klien.
 *
 * Model event (§7):
 *   - Audience (chat)  → SUPPRESSION: dedup → cooldown → drop. Tidak antre.
 *   - Donation         → FIFO antrean cap 20; penuh → item BARU ditolak
 *                        dengan feedback eksplisit (feed system), item lama
 *                        tidak pernah di-silent-evict.
 *   - Operator         → FIFO antrean sendiri cap 20; instruksi eksplisit
 *                        operator — boleh masuk walau respondChat/Donation
 *                        dimatikan.
 *   - Active slot TUNGGAL dibagi donation+operator; item aktif tidak bisa
 *     dipreempt kelas lain. Slot bebas → donation dulu, baru operator
 *     (precedence terkunci §7).
 *
 * Semua dependensi eksternal (LLM, emit event, jam, tunggu) di-inject supaya
 * bisa di-unit-test tanpa jaringan/timer nyata.
 */
import { estimateSpeechMs } from "../shared/speech-timing";

export type SchedulerClass = "audience" | "donation" | "operator";

export type SchedulerEvent = {
  type: SchedulerClass;
  user: string;
  text: string;
  amount?: string;
};

type QueueItem = SchedulerEvent & { id: number; enqueuedAt: number };

export type SchedulerDeps = {
  /** Panggil LLM role "chat" — kontrak llmForRole (messages, system). */
  llm: (messages: Array<{ role: string; content: string }>, system: string) => Promise<string>;
  /** Balasan karakter → event feed type "agent". */
  emitAgent: (text: string) => void;
  /** Pesan sistem/feedback → event feed type "system". */
  emitSystem: (text: string) => void;
  now?: () => number;
  /** Tunggu pasca-balasan (estimasi bicara) — di-inject supaya test instan. */
  wait?: (ms: number) => Promise<void>;
  log?: (msg: string) => void;
};

export type SchedulerConfig = {
  persona: string;
  cooldownMs: number;
  respondChat: boolean;
  respondDonation: boolean;
};

const DEFAULT_CONFIG: SchedulerConfig = {
  persona: "ceria dan ramah",
  cooldownMs: 12000,
  respondChat: false, // aman-by-default: tanpa flag eksplisit, tanpa LLM
  respondDonation: false,
};

/** §7: antrean FIFO maksimum 20 (donation & operator). */
export const VTUBER_QUEUE_CAP = 20;
/** Dedup audience: user+teks sama dalam jendela ini → drop. */
export const VTUBER_DEDUP_WINDOW_MS = 30_000;
/** Margin setelah estimasi bicara sebelum slot dianggap bebas. */
const SPEAK_BUFFER_MS = 500;
const MIN_COOLDOWN_MS = 5000;

// Prompt protokol antar-komponen — bahasa Indonesia (aturan repo: kosakata
// protokol tetap Indonesia, konsisten EVENT_PROMPTS di brain.ts).
function systemPrompt(persona: string): string {
  return (
    "Kamu adalah VTuber Live2D yang sedang streaming. Gaya bicara: " +
    (persona || "ceria dan ramah") +
    ". Jawab HANYA kalimat yang akan diucapkan, tanpa awalan nama. Bahasa Indonesia."
  );
}
function chatPrompt(user: string, text: string): string {
  return (
    'Penonton bernama ' + user + ' bilang di live chat: "' + text +
    '". Balas singkat (1 kalimat) yang fun dan personal.'
  );
}
function donationPrompt(user: string, text: string, amount: string): string {
  return (
    'Penonton bernama ' + user + ' baru saja donasi ' + amount +
    ' dengan pesan: "' + text + '". Ucapkan terima kasih hangat yang khas (1-2 kalimat).'
  );
}
function operatorPrompt(text: string): string {
  return (
    'Operator stream memberimu instruksi: "' + text +
    '". Ucapkan sesuai instruksi dengan gaya bicaramu (maks 2 kalimat).'
  );
}

export type VtuberScheduler = {
  ingest(ev: SchedulerEvent): { accepted: boolean; reason?: string };
  setConfig(partial: Partial<SchedulerConfig>): void;
  stats(): { active: SchedulerClass | null; donationQueue: number; operatorQueue: number };
  stop(): void;
};

export function createVtuberScheduler(deps: SchedulerDeps): VtuberScheduler {
  const now = deps.now || (() => Date.now());
  const wait = deps.wait || ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)));
  const log = deps.log || (() => {});

  let cfg = { ...DEFAULT_CONFIG };
  let nextItemId = 1;
  let donationQueue: QueueItem[] = [];
  let operatorQueue: QueueItem[] = [];
  let active: { item: QueueItem } | null = null;
  let lastAudienceRespondAt = 0;
  /** user+\x00+text → ts terakhir terlihat (dedup audience). */
  const recentChat = new Map<string, number>();
  let stopped = false;

  const speakMs = (text: string) => estimateSpeechMs(text) + SPEAK_BUFFER_MS;

  async function runItem(item: QueueItem): Promise<void> {
    const user = String(item.user || "Penonton").slice(0, 60);
    const text = String(item.text || "").slice(0, 400);
    const amount = String(item.amount || "").slice(0, 40);
    let prompt: string;
    if (item.type === "audience") prompt = chatPrompt(user, text);
    else if (item.type === "donation")
      prompt = donationPrompt(user, text, amount || "sebuah donasi");
    else prompt = operatorPrompt(text);
    try {
      const reply = String(
        (await deps.llm([{ role: "user", content: prompt }], systemPrompt(cfg.persona))) || "",
      ).trim();
      if (stopped) return;
      if (reply) deps.emitAgent(reply.slice(0, 500));
      else log("balasan kosong untuk " + item.type + " — lewati");
      if (reply) await wait(speakMs(reply));
    } catch (e: any) {
      deps.emitSystem("AI gagal membalas " + item.type + ": " + String(e?.message || e).slice(0, 120));
    }
  }

  function drain(): void {
    if (stopped || active) return;
    // Precedence terkunci §7: donation selalu lebih dulu daripada operator.
    const next = donationQueue.shift() || operatorQueue.shift();
    if (!next) return;
    active = { item: next };
    log("slot aktif: " + next.type + " #" + next.id);
    runItem(next).finally(() => {
      // Slot hanya lepas setelah balasan selesai "dibicarakan" (estimasi) —
      // tidak ada kelas lain yang bisa mempreempt item aktif.
      active = null;
      if (!stopped) drain();
    });
  }

  function pruneDedup(): void {
    const cutoff = now() - VTUBER_DEDUP_WINDOW_MS;
    for (const [k, ts] of recentChat) if (ts < cutoff) recentChat.delete(k);
  }

  function ingestChat(ev: SchedulerEvent): { accepted: boolean; reason?: string } {
    if (!cfg.respondChat) return { accepted: false, reason: "respond chat mati" };
    const key = ev.user + "\x00" + ev.text;
    pruneDedup();
    const seen = recentChat.get(key);
    if (seen != null && now() - seen < VTUBER_DEDUP_WINDOW_MS)
      return { accepted: false, reason: "duplikat" }; // §7 audience: duplicate? → DROP
    recentChat.set(key, now());
    if (now() - lastAudienceRespondAt < Math.max(MIN_COOLDOWN_MS, cfg.cooldownMs))
      return { accepted: false, reason: "cooldown" }; // §7 audience: cooldown? → DROP
    if (active || donationQueue.length || operatorQueue.length)
      return { accepted: false, reason: "slot sibuk" }; // suppression, bukan antre
    lastAudienceRespondAt = now();
    const item: QueueItem = { ...ev, id: nextItemId++, enqueuedAt: now() };
    active = { item };
    log("slot aktif: audience #" + item.id);
    runItem(item).finally(() => {
      active = null;
      if (!stopped) drain();
    });
    return { accepted: true };
  }

  function enqueue(
    queue: QueueItem[],
    ev: SchedulerEvent,
    cls: SchedulerClass,
  ): { accepted: boolean; reason?: string } {
    if (queue.length >= VTUBER_QUEUE_CAP) {
      // §7: item BARU ditolak eksplisit — item lama tidak pernah dibuang senyap.
      deps.emitSystem(
        "Antrean " + (cls === "donation" ? "terima kasih donasi" : "instruksi operator") +
          " penuh (" + VTUBER_QUEUE_CAP + ") — pesan dari " +
          String(ev.user || cls).slice(0, 60) + " tidak masuk antrean.",
      );
      return { accepted: false, reason: "queue-full" };
    }
    queue.push({ ...ev, id: nextItemId++, enqueuedAt: now() });
    drain();
    return { accepted: true };
  }

  return {
    ingest(ev: SchedulerEvent): { accepted: boolean; reason?: string } {
      if (stopped) return { accepted: false, reason: "scheduler berhenti" };
      if (ev.type === "audience") return ingestChat(ev);
      if (ev.type === "donation") {
        // Donasi selalu tampil di feed; antrean ucapan hanya saat flag nyala.
        if (!cfg.respondDonation) return { accepted: false, reason: "respond donasi mati" };
        return enqueue(donationQueue, ev, "donation");
      }
      return enqueue(operatorQueue, ev, "operator");
    },

    setConfig(partial: Partial<SchedulerConfig>): void {
      cfg = {
        persona: typeof partial.persona === "string" && partial.persona.trim()
          ? partial.persona.slice(0, 800)
          : cfg.persona,
        cooldownMs:
          typeof partial.cooldownMs === "number" && Number.isFinite(partial.cooldownMs)
            ? Math.max(MIN_COOLDOWN_MS, partial.cooldownMs)
            : cfg.cooldownMs,
        respondChat: typeof partial.respondChat === "boolean" ? partial.respondChat : cfg.respondChat,
        respondDonation:
          typeof partial.respondDonation === "boolean" ? partial.respondDonation : cfg.respondDonation,
      };
      // Config baru bisa menyalakan drain (mis. respond dinyalakan saat
      // antrean sudah terisi).
      drain();
    },

    stats() {
      return {
        active: active ? active.item.type : null,
        donationQueue: donationQueue.length,
        operatorQueue: operatorQueue.length,
      };
    },

    stop(): void {
      stopped = true;
      donationQueue = [];
      operatorQueue = [];
      active = null;
    },
  };
}
