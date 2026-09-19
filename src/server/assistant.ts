/**
 * server/assistant.ts — FACADE mode AI Assistant/Agent.
 * Semua logika berat sudah dipindah ke modul agent/ (otak kerja) dan
 * persona/ (otak akting). File ini hanya:
 *   - kelola runtime (start/stop/reset/status/history),
 *   - jembatan API lama (index.ts & panel web) ke agent loop,
 *   - panggil persona narrator di akhir tugas.
 * Kontrak API TIDAK berubah: index.ts, panel web, dan CLI tetap sama.
 */
import type { ConfigManager } from "../shared/config";
import { appRoot } from "../shared/paths";
import { makeRuntime, getRuntime, setRuntime, loadSession, saveSession, pushMsg, MAX_PARKED_TASKS } from "./agent/state";
import type { WorkerTask } from "./agent/state";
import { makeSessionsStore } from "./agent/sessions";
import { agentAsk, agentRunApproved } from "./agent/loop";
import { stripToolDirective } from "./agent/parse";
import { readEvents, emitEvent } from "./agent/bus";
import { narrate } from "./persona/narrator";
import { cleanForSpeech } from "./persona/clean";
import { TOOLS, publicToolArgs } from "./agent/tools/index";
import { setSubagentConfig } from "./agent/tools/subagent";
import { memoryList, memoryDelete } from "./agent/memory";
import { undoList, revertUndo } from "./agent/undo";

export type { AsMsg, AsApproval, PlanItem } from "./agent/state";
export type { AsEvent } from "./assistant-events";
export { memoryList as assistantMemoryList, memoryDelete as assistantMemoryDelete };

let configWired = false;
// Store sesi — LAZY seperti di agent/state.ts: test mengisolasi lewat
// LIVE2D_TEST_SESSION_ROOT supaya operasi sesi tidak menyentuh data/ user.
let sessionsStore: ReturnType<typeof makeSessionsStore> | null = null;
function sessionStoreApi(): ReturnType<typeof makeSessionsStore> {
  if (!sessionsStore)
    sessionsStore = makeSessionsStore(process.env.LIVE2D_TEST_SESSION_ROOT || appRoot());
  return sessionsStore;
}
/** Dipanggil sekali dari index.ts saat server boot. */
export function initAssistant(config: ConfigManager): void {
  if (!configWired) {
    setSubagentConfig(config);
    configWired = true;
  }
}

// ── Status & lifecycle ─────────────────────────────────────────

export function assistantStatus() {
  const rt = getRuntime();
  const bus = readEvents(0);
  const lastEvent = rt && bus.events.length
    ? bus.events[bus.events.length - 1]
    : null;
  return {
    running: !!rt,
    busy: rt?.busy || false,
    workDir: rt?.workDir || null,
    historyCount: rt?.history.length || 0,
    pendingApprovals: rt ? Array.from(rt.approvals.values(), (ap) => ({
      ...ap,
      args: publicToolArgs(ap.tool, ap.args),
    })) : [],
    /** Rencana kerja aktif (update_plan) — untuk kotak progress di panel. */
    plan: rt?.plan || [],
    /** File yang tersentuh sesi ini (notes) — untuk tab Review panel. */
    notes: { filesTouched: rt ? rt.notes.filesTouched.slice() : [] },
    /** Metadata level tool (safe/mutating) — badge "auto"/"izin" di panel.
     *  Sumber kebenaran tetap registry TOOLS; client tidak menduplikasi. */
    tools: TOOLS.map((t) => ({ name: t.name, level: t.level })),
    /** Aktivitas agent terakhir — stage chip menampilkan apa yang sedang
     *  dikerjakan tanpa membuka panel. Null bila runtime mati/bus kosong. */
    lastEvent: lastEvent ? { type: lastEvent.type, label: lastEvent.label } : null,
    /** Task identity Worker (§9): slot aktif + antrean menunggu. */
    activeTask: rt?.activeTask
      ? { taskId: rt.activeTask.taskId, status: rt.activeTask.status, prompt: rt.activeTask.prompt.slice(0, 120) }
      : null,
    parkedTasks: rt
      ? rt.parkedTasks.map((t) => ({ taskId: t.taskId, prompt: t.prompt.slice(0, 120) }))
      : [],
  };
}

