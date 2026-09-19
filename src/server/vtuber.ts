/**
 * server/vtuber.ts — Runtime konektor + otak behavior mode AI VTuber.
 * Single active runtime: start() menghancurkan runtime lama dulu.
 *
 * Provider:
 *  - mock    : simulator penonton + donasi (tanpa API key, untuk tes)
 *  - twitch  : baca live chat via IRC WebSocket (wss://irc-ws.chat.twitch.tv:443)
 *              tanpa token = anonymous read-only (justinfan); token opsional.
 *  - youtube : poll liveChatMessages.list (API key + videoId live);
 *              superChat → event donasi.
 *
 * Behavior (dedup/cooldown audience, antrean donation FIFO-20, antrean
 * operator + precedence, LLM balasan) hidup DI SINI via vtuber-scheduler —
 * satu scheduler untuk semua klien (app utama + overlay OBS hanya render
 * feed dan memutar balasan; race dobel-balasan mati dari akarnya).
 */
import { WebSocket } from "ws";
import { createVtuberScheduler, type VtuberScheduler } from "./vtuber-scheduler";
import { llmForRole } from "../shared/llm-client";
import { ConfigManager } from "../shared/config";
import { appRoot } from "../shared/paths";
import { join } from "path";

// Instans config sendiri (file yang sama dengan index.ts; tulisan lewat
// queueJsonWrite modul-level jadi tetap terserialisasi).
const config = new ConfigManager(join(appRoot(), "data"));

export type VtEvent = {
  id: number;
  ts: number;
  type: "chat" | "donation" | "system" | "agent";
  user: string;
  text: string;
  amount?: string;
};

type Runtime = {
  cfg: any;
  events: VtEvent[];
  scheduler: VtuberScheduler | null;
  destroy: () => void;
};

let runtime: Runtime | null = null;
let nextId = 1;

function pushEvent(e: Omit<VtEvent, "id" | "ts">): VtEvent {
  if (!runtime) throw new Error("vtuber runtime tidak aktif");
  const ev: VtEvent = { id: nextId++, ts: Date.now(), ...e };
  runtime.events.push(ev);
  if (runtime.events.length > 500) runtime.events.splice(0, runtime.events.length - 500);
  // Intake behavior (§7): hanya event penonton/donasi — "agent"/"system"
  // adalah balasan/feedback, bukan input scheduler.
  if (runtime.scheduler && (e.type === "chat" || e.type === "donation")) {
    const r = runtime.scheduler.ingest({
      type: e.type === "chat" ? "audience" : "donation",
      user: ev.user,
      text: ev.text,
      amount: ev.amount,
    });
    if (!r.accepted && r.reason && r.reason !== "cooldown" && r.reason !== "duplikat" && r.reason !== "respond chat mati" && r.reason !== "respond donasi mati")
      console.log("[vtuber] event", e.type, "tidak masuk scheduler:", r.reason);
  }
  return ev;
}

export function vtuberStatus() {
  const q = runtime?.scheduler?.stats();
  return {
    running: !!runtime,
    provider: runtime?.cfg?.provider || null,
    channel: runtime?.cfg?.channel || runtime?.cfg?.videoId || null,
    respond: runtime?.cfg?.respondChat ?? false,
    eventCount: runtime?.events.length || 0,
    queues: q || { active: null, donationQueue: 0, operatorQueue: 0 },
  };
}

export function vtuberEvents(since: number): { events: VtEvent[]; cursor: number } {
  if (!runtime) return { events: [], cursor: since };
  const events = runtime.events.filter((e) => e.id > since);
  return { events, cursor: nextId - 1 };
}

export function vtuberInjectEvent(body: { type?: string; user?: string; text?: string; amount?: string }): VtEvent | null {
  if (!runtime) return null;
  // "agent" diizinkan: mock-event adalah pintu resmi client menandai balasan
  // AI (kelas .agent di Feed Live) — jangan dipaksa jadi "chat".
  const type =
    body.type === "donation" || body.type === "agent"
      ? body.type
      : "chat";
  return pushEvent({ type, user: String(body.user || "Guest"), text: String(body.text || "").slice(0, 400), amount: body.amount });
}

export function vtuberAgentSay(text: string): VtEvent | null {
  if (!runtime) return null;
  return pushEvent({ type: "agent", user: "AI", text: String(text || "").slice(0, 500) });
}

export function vtuberStop() {
  if (runtime) {
    try { runtime.scheduler?.stop(); } catch {}
    try { runtime.destroy(); } catch {}
    runtime = null;
  }
  return { ok: true };
}

/** Ubah config behavior JALAN (tanpa restart stream): persona, cooldown,
 *  flag respond — dipanggil form klien saat runtime hidup. */
export function vtuberSetConfig(partial: {
  persona?: string;
  cooldownMs?: number;
  respondChat?: boolean;
  respondDonation?: boolean;
}): { ok: boolean } {
  runtime?.scheduler?.setConfig(partial ?? {});
  return { ok: !!runtime };
}

/** Instruksi operator stream (§7 kelas sendiri): echo ke feed + masuk
 *  antrean operator. Operator = aksi eksplisit — tidak tergantung flag respond. */
