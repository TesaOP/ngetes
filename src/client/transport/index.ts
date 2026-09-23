/**
 * client/transport — SEAM TUNGGAL komunikasi frontend↔backend.
 *
 * Arsitektur (satu binary, satu proses):
 * - Produksi: halaman di-embed di Companion.exe (origin lokal tauri.localhost).
 *   Domain yang SUDAH migrasi → perintah IPC (`invoke`) langsung ke logika
 *   `live2d_core` dalam proses yang sama (tanpa HTTP). Domain yang BELUM →
 *   HTTP loopback proses-sendiri (adapter yang sama melayani CLI/OBS/dev).
 * - Dev browser (`cargo run -p live2d-core` + browser): tanpa __TAURI__ →
 *   semuanya HTTP same-origin.
 *
 * Aturan: tiap domain yang migrasi IPC memakai fungsi bernama di sini
 * (modeGet/modeSet/…) dengan HTTP sebagai jembatan transisi (+ console.warn)
 * sampai migrasinya terbukti — lalu jembatan dicabut per-domain. JANGAN
 * menambah fetch mentah ke domain yang sudah punya helper.
 */

/**
 * Origin backend HTTP. DERIVED, bukan literal — dev: halaman disajikan server
 * yang sama (location.origin benar); embedded: loopback proses-sendiri
 * (port via initLoopback, default 8310). Literal hanya fallback file://.
 * Pola ini WAJIB sama dengan static/js/app.js (dijaga
 * test/legacy/test-api-origin.js).
 */
export function apiBase(): string {
  return httpBase();
}

let loopPort: number | null = null;
let loopInit: Promise<number | null> | null = null;

/** True bila berjalan di dalam shell Companion (withGlobalTauri). */
export function isEmbedded(): boolean {
  return typeof (globalThis as any).__TAURI__ !== "undefined";
}

/**
 * Temukan port loopback (sekali, di-cache). Embedded: tanya command
 * `server_port` ke shell; dev: null (= pakai location.origin).
 * Aman dipanggil berkali-kali; tak pernah melempar.
 */
export function initLoopback(): Promise<number | null> {
  if (!loopInit) {
    loopInit = (async () => {
      try {
        const t = (globalThis as any).__TAURI__;
        const invoke = t?.core?.invoke ?? t?.invoke;
        if (typeof invoke !== "function") return null;
        const p = await invoke("server_port");
        if (typeof p === "number" && p > 0 && p < 65536) {
          loopPort = p;
          return p;
        }
        return null;
      } catch {
        return null;
      }
    })();
  }
  return loopInit;
}

/** Basis HTTP sinkron. Embedded → loopback (port cache/default); dev → origin. */
export function httpBase(): string {
  if (isEmbedded()) return "http://127.0.0.1:" + (loopPort ?? 8310);
  return typeof location !== "undefined" && /^https?:$/.test(location.protocol)
    ? location.origin
    : "http://127.0.0.1:8310";
}

/** Panggil command IPC; `undefined` di luar shell / bila gagal. */
export async function invoke<T = unknown>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T | undefined> {
  try {
    const t = (globalThis as any).__TAURI__;
    const fn = t?.core?.invoke ?? t?.invoke;
    if (typeof fn !== "function") return undefined;
    return (await fn(cmd, args)) as T;
  } catch {
    return undefined;
  }
}

/** URL absolut untuk sebuah path API relatif ("/api/...", "/model/..."). */
export function apiUrl(path: string): string {
  if (/^https?:\/\//.test(path)) return path;
  const base = httpBase();
  return path.startsWith("/") ? base + path : base + "/" + path;
}

/**
 * fetch terpusat ke backend HTTP. Semua call-site frontend lewat sini
 * (atau getJson/postJson) alih-alih `fetch("/api/..")` langsung.
 */
export function apiFetch(path: string, init?: RequestInit): Promise<Response> {
  return fetch(apiUrl(path), init);
}

/** Fasad transport — HTTP (adapter) + helper IPC per-domain yang termigrasi. */
export const transport = {
  apiBase,
  apiUrl,
  fetch: apiFetch,
  isEmbedded,
  initLoopback,
  httpBase,
  invoke,

  /** GET JSON. */
  async getJson<T = any>(path: string, init?: RequestInit): Promise<T> {
    const r = await apiFetch(path, init);
    if (!r.ok) throw new Error("HTTP " + r.status + " " + path);
    return (await r.json()) as T;
  },

  /** POST JSON → JSON. */
  async postJson<T = any>(path: string, body: unknown, init?: RequestInit): Promise<T> {
    const r = await apiFetch(path, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
      ...init,
    });
    if (!r.ok) throw new Error("HTTP " + r.status + " " + path);
    return (await r.json()) as T;
  },

  /**
   * GET /api/mode — domain MODE (migrasi IPC #1). Embedded → command
   * `get_mode` (direct core); dev → HTTP. Jembatan HTTP sementara sampai
   * migrasi terbukti (lihat console).
   */
  async modeGet<T = any>(): Promise<T> {
    if (isEmbedded()) {
      const r = await invoke<T>("get_mode");
      if (r !== undefined) return r;
      console.warn("[transport] get_mode IPC gagal — jembatan HTTP (transisi)");
    }
    return this.getJson<T>("/api/mode");
  },

  /**
   * POST /api/mode — domain MODE (migrasi IPC #1). Embedded → command
   * `set_mode`; dev → HTTP. Jembatan HTTP sementara (lihat console).
   */
  async modeSet<T = any>(mode: string): Promise<T> {
    if (isEmbedded()) {
      try {
        const r = await invoke<T>("set_mode", { mode });
        if (r !== undefined) return r;
      } catch (e) {
        console.warn("[transport] set_mode IPC gagal — jembatan HTTP (transisi):", (e as Error)?.message ?? e);
      }
    }
    return this.postJson<T>("/api/mode", { mode });
  },

  /**
   * Versi core — Embedded → command `core_version`; dev → HTTP /api/version.
   * Non-blokir (indikator judul saja).
   */
  async coreVersion(): Promise<string | undefined> {
    if (isEmbedded()) {
      const v = await invoke<string>("core_version");
      if (typeof v === "string" && v) return v;
    }
    try {
      const r = await this.getJson<{ core_version?: string }>("/api/version");
      return r?.core_version;
    } catch {
      return undefined;
    }
  },
};

export type Transport = typeof transport;

/** Reset cache loopback — HANYA untuk unit test. */
export function _resetLoopbackForTest(): void {
  loopPort = null;
  loopInit = null;
}
