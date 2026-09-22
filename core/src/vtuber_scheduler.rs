//! vtuber_scheduler.rs — Otak behavior VTuber (§7 ARSITEKTUR-TARGET), port dari
//! `src/server/vtuber-scheduler.ts`.
//!
//! SATU scheduler di server; dua klien (app utama + overlay OBS) hanya render
//! feed + memutar balasan. Ini mematikan race app-vs-overlay dari akarnya dan
//! membuat antrean tahan reconnect klien.
//!
//! Model event (§7):
//!   - Audience (chat)  → SUPPRESSION: dedup → cooldown → drop. Tidak antre.
//!   - Donation         → FIFO antrean cap 20; penuh → item BARU ditolak
//!                        eksplisit (feedback feed system), item lama tak pernah
//!                        di-silent-evict.
//!   - Operator         → FIFO antrean sendiri cap 20; instruksi eksplisit
//!                        operator — boleh masuk walau respondChat/Donation mati.
//!   - Active slot TUNGGAL dibagi donation+operator; item aktif tak bisa
//!     dipreempt kelas lain. Slot bebas → donation dulu, baru operator
//!     (precedence terkunci §7).
//!
//! BEDA dari TS: TS mengikat eksekusi LLM ke dalam scheduler lewat deps yang
//! di-inject. Di Rust kita PISAHKAN keputusan (murni, sinkron, mudah diuji:
//! dedup/cooldown/cap/precedence) dari eksekusi async (LLM + tunggu bicara) yang
//! didorong runtime (`vtuber.rs`). Feedback sistem (antrean penuh) dikumpulkan
//! ke `pending_system` lalu ditarik runtime menjadi event feed.

use std::collections::HashMap;
use std::collections::VecDeque;

/// §7: antrean FIFO maksimum 20 (donation & operator).
pub const VTUBER_QUEUE_CAP: usize = 20;
/// Dedup audience: user+teks sama dalam jendela ini → drop.
pub const VTUBER_DEDUP_WINDOW_MS: i64 = 30_000;
/// Margin setelah estimasi bicara sebelum slot dianggap bebas.
pub const SPEAK_BUFFER_MS: i64 = 500;
const MIN_COOLDOWN_MS: i64 = 5000;

/// Estimasi durasi bicara — padanan shared/speech-timing.ts::estimateSpeechMs.
pub fn estimate_speech_ms(text: &str) -> i64 {
    let t = text.trim();
    if t.is_empty() {
        return 0;
    }
    (500 + t.chars().count() as i64 * 62).min(12000)
}

/// Durasi tahan slot = estimasi bicara + buffer.
pub fn speak_ms(text: &str) -> i64 {
    estimate_speech_ms(text) + SPEAK_BUFFER_MS
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SchedulerClass {
    Audience,
    Donation,
    Operator,
}

impl SchedulerClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            SchedulerClass::Audience => "audience",
            SchedulerClass::Donation => "donation",
            SchedulerClass::Operator => "operator",
        }
    }
}

#[derive(Clone, Debug)]
pub struct SchedulerEvent {
    pub class: SchedulerClass,
    pub user: String,
    pub text: String,
    pub amount: Option<String>,
}

#[derive(Clone, Debug)]
pub struct QueueItem {
    pub id: u64,
    pub class: SchedulerClass,
    pub user: String,
    pub text: String,
    pub amount: Option<String>,
    pub enqueued_at: i64,
}

#[derive(Clone, Debug)]
pub struct SchedulerConfig {
    pub persona: String,
    pub cooldown_ms: i64,
    pub respond_chat: bool,
    pub respond_donation: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        SchedulerConfig {
            persona: "ceria dan ramah".into(),
            cooldown_ms: 12000,
            respond_chat: false, // aman-by-default: tanpa flag eksplisit, tanpa LLM
            respond_donation: false,
        }
    }
}

