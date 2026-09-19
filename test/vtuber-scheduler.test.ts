/**
 * vtuber-scheduler.test.ts — Fase 5 rework arsitektur: otak behavior VTuber
 * server-side (§7 ARSITEKTUR-TARGET). Deps di-stub penuh (LLM, emit, jam,
 * tunggu) — tanpa jaringan, tanpa timer nyata. `wait` diganti gerbang yang
 * dilepas test supaya "durasi bicara" deterministik: slot tetap sibuk
 * sampai gerbang dibuka, persis kontrak active slot §7.
 */
import { describe, test, expect } from "bun:test";
import {
  createVtuberScheduler,
  VTUBER_QUEUE_CAP,
} from "../src/server/vtuber-scheduler";
import { estimateSpeechMs } from "../src/shared/speech-timing";

const CHAT_REPLY = "Terima kasih banyak! Senang banget kamu hadir.";

function makeHarness(cfg: Record<string, unknown> = {}) {
  const agentEvents: string[] = [];
  const systemEvents: string[] = [];
  const llmCalls: Array<{ prompt: string; system: string }> = [];
  const waitGates: Array<{ ms: number; release: () => void }> = [];
  let llmReply = CHAT_REPLY;
  let llmError: Error | null = null;
  let clock = 1_000_000;

  const s = createVtuberScheduler({
    llm: async (messages, system) => {
      llmCalls.push({ prompt: messages[0]?.content || "", system });
      if (llmError) throw llmError;
      return llmReply;
    },
    emitAgent: (t) => agentEvents.push(t),
    emitSystem: (t) => systemEvents.push(t),
    now: () => clock,
    wait: (ms) =>
      new Promise<void>((resolve) => {
        waitGates.push({ ms, release: resolve });
      }),
    log: () => {},
  });
  s.setConfig({ respondChat: true, respondDonation: true, ...cfg });

  return {
    s,
    agentEvents,
    systemEvents,
    llmCalls,
    waitGates,
    advance: (ms: number) => (clock += ms),
    setReply: (v: string) => (llmReply = v),
    failLlm: (e: Error) => (llmError = e),
    /** Buka gerbang bicara tertua → "audio selesai" → slot lepas + drain. */
    release: () => {
      const g = waitGates.shift();
      if (g) g.release();
    },
    settle: () => new Promise((r) => setTimeout(r, 2)),
  };
}

const chat = (user: string, text: string) => ({ type: "audience" as const, user, text });
const dono = (user: string, n = 0) => ({
  type: "donation" as const,
  user,
  text: "dukungan buat stream!",
  amount: "Rp " + (10 + n) + ".000",
});
const op = (text: string) => ({ type: "operator" as const, user: "operator", text });

