/**
 * server/agent/state.ts — State runtime agent (otak kerja).
 * Semua yang lintas-turn tinggal di sini: riwayat, approval, catatan sesi,
 * dan persistensi ke disk. Loop (loop.ts) hanya menerima rt ini — tidak
 * tahu apa-apa soal persona/karakter.
 */

export type AsMsg = { role: "user" | "assistant" | "tool"; content: string; ts: number };
export type AsApproval = { id: string; tool: string; args: any; ts: number };

/** Identitas satu task Worker (§9 ARSITEKTUR-TARGET): agent bukan sekadar
 *  busy=true — ada slot aktif + antrean, dan tiap task punya taskId yang bisa
 *  ditunjuk untuk cancel/modify. */
export type WorkerTask = {
  taskId: string;
  prompt: string;
  status: "running" | "awaiting_approval";
  createdAt: number;
};

/** §9: antrean task FIFO maksimum 20 — item baru saat penuh DITOLAK eksplisit,
 *  item lama tidak pernah dibuang senyap. */
export const MAX_PARKED_TASKS = 20;

/** Catatan state penting yang WAJIB selamat dari summarization history. */
export type SessionNotes = {
  /** File yang pernah ditulis/diubah agent (path relatif). */
  filesTouched: string[];
  /** Keputusan/permintaan user penting (kalimat singkat). */
  decisions: string[];
};

/** Todo list rencana kerja — state terpisah dari teks jawaban. */
export type PlanItem = {
  id: string;
  task: string;
  status: "pending" | "in_progress" | "done" | "failed";
  note?: string;
};

/** Satu rekaman undo: isi file SEBELUM tool mutasi file berhasil. */
export type UndoRecord = {
  id: string;
  /** Path absolut file saat snapshot. */
  absPath: string;
  /** Path relatif (untuk tampilan & pencocokan panel). */
  relPath: string;
  /** Isi file sebelum mutasi; null = file belum ada (revert = hapus). */
  prevContent: string | null;
  ts: number;
  /** Disetel true setelah revert (entri tetap sebagai jejak, tak revertable). */
  reverted?: boolean;
};

export type Runtime = {
  cfg: any;
  history: AsMsg[];
  approvals: Map<string, AsApproval>;
  busy: boolean;
  workDir: string;
  destroyed: boolean;
  /** Persona karakter (userNote sheet) — diisi client saat start. */
  persona: string;
  /** Ring buffer event (bus.ts menulis, endpoint /events membaca). */
  events: { seq: number; type: string; label: string; ts: number }[];
  eventSeq: number;
  /** State penting di luar history — selamat dari summarization. */
  notes: SessionNotes;
  /** Rencana kerja saat ini (update_plan tool). */
  plan: PlanItem[];
  /** Pelacakan verifikasi: tulis vs baca/jalan terakhir. */
  lastWriteAt: number;
  lastVerifyAt: number;
  /** Penghitung pelanggaran verifikasi per item plan. */
  planBlocks: Record<string, number>;
  /** Jumlah ringkasan history yang sudah dilakukan (untuk log/debug). */
  summarizations: number;
  /** Riwayat snapshot file sebelum mutasi (undo) — cap MAX_UNDO, in-memory. */
  undo: UndoRecord[];
  /** Sesi aktif (assistant-sessions.json) — untuk persist multi-session. */
  sessionId: string;
  /** Permintaan cancel kooperatif (POST /api/assistant/cancel) — dicek loop
   *  antar-langkah; tool yang sedang jalan selesai dulu (run_command ≤30 dtk). */
  cancelRequested: boolean;
  // ── Task identity Worker (§9–12) ────────────────────────────────
  /** Slot task aktif — satu-satunya; null = slot kosong. Saat loop pause
   *  menunggu approval, slot TETAP dipegang task ini (status awaiting_approval). */
  activeTask: WorkerTask | null;
  /** Antrean task menunggu (FIFO, cap MAX_PARKED_TASKS). Prompt TIDAK masuk
   *  history sebelum task-nya benar-benar jalan (isolasi §14). */
  parkedTasks: WorkerTask[];
  /** Replacement dari assistantModify untuk task aktif (§12) — mewarisi slot
   *  begitu task lama terminal, didahulukan dari antrean. */
  pendingReplacement: WorkerTask | null;
  /** Penghitung taskId (t_1, t_2, …). */
  nextTaskSeq: number;
};