/// Patch config live (semua opsional, hanya field ada yang diterapkan).
#[derive(Default, Debug)]
pub struct ConfigPatch {
    pub persona: Option<String>,
    pub cooldown_ms: Option<i64>,
    pub respond_chat: Option<bool>,
    pub respond_donation: Option<bool>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct IngestResult {
    pub accepted: bool,
    pub reason: Option<String>,
}

// Prompt protokol antar-komponen — bahasa Indonesia (aturan repo: kosakata
// protokol tetap Indonesia, konsisten EVENT_PROMPTS di brain.ts).
pub fn system_prompt(persona: &str) -> String {
    let p = if persona.trim().is_empty() { "ceria dan ramah" } else { persona };
    format!(
        "Kamu adalah VTuber Live2D yang sedang streaming. Gaya bicara: {p}. \
         Jawab HANYA kalimat yang akan diucapkan, tanpa awalan nama. Bahasa Indonesia."
    )
}
fn chat_prompt(user: &str, text: &str) -> String {
    format!(
        "Penonton bernama {user} bilang di live chat: \"{text}\". \
         Balas singkat (1 kalimat) yang fun dan personal."
    )
}
fn donation_prompt(user: &str, text: &str, amount: &str) -> String {
    format!(
        "Penonton bernama {user} baru saja donasi {amount} dengan pesan: \"{text}\". \
         Ucapkan terima kasih hangat yang khas (1-2 kalimat)."
    )
}
fn operator_prompt(text: &str) -> String {
    format!(
        "Operator stream memberimu instruksi: \"{text}\". \
         Ucapkan sesuai instruksi dengan gaya bicaramu (maks 2 kalimat)."
    )
}

/// State scheduler — keputusan murni. Eksekusi LLM/tunggu didorong runtime.
pub struct Scheduler {
    cfg: SchedulerConfig,
    next_item_id: u64,
    donation_queue: VecDeque<QueueItem>,
    operator_queue: VecDeque<QueueItem>,
    active: Option<QueueItem>,
    last_audience_respond_at: i64,
    /// user+\x00+text → ts terakhir terlihat (dedup audience).
    recent_chat: HashMap<String, i64>,
    stopped: bool,
    /// Feedback sistem (antrean penuh) — runtime menariknya ke event feed.
    pending_system: Vec<String>,
    /// Jam tetap untuk test (None = wall clock).
    now_fixed: Option<i64>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Scheduler {
            cfg: SchedulerConfig::default(),
            next_item_id: 1,
            donation_queue: VecDeque::new(),
            operator_queue: VecDeque::new(),
            active: None,
            last_audience_respond_at: 0,
            recent_chat: HashMap::new(),
            stopped: false,
            pending_system: Vec::new(),
            now_fixed: None,
        }
    }
}

impl Scheduler {
    pub fn new() -> Self {
        Self::default()
    }