describe("§7 audience — suppression, bukan antrean", () => {
  test("chat lolos → LLM → agent event → slot sibuk selama estimasi bicara", async () => {
    const h = makeHarness();
    h.s.ingest(chat("Rian", "Halo!"));
    await h.settle();
    expect(h.llmCalls.length).toBe(1);
    expect(h.llmCalls[0].prompt).toContain("Rian");
    expect(h.agentEvents).toEqual([CHAT_REPLY]);
    expect(h.s.stats().active).toBe("audience"); // slot masih dipegang
    expect(h.waitGates.length).toBe(1);
    expect(h.waitGates[0].ms).toBe(estimateSpeechMs(CHAT_REPLY) + 500); // timing shared
    h.release();
    await h.settle();
    expect(h.s.stats().active).toBeNull();
  });

  test("cooldown antar balasan audience → chat ke-2 di-drop", async () => {
    const h = makeHarness();
    h.s.ingest(chat("Rian", "Halo!"));
    await h.settle();
    h.release();
    await h.settle();
    h.advance(3000); // < cooldown default 12 dtk
    h.s.ingest(chat("Sinta", "Pertanyaan pertama!"));
    await h.settle();
    expect(h.llmCalls.length).toBe(1); // tidak ada LLM kedua
    h.advance(10_000); // total > 12 dtk
    h.s.ingest(chat("Sinta", "Pertanyaan kedua!"));
    await h.settle();
    expect(h.llmCalls.length).toBe(2);
    expect(h.s.stats().active).toBe("audience");
  });

  test("dedup: user+teks sama dalam 30 dtk → drop walau cooldown sudah lewat", async () => {
    const h = makeHarness();
    h.s.ingest(chat("Rian", "w"));
    await h.settle();
    h.release();
    await h.settle();
    h.advance(20_000); // lewat cooldown (12 dtk), masih < jendela dedup (30 dtk)
    expect(h.s.ingest(chat("Rian", "w")).accepted).toBe(false);
    expect(h.s.ingest(chat("Rian", "w")).reason).toBe("duplikat");
    expect(h.s.ingest(chat("Sinta", "w")).accepted).toBe(true); // user beda = bukan duplikat
    await h.settle();
    expect(h.llmCalls.length).toBe(2);
  });

  test("respondChat=false → chat dibuang tanpa LLM; donasi tetap jalan", async () => {
    const h = makeHarness({ respondChat: false });
    h.s.ingest(chat("Rian", "Halo"));
    await h.settle();
    expect(h.llmCalls.length).toBe(0);
    h.s.ingest(dono("Kevin"));
    await h.settle();
    expect(h.llmCalls.length).toBe(1);
    expect(h.llmCalls[0].prompt).toContain("donasi");
  });

  test("respondDonation=false → donasi tidak masuk antrean ucapan", async () => {
    const h = makeHarness({ respondDonation: false });
    expect(h.s.ingest(dono("Kevin")).accepted).toBe(false);
    expect(h.llmCalls.length).toBe(0);
    expect(h.s.stats().donationQueue).toBe(0);
  });
});

describe("§7 antrean donation FIFO-20 — tolak eksplisit, bukan silent-evict", () => {
  test("penuh (20) → item ke-21 ditolak + feedback system; item lama utuh", async () => {
    const h = makeHarness();
    h.s.ingest(dono("A")); // A pegang slot
    await h.settle();
    for (let i = 0; i < VTUBER_QUEUE_CAP; i++) h.s.ingest(dono("Q" + i, i)); // isi antrean
    expect(h.s.stats().donationQueue).toBe(VTUBER_QUEUE_CAP);
    const r = h.s.ingest(dono("Terlambat"));
    expect(r.accepted).toBe(false);
    expect(r.reason).toBe("queue-full");
    expect(h.systemEvents.some((s) => s.includes("penuh") && s.includes("Terlambat"))).toBe(true);
    // FIFO: drain semua — urutan A dulu, lalu Q0..Q19.
    for (let i = 0; i < VTUBER_QUEUE_CAP + 1; i++) {
      h.release();
      await h.settle();
    }
    expect(h.llmCalls.length).toBe(VTUBER_QUEUE_CAP + 1); // A + 20 antrean
    expect(h.llmCalls[0].prompt).toContain("Penonton bernama A");
    for (let i = 0; i < VTUBER_QUEUE_CAP; i++)
      expect(h.llmCalls[i + 1].prompt).toContain("Q" + i);
    expect(h.s.stats().donationQueue).toBe(0);
  });

  test("item aktif tidak dipreempt: donation antre sampai chat selesai", async () => {
    const h = makeHarness();
    h.s.ingest(chat("Rian", "halo")); // audience pegang slot
    await h.settle();
    h.s.ingest(dono("Kevin")); // datang saat slot sibuk → antre, bukan preempt
    expect(h.llmCalls.length).toBe(1);
    expect(h.s.stats().donationQueue).toBe(1);
    h.release();
    await h.settle();
    expect(h.llmCalls.length).toBe(2); // giliran donasi setelah chat selesai
    expect(h.llmCalls[1].prompt).toContain("donasi");
    h.release();
    await h.settle();
    expect(h.s.stats().active).toBeNull();
  });
});

