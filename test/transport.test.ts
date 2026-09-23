// transport.test.ts — seam komunikasi frontend↔backend (satu binary).
// Menjaga:
// - dev/browser: derivasi origin (http/https → location.origin; selain itu →
//   fallback literal 8310) — kontrak guard test-api-origin.js.
// - embedded (shell Companion): loopback proses-sendiri (port via IPC
//   server_port, default 8310) + domain termigrasi via IPC (modeGet/modeSet,
//   coreVersion) dengan jembatan HTTP sementara.
import { test, expect, afterEach } from "bun:test";
import {
  apiBase,
  apiUrl,
  isEmbedded,
  initLoopback,
  httpBase,
  _resetLoopbackForTest,
} from "../src/client/transport";

const g = globalThis as any;
afterEach(() => {
  delete g.location;
  delete g.__TAURI__;
  _resetLoopbackForTest();
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

test("isEmbedded: false tanpa __TAURI__, true di shell", () => {
  expect(isEmbedded()).toBe(false);
  g.__TAURI__ = {};
  expect(isEmbedded()).toBe(true);
});

test("embedded: httpBase default loopback:8310 sebelum init", () => {
  g.__TAURI__ = {};
  g.location = { protocol: "https:", origin: "https://tauri.localhost" };
  expect(httpBase()).toBe("http://127.0.0.1:8310");
});

test("embedded: initLoopback membaca port dari IPC server_port", async () => {
  g.__TAURI__ = {
    core: { invoke: async (cmd: string) => (cmd === "server_port" ? 8317 : undefined) },
  };
  g.location = { protocol: "https:", origin: "https://tauri.localhost" };
  expect(await initLoopback()).toBe(8317);
  expect(httpBase()).toBe("http://127.0.0.1:8317");
  expect(apiBase()).toBe("http://127.0.0.1:8317");
});

test("embedded: initLoopback gagal → null, base tetap default", async () => {
  g.__TAURI__ = {
    core: {
      invoke: async () => {
        throw new Error("denied");
      },
    },
  };
  expect(await initLoopback()).toBeNull();
  expect(httpBase()).toBe("http://127.0.0.1:8310");
});

test("modeGet/modeSet: embedded via IPC", async () => {
  const seen: any[] = [];
  g.__TAURI__ = {
    core: {
      invoke: async (cmd: string, args: any) => {
        seen.push([cmd, args]);
        if (cmd === "get_mode") return { active: "stage" };
        if (cmd === "set_mode") return { ok: true };
        return undefined;
      },
    },
  };
  const { transport } = await import("../src/client/transport");
  expect(await transport.modeGet()).toEqual({ active: "stage" });
  expect(await transport.modeSet("vtuber")).toEqual({ ok: true });
  expect(seen).toEqual([
    ["get_mode", undefined],
    ["set_mode", { mode: "vtuber" }],
  ]);
});

test("coreVersion: embedded via IPC", async () => {
  g.__TAURI__ = { invoke: async (cmd: string) => (cmd === "core_version" ? "0.1.0" : undefined) };
  const { transport } = await import("../src/client/transport");
  expect(await transport.coreVersion()).toBe("0.1.0");
});

test("transport: permukaan seam lengkap", async () => {
  const { transport } = await import("../src/client/transport");
  for (const k of ["fetch", "getJson", "postJson", "invoke", "modeGet", "modeSet", "coreVersion", "initLoopback", "httpBase", "isEmbedded"]) {
    expect(typeof (transport as any)[k], k).toBe("function");
  }
});
