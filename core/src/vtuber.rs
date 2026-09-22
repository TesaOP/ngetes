//! vtuber.rs — Runtime konektor + otak behavior mode AI VTuber, port dari
//! `src/server/vtuber.ts`. Single active runtime: `start()` menghancurkan
//! runtime lama dulu (bump epoch → task lama mati sendiri).
//!
//! Provider yang diport batch ini: **mock** (simulator penonton + donasi, tanpa
//! API key) — jalur verifikasi headless. Provider `twitch` (IRC WS) & `youtube`
//! (poll) BELUM diport ke core (butuh WS client + kunci live) → `start` menolak
//! dengan pesan eksplisit; menyusul di batch berikut.
//!
//! Behavior (dedup/cooldown audience, antrean donation/operator FIFO-20 +
//! precedence, LLM balasan) hidup DI SINI via `vtuber_scheduler` — satu
//! scheduler untuk semua klien (app utama + overlay OBS hanya render feed &
//! memutar balasan; race dobel-balasan mati dari akarnya).

use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};

use crate::llm::{self, ChatMessage};
use crate::vtuber_scheduler::{
    speak_ms, ConfigPatch, QueueItem, Scheduler, SchedulerClass, SchedulerEvent,
};

const MAX_EVENTS: usize = 500;

#[derive(Clone)]
struct VtEvent {
    id: u64,
    ts: i64,
    kind: String, // "chat" | "donation" | "system" | "agent"
    user: String,
    text: String,
    amount: Option<String>,
}

impl VtEvent {
    fn to_json(&self) -> Value {
        let mut o = json!({
            "id": self.id,
            "ts": self.ts,
            "type": self.kind,
            "user": self.user,
            "text": self.text,
        });
        if let Some(a) = &self.amount {
            o["amount"] = json!(a);
        }
        o
    }
}

struct VtRuntime {
    epoch: u64,
    provider: String,
    cfg: Value,
    events: Vec<VtEvent>,
    next_id: u64,
    scheduler: Scheduler,
    config_path: PathBuf,
}

fn rt() -> &'static Mutex<Option<VtRuntime>> {
    static R: OnceLock<Mutex<Option<VtRuntime>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(None))
}

fn epoch_counter() -> &'static AtomicU64 {
    static E: OnceLock<AtomicU64> = OnceLock::new();
    E.get_or_init(|| AtomicU64::new(0))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Dorong event ke feed (trim ke MAX_EVENTS). Return JSON event.
/// TIDAK menyentuh scheduler — dipakai untuk agent/system dan sebagai primitif
/// oleh `ingest_feed`.
fn push_feed(rt: &mut VtRuntime, kind: &str, user: &str, text: &str, amount: Option<String>) -> Value {
    let ev = VtEvent {
        id: rt.next_id,
        ts: now_ms(),
        kind: kind.to_string(),
        user: user.to_string(),
        text: text.to_string(),
        amount,
    };
    rt.next_id += 1;
    let out = ev.to_json();
    rt.events.push(ev);
    if rt.events.len() > MAX_EVENTS {
        let drop = rt.events.len() - MAX_EVENTS;
        rt.events.drain(0..drop);
    }
    out
}

/// Event penonton/donasi: masuk feed LALU scheduler (§7 intake). Menyerap
/// feedback sistem (antrean penuh) sebagai event feed, dan mengembalikan item
/// audience yang harus langsung dijalankan (run_now) bila ada.
fn ingest_feed(rt: &mut VtRuntime, kind: &str, user: &str, text: &str, amount: Option<String>) -> (Value, Option<QueueItem>) {
    let out = push_feed(rt, kind, user, text, amount.clone());
    let mut run_now = None;
    if kind == "chat" || kind == "donation" {
        let class = if kind == "chat" { SchedulerClass::Audience } else { SchedulerClass::Donation };
        let (_r, item) = rt.scheduler.ingest(SchedulerEvent {
            class,
            user: user.to_string(),
            text: text.to_string(),
            amount,
        });
        run_now = item;
        for msg in rt.scheduler.drain_system() {
            push_feed(rt, "system", "system", &msg, None);
        }
    }
    (out, run_now)
}