let runtime: Runtime | null = null;

export const MAX_HISTORY = 60;
/** Budget karakter total riwayat sebelum summarization (~12k token). */
export const HISTORY_CHAR_BUDGET = 30000;
/** Cap rekaman undo (FIFO) — isi file disimpan in-memory saja. */
export const MAX_UNDO = 20;

export function getRuntime(): Runtime | null {
  return runtime;
}

export function setRuntime(rt: Runtime | null): void {
  runtime = rt;
}

export function makeRuntime(cfg: any, workDir: string, history: AsMsg[]): Runtime {
  return {
    cfg: cfg || {},
    history,
    approvals: new Map(),
    busy: false,
    workDir,
    destroyed: false,
    persona: typeof cfg?.persona === "string" ? cfg.persona.slice(0, 800) : "",
    events: [],
    eventSeq: 0,
    notes: { filesTouched: [], decisions: [] },
    plan: [],
    lastWriteAt: 0,
    lastVerifyAt: 0,
    planBlocks: {},
    summarizations: 0,
    undo: [],
    sessionId: "",
    cancelRequested: false,
    activeTask: null,
    parkedTasks: [],
    pendingReplacement: null,
    nextTaskSeq: 1,
  };
}

export function pushMsg(rt: Runtime, m: Omit<AsMsg, "ts">): void {
  rt.history.push({ ...m, ts: Date.now() });
  if (rt.history.length > MAX_HISTORY) rt.history.splice(0, rt.history.length - MAX_HISTORY);
  saveSession(rt);
}

// ── Persist sesi — lewat store multi-session (sessions.ts).
// loadSession/saveSession adalah wrapper kompatibilitas: CLI & panel tidak
// perlu tahu store-nya; sesi aktif disimpan di assistant-sessions.json
// (migrasi otomatis dari assistant-history.json format lama).
// Store dibuat LAZY: test mengisolasi lewat LIVE2D_TEST_SESSION_ROOT
// (folder root berbeda, diset sebelum operasi sesi pertama). Tanpa ini,
// test yang menulis history ikut menimpa sesi AKTIF user di data/ —
// pencemaran nyata (pernah terjadi: workDir sesi user berubah jadi
// folder temp test).
import { appRoot } from "../../shared/paths";
import { makeSessionsStore } from "./sessions";

let store: ReturnType<typeof makeSessionsStore> | null = null;
function sessionStore(): ReturnType<typeof makeSessionsStore> {
  if (!store)
    store = makeSessionsStore(
      process.env.LIVE2D_TEST_SESSION_ROOT || appRoot(),
    );
  return store;
}

export function loadSession(): { history: AsMsg[]; workDir: string | null; sessionId?: string } | null {
  const rec = sessionStore().activeRec();
  if (!rec) return null;
  return { history: rec.messages, workDir: rec.workDir || null, sessionId: rec.id };
}

export function saveSession(rt: Runtime): void {
  sessionStore().persistActive(rt.history, rt.workDir);
}

/** Pencatatan state penting — dipanggil loop setelah tool mutating sukses. */
export function noteFileTouched(rt: Runtime, path: string): void {
  const p = String(path || "").replace(/\\/g, "/");
  if (p && !rt.notes.filesTouched.includes(p)) rt.notes.filesTouched.push(p);
  if (rt.notes.filesTouched.length > 30) rt.notes.filesTouched.splice(0, rt.notes.filesTouched.length - 30);
}

/**
 * Catat snapshot undo SATU file (isi saat ini di disk). Dipanggil execTool
 * SETELAH tool mutasi file sukses. Return rekamannya (atau null bila path
 * kosong). Cap MAX_UNDO — terlama dibuang.
 */
export function pushUndo(rt: Runtime, rec: Omit<UndoRecord, "id" | "ts">): UndoRecord | null {
  if (!rec.relPath) return null;
  const full: UndoRecord = {
    ...rec,
    id: "un_" + Date.now().toString(36) + "_" + Math.random().toString(36).slice(2, 8),
    ts: Date.now(),
  };
  rt.undo.push(full);
  if (rt.undo.length > MAX_UNDO) rt.undo.splice(0, rt.undo.length - MAX_UNDO);
  return full;
}
