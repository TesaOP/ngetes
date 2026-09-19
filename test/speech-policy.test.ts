import { describe, test, expect } from "bun:test";
import {
  decide,
  normalizeSpeechClass,
  createSpeechPolicy,
  SPEECH_QUEUE_CAP,
} from "../src/client/speech/speech-policy";
import type { SpeechJob } from "../src/client/speech/speech-policy";

function job(cls: string, extra: Partial<SpeechJob> = {}): SpeechJob {
  return { text: "tes", cls, ...extra };
}

describe("decide — matriks policy speech (§15–16)", () => {
  test("slot kosong → ALLOW untuk semua kelas", () => {
    for (const cls of [
      "companion",
      "direct",
      "vtuber",
      "worker_narration",
      "companion_proactive",
      "worker_actor",
    ] as const) {
      expect(decide(cls, null)).toBe("ALLOW");
    }
  });

  test("§16-1: input user companion menang atas speech worker → PREEMPT", () => {
    expect(decide("companion", "worker_actor")).toBe("PREEMPT");
    expect(decide("companion", "worker_narration")).toBe("PREEMPT");
    expect(decide("direct", "worker_actor")).toBe("PREEMPT");
  });

  test("§16-2: worker tidak boleh memotong companion bicara → SUPPRESS", () => {
    expect(decide("worker_narration", "companion")).toBe("SUPPRESS");
    expect(decide("worker_actor", "companion")).toBe("SUPPRESS");
    expect(decide("worker_actor", "direct")).toBe("SUPPRESS");
  });

  test("§16-3: worker vs worker diserialisasi → QUEUE (dua arah)", () => {
    expect(decide("worker_actor", "worker_narration")).toBe("QUEUE");
    expect(decide("worker_narration", "worker_actor")).toBe("QUEUE");
  });

  test("§6: input user baru saat companion bicara → PREEMPT", () => {
    expect(decide("companion", "companion")).toBe("PREEMPT");
    expect(decide("companion", "direct")).toBe("PREEMPT");
    expect(decide("direct", "companion")).toBe("PREEMPT");
  });

  test("default: tier 1 tidak saling memotong → QUEUE; tier 0 tunduk → SUPPRESS", () => {
    expect(decide("vtuber", "vtuber")).toBe("QUEUE");
    expect(decide("vtuber", "worker_narration")).toBe("QUEUE");
    expect(decide("companion_proactive", "worker_narration")).toBe("QUEUE");
    expect(decide("worker_actor", "vtuber")).toBe("SUPPRESS");
    expect(decide("vtuber", "companion")).toBe("SUPPRESS");
    expect(decide("companion_proactive", "companion")).toBe("SUPPRESS");
  });
});

describe("normalizeSpeechClass", () => {
  test("kelas dikenal dipakai apa adanya", () => {
    expect(normalizeSpeechClass("companion")).toBe("companion");
    expect(normalizeSpeechClass("worker_actor")).toBe("worker_actor");
  });

  test("kelas tak dikenal / kosong → 'direct' (default aman app.js)", () => {
    expect(normalizeSpeechClass("bogus")).toBe("direct");
    expect(normalizeSpeechClass("")).toBe("direct");
    expect(normalizeSpeechClass(null)).toBe("direct");
  });
});