    fn now(&self) -> i64 {
        self.now_fixed.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0)
        })
    }

    #[cfg(test)]
    fn set_now(&mut self, ts: i64) {
        self.now_fixed = Some(ts);
    }

    pub fn set_config(&mut self, patch: ConfigPatch) {
        if let Some(p) = patch.persona {
            if !p.trim().is_empty() {
                self.cfg.persona = p.chars().take(800).collect();
            }
        }
        if let Some(c) = patch.cooldown_ms {
            // TS menjaga Number.isFinite; i64 dari JSON number selalu finite.
            self.cfg.cooldown_ms = c.max(MIN_COOLDOWN_MS);
        }
        if let Some(b) = patch.respond_chat {
            self.cfg.respond_chat = b;
        }
        if let Some(b) = patch.respond_donation {
            self.cfg.respond_donation = b;
        }
    }

    pub fn persona(&self) -> &str {
        &self.cfg.persona
    }

    /// Tarik feedback sistem terkumpul (runtime → event feed).
    pub fn drain_system(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_system)
    }

    fn prune_dedup(&mut self) {
        let cutoff = self.now() - VTUBER_DEDUP_WINDOW_MS;
        self.recent_chat.retain(|_, ts| *ts >= cutoff);
    }

    fn new_item(&mut self, ev: &SchedulerEvent) -> QueueItem {
        let id = self.next_item_id;
        self.next_item_id += 1;
        QueueItem {
            id,
            class: ev.class,
            user: ev.user.clone(),
            text: ev.text.clone(),
            amount: ev.amount.clone(),
            enqueued_at: self.now(),
        }
    }

    /// Terima event. Audience yang diterima langsung mereservasi active slot dan
    /// dikembalikan sebagai `run_now`. Donation/operator masuk antrean; runtime
    /// memanggil `try_start()` untuk menariknya. Return: (hasil, item-untuk-jalan).
    pub fn ingest(&mut self, ev: SchedulerEvent) -> (IngestResult, Option<QueueItem>) {
        if self.stopped {
            return (reject("scheduler berhenti"), None);
        }
        match ev.class {
            SchedulerClass::Audience => self.ingest_chat(ev),
            SchedulerClass::Donation => {
                // Donasi selalu tampil di feed; antrean ucapan hanya saat flag nyala.
                if !self.cfg.respond_donation {
                    return (reject("respond donasi mati"), None);
                }
                (self.enqueue(ev, SchedulerClass::Donation), None)
            }
            SchedulerClass::Operator => (self.enqueue(ev, SchedulerClass::Operator), None),
        }
    }

    fn ingest_chat(&mut self, ev: SchedulerEvent) -> (IngestResult, Option<QueueItem>) {
        if !self.cfg.respond_chat {
            return (reject("respond chat mati"), None);
        }
        let key = format!("{}\x00{}", ev.user, ev.text);
        self.prune_dedup();
        let now = self.now();
        if let Some(seen) = self.recent_chat.get(&key) {
            if now - *seen < VTUBER_DEDUP_WINDOW_MS {
                return (reject("duplikat"), None); // §7 audience: duplicate? → DROP
            }
        }
        self.recent_chat.insert(key, now);
        if now - self.last_audience_respond_at < self.cfg.cooldown_ms.max(MIN_COOLDOWN_MS) {
            return (reject("cooldown"), None); // §7 audience: cooldown? → DROP
        }
        if self.active.is_some() || !self.donation_queue.is_empty() || !self.operator_queue.is_empty() {
            return (reject("slot sibuk"), None); // suppression, bukan antre
        }
        self.last_audience_respond_at = now;
        let item = self.new_item(&ev);
        self.active = Some(item.clone());
        (IngestResult { accepted: true, reason: None }, Some(item))
    }

    fn enqueue(&mut self, ev: SchedulerEvent, cls: SchedulerClass) -> IngestResult {
        let len = match cls {
            SchedulerClass::Donation => self.donation_queue.len(),
            _ => self.operator_queue.len(),
        };
        if len >= VTUBER_QUEUE_CAP {
            // §7: item BARU ditolak eksplisit — item lama tak pernah dibuang senyap.
            let label = if cls == SchedulerClass::Donation {
                "terima kasih donasi"
            } else {
                "instruksi operator"
            };
            let who: String = if ev.user.is_empty() { cls.as_str().to_string() } else { ev.user.clone() };
            let who: String = who.chars().take(60).collect();
            self.pending_system.push(format!(
                "Antrean {label} penuh ({VTUBER_QUEUE_CAP}) — pesan dari {who} tidak masuk antrean."
            ));
            return reject("queue-full");
        }
        let item = self.new_item(&ev);
        match cls {
            SchedulerClass::Donation => self.donation_queue.push_back(item),
            _ => self.operator_queue.push_back(item),
        }
        IngestResult { accepted: true, reason: None }
    }

    /// Reservasi item berikut dari antrean bila slot bebas (donation dulu, baru
    /// operator — precedence terkunci §7). Return item untuk dijalankan runtime.
    pub fn try_start(&mut self) -> Option<QueueItem> {
        if self.stopped || self.active.is_some() {
            return None;
        }
        let next = self.donation_queue.pop_front().or_else(|| self.operator_queue.pop_front())?;
        self.active = Some(next.clone());
        Some(next)
    }

    /// Lepas slot aktif (dipanggil runtime setelah balasan selesai dibicarakan).
    pub fn finish(&mut self) {
        self.active = None;
    }

    /// Bangun prompt user untuk item (dipakai runtime saat memanggil LLM).
    pub fn build_prompt(&self, item: &QueueItem) -> String {
        let user: String = item.user.chars().take(60).collect();
        let text: String = item.text.chars().take(400).collect();
        let amount: String = item.amount.clone().unwrap_or_default().chars().take(40).collect();
        match item.class {
            SchedulerClass::Audience => chat_prompt(&user, &text),
            SchedulerClass::Donation => {
                let amt = if amount.is_empty() { "sebuah donasi".to_string() } else { amount };
                donation_prompt(&user, &text, &amt)
            }
            SchedulerClass::Operator => operator_prompt(&text),
        }
    }

    pub fn system_prompt(&self) -> String {
        system_prompt(&self.cfg.persona)
    }

    pub fn stats(&self) -> (Option<SchedulerClass>, usize, usize) {
        (
            self.active.as_ref().map(|i| i.class),
            self.donation_queue.len(),
            self.operator_queue.len(),
        )
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    pub fn stop(&mut self) {
        self.stopped = true;
        self.donation_queue.clear();
        self.operator_queue.clear();
        self.active = None;
    }
}

