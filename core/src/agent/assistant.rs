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

use crate::agent::{loop_, memory};
use crate::llm::{self, ChatMessage};

const MAX_ITERATIONS: usize = 25;
const MAX_HISTORY: usize = 60;

#[derive(Default)]
pub struct Runtime {
    pub running: bool,
    pub busy: bool,
    pub work_dir: String,
    pub history: Vec<Value>, // {role, content}
    pub approvals: Vec<Value>, // {id, tool, args, ts}
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

/// Mulai/attach runtime ke workDir.
pub async fn start(work_dir: &str) -> Value {
    let mut r = rt().lock().await;
    r.running = true;
    r.busy = false;
    if !work_dir.is_empty() {
        r.work_dir = work_dir.to_string();
    }
    json!({ "ok": true, "workDir": r.work_dir })
}

/// Status untuk panel/probe.
pub async fn status() -> Value {
    let r = rt().lock().await;
    json!({
        "running": r.running,
        "busy": r.busy,
        "workDir": r.work_dir,
        "historyCount": r.history.len(),
        "pendingApprovals": r.approvals,
        "plan": [],
        "tools": loop_::TOOLS.iter().map(|t| json!({ "name": t.name, "level": t.level })).collect::<Vec<_>>()
    })
}

pub async fn stop() -> Value {
    let mut r = rt().lock().await;
    r.running = false;
    r.busy = false;
    r.history.clear();
    r.approvals.clear();
    json!({ "ok": true })
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

        // tool safe → eksekusi langsung.
        let result = loop_::exec_tool(root, &wd, &name, &args);
        let mut r = rt().lock().await;
        push_msg(&mut r, "assistant", &strip_tool_directive(&reply));
        push_msg(&mut r, "tool", &format!("[{name}] {}", clip_tool(&result)));
    }

    if final_text.is_empty() {
        final_text = format!("(berhenti tanpa jawaban setelah {MAX_ITERATIONS} langkah — coba pecah tugasnya)");
    }
    let final_clean = strip_tool_directive(&final_text);
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

    if approve_it {
        let result = loop_::exec_tool(root, &wd, &name, &args);
        let mut r = rt().lock().await;
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
