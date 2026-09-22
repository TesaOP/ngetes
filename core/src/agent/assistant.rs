//! Assistant runtime + loop — port `agentAsk` (loop.ts) + facade (assistant.ts).
//! Satu runtime aktif (workDir, history, approvals, busy). Loop: LLM
//! ("assistant") → detect tool → gate (mutating: pause minta izin) / exec →
//! ulang sampai final. Permission gate menjeda loop; /approve melanjutkannya.
//!
//! Verifikasi: run_command & FS diuji di tools/loop_ (3d-3/3d-4). Loop no-tool
//! diuji dgn mock; jalur tool+gate lewat LLM sungguhan (mock tak emit "TOOL:").

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::agent::{bus, loop_, memory, plan, tools};
use crate::llm::{self, ChatMessage};

const MAX_ITERATIONS: usize = 25;
const MAX_HISTORY: usize = 60;
const MAX_UNDO: usize = 50;

/// Rekaman undo satu mutasi file (snapshot isi SEBELUM tool write/edit/delete).
#[derive(Clone)]
struct UndoRec {
    id: String,
    rel_path: String,
    abs_path: PathBuf,
    /// None = file belum ada sebelum mutasi (revert = hapus file).
    prev_content: Option<String>,
    ts: i64,
    reverted: bool,
}

#[derive(Default)]
pub struct Runtime {
    pub running: bool,
    pub busy: bool,
    pub work_dir: String,
    pub history: Vec<Value>, // {role, content}
    pub approvals: Vec<Value>, // {id, tool, args, ts}
    plan: Vec<Value>,          // update_plan items
    notes_files: Vec<String>,  // file tersentuh sesi ini (relatif)
    undo: Vec<UndoRec>,        // snapshot mutasi (cap MAX_UNDO)
    cancel: bool,              // cancel kooperatif antar-langkah
}

fn rt() -> &'static Mutex<Runtime> {
    static R: OnceLock<Mutex<Runtime>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(Runtime::default()))
}