describe("SpeechController — claim, preempt, queue, drain", () => {
  test("request pertama → ALLOW + claim aktif", () => {
    const c = createSpeechPolicy();
    const r = c.request(job("companion"));
    expect(r.status).toBe("ALLOW");
    if (r.status !== "ALLOW") return;
    expect(r.claim.isActive()).toBe(true);
    expect(c.current()?.cls).toBe("companion");
  });

  test("preempt: claim lama mati, cleanup executor + onPreempted producer jalan, onDone TIDAK", () => {
    const c = createSpeechPolicy();
    const done1: string[] = [];
    const preempted1: string[] = [];
    const cleaned1: string[] = [];
    const r1 = c.request(job("companion", { onDone: () => done1.push("a"), onPreempted: () => preempted1.push("a") }));
    if (r1.status !== "ALLOW") throw new Error("harusnya ALLOW");
    r1.claim.onPreempted(() => cleaned1.push("audio"));

    const r2 = c.request(job("companion", { text: "baru" }));
    expect(r2.status).toBe("ALLOW");
    expect(r1.claim.isActive()).toBe(false);
    expect(cleaned1).toEqual(["audio"]); // executor dibersihkan lebih dulu
    expect(preempted1).toEqual(["a"]); // lalu hook producer
    expect(done1).toEqual([]); // dipotong ≠ completed (§6)
    expect(c.current()?.cls).toBe("companion");
  });

  test("SUPPRESS: worker saat companion bicara tidak dieksekusi & tidak menggulingkan", () => {
    const c = createSpeechPolicy();
    c.request(job("companion"));
    const r = c.request(job("worker_actor"));
    expect(r.status).toBe("SUPPRESS");
    expect(c.current()?.cls).toBe("companion"); // holder tetap
  });

  test("QUEUE: worker serial dua arah; drain FIFO setelah release", () => {
    const c = createSpeechPolicy();
    const executed: string[] = [];
    // Executor tidak melepas slot — rilis dikendalikan test agar antrean
    // benar-benar terbentuk.
    c.setExecutor((j, claim) => {
      executed.push(j.cls + ":" + j.text);
      expect(claim.isActive()).toBe(true);
    });
    const r1 = c.request(job("worker_narration", { text: "narasi" }));
    expect(r1.status).toBe("ALLOW");
    if (r1.status !== "ALLOW") return;
    const r2 = c.request(job("worker_actor", { text: "quip" }));
    expect(r2.status).toBe("QUEUED");
    const r3 = c.request(job("worker_actor", { text: "filler" }));
    expect(r3.status).toBe("QUEUED");
    // ALLOW dieksekusi caller (app.js runSpeech) — executor controller hanya
    // melayani antrean, jadi sampai di sini belum ada yang dieksekusi.
    expect(executed).toEqual([]);
    c.release(r1.claim.id);
    expect(executed).toEqual(["worker_actor:quip"]);
    const cur = c.current();
    expect(cur?.cls).toBe("worker_actor");
    if (cur) c.release(cur.id);
    expect(executed).toEqual(["worker_actor:quip", "worker_actor:filler"]);
  });

  test("ALLOW dieksekusi caller, bukan executor controller — antrean tidak jalan selama slot hidup", () => {
    const c = createSpeechPolicy();
    const executed: string[] = [];
    c.setExecutor((j) => executed.push(j.text));
    const r1 = c.request(job("vtuber", { text: "satu" })); // ALLOW — app.js runSpeech yang jalan
    expect(r1.status).toBe("ALLOW");
    expect(executed).toEqual([]); // controller tidak menyentuh jalur ALLOW
    const r = c.request(job("worker_narration", { text: "dua" })); // tier sama → antre
    expect(r.status).toBe("QUEUED");
    expect(executed).toEqual([]); // slot masih dipegang claim "satu"
    if (r1.status !== "ALLOW") return;
    c.release(r1.claim.id);
    expect(executed).toEqual(["dua"]);
  });

  test("cap antrean: item ke-" + (SPEECH_QUEUE_CAP + 1) + " dibuang, item lama utuh", () => {
    const c = createSpeechPolicy();
    c.setExecutor(() => {}); // slot ditahan selamanya di test ini
    c.request(job("vtuber", { text: "aktif" }));
    for (let i = 0; i < SPEECH_QUEUE_CAP; i++) {
      expect(c.request(job("worker_narration", { text: "q" + i })).status).toBe("QUEUED");
    }
    expect(c.request(job("worker_narration", { text: "lima" })).status).toBe("SUPPRESS");
    expect(c.queued().length).toBe(SPEECH_QUEUE_CAP);
    expect(c.queued().map((q) => q.cls)).toEqual(
      Array.from({ length: SPEECH_QUEUE_CAP }, () => "worker_narration"),
    );
  });

  test("stopAll: holder digulingkan + antrean dikosongkan (CANCEL §15)", () => {
    const c = createSpeechPolicy();
    const preempted: string[] = [];
    c.setExecutor(() => {});
    const r1 = c.request(job("companion", { onPreempted: () => preempted.push("holder") }));
    if (r1.status !== "ALLOW") throw new Error("harusnya ALLOW");
    c.request(job("worker_actor", { text: "q" }));
    c.stopAll("teardown");
    expect(preempted).toEqual(["holder"]);
    expect(r1.claim.isActive()).toBe(false);
    expect(c.current()).toBe(null);
    expect(c.queued()).toEqual([]);
    // Slot bebas lagi setelah stopAll.
    expect(c.request(job("companion")).status).toBe("ALLOW");
  });

  test("release id claim yang sudah mati / asing → no-op", () => {
    const c = createSpeechPolicy();
    const r1 = c.request(job("companion"));
    if (r1.status !== "ALLOW") throw new Error("harusnya ALLOW");
    c.request(job("companion", { text: "baru" })); // preempt #1
    c.release(r1.claim.id); // id claim mati — tidak boleh menjatuhkan holder baru
    expect(c.current()?.cls).toBe("companion");
    c.release(99999); // id asing
    expect(c.current()?.cls).toBe("companion");
  });

  test("executor terlambat: antrean menunggu, lalu drain saat setExecutor", () => {
    const c = createSpeechPolicy();
    const executed: string[] = [];
    const r1 = c.request(job("vtuber", { text: "aktif" }));
    expect(r1.status).toBe("ALLOW");
    const r2 = c.request(job("worker_narration", { text: "menunggu" }));
    expect(r2.status).toBe("QUEUED");
    c.setExecutor((j) => executed.push(j.text));
    expect(executed).toEqual([]); // slot masih dipegang claim 1
    if (r1.status !== "ALLOW") return;
    c.release(r1.claim.id);
    expect(executed).toEqual(["menunggu"]);
  });

  test("executor melempar → slot dibebaskan + antrean lanjut", () => {
    const c = createSpeechPolicy();
    let first = true;
    c.setExecutor((j) => {
      if (first) {
        first = false;
        throw new Error("boom");
      }
    });
    c.request(job("companion", { text: "meledak" }));
    expect(c.request(job("companion", { text: "lanjut" })).status).toBe("ALLOW");
  });

  test("preempt sinkron dari dalam cleanup tidak merebut slot claim baru", () => {
    const c = createSpeechPolicy();
    const order: string[] = [];
    const r1 = c.request(job("companion", { text: "lama" }));
    if (r1.status !== "ALLOW") throw new Error("harusnya ALLOW");
    r1.claim.onPreempted(() => {
      order.push("cleanup-lama");
      // Cleanup memanggil speak() sinkron — harus melihat claim BARU sebagai
      // holder (ditolak/preempt), bukan merebut slot dari claim baru.
      const sneaky = c.request(job("direct", { text: "nyelonong" }));
      order.push("nyelonong:" + sneaky.status);
    });
    c.request(job("companion", { text: "baru" }));
    expect(order[0]).toBe("cleanup-lama");
    expect(c.current()?.cls).toBe("companion");
    expect(c.queued().map((q) => q.cls)).toContain("direct");
  });
});