export function assistantHistory() {
  const rt = getRuntime();
  return rt ? rt.history : [];
}

export function assistantStop() {
  const rt = getRuntime();
  if (rt) rt.destroyed = true;
  setRuntime(null);
  return { ok: true };
}

export function assistantStart(cfg: any): { ok: boolean; error?: string } {
  assistantStop();
  // Default = akar app (bukan process.cwd()) supaya deterministik di portable.
  const saved = loadSession();
  const workDir = String(cfg?.workDir || saved?.workDir || appRoot()).trim();
  const rt = makeRuntime(cfg || {}, workDir, saved?.history ? saved.history.slice() : []);
  rt.sessionId = saved?.sessionId || "";
  setRuntime(rt);
  saveSession(rt);
  return { ok: true };
}

export function assistantReset() {
  const rt = getRuntime();
  if (rt?.busy)
    return { ok: false, error: "masih memproses tugas — riwayat tidak bisa dikosongkan saat task berjalan" };
  if (rt) {
    rt.history = [];
    saveSession(rt);
  }
  return { ok: true };
}

/**
 * Cancel task (§11) TANPA mematikan runtime. Kooperatif untuk task yang
 * sedang berjalan loop-nya (flag dicek antar-langkah; tool yang sedang
 * eksekusi selesai dulu). Task yang SEDANG menunggu approval atau yang masih
 * di antrean langsung terminal — loop-nya tidak sedang jalan.
 * Tanpa taskId: task aktif (kompatibilitas tombol panel/CLI lama).
 */
export function assistantCancel(taskId?: string): {
  ok: boolean;
  accepted: boolean;
  error?: string;
  target?: "active" | "paused" | "parked";
} {
  const rt = getRuntime();
  if (!rt) return { ok: true, accepted: false };
  if (taskId) {
    const active = rt.activeTask;
    if (active?.taskId === taskId) {
      if (active.status === "awaiting_approval") {
        // Pause = loop tidak jalan → terminal seketika, tanpa menunggu apa pun.
        pushMsg(rt, { role: "assistant", content: "Dibatalkan oleh user." });
        emitEvent("error", "dibatalkan: oleh user");
        finalizeTask(rt);
        return { ok: true, accepted: true, target: "paused" };
      }
      rt.cancelRequested = true; // running → kooperatif antar-langkah
      return { ok: true, accepted: true, target: "active" };
    }
    const i = rt.parkedTasks.findIndex((t) => t.taskId === taskId);
    if (i >= 0) {
      rt.parkedTasks.splice(i, 1);
      return { ok: true, accepted: true, target: "parked" };
    }
    return { ok: false, accepted: false, error: "task tidak ditemukan: " + taskId };
  }
  // Tanpa id → task aktif saja; task di antrean harus disebut eksplisit.
  if (!rt.activeTask && !rt.busy) return { ok: true, accepted: false };
  rt.cancelRequested = true;
  return { ok: true, accepted: true, target: rt.activeTask?.status === "awaiting_approval" ? "paused" : "active" };
}

// ── Event stream untuk panel/pet/akting (bus ber-seq) ──────────

export function assistantEvents(sinceSeq = 0) {
  const d = readEvents(sinceSeq);
  return { latest: d.latest, busy: !!getRuntime()?.busy, events: d.events };
}

// ── Ask: task identity Worker (§9–12) + jembatan ke agent loop ──

function assistantAskNoop(): void {}

type AskFacadeResult = {
  ok: boolean;
  error?: string;
  reply?: string;
  speak?: string;
  parked?: boolean;
  taskId?: string;
  position?: number;
  paused?: boolean;
};