fn push_msg(r: &mut Runtime, role: &str, content: &str) {
    r.history.push(json!({ "role": role, "content": content }));
    if r.history.len() > MAX_HISTORY {
        let drop = r.history.len() - MAX_HISTORY;
        r.history.drain(0..drop);
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Buang baris directive TOOL: dari teks final (padanan stripToolDirective).
fn strip_tool_directive(text: &str) -> String {
    text.lines()
        .filter(|l| {
            let t = l.trim_start();
            !loop_::TOOLS.iter().any(|tool| t.starts_with(&format!("TOOL: {}", tool.name)) || t.to_lowercase().starts_with("tool:"))
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Mulai/attach runtime ke workDir. Bus di-reset (sesi baru).
pub async fn start(work_dir: &str) -> Value {
    bus::reset();
    let mut r = rt().lock().await;
    r.running = true;
    r.busy = false;
    r.cancel = false;
    if !work_dir.is_empty() {
        r.work_dir = work_dir.to_string();
    }
    json!({ "ok": true, "workDir": r.work_dir })
}

/// Status untuk panel/probe. activeTask/parkedTasks masih stub (task-identity
/// Worker belum diport) — panel degrade anggun (satu ask sinkron per waktu).
pub async fn status() -> Value {
    let r = rt().lock().await;
    let pending: Vec<Value> = r.approvals.iter().map(|ap| {
        let name = ap.get("tool").and_then(|x| x.as_str()).unwrap_or("");
        let args = ap.get("args").cloned().unwrap_or(json!({}));
        json!({
            "id": ap.get("id").cloned().unwrap_or(Value::Null),
            "tool": name,
            "args": loop_::public_tool_args(name, &args),
            "ts": ap.get("ts").cloned().unwrap_or(Value::Null),
        })
    }).collect();
    json!({
        "running": r.running,
        "busy": r.busy,
        "workDir": r.work_dir,
        "historyCount": r.history.len(),
        "pendingApprovals": pending,
        "plan": r.plan,
        "notes": { "filesTouched": r.notes_files },
        "lastEvent": if r.running { bus::last_event() } else { Value::Null },
        "tools": loop_::TOOLS.iter().map(|t| json!({ "name": t.name, "level": t.level })).collect::<Vec<_>>(),
        "activeTask": Value::Null,
        "parkedTasks": [],
    })
}

pub async fn stop() -> Value {
    let mut r = rt().lock().await;
    r.running = false;
    r.busy = false;
    r.cancel = false;
    r.history.clear();
    r.approvals.clear();
    r.plan.clear();
    r.notes_files.clear();
    r.undo.clear();
    json!({ "ok": true })
}

/// GET /api/assistant/history.
pub async fn history() -> Value {
    let r = rt().lock().await;
    Value::Array(r.history.clone())
}

/// POST /api/assistant/quip {persona?, event?} — komentar berkarakter singkat
/// (SUARA pet/VTuber) atas aksi agent. LLM role "chat". Stateless. Gagal →
/// {quip:"", error}. Padanan handleAssistantQuip.
pub async fn quip(config_path: &Path, persona: &str, event: &str) -> Value {
    let persona: String = persona.chars().take(800).collect();
    let event: String = event.chars().take(160).collect();
    let en = lang_of(config_path) == "en";
    let base = if en {
        "You are the VOICE of a living character (desktop pet / VTuber) accompanying an AI agent as it works. You briefly react to what the agent JUST did — one casual spoken line, max 15 words, with personality. Never mention tool names, file paths, or technical terms. No emoji, no quotation marks."
    } else {
        "Kamu adalah SUARA karakter hidup (pet / VTuber) yang menemani agent AI bekerja. Reaksilah singkat atas apa yang agent BARU lakukan — satu kalimat santai, maksimal 15 kata, dengan kepribadian. Jangan sebut nama tool, path file, atau istilah teknis. Tanpa emoji, tanpa tanda kutip."
    };
    let sys = if persona.trim().is_empty() {
        base.to_string()
    } else if en {
        format!("{base}\n\nYour character:\n{persona}")
    } else {
        format!("{base}\n\nKaraktermu:\n{persona}")
    };
    let user = if event.trim().is_empty() { "agent mulai berpikir".to_string() } else { event };
    let messages = [ChatMessage { role: "user".into(), content: user }];
    match llm::llm_for_role(config_path, "chat", &messages, &sys).await {
        Ok(ok) => json!({ "quip": ok.reply.trim().chars().take(140).collect::<String>() }),
        Err((_, msg)) => json!({ "quip": "", "error": msg }),
    }
}

/// POST /api/assistant/reset — kosongkan riwayat (ditolak saat busy).
pub async fn reset() -> Value {
    let mut r = rt().lock().await;
    if r.busy {
        return json!({ "ok": false, "error": "masih memproses tugas — riwayat tidak bisa dikosongkan saat task berjalan" });
    }
    r.history.clear();
    json!({ "ok": true })
}

/// POST /api/assistant/cancel — minta batal kooperatif (dicek antar-langkah).
/// Model core sinkron: tanpa task-identity, cancel menyetel flag yang dibaca
/// run_loop di awal tiap turn.
pub async fn cancel() -> Value {
    let mut r = rt().lock().await;
    if !r.running || (!r.busy && r.approvals.is_empty()) {
        return json!({ "ok": true, "accepted": false });
    }
    r.cancel = true;
    json!({ "ok": true, "accepted": true })
}

/// GET /api/assistant/events?since=N.
pub async fn events(since: u64) -> Value {
    let busy = rt().lock().await.busy;
    let mut d = bus::read(since);
    d["busy"] = json!(busy);
    d
}

/// GET /api/assistant/undo — daftar snapshot (terbaru dulu).
pub async fn undo_list() -> Value {
    let r = rt().lock().await;
    let list: Vec<Value> = r.undo.iter().rev().map(|u| {
        json!({
            "id": u.id,
            "path": u.rel_path,
            "ts": u.ts,
            "reverted": u.reverted,
            "kind": if u.prev_content.is_none() { "created" } else { "modified" },
        })
    }).collect();
    Value::Array(list)
}

/// POST /api/assistant/revert {id} — kembalikan file ke snapshot. Ok/Err.
pub async fn revert(id: &str) -> Result<String, String> {
    let mut r = rt().lock().await;
    let idx = r.undo.iter().position(|u| u.id == id).ok_or_else(|| format!("rekaman undo tidak dikenal: {id}"))?;
    if r.undo[idx].reverted {
        return Err("rekaman ini sudah pernah di-revert".into());
    }
    let (abs, prev, rel) = {
        let u = &r.undo[idx];
        (u.abs_path.clone(), u.prev_content.clone(), u.rel_path.clone())
    };
    let msg = match &prev {
        None => {
            let _ = std::fs::remove_file(&abs);
            format!("Dikembalikan: {rel} dihapus (sebelumnya belum ada).")
        }
        Some(content) => {
            std::fs::write(&abs, content).map_err(|e| e.to_string())?;
            format!("Dikembalikan: {rel} ke isi sebelum mutasi agent ({} char).", content.chars().count())
        }
    };
    r.undo[idx].reverted = true;
    bus::emit("verification_result", &format!("revert: {rel}"));
    Ok(msg)
}

/// Hasil ask/approve.
pub struct AskResult {
    pub ok: bool,
    pub reply: String,
    pub paused: bool,
    pub error: Option<String>,
}

/// Bahasa balasan dari config.i18n.
fn lang_of(config_path: &Path) -> String {
    let cfg = crate::config::load(config_path);
    if cfg.get("i18n").and_then(|i| i.get("lang")).and_then(|l| l.as_str()) == Some("en") {
        "en".into()
    } else {
        "id".into()
    }
}

/// Jalankan loop dari history saat ini sampai final / paused / batas iterasi.
/// Runtime di-lock per-langkah (lepas saat await LLM) supaya status bisa dibaca.
async fn run_loop(config_path: &Path, root: &Path) -> AskResult {
    let lang = lang_of(config_path);
    let work_dir = { rt().lock().await.work_dir.clone() };
    let wd = PathBuf::from(&work_dir);
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut final_text = String::new();
    let mut paused = false;

    for _turn in 0..MAX_ITERATIONS {
        // Cancel kooperatif: dicek di awal tiap turn (loop lepas lock saat await
        // LLM, jadi /cancel dari request lain bisa menyetel flag ini).
        {
            let mut r = rt().lock().await;
            if r.cancel {
                r.cancel = false;
                push_msg(&mut r, "assistant", "Dibatalkan oleh user.");
                bus::emit("error", "dibatalkan: oleh user");
                r.busy = false;
                return AskResult { ok: true, reply: "Dibatalkan oleh user.".into(), paused: false, error: None };
            }
        }
        bus::emit("thinking_start", "");
        // rakit messages dari history (tool → user "[hasil tool] …").
        let messages: Vec<ChatMessage> = {
            let r = rt().lock().await;
            r.history.iter().map(|m| {
                let role = m.get("role").and_then(|x| x.as_str()).unwrap_or("user");
                let content = m.get("content").and_then(|x| x.as_str()).unwrap_or("");
                if role == "tool" {
                    ChatMessage { role: "user".into(), content: format!("[hasil tool] {content}") }
                } else {
                    ChatMessage { role: role.into(), content: content.into() }
                }
            }).collect()
        };
        let system = format!("{}{}", loop_::build_system(&lang, &work_dir), memory::memory_prompt_block(root));

        let reply = match llm::llm_for_role(config_path, "assistant", &messages, &system).await {
            Ok(ok) => ok.reply,
            Err((_, msg)) => {
                bus::emit("error", &msg);
                let mut r = rt().lock().await;
                push_msg(&mut r, "assistant", &format!("⚠️ {msg}"));
                return AskResult { ok: false, reply: String::new(), paused: false, error: Some(msg) };
            }
        };

        let detected = loop_::detect_tool_call(&reply);
        let (name, args) = match detected {
            None => {
                final_text = if reply.trim().is_empty() { "(kosong)".into() } else { reply };
                break;
            }
            Some(d) => d,
        };
        let call_key = format!("{name} {}", serde_json::to_string(&args).unwrap_or_default());
        let level = loop_::tool_level(&name);
        if level.is_none() {
            let mut r = rt().lock().await;
            push_msg(&mut r, "assistant", &strip_tool_directive(&reply));
            push_msg(&mut r, "tool", &format!("ERROR: tool tidak dikenal: {name}"));
            continue;
        }
        if seen.contains(&call_key) {
            final_text = {
                let s = strip_tool_directive(&reply);
                if s.is_empty() { "(berhenti setelah duplikasi tool)".into() } else { s }
            };
            break;
        }
        seen.insert(call_key);

        if level == Some("mutating") {
            // PERMISSION GATE — jeda, minta izin.
            bus::emit("permission_request", &name);
            let id = format!("ap_{}", crate::config::base36_pub(now_ms() as u128));
            let pub_args = loop_::public_tool_args(&name, &args);
            let mut r = rt().lock().await;
            while r.approvals.len() >= 8 {
                r.approvals.remove(0);
            }
            r.approvals.push(json!({ "id": id, "tool": name, "args": args, "ts": now_ms() }));
            push_msg(&mut r, "assistant", &strip_tool_directive(&reply));
            push_msg(&mut r, "tool", &format!("MENUNGGU PERSETUJUAN: {name} {} (id {id})", serde_json::to_string(&pub_args).unwrap_or_default().chars().take(300).collect::<String>()));
            paused = true;
            final_text = format!("{}\n\n⏳ Aku butuh izinmu untuk {name} — cek panel Assistant.", strip_tool_directive(&reply));
            break;
        }

        // update_plan: state terpisah dari teks — ubah rt.plan (bukan exec_tool).
        if name == "update_plan" {
            let reason = args.get("reason").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let result = match plan::sanitize_plan(args.get("todos").unwrap_or(&Value::Null)) {
                None => "ERROR: 'todos' kosong/invalid — kirim array {id,task,status}".to_string(),
                Some(todos) => {
                    let mut r = rt().lock().await;
                    let (ok, why) = plan::apply_plan(&mut r.plan, todos, &reason);
                    if ok {
                        format!("Rencana diperbarui: {}", plan::plan_label(&r.plan))
                    } else {
                        format!("ERROR: {}", why.unwrap_or_default())
                    }
                }
            };
            let mut r = rt().lock().await;
            push_msg(&mut r, "assistant", &strip_tool_directive(&reply));
            push_msg(&mut r, "tool", &format!("[update_plan] {result}"));
            continue;
        }

        // tool safe → eksekusi langsung.
        bus::emit("tool_call_start", &name);
        let result = loop_::exec_tool(root, &wd, &name, &args);
        bus::emit("tool_call_end", &name);
        let mut r = rt().lock().await;
        push_msg(&mut r, "assistant", &strip_tool_directive(&reply));
        push_msg(&mut r, "tool", &format!("[{name}] {}", clip_tool(&result)));
    }

    if final_text.is_empty() {
        final_text = format!("(berhenti tanpa jawaban setelah {MAX_ITERATIONS} langkah — coba pecah tugasnya)");
    }
    let final_clean = strip_tool_directive(&final_text);
    if !paused {
        bus::emit("final_answer", "");
    }
    {
        let mut r = rt().lock().await;
        push_msg(&mut r, "assistant", &final_clean);
        if !paused {
            r.busy = false;
        }
    }
    AskResult { ok: true, reply: final_clean, paused, error: None }
}

fn clip_tool(s: &str) -> String {
    let n = s.chars().count();
    if n > 4000 {
        format!("{}\n…(terpotong)", s.chars().take(4000).collect::<String>())
    } else {
        s.to_string()
    }
}

/// Snapshot isi file SEBELUM tool mutasi (write/edit/delete). None utk tool
/// non-file. Return (relPath, absPath, prevContent). prevContent None = file
/// belum ada (revert = hapus).
fn snapshot_before(work_dir: &Path, name: &str, args: &Value) -> Option<(String, PathBuf, Option<String>)> {
    if !matches!(name, "write_file" | "edit_file" | "delete_file") {
        return None;
    }
    let rel = args.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if rel.is_empty() {
        return None;
    }
    let abs = tools::safe_path(work_dir, &rel).ok()?;
    let prev = std::fs::read_to_string(&abs).ok();
    Some((rel, abs, prev))
}

/// Catat rekaman undo + tandai file tersentuh (notes). Cap MAX_UNDO.
fn record_undo(r: &mut Runtime, rel: String, abs: PathBuf, prev: Option<String>) {
    let id = format!("un_{}", crate::config::base36_pub(now_ms() as u128));
    r.undo.push(UndoRec { id, rel_path: rel.clone(), abs_path: abs, prev_content: prev, ts: now_ms(), reverted: false });
    if r.undo.len() > MAX_UNDO {
        let drop = r.undo.len() - MAX_UNDO;
        r.undo.drain(0..drop);
    }
    if !r.notes_files.contains(&rel) {
        r.notes_files.push(rel);
    }
}

/// POST /api/assistant/ask — jalankan tugas (loop penuh, sinkron).
pub async fn ask(config_path: &Path, root: &Path, text: &str) -> AskResult {
    {
        let mut r = rt().lock().await;
        r.running = true;
        r.busy = true;
        let t: String = text.chars().take(4000).collect();
        push_msg(&mut r, "user", &t);
    }
    run_loop(config_path, root).await
}

/// POST /api/assistant/approve — resume loop setelah izin tool mutating.
pub async fn approve(config_path: &Path, root: &Path, id: &str, approve_it: bool) -> AskResult {
    let pending = {
        let mut r = rt().lock().await;
        let idx = r.approvals.iter().position(|a| a.get("id").and_then(|x| x.as_str()) == Some(id));
        match idx {
            Some(i) => Some(r.approvals.remove(i)),
            None => None,
        }
    };
    let pending = match pending {
        Some(p) => p,
        None => return AskResult { ok: false, reply: String::new(), paused: false, error: Some(format!("approval tidak dikenal: {id}")) },
    };
    let name = pending.get("tool").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let args = pending.get("args").cloned().unwrap_or(json!({}));
    let work_dir = { rt().lock().await.work_dir.clone() };
    let wd = PathBuf::from(work_dir);

    bus::emit("permission_resolved", &format!("{}: {name}", if approve_it { "disetujui" } else { "ditolak" }));

    if approve_it {
        // Snapshot undo SEBELUM tool mutasi file (write/edit/delete) dieksekusi.
        let snap = snapshot_before(&wd, &name, &args);
        bus::emit("tool_call_start", &name);
        let result = loop_::exec_tool(root, &wd, &name, &args);
        bus::emit("tool_call_end", &name);
        let ok = !result.starts_with("ERROR");
        let mut r = rt().lock().await;
        if ok {
            if let Some((rel, abs, prev)) = snap {
                record_undo(&mut r, rel, abs, prev);
            }
        }
        push_msg(&mut r, "tool", &format!("[{name}] {}", clip_tool(&result)));
    } else {
        let mut r = rt().lock().await;
        push_msg(&mut r, "tool", &format!("User MENOLAK menjalankan {name}. Cari pendekatan lain atau tanyakan."));
    }
    run_loop(config_path, root).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ask_no_tool_final_dgn_mock() {
        // mock LLM (echo) tak emit "TOOL:" → loop langsung final.
        let dir = std::env::temp_dir().join(format!("l2das-{}-{}", std::process::id(), now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("config.json");
        std::fs::write(&f, r#"{"activeId":"m","connections":[{"id":"m","provider":"mock"}]}"#).unwrap();
        start("/tmp/work").await;
        let res = ask(&f, &dir, "halo agent").await;
        assert!(res.ok);
        assert!(!res.paused);
        assert!(res.reply.to_lowercase().contains("halo"));
        stop().await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn strip_directive() {
        let t = "Baik, aku baca.\nTOOL: read_file {\"path\":\"x\"}\nsisa";
        let s = strip_tool_directive(t);
        assert!(!s.contains("TOOL:"));
        assert!(s.contains("Baik"));
    }
}