fn reject(reason: &str) -> IngestResult {
    IngestResult { accepted: false, reason: Some(reason.to_string()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(class: SchedulerClass, user: &str, text: &str) -> SchedulerEvent {
        SchedulerEvent { class, user: user.into(), text: text.into(), amount: None }
    }

    #[test]
    fn default_respond_off_tanpa_llm() {
        let mut s = Scheduler::new();
        // respondChat/Donation default MATI → ditolak tanpa memanggil apa pun.
        let (r, run) = s.ingest(ev(SchedulerClass::Audience, "Rian", "halo"));
        assert!(!r.accepted);
        assert_eq!(r.reason.as_deref(), Some("respond chat mati"));
        assert!(run.is_none());
        let (r2, _) = s.ingest(ev(SchedulerClass::Donation, "Kevin", "makasih"));
        assert_eq!(r2.reason.as_deref(), Some("respond donasi mati"));
    }

    #[test]
    fn audience_dedup_lalu_cooldown() {
        let mut s = Scheduler::new();
        s.set_config(ConfigPatch { respond_chat: Some(true), cooldown_ms: Some(5000), ..Default::default() });
        s.set_now(100_000);
        // pertama diterima → reservasi slot
        let (r, run) = s.ingest(ev(SchedulerClass::Audience, "Rian", "halo"));
        assert!(r.accepted && run.is_some());
        s.finish(); // anggap balasan selesai
        // duplikat persis dalam jendela dedup → drop
        s.set_now(101_000);
        let (r2, _) = s.ingest(ev(SchedulerClass::Audience, "Rian", "halo"));
        assert_eq!(r2.reason.as_deref(), Some("duplikat"));
        // teks beda tapi masih dalam cooldown → drop cooldown
        let (r3, _) = s.ingest(ev(SchedulerClass::Audience, "Rian", "apa kabar"));
        assert_eq!(r3.reason.as_deref(), Some("cooldown"));
        // lewat cooldown + teks BARU (belum pernah terlihat) → diterima lagi.
        // ("apa kabar" sudah tercatat di recentChat saat cooldown-reject di atas —
        // dicatat SEBELUM cek cooldown, sama seperti TS — jadi tak bisa dipakai.)
        s.set_now(107_000);
        let (r4, run4) = s.ingest(ev(SchedulerClass::Audience, "Rian", "malam"));
        assert!(r4.accepted && run4.is_some());
    }

    #[test]
    fn audience_slot_sibuk_saat_antrean_terisi() {
        let mut s = Scheduler::new();
        s.set_config(ConfigPatch { respond_chat: Some(true), respond_donation: Some(true), ..Default::default() });
        // now jauh di atas cooldown default (12s) supaya cek cooldown lolos dan
        // kita benar-benar menguji cabang "slot sibuk".
        s.set_now(100_000);
        // donasi mengisi antrean (belum di-start)
        let (rd, _) = s.ingest(ev(SchedulerClass::Donation, "Kevin", "mantap"));
        assert!(rd.accepted);
        // audience saat antrean donasi terisi → suppression "slot sibuk"
        let (ra, _) = s.ingest(ev(SchedulerClass::Audience, "Rian", "halo"));
        assert_eq!(ra.reason.as_deref(), Some("slot sibuk"));
    }

    #[test]
    fn precedence_donation_sebelum_operator() {
        let mut s = Scheduler::new();
        s.set_config(ConfigPatch { respond_donation: Some(true), ..Default::default() });
        // operator masuk lebih dulu, lalu donasi
        s.ingest(ev(SchedulerClass::Operator, "operator", "sapa dong"));
        s.ingest(ev(SchedulerClass::Donation, "Kevin", "makasih"));
        // slot bebas → donasi harus diambil DULU (precedence §7)
        let first = s.try_start().expect("ada item");
        assert_eq!(first.class, SchedulerClass::Donation);
        // item aktif tak bisa dipreempt: try_start lagi = None (slot sibuk)
        assert!(s.try_start().is_none());
        s.finish();
        let second = s.try_start().expect("operator menyusul");
        assert_eq!(second.class, SchedulerClass::Operator);
    }

    #[test]
    fn operator_masuk_walau_respond_mati() {
        let mut s = Scheduler::new();
        // semua respond MATI (default) — operator tetap masuk antrean
        let (r, _) = s.ingest(ev(SchedulerClass::Operator, "operator", "bilang halo"));
        assert!(r.accepted);
        assert_eq!(s.stats().2, 1); // operatorQueue = 1
    }

    #[test]
    fn queue_full_tolak_baru_feedback_sistem() {
        let mut s = Scheduler::new();
        s.set_config(ConfigPatch { respond_donation: Some(true), ..Default::default() });
        for i in 0..VTUBER_QUEUE_CAP {
            let (r, _) = s.ingest(ev(SchedulerClass::Donation, "D", &format!("d{i}")));
            assert!(r.accepted, "item ke-{i} harus masuk");
        }
        // ke-21 ditolak eksplisit + feedback sistem
        let (r, _) = s.ingest(ev(SchedulerClass::Donation, "Zed", "penuh?"));
        assert_eq!(r.reason.as_deref(), Some("queue-full"));
        let sys = s.drain_system();
        assert_eq!(sys.len(), 1);
        assert!(sys[0].contains("penuh"));
        assert_eq!(s.stats().1, VTUBER_QUEUE_CAP); // item lama tetap utuh
    }

    #[test]
    fn stop_kosongkan_antrean() {
        let mut s = Scheduler::new();
        s.set_config(ConfigPatch { respond_donation: Some(true), ..Default::default() });
        s.ingest(ev(SchedulerClass::Donation, "K", "x"));
        s.stop();
        assert!(s.is_stopped());
        assert_eq!(s.stats().1, 0);
        let (r, _) = s.ingest(ev(SchedulerClass::Donation, "K", "y"));
        assert_eq!(r.reason.as_deref(), Some("scheduler berhenti"));
    }

    #[test]
    fn estimasi_bicara_dibatasi() {
        assert_eq!(estimate_speech_ms(""), 0);
        assert_eq!(estimate_speech_ms("ab"), 500 + 2 * 62);
        assert_eq!(estimate_speech_ms(&"x".repeat(1000)), 12000); // clamp
        assert_eq!(speak_ms("ab"), 500 + 2 * 62 + SPEAK_BUFFER_MS);
    }
}