function nextTaskId(rt: { nextTaskSeq: number }): string {
  return "t_" + rt.nextTaskSeq++;
}

/** Slot resmi kosong: task aktif terminal, busy turun, antrean di-drain.
 *  Pause TIDAK boleh lewat sini — slot milik task yang menunggu approval (§10). */
function finalizeTask(rt: import("./agent/state").Runtime): void {
  rt.activeTask = null;
  rt.busy = false;
  rt.cancelRequested = false;
}

/** Jalankan task berikutnya setelah slot kosong: replacement (§12) mewarisi
 *  slot SEBELUM antrean FIFO. Task hasil drain berjalan tanpa sink SSE
 *  (peminta aslinya sudah pergi) — aktivitasnya tetap hidup di bus + history;
 *  panel mengikuti lewat jalur itu. */
function drainNext(rt: import("./agent/state").Runtime, config: ConfigManager): void {
  if (rt.destroyed) return;
  const replacement = rt.pendingReplacement;
  rt.pendingReplacement = null;
  const task = replacement || rt.parkedTasks.shift() || null;
  if (!task) return;
  void runTask(rt, task.prompt, config, undefined, task).catch(() => {});
}

/** Narasi akhir task (komentar berkarakter) — hanya task terminal, bukan pause. */
async function narrateResult(
  rt: import("./agent/state").Runtime,
  reply: string,
  config: ConfigManager,
): Promise<string | undefined> {
  const clean = cleanForSpeech(stripToolDirective(reply, TOOLS.map((t) => t.name)));
  if (!clean || reply.includes("⏳")) return undefined;
  if (clean.length <= 240 && !rt.persona) return clean;
  const n = await narrate({ event: "", result: clean, isError: false, persona: rt.persona }, config);
  return n.speak;
}

/** Jalankan SATU task sampai terminal/pause. `existing` = task yang sudah
 *  teridentitas (drain/modify) — tidak bikin taskId baru. `onEvent` opsional:
 *  task hasil drain tidak punya sink SSE lagi. */
async function runTask(
  rt: import("./agent/state").Runtime,
  text: string,
  config: ConfigManager,
  onEvent?: (e: any) => void,
  existing?: WorkerTask,
): Promise<AskFacadeResult> {
  const task: WorkerTask =
    existing ??
    { taskId: nextTaskId(rt), prompt: String(text || "").slice(0, 4000), status: "running", createdAt: Date.now() };
  rt.activeTask = task;
  // Resume (existing) datang dari status awaiting_approval — balikkan ke
  // running SUPAYA finally di bawah mengenali task ini terminal saat
  // kelanjutannya selesai (tanpa ini slot nyangkut + antrean tak di-drain —
  // bug yang tertangkap runtime verify Fase 6).
  task.status = "running";
  try {
    const r = await agentAsk(rt, text, config, onEvent !== assistantAskNoop ? onEvent : undefined);
    if (r.paused) {
      // §10: approval pause MEMEGANG slot — busy tetap hidup; task baru
      // datang setelah ini akan di-park, bukan dicampur ke history task ini.
      task.status = "awaiting_approval";
      return { ok: true, reply: r.reply, paused: true, taskId: task.taskId };
    }
    const speak = r.ok ? await narrateResult(rt, r.reply || "", config) : undefined;
    if (speak && onEvent) onEvent({ type: "speak", text: speak });
    return { ok: r.ok, error: r.error, reply: r.reply, speak };
  } finally {
    // Hanya finalisasi bila task ini masih pemilik slot DAN bukan pause.
    if (rt.activeTask === task && task.status !== "awaiting_approval") {
      finalizeTask(rt);
      drainNext(rt, config);
    }
  }
}

