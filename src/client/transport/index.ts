/**
 * client/transport — SEAM TUNGGAL komunikasi frontend↔backend.
 *
 * Sebelumnya ±60 pemanggilan `/api/*` tersebar di 6+ file dengan 5 pola derivasi
 * origin berbeda. Modul ini menyatukannya di satu tempat supaya, saat backend
 * pindah dari server HTTP Bun ke Rust core (Tauri IPC), call-site tidak perlu
 * tahu transport mana yang dipakai. Lihat docs/ARCHITECTURE-TAURI-RUST.md §3.
 *
 * Dua backend:
 *   - HttpTransport  : fetch/SSE ke server HTTP (mode kompatibilitas & default
 *                      saat berjalan di browser biasa / dev).
 *   - TauriTransport : invoke() + event (primer desktop) — diaktifkan hanya bila
 *                      berjalan di dalam jendela Tauri (window.__TAURI__ ada) DAN
 *                      command yang diminta sudah tersedia (progresif per stage).
 *
 * Stage 1a: HttpTransport berperilaku IDENTIK dengan kode lama (derivasi origin
 * sama persis dengan guard test-api-origin.js). TauriTransport masih deteksi +
 * fallback — belum ada command yang dipindah. Migrasi call-site & pengalihan ke
 * Tauri terjadi bertahap di stage berikutnya.
 */

/** True bila berjalan di dalam jendela Tauri (withGlobalTauri). */
export function hasTauri(): boolean {
  return typeof (globalThis as any).__TAURI__ !== "undefined";
}

/**
 * Panggil command Tauri (invoke) bila berjalan di dalam shell Tauri; kembalikan
 * `undefined` bila tidak (caller fallback ke HTTP). Jalur IPC primer desktop —
 * command didaftarkan di agent-shell/src/main.rs (Stage 1b: `app_info`).
 */
export async function tauriInvoke<T = unknown>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T | undefined> {
  const t = (globalThis as any).__TAURI__;
  const invoke = t?.core?.invoke ?? t?.invoke;
  if (typeof invoke !== "function") return undefined;
  return (await invoke(cmd, args)) as T;
}

/**
 * Origin backend HTTP. DERIVED, bukan literal — halaman selalu disajikan oleh
 * server yang sama, jadi location.origin benar by construction. Literal hanya
 * bertahan sebagai fallback file:// (buka index.html langsung). Pola ini WAJIB
 * sama dengan static/js/app.js & src/client/agent/brain.ts (dijaga
 * test/legacy/test-api-origin.js).
 */
export function apiBase(): string {
  return typeof location !== "undefined" && /^https?:$/.test(location.protocol)
    ? location.origin
    : "http://127.0.0.1:8310";
}

/** URL absolut untuk sebuah path API relatif ("/api/...", "/model/..."). */
export function apiUrl(path: string): string {
  if (/^https?:\/\//.test(path)) return path;
  const base = apiBase();
  return path.startsWith("/") ? base + path : base + "/" + path;
}

/**
 * fetch terpusat ke backend HTTP. Semua call-site frontend sebaiknya lewat sini
 * (atau `api.*` di bawah) alih-alih `fetch("/api/..")` langsung, supaya satu
 * titik pengalihan saat pindah ke Tauri IPC.
 */
export function apiFetch(path: string, init?: RequestInit): Promise<Response> {
  return fetch(apiUrl(path), init);
}

/**
 * Fasad transport. Stage 1a: semua metode delegasi ke HTTP (perilaku lama).
 * Metode `call/stream/subscribe` adalah bentuk akhir (coarse-grained) yang akan
 * dipetakan ke Tauri command/Channel/event di stage berikutnya — tanda tangannya
 * sudah stabil supaya call-site tak berubah lagi setelah diadopsi.
 */
export interface AppInfo {
  shell_version: string;
  core_version: string;
  core_ready: boolean;
}

export const transport = {
  hasTauri,
  apiBase,
  apiUrl,
  fetch: apiFetch,
  invoke: tauriInvoke,

  /** Info aplikasi via IPC Tauri (command app_info); undefined di luar shell. */
  appInfo(): Promise<AppInfo | undefined> {
    return tauriInvoke<AppInfo>("app_info");
  },

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
};

export type Transport = typeof transport;
