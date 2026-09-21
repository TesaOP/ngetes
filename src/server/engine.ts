/**
 * server/engine.ts — manajer sidecar inferensi native (live2d-engine).
 *
 * Satu proses Rust (crate `engine/`) melayani TTS (SuperTonic) + STT (Whisper)
 * lewat HTTP loopback. Server Bun menyalakannya saat start, menunggu /health,
 * lalu mem-proxy permintaan TTS/STT ke sana. Dibunuh saat server keluar.
 *
 * Model TIDAK dibundel — diunduh on-demand dari HuggingFace saat provider native
 * pertama dipakai (pola sama dengan SuperTonic Python & Whisper transformers.js
 * sekarang), lalu di-cache di disk. Bila offline & model belum ada → error jelas,
 * pemanggil (index.ts) degrade anggun ke provider lain.
 *
 * Privasi: sidecar bind loopback (127.0.0.1). Audio mic keluar browser HANYA ke
 * proses lokal ini, tak pernah ke jaringan (revisi aturan: loopback lokal
 * diizinkan; cloud hanya bila user memilih provider cloud secara sadar).
 */
import { spawn, execSync, type ChildProcess } from "child_process";
import { existsSync, mkdirSync, createWriteStream } from "fs";
import { join } from "path";
import { homedir } from "os";
import { appRoot } from "../shared/paths";

const ROOT = appRoot();
const ENGINE_PORT = 8330;
const ENGINE_HOST = "127.0.0.1";
const BASE = `http://${ENGINE_HOST}:${ENGINE_PORT}`;

// SHA model SuperTonic-3 (dipin di supertonic/config.py — reproducible).
const SUPERTONIC_REPO = "Supertone/supertonic-3";
const SUPERTONIC_SHA = "724fb5abbf5502583fb520898d45929e62f02c0b";
const SUPERTONIC_FILES = [
  "onnx/duration_predictor.onnx",
  "onnx/text_encoder.onnx",
  "onnx/vector_estimator.onnx",
  "onnx/vocoder.onnx",
  "onnx/tts.json",
  "onnx/unicode_indexer.json",
  "voice_styles/F1.json", "voice_styles/F2.json", "voice_styles/F3.json",
  "voice_styles/F4.json", "voice_styles/F5.json", "voice_styles/M1.json",
  "voice_styles/M2.json", "voice_styles/M3.json", "voice_styles/M4.json",
  "voice_styles/M5.json",
];

const WHISPER_REPO = "ggerganov/whisper.cpp";

// ── lokasi exe & model ─────────────────────────────────────────
const EXE_CANDIDATES = [
  join(ROOT, "engines", "live2d-engine.exe"),          // folder release portable
  join(ROOT, "engines", "live2d-engine"),              // non-Windows
  join(ROOT, "engine", "target", "release", "live2d-engine.exe"), // dev
  join(ROOT, "engine", "target", "release", "live2d-engine"),
];

function findEngineExe(): string | null {
  for (const c of EXE_CANDIDATES) if (existsSync(c)) return c;
  return null;
}

// TTS: pakai cache ~/.cache/supertonic3 bila sudah lengkap (berbagi dengan
// Python), selain itu ke engines/models/supertonic3 (lokasi unduh sendiri).
function ttsModelDir(): string {
  const shared = join(homedir(), ".cache", "supertonic3");
  if (existsSync(join(shared, "onnx", "vocoder.onnx"))) return shared;
  return join(ROOT, "engines", "models", "supertonic3");
}

function sttModelPath(name: string): string {
  const clean = name.replace(/[^a-z0-9.\-]/gi, "");
  return join(ROOT, "engines", "models", `ggml-${clean}.bin`);
}

// ── unduh model on-demand ──────────────────────────────────────
async function downloadFile(url: string, dest: string, label: string): Promise<void> {
  mkdirSync(join(dest, ".."), { recursive: true });
  const res = await fetch(url);
  if (!res.ok || !res.body) throw new Error(`unduh ${label} gagal: HTTP ${res.status}`);
  const total = Number(res.headers.get("content-length") || 0);
  const tmp = dest + ".part";
  const out = createWriteStream(tmp);
  let got = 0, lastLog = 0;
  const reader = res.body.getReader();
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    out.write(value);
    got += value.length;
    if (total && Date.now() - lastLog > 1500) {
      lastLog = Date.now();
      console.log(`[engine] unduh ${label}: ${(got / 1048576).toFixed(0)}/${(total / 1048576).toFixed(0)} MB`);
    }
  }
  await new Promise<void>((r, j) => out.end((e: any) => (e ? j(e) : r())));
  const { renameSync } = await import("fs");
  renameSync(tmp, dest);
}

let ttsDownloading: Promise<void> | null = null;
export async function ensureTtsModel(): Promise<string> {
  const dir = ttsModelDir();
  if (existsSync(join(dir, "onnx", "vocoder.onnx"))) return dir;
  if (!ttsDownloading) {
    ttsDownloading = (async () => {
      console.log("[engine] mengunduh model TTS SuperTonic (~385 MB, sekali saja)…");
      for (const rel of SUPERTONIC_FILES) {
        const dest = join(dir, rel);
        if (existsSync(dest)) continue;
        const url = `https://huggingface.co/${SUPERTONIC_REPO}/resolve/${SUPERTONIC_SHA}/${rel}`;
        await downloadFile(url, dest, rel);
      }
      console.log("[engine] model TTS siap.");
    })().catch((e) => { ttsDownloading = null; throw e; });
  }
  await ttsDownloading;
  return dir;
}