/// Jalankan satu item (LLM role "chat") lalu tahan slot selama estimasi bicara,
/// finish, dan tarik item berikut dari antrean (drain). Berhenti sendiri bila
/// runtime sudah diganti (epoch beda) atau dihentikan.
fn spawn_run(epoch: u64, item: QueueItem) {
    tokio::spawn(async move {
        // Rakit prompt + system + config_path di bawah lock (lepas sebelum await).
        let prepared = {
            let mut g = rt().lock().unwrap();
            match g.as_mut() {
                Some(r) if r.epoch == epoch && !r.scheduler.is_stopped() => {
                    Some((r.scheduler.build_prompt(&item), r.scheduler.system_prompt(), r.config_path.clone()))
                }
                _ => None,
            }
        };
        let Some((prompt, system, config_path)) = prepared else { return };

        let messages = [ChatMessage { role: "user".into(), content: prompt }];
        let result = llm::llm_for_role(&config_path, "chat", &messages, &system).await;

        // Emit balasan / feedback error, lalu tahan slot.
        let reply: String = match result {
            Ok(ok) => ok.reply.trim().chars().take(500).collect(),
            Err((_, msg)) => {
                let feedback = format!("AI gagal membalas {}: {}", item.class.as_str(), msg.chars().take(120).collect::<String>());
                let mut g = rt().lock().unwrap();
                if let Some(r) = g.as_mut() {
                    if r.epoch == epoch {
                        push_feed(r, "system", "system", &feedback, None);
                    }
                }
                String::new()
            }
        };

        let wait = if reply.is_empty() {
            0
        } else {
            let mut g = rt().lock().unwrap();
            if let Some(r) = g.as_mut() {
                if r.epoch == epoch && !r.scheduler.is_stopped() {
                    push_feed(r, "agent", "AI", &reply, None);
                }
            }
            speak_ms(&reply)
        };
        if wait > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(wait as u64)).await;
        }

        // Slot lepas → tarik item berikut (precedence donation>operator).
        let next = {
            let mut g = rt().lock().unwrap();
            match g.as_mut() {
                Some(r) if r.epoch == epoch && !r.scheduler.is_stopped() => {
                    r.scheduler.finish();
                    r.scheduler.try_start()
                }
                _ => None,
            }
        };
        if let Some(n) = next {
            spawn_run(epoch, n);
        }
    });
}

/// Loop simulator penonton (provider mock). Berhenti saat epoch berubah.
fn spawn_mock(epoch: u64, interval_ms: u64) {
    tokio::spawn(async move {
        const NAMES: &[&str] = &["Rian", "Sinta", "Budi", "Ayu", "Kevin", "Nadia", "Fajar", "Tania", "Yoga", "Melati"];
        const CHATS: &[&str] = &[
            "Halo semua!", "Kamu dari mana?", "Suara lucu banget 😆", "Sedih banget lagunya",
            "Main game dong!", "Jam berapa stream selesai?", "Keren sih karakternya",
            "Ada yang tahu cara donasi?", "Request lagu boleh?", "Hari ini ngapain aja?",
        ];
        const DONORS: &[&str] = &["Rian", "Kevin", "Ayu", "Fajar"];
        const AMOUNTS: &[&str] = &["Rp 10.000", "Rp 25.000", "Rp 50.000", "Rp 100.000"];
        let mut tick: u64 = 0;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(interval_ms)).await;
            tick += 1;
            let run_now;
            {
                let mut g = rt().lock().unwrap();
                let Some(r) = g.as_mut() else { return };
                if r.epoch != epoch {
                    return; // runtime diganti/berhenti
                }
                if tick % 5 == 0 {
                    let user = DONORS[pseudo_rand(tick) % DONORS.len()];
                    let amount = AMOUNTS[pseudo_rand(tick.wrapping_mul(7)) % AMOUNTS.len()];
                    let (_o, item) = ingest_feed(r, "donation", user, "Dukung terus streamnya!", Some(amount.to_string()));
                    run_now = item;
                } else {
                    let user = NAMES[pseudo_rand(tick.wrapping_mul(3)) % NAMES.len()];
                    let text = CHATS[pseudo_rand(tick.wrapping_mul(11)) % CHATS.len()];
                    let (_o, item) = ingest_feed(r, "chat", user, text, None);
                    run_now = item;
                }
            }
            if let Some(item) = run_now {
                spawn_run(epoch, item);
            }
        }
    });
}

/// PRNG murah (tanpa dep rand): xorshift dari tick + nanos.
fn pseudo_rand(seed: u64) -> usize {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    let mut x = seed ^ nanos ^ 0x9E3779B97F4A7C15;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x as usize
}

// ── API publik (dipakai handler lib.rs) ────────────────────────────────────

/// Status runtime + antrean scheduler.
pub fn status() -> Value {
    let g = rt().lock().unwrap();
    match g.as_ref() {
        None => json!({
            "running": false,
            "provider": null,
            "channel": null,
            "respond": false,
            "eventCount": 0,
            "queues": { "active": null, "donationQueue": 0, "operatorQueue": 0 },
        }),
        Some(r) => {
            let (active, dq, oq) = r.scheduler.stats();
            let channel = r.cfg.get("channel").and_then(|v| v.as_str())
                .or_else(|| r.cfg.get("videoId").and_then(|v| v.as_str()));
            json!({
                "running": true,
                "provider": r.provider,
                "channel": channel,
                "respond": r.cfg.get("respondChat").and_then(|v| v.as_bool()).unwrap_or(false),
                "eventCount": r.events.len(),
                "queues": {
                    "active": active.map(|c| c.as_str()),
                    "donationQueue": dq,
                    "operatorQueue": oq,
                },
            })
        }
    }
}

