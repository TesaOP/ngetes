/**
 * test/worker-tasks.test.ts — Fase 6 rework arsitektur: task identity Worker
 * (§9–12 ARSITEKTUR-TARGET). Dua strategi:
 *   A. State-poking deterministik: gerbang facade (park/cancel/modify)
 *      hanya membaca `rt.activeTask` — dipasang manual tanpa menjalankan
 *      loop, jadi antrean/pause bisa diuji tanpa timing.
 *   B. E2E drain: provider mock (300ms, echo prompt) membuktikan task baru
 *      TIDAK mematikan task aktif dan antrean jalan otomatis berurutan.
 * Tanpa jaringan; runtime lokal via makeRuntime/setRuntime (bukan
 * assistantStart) supaya data/ user tidak tersentuh.
 */
import { describe, it, expect, afterAll } from "bun:test";
import { mkdtempSync, rmSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { makeRuntime, setRuntime, MAX_PARKED_TASKS } from "../src/server/agent/state";
import type { Runtime } from "../src/server/agent/state";
import {
  assistantAsk,
  assistantCancel,
  assistantModify,
  assistantResolveApproval,
  assistantStatus,
} from "../src/server/assistant";

const workDir = mkdtempSync(join(tmpdir(), "worker-tasks-"));
// Isolasi sesi (lihat catatan agent/state.ts): operasi persist diarahkan ke
// folder temp test — tanpa ini pushMsg dari test MENIMPA sesi aktif user
// di data/assistant-sessions.json (pencemaran nyata, pernah terjadi).
process.env.LIVE2D_TEST_SESSION_ROOT = workDir;
afterAll(() => {
  setRuntime(null);
  try { rmSync(workDir, { recursive: true, force: true }); } catch {}
});

function makeMockConfig(): any {
  return {
    load: () => ({ i18n: { lang: "id" } }),
    connections: [{ id: "m", name: "mock", provider: "mock", apiKey: "mock" }],
    activeConnection: { id: "m", name: "mock", provider: "mock", apiKey: "mock" },
    saveConnections: () => {},
  };
}
const cfg = makeMockConfig();

/** Pasang task aktif deterministik — gerbang facade cuma baca state.
 *  nextTaskSeq ikut digeser supaya taskId berikutnya realistis (t_1 dipakai
 *  → alokasi berikutnya t_2). */
function pokeActive(rt: Runtime, status: "running" | "awaiting_approval", taskId = "t_1") {
  rt.activeTask = { taskId, prompt: "tugas aktif (palsu)", status, createdAt: Date.now() };
  rt.busy = true;
  rt.nextTaskSeq = Math.max(rt.nextTaskSeq, Number(taskId.slice(2)) + 1);
}

describe("§9 park — task baru tidak mematikan task aktif", () => {
  it("ask saat slot dipegang → parked, dan history TIDAK tersentuh (isolasi §14)", async () => {
    const rt = makeRuntime({}, workDir, []);
    setRuntime(rt);
    pokeActive(rt, "running");
    const r = await assistantAsk("Tugas kedua", cfg);
    expect(r).toMatchObject({ ok: true, parked: true, position: 1 });
    expect(r.taskId).toBe("t_2");
    expect(rt.history.length).toBe(0); // prompt TIDAK masuk history sebelum jalan
    expect(rt.parkedTasks.length).toBe(1);
  });

  it("antrean FIFO cap 20 — item ke-21 DITOLAK eksplisit, antrean utuh", async () => {
    const rt = makeRuntime({}, workDir, []);
    setRuntime(rt);
    pokeActive(rt, "running");
    for (let i = 0; i < MAX_PARKED_TASKS; i++) {
      const r = await assistantAsk("park " + i, cfg);
      expect(r.ok).toBe(true);
    }
    const full = await assistantAsk("kelebihan", cfg);
    expect(full.ok).toBe(false);
    expect(full.error).toContain("penuh");
    expect(rt.parkedTasks.length).toBe(MAX_PARKED_TASKS); // tidak silent-evict
  });

  it("pause approval tetap memegang slot — ask baru tetap di-park (§10)", async () => {
    const rt = makeRuntime({}, workDir, []);
    setRuntime(rt);
    pokeActive(rt, "awaiting_approval");
    const r = await assistantAsk("datang saat menunggu izin", cfg);
    expect(r).toMatchObject({ ok: true, parked: true });
    expect(rt.activeTask?.status).toBe("awaiting_approval"); // slot tidak direbut
    expect(rt.busy).toBe(true);
  });
});

describe("§11 cancel per-task", () => {
  it("task parked dikeluarkan spesifik; task lain di antrean tetap (FIFO utuh)", async () => {
    const rt = makeRuntime({}, workDir, []);
    setRuntime(rt);
    pokeActive(rt, "running");
    await assistantAsk("B", cfg); // t_2
    await assistantAsk("C", cfg); // t_3
    await assistantAsk("D", cfg); // t_4
    const r = assistantCancel("t_3");
    expect(r).toMatchObject({ ok: true, accepted: true, target: "parked" });
    expect(rt.parkedTasks.map((t) => t.taskId)).toEqual(["t_2", "t_4"]);
  });

  it("task aktif running → kooperatif (cancelRequested); id asing → error", async () => {
    const rt = makeRuntime({}, workDir, []);
    setRuntime(rt);
    pokeActive(rt, "running");
    expect(assistantCancel("t_1")).toMatchObject({ ok: true, accepted: true, target: "active" });
    expect(rt.cancelRequested).toBe(true);
    const asing = assistantCancel("t_99");
    expect(asing.ok).toBe(false);
    expect(asing.error).toContain("tidak ditemukan");
  });

  it("task paused → terminal LANGSUNG; approval-nya jadi yatim", async () => {
    const rt = makeRuntime({}, workDir, []);
    setRuntime(rt);
    pokeActive(rt, "awaiting_approval");
    rt.approvals.set("ap_x", { id: "ap_x", tool: "write_file", args: {}, ts: Date.now() });
    expect(assistantCancel("t_1")).toMatchObject({ ok: true, accepted: true, target: "paused" });
    expect(rt.activeTask).toBeNull();
    expect(rt.busy).toBe(false);
    const r = await assistantResolveApproval("ap_x", true, cfg);
    expect(r.ok).toBe(true);
    expect(r.reply).toContain("sudah tidak aktif"); // izin tanpa pemilik diabaikan
  });

  it("tanpa taskId → hanya task AKTIF; antrean tidak tersentuh", async () => {
    const rt = makeRuntime({}, workDir, []);
    setRuntime(rt);
    pokeActive(rt, "running");
    await assistantAsk("B", cfg); // t_2 ter-park
    const r = assistantCancel(); // tanpa id → task aktif (kooperatif)
    expect(r).toMatchObject({ ok: true, accepted: true, target: "active" });
    expect(rt.cancelRequested).toBe(true);
    expect(rt.parkedTasks.map((t) => t.taskId)).toEqual(["t_2"]); // antrean utuh
    expect(assistantCancel("t_2")).toMatchObject({ accepted: true, target: "parked" });
    expect(rt.parkedTasks.length).toBe(0);
    // Slot kosong + tanpa id → tidak ada yang dibatalkan.
    rt.activeTask = null;
    rt.busy = false;
    rt.cancelRequested = false;
    expect(assistantCancel()).toMatchObject({ ok: true, accepted: false });
  });
});

describe("§12 modify — replacement mewarisi posisi", () => {
  it("task parked → prompt diganti in-place, posisi antrean tetap", async () => {
    const rt = makeRuntime({}, workDir, []);
    setRuntime(rt);
    pokeActive(rt, "running");
    await assistantAsk("B lama", cfg); // t_2
    await assistantAsk("C", cfg); // t_3
    const r = assistantModify("t_2", "B baru", cfg);
    expect(r).toMatchObject({ ok: true, target: "parked" });
    expect(r.taskId).toBe("t_4");
    expect(rt.parkedTasks.map((t) => t.prompt)).toEqual(["B baru", "C"]);
  });

  it("task aktif running → cancel kooperatif + replacement tertunda (sekali saja)", async () => {
    const rt = makeRuntime({}, workDir, []);
    setRuntime(rt);
    pokeActive(rt, "running");
    const r = assistantModify("t_1", "tugas pengganti", cfg);
    expect(r).toMatchObject({ ok: true, target: "active" });
    expect(rt.cancelRequested).toBe(true);
    expect(rt.pendingReplacement?.prompt).toBe("tugas pengganti");
    expect(assistantModify("t_1", "pengganti kedua", cfg).ok).toBe(false); // satu replacement
  });

  it("task paused → terminal segera + replacement langsung jalan", async () => {
    const rt = makeRuntime({}, workDir, []);
    setRuntime(rt);
    pokeActive(rt, "awaiting_approval");
    const r = assistantModify("t_1", "tugas pengganti paused", cfg);
    expect(r).toMatchObject({ ok: true, target: "paused" });
    expect(rt.activeTask?.taskId).toBe(r.taskId); // replacement sudah pegang slot
    await new Promise((res) => setTimeout(res, 500)); // mock LLM 300ms
    expect(rt.activeTask).toBeNull();
    expect(rt.busy).toBe(false);
    expect(rt.history.some((m) => m.role === "user" && m.content.includes("tugas pengganti paused"))).toBe(true);
  });

  it("id asing → error eksplisit", () => {
    const rt = makeRuntime({}, workDir, []);
    setRuntime(rt);
    expect(assistantModify("t_404", "apa pun", cfg).ok).toBe(false);
  });
});

describe("drain otomatis — antrean jalan sendiri setelah slot kosong (E2E mock)", () => {
  it("A jalan → B ter-park → A selesai → B menyala otomatis berurutan", async () => {
    const rt = makeRuntime({}, workDir, []);
    setRuntime(rt);
    const p1 = assistantAsk("Tugas A", cfg);
    await new Promise((r) => setTimeout(r, 60)); // A sudah pegang slot (mock 300ms)
    expect(rt.activeTask?.prompt).toContain("Tugas A");
    const p2 = await assistantAsk("Tugas B", cfg);
    expect(p2.parked).toBe(true);
    const r1 = await p1;
    expect(r1.ok).toBe(true);
    expect(r1.parked).toBeUndefined();
    await new Promise((r) => setTimeout(r, 500)); // drain B otomatis (mock 300ms)
    expect(rt.activeTask).toBeNull();
    expect(rt.busy).toBe(false);
    expect(rt.parkedTasks.length).toBe(0);
    const userMsgs = rt.history.filter((m) => m.role === "user").map((m) => m.content);
    expect(userMsgs[0]).toContain("Tugas A");
    expect(userMsgs.some((m) => m.includes("Tugas B"))).toBe(true); // B jalan setelah A
    // A → B, bukan B dulu (drain menghormati urutan FIFO)
    expect(userMsgs.findIndex((m) => m.includes("Tugas A")))
      .toBeLessThan(userMsgs.findIndex((m) => m.includes("Tugas B")));
  });
});

describe("resume approval — slot lepas setelah kelanjutan selesai (regresi)", () => {
  it("approve → kelanjutan selesai → slot lepas + antrean drain (bukan nyangkut)", async () => {
    const rt = makeRuntime({}, workDir, []);
    setRuntime(rt);
    pokeActive(rt, "awaiting_approval", "t_1");
    rt.approvals.set("ap_r", {
      id: "ap_r", tool: "search_code", args: { query: "x", path: "." }, ts: Date.now(),
    });
    await assistantAsk("Tugas B", cfg); // t_2 ter-park saat t_1 menunggu izin
    const r = await assistantResolveApproval("ap_r", true, cfg);
    expect(r.ok).toBe(true);
    // Inti regresi: tanpa reset status ke "running" saat resume, slot
    // nyangkut di t_1 awaiting_approval dan antrean tidak pernah di-drain.
    expect(rt.activeTask?.taskId).not.toBe("t_1"); // slot lepas; drain bisa sudah menyalakan t_2
    await new Promise((res) => setTimeout(res, 500)); // t_2 (mock 300ms) selesai
    expect(rt.activeTask).toBeNull();
    expect(rt.busy).toBe(false);
    expect(rt.parkedTasks.length).toBe(0);
    expect(rt.history.some((m) => m.role === "user" && m.content.includes("Lanjutkan tugas"))).toBe(true);
    expect(rt.history.some((m) => m.role === "user" && m.content.includes("Tugas B"))).toBe(true);
  });
});

describe("status mengekspos task identity", () => {
  it("activeTask + parkedTasks terlihat di /api/assistant/status", async () => {
    const rt = makeRuntime({}, workDir, []);
    setRuntime(rt);
    pokeActive(rt, "awaiting_approval", "t_9");
    await assistantAsk("yang di antrean", cfg);
    const st = assistantStatus() as any;
    expect(st.activeTask).toMatchObject({ taskId: "t_9", status: "awaiting_approval" });
    expect(st.parkedTasks.length).toBe(1);
    expect(st.busy).toBe(true);
  });
});