export async function assistantAsk(
  text: string,
  config: ConfigManager,
  onEvent: (e: any) => void = assistantAskNoop,
): Promise<AskFacadeResult> {
  const rt = getRuntime();
  if (!rt) return { ok: false, error: "assistant mode tidak aktif" };
  // §9: task baru TIDAK mematikan task aktif — masuk antrean FIFO. Termasuk
  // saat task aktif menunggu approval (slot tetap miliknya, §10). Prompt
  // TIDAK menyentuh history sebelum task-nya jalan (isolasi §14 — dulu
  // dua task bisa tercampur dalam satu history bersama).
  if (rt.activeTask) {
    if (rt.parkedTasks.length >= MAX_PARKED_TASKS)
      return { ok: false, error: "antrean task penuh (" + MAX_PARKED_TASKS + ") — tugas baru ditolak, selesaikan yang ada dulu" };
    const task: WorkerTask = {
      taskId: nextTaskId(rt),
      prompt: String(text || "").slice(0, 4000),
      status: "running",
      createdAt: Date.now(),
    };
    rt.parkedTasks.push(task);
    return { ok: true, parked: true, taskId: task.taskId, position: rt.parkedTasks.length };
  }
  return runTask(rt, text, config, onEvent);
}

/**
 * Modify task (§12): pengganti BUKAN task independen baru — dia mewarisi
 * posisi task yang diganti.
 *  - Aktif running: cancel kooperatif; begitu terminal, replacement mengambil
 *    slot (didahulukan dari antrean): urutan A → A' → B → C.
 *  - Aktif paused (menunggu approval): terminal langsung, replacement jalan segera.
 *  - Di antrean: prompt diganti in-place, posisi antrean tetap.
 */
export function assistantModify(
  taskId: string,
  text: string,
  config: ConfigManager,
): { ok: boolean; error?: string; taskId?: string; target?: "active" | "paused" | "parked" } {
  const rt = getRuntime();
  if (!rt) return { ok: false, error: "assistant mode tidak aktif" };
  const prompt = String(text || "").slice(0, 4000);
  if (!prompt) return { ok: false, error: "teks task kosong" };
  const replacement: WorkerTask = { taskId: nextTaskId(rt), prompt, status: "running", createdAt: Date.now() };
  const active = rt.activeTask;
  if (active?.taskId === taskId) {
    if (active.status === "running") {
      if (rt.pendingReplacement)
        return { ok: false, error: "sudah ada pengganti tertunda untuk task ini" };
      // Kooperatif: loop berhenti di titik aman berikutnya → drainNext
      // menyalakan replacement (mewarisi slot).
      rt.cancelRequested = true;
      rt.pendingReplacement = replacement;
      pushMsg(rt, { role: "tool", content: "User MENGGANTI tugas ini — hentikan pekerjaan saat ini dan tunggu tugas baru." });
      return { ok: true, taskId: replacement.taskId, target: "active" };
    }
    // Paused: loop tidak sedang jalan → terminal seketika + replacement menyala.
    pushMsg(rt, { role: "assistant", content: "(tugas diganti user)" });
    finalizeTask(rt);
    void runTask(rt, replacement.prompt, config, undefined, replacement).catch(() => {});
    return { ok: true, taskId: replacement.taskId, target: "paused" };
  }
  const i = rt.parkedTasks.findIndex((t) => t.taskId === taskId);
  if (i >= 0) {
    rt.parkedTasks[i] = replacement; // in-place — posisi antrean tetap (§12)
    return { ok: true, taskId: replacement.taskId, target: "parked" };
  }
  return { ok: false, error: "task tidak ditemukan: " + taskId };
}