export function vtuberOperatorSay(body: { text?: string }): { ok: boolean; error?: string } {
  if (!runtime) return { ok: false, error: "vtuber runtime tidak aktif" };
  const text = String(body?.text || "").slice(0, 400).trim();
  if (!text) return { ok: false, error: "teks instruksi kosong" };
  pushEvent({ type: "system", user: "operator", text: "Operator: " + text });
  runtime.scheduler?.ingest({ type: "operator", user: "operator", text });
  return { ok: true };
}

export function vtuberStart(cfg: any): { ok: boolean; error?: string } {
  vtuberStop();
  const provider = String(cfg?.provider || "mock");
  const rt: Runtime = { cfg, events: [], scheduler: null, destroy: () => {} };
  const timers: any[] = [];
  let ws: WebSocket | null = null;

  // Otak behavior (§7): LLM role "chat" — pola yang sama dengan quip/narator.
  // Flag respond default MATI: tanpa persetujuan eksplisit form, tidak ada
  // panggilan LLM (test lama tetap bebas jaringan).
  rt.scheduler = createVtuberScheduler({
    llm: (messages, system) =>
      llmForRole(
        "chat",
        () => config.connections,
        () => config.activeConnection,
        (conns) => config.saveConnections(conns, config.load().activeId),
        messages as import("../shared/types").ChatMessage[],
        system,
      ).then((r: any) => r.reply),
    emitAgent: (text) => {
      if (runtime === rt) pushEvent({ type: "agent", user: "AI", text });
    },
    emitSystem: (text) => {
      if (runtime === rt) pushEvent({ type: "system", user: "system", text });
    },
    log: (m) => console.log("[vtuber]", m),
  });
  rt.scheduler.setConfig({
    persona: cfg?.persona,
    cooldownMs: cfg?.cooldownMs,
    respondChat: !!cfg?.respondChat,
    respondDonation: !!cfg?.respondDonation,
  });

  const push = (e: Omit<VtEvent, "id" | "ts">) => {
    if (runtime === rt) pushEvent(e);
  };

  try {
    if (provider === "mock") {
      const names = ["Rian", "Sinta", "Budi", "Ayu", "Kevin", "Nadia", "Fajar", "Tania", "Yoga", "Melati"];
      const chats = [
        "Halo semua!", "Kamu dari mana?", "Suara lucu banget 😆", "Sedih banget lagunya",
        "Main game dong!", "Jam berapa stream selesai?", "Keren sih karakternya",
        "Ada yang tahu cara donasi?", "Request lagu boleh?", "Hari ini ngapain aja?",
      ];
      const donors = ["Rian", "Kevin", "Ayu", "Fajar"];
      let tick = 0;
      const timer = setInterval(() => {
        if (runtime !== rt) return;
        tick++;
        if (tick % 5 === 0) {
          const amount = ["Rp 10.000", "Rp 25.000", "Rp 50.000", "Rp 100.000"][Math.floor(Math.random() * 4)];
          push({ type: "donation", user: donors[Math.floor(Math.random() * donors.length)], text: "Dukung terus streamnya!", amount });
        } else {
          push({ type: "chat", user: names[Math.floor(Math.random() * names.length)], text: chats[Math.floor(Math.random() * chats.length)] });
        }
      }, Math.max(3000, Number(cfg?.mockIntervalMs) || 6000));
      timers.push(timer);
      push({ type: "system", user: "system", text: "Mode mock aktif — penonton simulasi (tanpa API key)." });
    } else if (provider === "twitch") {
      const channel = String(cfg?.channel || "").trim().toLowerCase().replace(/^#/, "");
      if (!channel) return { ok: false, error: "nama channel Twitch wajib diisi" };
      const token = String(cfg?.apiKey || "").trim();
      const nick = token ? String(cfg?.nick || "justinfan12345").toLowerCase() : "justinfan" + Math.floor(10000 + Math.random() * 89999);
      ws = new WebSocket("wss://irc-ws.chat.twitch.tv:443");
      let closed = false;
      ws.addEventListener("open", () => {
        ws!.send("CAP REQ :twitch.tv/tags");
        if (token) ws!.send("PASS oauth:" + token.replace(/^oauth:/i, ""));
        ws!.send("NICK " + nick);
        ws!.send("JOIN #" + channel);
        push({ type: "system", user: "system", text: "Terhubung ke Twitch #" + channel + (token ? " (auth)" : " (anonim)") });
      });
      ws.addEventListener("message", (ev: any) => {
        const raw = String(ev.data || "");
        for (const line of raw.split("\r\n")) {
          if (!line) continue;
          if (line.startsWith("PING")) { ws!.send("PONG :tmi.twitch.tv"); continue; }
          if (line.includes(" PRIVMSG ")) {
            // @tags :nick!nick@nick.tmi.twitch.tv PRIVMSG #chan :pesan
            let user = "";
            let text = "";
            let rest = line;
            if (rest.startsWith("@")) { const sp = rest.indexOf(" "); rest = sp >= 0 ? rest.slice(sp + 1) : rest; }
            const m = /^:([^!\s]+)![^\s]*\sPRIVMSG\s+#[^\s]+\s+:(.*)$/.exec(rest);
            if (m) { user = m[1]; text = m[2]; }
            if (rest.startsWith("@badge")) {
              const dm = /display-name=([^;]*)/.exec(line);
              if (dm && dm[1]) user = dm[1];
            }
            if (user && text) push({ type: "chat", user, text: text.slice(0, 400) });
          } else if (line.includes("Login authentication failed") || line.includes("Improperly formatted auth")) {
            push({ type: "system", user: "system", text: "Auth Twitch gagal — cek token/nick. Coba tanpa token (anonim)." });
          }
        }
      });
      ws.addEventListener("close", () => {
        if (closed || runtime !== rt) return;
        push({ type: "system", user: "system", text: "Koneksi Twitch tertutup. Mencoba ulang 5 dtk…" });
        const t = setTimeout(() => { if (runtime === rt) { const c = cfg; vtuberStart(c); } }, 5000);
        timers.push(t);
      });
      ws.addEventListener("error", () => {
        push({ type: "system", user: "system", text: "Gagal terhubung ke Twitch IRC." });
      });
      rt.destroy = () => { closed = true; try { ws?.close(); } catch {} timers.forEach(clearTimeout); };
    } else if (provider === "youtube") {
      const videoId = String(cfg?.videoId || "").trim();
      const key = String(cfg?.apiKey || "").trim();
      if (!videoId || !key) return { ok: false, error: "videoId live + API key YouTube wajib diisi" };
      let liveChatId = "";
      let pageToken = "";
      let stopped = false;
      const base = "https://www.googleapis.com/youtube/v3";
      (async () => {
        try {
          const r = await fetch(`${base}/videos?part=liveStreamingDetails&id=${encodeURIComponent(videoId)}&key=${key}`);
          const j: any = await r.json();
          liveChatId = j?.items?.[0]?.liveStreamingDetails?.activeLiveChatId || "";
          if (!liveChatId) {
            push({ type: "system", user: "system", text: "liveChatId tidak ditemukan — pastikan video sedang live dan chat aktif." });
            return;
          }
          push({ type: "system", user: "system", text: "Terhubung ke live chat YouTube." });
        } catch (e: any) {
          push({ type: "system", user: "system", text: "Gagal ambil liveChatId: " + e.message });
          return;
        }
        const poll = async () => {
          while (!stopped && runtime === rt && liveChatId) {
            try {
              const r = await fetch(`${base}/liveChat/messages?liveChatId=${encodeURIComponent(liveChatId)}&part=snippet,authorDetails&pageToken=${encodeURIComponent(pageToken)}&key=${key}`);
              const j: any = await r.json();
              if (j.error) { push({ type: "system", user: "system", text: "YouTube API: " + (j.error.message || r.status) }); break; }
              for (const it of j.items || []) {
                const sn = it.snippet || {};
                const user = it.authorDetails?.displayName || sn.authorChannelId || "?";
                if (sn.type === "superChatEvent" && sn.superChatDetails) {
                  const amt = (Number(sn.superChatDetails.amountMicros) / 1e6).toFixed(0);
                  push({ type: "donation", user, text: sn.superChatDetails.userComment || "", amount: amt + " " + (sn.superChatDetails.currency || "") });
                } else if (sn.type === "textMessageEvent" && sn.textMessageDetails?.messageText) {
                  push({ type: "chat", user, text: String(sn.textMessageDetails.messageText).slice(0, 400) });
                } else if (sn.type === "superStickerEvent" && sn.superStickerDetails) {
                  push({ type: "donation", user, text: "[super sticker]", amount: (Number(sn.superStickerDetails.amountMicros) / 1e6).toFixed(0) + " " + (sn.superStickerDetails.currency || "") });
                }
              }
              pageToken = j.nextPageToken || pageToken;
              var wait = Math.max(5000, Number(j.pollingIntervalMillis) || 5000);
            } catch (e: any) {
              push({ type: "system", user: "system", text: "Poll YouTube gagal: " + e.message });
              var wait = 10000;
            }
            await new Promise((res) => setTimeout(res, wait));
          }
        };
        poll();
      })();
      rt.destroy = () => { stopped = true; timers.forEach(clearTimeout); };
    } else {
      return { ok: false, error: "provider tidak dikenal: " + provider };
    }

    runtime = rt;
    return { ok: true };
  } catch (e: any) {
    try { rt.destroy(); } catch {}
    return { ok: false, error: e.message };
  }
}

// ── Overlay OBS ──────────────────────────────────────────────────────────────
// Halaman vtuber.html (Browser Source OBS) mengirim heartbeat berkala. Selama
// heartbeat segar, app utama menahan balasan otomatisnya supaya balasan tidak
// dobel (di app utama dan di overlay). Timestamp 0 = belum pernah nyambung.
let overlayBeat = 0;

export function overlayPing(): { ok: boolean; active: boolean } {
  overlayBeat = Date.now();
  return { ok: true, active: true };
}

export function overlayActive(): boolean {
  return overlayBeat > 0 && Date.now() - overlayBeat < 8000;
}
