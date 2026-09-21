// transport.test.ts — seam komunikasi frontend↔backend (Stage 1a migrasi Tauri).
// Menjaga: derivasi origin identik dengan kontrak guard test-api-origin.js
// (http/https → location.origin; selain itu → fallback literal 8310).
import { test, expect, afterEach } from "bun:test";
import { apiBase, apiUrl, hasTauri } from "../src/client/transport";

const g = globalThis as any;
afterEach(() => {
  delete g.location;
  delete g.__TAURI__;
});

test("apiBase: http → location.origin", () => {
  g.location = { protocol: "http:", origin: "http://127.0.0.1:8312" };
  expect(apiBase()).toBe("http://127.0.0.1:8312");
});

test("apiBase: https → location.origin", () => {
  g.location = { protocol: "https:", origin: "https://lumi.local" };
  expect(apiBase()).toBe("https://lumi.local");
});

test("apiBase: file:// → fallback literal 8310 (dokumented)", () => {
  g.location = { protocol: "file:", origin: "null" };
  expect(apiBase()).toBe("http://127.0.0.1:8310");
});

test("apiBase: origin diturunkan pada port non-8310 (bukan literal)", () => {
  // Server bisa jalan di port mana pun; seam harus ikut, bukan hardcode 8310.
  g.location = { protocol: "http:", origin: "http://127.0.0.1:9999" };
  expect(apiBase()).toBe("http://127.0.0.1:9999");
});

test("apiUrl: gabung path relatif ke base", () => {
  g.location = { protocol: "http:", origin: "http://127.0.0.1:8310" };
  expect(apiUrl("/api/config")).toBe("http://127.0.0.1:8310/api/config");
  expect(apiUrl("api/config")).toBe("http://127.0.0.1:8310/api/config");
});

test("apiUrl: URL absolut dibiarkan apa adanya", () => {
  g.location = { protocol: "http:", origin: "http://127.0.0.1:8310" };
  expect(apiUrl("https://cdn.example/x.png")).toBe("https://cdn.example/x.png");
});

test("hasTauri: false tanpa window.__TAURI__, true dengan", () => {
  expect(hasTauri()).toBe(false);
  g.__TAURI__ = {};
  expect(hasTauri()).toBe(true);
});

test("tauriInvoke: undefined tanpa shell Tauri (caller fallback HTTP)", async () => {
  const { tauriInvoke } = await import("../src/client/transport");
  expect(await tauriInvoke("app_info")).toBeUndefined();
});

test("tauriInvoke: meneruskan ke __TAURI__.core.invoke bila ada", async () => {
  const { tauriInvoke } = await import("../src/client/transport");
  let seen: any = null;
  g.__TAURI__ = { core: { invoke: async (cmd: string, args: any) => { seen = { cmd, args }; return { ok: true }; } } };
  const r = await tauriInvoke<{ ok: boolean }>("app_info", { a: 1 });
  expect(seen).toEqual({ cmd: "app_info", args: { a: 1 } });
  expect(r).toEqual({ ok: true });
});