/// Event sejak cursor `since` (+ cursor terbaru).
pub fn events(since: u64) -> Value {
    let g = rt().lock().unwrap();
    match g.as_ref() {
        None => json!({ "events": [], "cursor": since }),
        Some(r) => {
            let evs: Vec<Value> = r.events.iter().filter(|e| e.id > since).map(|e| e.to_json()).collect();
            json!({ "events": evs, "cursor": r.next_id.saturating_sub(1) })
        }
    }
}

/// Injeksi event manual (mock-event UI). type: donation|agent|else→chat.
pub fn inject_event(body: &Value) -> Option<Value> {
    let raw_type = body.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let kind = if raw_type == "donation" || raw_type == "agent" { raw_type } else { "chat" };
    let user = body.get("user").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).unwrap_or("Guest").to_string();
    let text: String = body.get("text").and_then(|v| v.as_str()).unwrap_or("").chars().take(400).collect();
    let amount = body.get("amount").and_then(|v| v.as_str()).map(String::from);

    let (out, run_now, epoch) = {
        let mut g = rt().lock().unwrap();
        let r = g.as_mut()?;
        let ep = r.epoch;
        let (o, item) = ingest_feed(r, kind, &user, &text, amount);
        (o, item, ep)
    };
    if let Some(item) = run_now {
        spawn_run(epoch, item);
    }
    Some(out)
}

/// Balasan agent → event feed type "agent".
pub fn agent_say(text: &str) -> Option<Value> {
    let mut g = rt().lock().unwrap();
    let r = g.as_mut()?;
    let t: String = text.chars().take(500).collect();
    Some(push_feed(r, "agent", "AI", &t, None))
}

/// Instruksi operator (§7): echo ke feed + masuk antrean operator.
pub fn operator_say(text: &str) -> (bool, Option<String>) {
    let text: String = text.chars().take(400).collect();
    let text = text.trim().to_string();
    if text.is_empty() {
        return (false, Some("teks instruksi kosong".into()));
    }
    let (run_now, epoch) = {
        let mut g = rt().lock().unwrap();
        let Some(r) = g.as_mut() else { return (false, Some("vtuber runtime tidak aktif".into())) };
        push_feed(r, "system", "operator", &format!("Operator: {text}"), None);
        let (_res, _item) = r.scheduler.ingest(SchedulerEvent {
            class: SchedulerClass::Operator,
            user: "operator".into(),
            text,
            amount: None,
        });
        // operator masuk antrean; tarik bila slot bebas
        let item = r.scheduler.try_start();
        for msg in r.scheduler.drain_system() {
            push_feed(r, "system", "system", &msg, None);
        }
        (item, r.epoch)
    };
    if let Some(item) = run_now {
        spawn_run(epoch, item);
    }
    (true, None)
}

/// Ubah config behavior JALAN (tanpa restart stream).
pub fn set_config(body: &Value) -> Value {
    let patch = ConfigPatch {
        persona: body.get("persona").and_then(|v| v.as_str()).map(String::from),
        cooldown_ms: body.get("cooldownMs").and_then(|v| v.as_i64()),
        respond_chat: body.get("respondChat").and_then(|v| v.as_bool()),
        respond_donation: body.get("respondDonation").and_then(|v| v.as_bool()),
    };
    let (ok, run_now, epoch) = {
        let mut g = rt().lock().unwrap();
        match g.as_mut() {
            None => (false, None, 0),
            Some(r) => {
                // cache flag respond utk status
                if let Some(b) = patch.respond_chat {
                    r.cfg["respondChat"] = json!(b);
                }
                r.scheduler.set_config(patch);
                // config baru bisa menyalakan drain (respond dinyalakan saat antre)
                let item = r.scheduler.try_start();
                (true, item, r.epoch)
            }
        }
    };
    if let Some(item) = run_now {
        spawn_run(epoch, item);
    }
    json!({ "ok": ok })
}

/// Hentikan runtime (bump epoch → task lama berhenti sendiri).
pub fn stop() -> Value {
    let mut g = rt().lock().unwrap();
    if let Some(r) = g.as_mut() {
        r.scheduler.stop();
    }
    *g = None;
    epoch_counter().fetch_add(1, Ordering::SeqCst);
    json!({ "ok": true })
}