export async function assistantResolveApproval(
  id: string,
  approve: boolean,
  config: ConfigManager,
  onEvent: (e: any) => void = assistantAskNoop,
): Promise<AskFacadeResult> {
  const rt = getRuntime();
  if (!rt) return { ok: false, error: "assistant mode tidak aktif" };
  const ap = rt.approvals.get(id);
  if (!ap) return { ok: false, error: "approval tidak ditemukan" };
  rt.approvals.delete(id);
  // Event bus permission_resolved: panel/pet menutup kartu izin secara reaktif.
  emitEvent("permission_resolved", (approve ? "disetujui: " : "ditolak: ") + ap.tool);
  const task = rt.activeTask;
  if (!task) {
    // Task yang meminta izin sudah terminal (dibatalkan/diganti saat pause) —
    // izin tanpa pemilik; jangan jalankan tool tanpa task yang menunggunya.
    return { ok: true, reply: "Task yang meminta izin sudah tidak aktif — izin diabaikan." };
  }
  if (!approve) {
    pushMsg(rt, { role: "tool", content: "User MENOLAK " + ap.tool + " — batalkan rencana itu dan tanyakan alternatif." });
    finalizeTask(rt);
    drainNext(rt, config);
    return { ok: true, reply: "Ditolak. Aku batalkan." };
  }
  await agentRunApproved(rt, ap.tool, ap.args, onEvent !== assistantAskNoop ? onEvent : undefined);
  // Lanjutkan reasoning untuk task yang SAMA — slot tidak lepas dan TIDAK
  // lewat gerbang park (task aktif sudah ada di slot-nya sendiri).
  const r = await runTask(rt, "Lanjutkan tugas berdasarkan hasil tool di atas.", config, onEvent, task);
  return r;
}

// ── Undo: daftar snapshot & revert (panel tab Review) ────────────

export function assistantUndoList() {
  const rt = getRuntime();
  return rt ? undoList(rt) : [];
}

export function assistantRevert(id: string): string {
  const rt = getRuntime();
  if (!rt) throw new Error("assistant mode tidak aktif");
  return revertUndo(rt, id);
}

// ── Multi-session: list / create / switch / delete ───────────────

export function assistantSessionsList() {
  return sessionStoreApi().list();
}

export function assistantSessionCreate(workDir?: string): { ok: boolean; error?: string } {
  const rt = getRuntime();
  if (rt?.busy) return { ok: false, error: "masih memproses pertanyaan sebelumnya" };
  const wd = String(workDir || rt?.workDir || appRoot()).trim();
  // Simpan dulu state sesi lama (bila ada), lalu buat & pindah.
  if (rt) saveSession(rt);
  const rec = sessionStoreApi().create(wd);
  if (rt) {
    rt.history = [];
    rt.workDir = wd;
    rt.sessionId = rec.id;
  }
  return { ok: true };
}

export function assistantSessionSwitch(id: string): { ok: boolean; error?: string } {
  const rt = getRuntime();
  if (rt?.busy) return { ok: false, error: "masih memproses pertanyaan sebelumnya" };
  if (rt) saveSession(rt); // simpan yang lama dulu
  const rec = sessionStoreApi().switchTo(String(id || ""));
  if (!rec) return { ok: false, error: "sesi tidak ditemukan" };
  if (rt) {
    rt.history = rec.messages.slice();
    if (rec.workDir) rt.workDir = rec.workDir;
    rt.sessionId = rec.id;
    saveSession(rt);
  }
  return { ok: true };
}

export function assistantSessionDelete(id: string): { ok: boolean; error?: string; newActive?: string } {
  const rt = getRuntime();
  if (rt?.busy) return { ok: false, error: "masih memproses pertanyaan sebelumnya" };
  const r = sessionStoreApi().remove(String(id || ""));
  if (!r.ok) return { ok: false, error: "sesi tidak ditemukan" };
  // Sesi aktif terhapus → runtime dipindah ke sesi sisa terbaru (atau kosong).
  if (rt && r.newActive !== undefined) {
    const rec = sessionStoreApi().activeRec();
    rt.history = rec ? rec.messages.slice() : [];
    rt.sessionId = rec ? rec.id : "";
    if (rec?.workDir) rt.workDir = rec.workDir;
  }
  return { ok: true, newActive: r.newActive };
}