describe("§7 operator — kelas sendiri + precedence donation dulu", () => {
  test("operator masuk walau respond chat/donasi mati; antrean sendiri cap 20", async () => {
    const h = makeHarness({ respondChat: false, respondDonation: false });
    h.s.ingest(op("sapa penonton sekali")); // pegang slot
    await h.settle();
    for (let i = 0; i < VTUBER_QUEUE_CAP; i++) h.s.ingest(op("instruksi " + i));
    const r = h.s.ingest(op("kelebihan"));
    expect(r.accepted).toBe(false);
    expect(r.reason).toBe("queue-full");
    expect(h.systemEvents.some((s) => s.includes("operator") && s.includes("penuh"))).toBe(true);
    expect(h.s.stats().operatorQueue).toBe(VTUBER_QUEUE_CAP);
    h.release();
    await h.settle();
    expect(h.llmCalls[1].prompt).toContain("instruksi"); // antrean operator jalan
  });

  test("precedence: donation diproses sebelum operator", async () => {
    const h = makeHarness();
    h.s.ingest(op("operator duluan antre")); // operator pegang slot
    await h.settle();
    h.s.ingest(dono("Kevin")); // donasi antre SETELAH operator masuk antrean
    h.s.ingest(op("operator kedua"));
    expect(h.s.stats()).toEqual({ active: "operator", donationQueue: 1, operatorQueue: 1 });
    h.release(); // slot bebas → donation duluan (precedence §7)
    await h.settle();
    expect(h.llmCalls[1].prompt).toContain("donasi");
    h.release(); // baru operator kedua
    await h.settle();
    expect(h.llmCalls[2].prompt).toContain("operator kedua");
    h.release();
    await h.settle();
    expect(h.s.stats().active).toBeNull();
  });
});

describe("ketahanan & lifecycle", () => {
  test("LLM gagal → feedback system + slot lepas + item berikut jalan", async () => {
    const h = makeHarness();
    h.failLlm(new Error("koneksi putus"));
    h.s.ingest(dono("A"));
    await h.settle();
    expect(h.systemEvents.some((s) => s.includes("AI gagal membalas donation"))).toBe(true);
    expect(h.s.stats().active).toBeNull(); // slot tidak nyangkut
    h.s.ingest(dono("B"));
    await h.settle();
    expect(h.s.stats().donationQueue).toBe(0); // B diproses (gagal lagi, tanpa antre mati)
    expect(h.systemEvents.filter((s) => s.includes("AI gagal")).length).toBe(2);
  });

  test("balasan kosong dari LLM → tanpa agent event, slot langsung bebas", async () => {
    const h = makeHarness();
    h.setReply("   ");
    h.s.ingest(dono("A"));
    await h.settle();
    expect(h.agentEvents).toEqual([]);
    expect(h.waitGates.length).toBe(0);
    expect(h.s.stats().active).toBeNull();
  });

  test("stop(): slot & antrean dibersihkan; ingest berikutnya ditolak", async () => {
    const h = makeHarness();
    h.s.ingest(dono("A")); // slot dipegang (gerbang belum dibuka)
    await h.settle();
    h.s.ingest(dono("B"));
    h.s.stop();
    expect(h.s.stats()).toEqual({ active: null, donationQueue: 0, operatorQueue: 0 });
    expect(h.s.ingest(dono("C")).accepted).toBe(false);
    h.release(); // gerbang lama dibuka — tidak membangkitkan apa pun
    await h.settle();
    expect(h.llmCalls.length).toBe(1);
    expect(h.agentEvents).toEqual([CHAT_REPLY]); // hanya A yang sempat bicara
  });

  test("persona diteruskan ke system prompt LLM", async () => {
    const h = makeHarness();
    h.s.setConfig({ persona: "kalem dan penyabar" });
    h.s.ingest(chat("Rian", "halo"));
    await h.settle();
    expect(h.llmCalls[0].system).toContain("kalem dan penyabar");
    expect(h.llmCalls[0].system).toContain("VTuber");
  });
});