const sttDownloading = new Map<string, Promise<void>>();
export async function ensureSttModel(name: string): Promise<string> {
  const path = sttModelPath(name);
  if (existsSync(path)) return path;
  if (!sttDownloading.has(name)) {
    const p = (async () => {
      console.log(`[engine] mengunduh model STT whisper (${name})…`);
      const url = `https://huggingface.co/${WHISPER_REPO}/resolve/main/ggml-${name}.bin`;
      await downloadFile(url, path, `ggml-${name}.bin`);
      console.log("[engine] model STT siap.");
    })().catch((e) => { sttDownloading.delete(name); throw e; });
    sttDownloading.set(name, p);
  }
  await sttDownloading.get(name);
  return path;
}

// ── lifecycle sidecar ──────────────────────────────────────────
let engineProc: ChildProcess | null = null;
let starting: Promise<boolean> | null = null;

async function health(timeoutMs = 800): Promise<boolean> {
  try {
    const ctrl = new AbortController();
    const t = setTimeout(() => ctrl.abort(), timeoutMs);
    const r = await fetch(`${BASE}/health`, { signal: ctrl.signal });
    clearTimeout(t);
    return r.ok;
  } catch { return false; }
}

/** Nyalakan sidecar (idempoten). Return true bila hidup & sehat. */
export async function startEngine(sttModelName = "base"): Promise<boolean> {
  if (await health()) return true;              // sudah jalan (mis. dev manual)
  if (starting) return starting;
  starting = (async () => {
    const exe = findEngineExe();
    if (!exe) {
      console.warn("[engine] exe live2d-engine tak ditemukan — fitur native nonaktif (build: cd engine && cargo build --release --features stt)");
      return false;
    }
    const args = ["--port", String(ENGINE_PORT), "--tts-model", ttsModelDir()];
    // STT model di-pass bila filenya sudah ada; kalau belum, diunduh saat /stt
    // pertama dan sidecar direstart dengan --stt-model (lihat sttViaEngine).
    const sttPath = sttModelPath(sttModelName);
    if (existsSync(sttPath)) args.push("--stt-model", sttPath);
    engineProc = spawn(exe, args, { detached: false, stdio: "ignore" });
    engineProc.on("exit", () => { engineProc = null; });
    // tunggu ready maks 10 dtk
    for (let i = 0; i < 50; i++) {
      if (await health()) { console.log("[engine] sidecar native siap di " + BASE); return true; }
      await new Promise((r) => setTimeout(r, 200));
    }
    console.warn("[engine] sidecar tak merespons /health dalam 10 dtk");
    return false;
  })().finally(() => { starting = null; });
  return starting;
}

export function stopEngine(): void {
  const pid = engineProc?.pid;
  if (!pid) return;
  try {
    if (process.platform === "win32") execSync(`taskkill /PID ${pid} /T /F`, { stdio: "ignore" });
    else engineProc?.kill("SIGTERM");
  } catch {}
  engineProc = null;
}

// ── proxy TTS / STT ────────────────────────────────────────────
export type EngineTtsOpts = { text: string; voice?: string; lang?: string; speed?: number; steps?: number };

/** Sintesis TTS via sidecar → WAV. Melempar bila engine/model tak tersedia. */
export async function ttsViaEngine(opts: EngineTtsOpts): Promise<{ buf: Buffer; type: string }> {
  await ensureTtsModel();
  if (!(await startEngine())) throw new Error("engine native tidak tersedia");
  const r = await fetch(`${BASE}/tts`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ text: opts.text, voice: opts.voice, lang: opts.lang, speed: opts.speed, steps: opts.steps }),
  });
  if (!r.ok) {
    let d = ""; try { d = (await r.text()).slice(0, 300); } catch {}
    throw new Error("engine TTS HTTP " + r.status + (d ? ": " + d : ""));
  }
  return { buf: Buffer.from(await r.arrayBuffer()), type: r.headers.get("content-type") || "audio/wav" };
}

/** Transkripsi STT via sidecar. `audioWav` = byte WAV; `lang` mis. "id"/"auto". */
export async function sttViaEngine(audioWav: Buffer, lang: string, sttModelName = "base"): Promise<string> {
  await ensureSttModel(sttModelName);
  // Bila sidecar dinyalakan tanpa --stt-model (model belum ada saat start),
  // restart sekali supaya memuat model yang baru diunduh.
  if (!(await health())) await startEngine(sttModelName);
  else await ensureSidecarHasStt(sttModelName);
  const r = await fetch(`${BASE}/stt`, {
    method: "POST",
    headers: { "X-Lang": lang || "auto" },
    body: new Uint8Array(audioWav),
  });
  if (!r.ok) {
    let d = ""; try { d = (await r.text()).slice(0, 300); } catch {}
    throw new Error("engine STT HTTP " + r.status + (d ? ": " + d : ""));
  }
  const j: any = await r.json();
  return String(j?.text || "");
}

/** Cek /health apakah stt aktif; bila belum, restart sidecar dengan --stt-model. */
async function ensureSidecarHasStt(sttModelName: string): Promise<void> {
  try {
    const r = await fetch(`${BASE}/health`);
    const j: any = await r.json();
    if (j?.stt === true) return;
  } catch {}
  stopEngine();
  await new Promise((r) => setTimeout(r, 300));
  await startEngine(sttModelName);
}

/** Daftar voice style TTS yang tersedia (untuk katalog UI). */
export async function engineTtsVoices(): Promise<string[]> {
  if (!(await startEngine())) return [];
  try {
    const r = await fetch(`${BASE}/voices`);
    const j: any = await r.json();
    return Array.isArray(j?.voices) ? j.voices : [];
  } catch { return []; }
}

export function engineAvailable(): boolean {
  return findEngineExe() !== null;
}