/// Mulai runtime. Menghancurkan runtime lama dulu. `config_path` untuk LLM.
pub fn start(cfg: Value, config_path: PathBuf) -> (bool, Option<String>) {
    // stop dulu (single active runtime)
    stop();
    let provider = cfg.get("provider").and_then(|v| v.as_str()).unwrap_or("mock").to_string();

    if provider != "mock" {
        // twitch (IRC WS) & youtube (poll) belum diport ke core.
        return (false, Some(format!("provider '{provider}' belum diport ke core — pakai 'mock' (twitch/youtube menyusul)")));
    }

    let epoch = epoch_counter().fetch_add(1, Ordering::SeqCst) + 1;
    let mut scheduler = Scheduler::new();
    scheduler.set_config(ConfigPatch {
        persona: cfg.get("persona").and_then(|v| v.as_str()).map(String::from),
        cooldown_ms: cfg.get("cooldownMs").and_then(|v| v.as_i64()),
        respond_chat: Some(cfg.get("respondChat").and_then(|v| v.as_bool()).unwrap_or(false)),
        respond_donation: Some(cfg.get("respondDonation").and_then(|v| v.as_bool()).unwrap_or(false)),
    });

    let interval_ms = cfg.get("mockIntervalMs").and_then(|v| v.as_u64()).unwrap_or(6000).max(3000);

    {
        let mut g = rt().lock().unwrap();
        let mut r = VtRuntime {
            epoch,
            provider: provider.clone(),
            cfg,
            events: Vec::new(),
            next_id: 1,
            scheduler,
            config_path,
        };
        push_feed(&mut r, "system", "system", "Mode mock aktif — penonton simulasi (tanpa API key).", None);
        *g = Some(r);
    }
    spawn_mock(epoch, interval_ms);
    (true, None)
}

// ── Overlay OBS ─────────────────────────────────────────────────────────────
// vtuber.html (Browser Source OBS) kirim heartbeat berkala; selama segar, app
// utama menahan balasan otomatisnya (anti-dobel). 0 = belum pernah nyambung.
fn overlay_beat() -> &'static AtomicI64 {
    static B: OnceLock<AtomicI64> = OnceLock::new();
    B.get_or_init(|| AtomicI64::new(0))
}

pub fn overlay_ping() -> Value {
    overlay_beat().store(now_ms(), Ordering::SeqCst);
    json!({ "ok": true, "active": true })
}

pub fn overlay_active() -> bool {
    let b = overlay_beat().load(Ordering::SeqCst);
    b > 0 && now_ms() - b < 8000
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_config(dir: &std::path::Path) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let f = dir.join("config.json");
        std::fs::write(&f, r#"{"activeId":"m","connections":[{"id":"m","provider":"mock"}]}"#).unwrap();
        f
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn start_stop_mock_status() {
        let dir = std::env::temp_dir().join(format!("l2dvt-{}-{}", std::process::id(), now_ms()));
        let cfg_path = fresh_config(&dir);
        // provider selain mock ditolak
        let (ok, err) = start(json!({ "provider": "twitch", "channel": "x" }), cfg_path.clone());
        assert!(!ok);
        assert!(err.unwrap().contains("belum diport"));

        // mock start → running + event system pembuka
        let (ok, _) = start(json!({ "provider": "mock", "mockIntervalMs": 3000 }), cfg_path);
        assert!(ok);
        let st = status();
        assert_eq!(st["running"], true);
        assert_eq!(st["provider"], "mock");
        let ev = events(0);
        assert!(ev["events"].as_array().unwrap().iter().any(|e| e["type"] == "system"));

        stop();
        assert_eq!(status()["running"], false);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn inject_dan_agent_say() {
        let dir = std::env::temp_dir().join(format!("l2dvt2-{}-{}", std::process::id(), now_ms()));
        let cfg_path = fresh_config(&dir);
        start(json!({ "provider": "mock", "mockIntervalMs": 60000 }), cfg_path);
        // inject chat (respondChat default mati → tak memicu LLM, tapi tampil feed)
        let ev = inject_event(&json!({ "type": "chat", "user": "Rian", "text": "halo" })).unwrap();
        assert_eq!(ev["type"], "chat");
        assert_eq!(ev["user"], "Rian");
        // agent say
        let a = agent_say("halo semua").unwrap();
        assert_eq!(a["type"], "agent");
        // operator tanpa runtime setelah stop → gagal
        stop();
        assert!(inject_event(&json!({ "type": "chat" })).is_none());
        assert!(agent_say("x").is_none());
        let (ok, _) = operator_say("test");
        assert!(!ok);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn overlay_heartbeat() {
        assert!(!overlay_active() || overlay_active()); // no panic
        overlay_ping();
        assert!(overlay_active());
    }
}
