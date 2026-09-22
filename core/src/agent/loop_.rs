//! Agent loop — port `src/server/agent/loop.ts` (bagian inti).
//! build_system (system prompt + katalog tool), detect_tool_call (parse longgar),
//! exec_tool (dispatch), tool_level. Loop + permission gate + SSE di batch 3d-5.

use std::path::Path;

use serde_json::{json, Value};

use crate::agent::{memory, tools};

/// Definisi tool: nama, deskripsi param (utk prompt), level.
pub struct ToolDef {
    pub name: &'static str,
    pub params: &'static str,
    pub level: &'static str,
}

/// Katalog tool. Yang belum diport (update_plan/subagent/browser_*) ditandai
/// tapi exec-nya membalas ERROR "belum diport" — LLM diberi tahu di prompt hanya
/// yang aktif supaya tak memanggil yang belum ada.
pub const TOOLS: &[ToolDef] = &[
    ToolDef { name: "list_dir", params: "path: string, default '.'", level: "safe" },
    ToolDef { name: "read_file", params: "path: string", level: "safe" },
    ToolDef { name: "search_code", params: "query: string, path: string opsional", level: "safe" },
    ToolDef { name: "git_diff", params: "", level: "safe" },
    ToolDef { name: "update_plan", params: "todos: [{id,task,status,note?}], reason?: string (wajib saat revisi rencana)", level: "safe" },
    ToolDef { name: "write_file", params: "path: string, content: string", level: "mutating" },
    ToolDef { name: "edit_file", params: "path: string, old: string, new: string", level: "mutating" },
    ToolDef { name: "delete_file", params: "path: string", level: "mutating" },
    ToolDef { name: "run_command", params: "command: string", level: "mutating" },
    ToolDef { name: "remember", params: "key: string, value: string", level: "safe" },
    ToolDef { name: "recall", params: "key: string opsional", level: "safe" },
    ToolDef { name: "spawn_subagent", params: "tasks: [{task: deskripsi goal}] — delegasi riset/analisa INDEPENDEN ke subagent read-only paralel (maks 4)", level: "safe" },
    ToolDef { name: "browser_status", params: "", level: "safe" },
    ToolDef { name: "browser_open", params: "url: string opsional (default https://example.com) — buka browser terisolasi", level: "mutating" },
    ToolDef { name: "browser_navigate", params: "url: string", level: "mutating" },
    ToolDef { name: "browser_inspect", params: "cursor: number opsional, maxChars: number opsional (maks 3500), snapshotId: string opsional", level: "safe" },
    ToolDef { name: "browser_click", params: "snapshotId: string, ref: string (dari inspect terakhir)", level: "mutating" },
    ToolDef { name: "browser_type", params: "snapshotId: string, ref: string, text: string, submit: boolean opsional", level: "mutating" },
    ToolDef { name: "browser_history", params: "action: back|forward|reload", level: "mutating" },
    ToolDef { name: "browser_close", params: "", level: "mutating" },
    ToolDef { name: "browser_grant_private", params: "origin: http/https tanpa path (izinkan localhost/LAN sesi ini)", level: "mutating" },
];

/// True bila tool dijalankan lewat jalur async browser manager (bukan exec_tool sinkron).
pub fn is_browser_tool(name: &str) -> bool {
    name.starts_with("browser_")
}

pub fn tool_level(name: &str) -> Option<&'static str> {
    TOOLS.iter().find(|t| t.name == name).map(|t| t.level)
}

/// System prompt agent (id/en) + katalog tool + folder kerja.
pub fn build_system(lang: &str, work_dir: &str) -> String {
    let en = lang == "en";
    let head = if en {
        "You are a local AI agent (like a coding agent) appearing as the user's Live2D desktop character. Your job is to COMPLETE the user's request in the working folder — not to chat. Style: concise, friendly."
    } else {
        "Kamu adalah AI agent lokal (seperti coding-agent) yang tampil sebagai karakter Live2D di desktop user. Tugasmu MENYELESAIKAN permintaan user di folder kerja — bukan mengobrol. Gaya: ringkas, padat, ramah."
    };
    let tools_hdr = if en {
        "TOOLS — to call one, reply with EXACTLY one line like this (valid JSON, no markdown):"
    } else {
        "TOOLS — untuk memanggil, balas DENGAN PERSIS satu baris ini (JSON valid, tanpa markdown):"
    };
    let rules: &[&str] = if en {
        &[
            "0. TOOL CALL FORMAT MUST be exactly as shown. FORBIDDEN: <tool_call>, XML, ```json.",
            "1. UNDERSTAND FIRST. Questions needing no files/commands are answered DIRECTLY — no tool.",
            "2. If you need data, call a tool. At most ONE short plan sentence before the TOOL: line.",
            "3. Every '[hasil tool] …' MUST be followed up: next tool or final answer. NEVER repeat same tool+args. If a tool FAILS, try another approach.",
            "4. FINAL reply: what you did + key findings. Max ~4 sentences, no markdown.",
            "5. Task done → stop calling tools. If unclear, ask ONE specific question.",
            "6. NEVER write/delete beyond the request or run destructive commands — mutating tools ask permission.",
            "9. MEMORY: user prefs / key decisions → remember (short key). Need context → recall.",
        ]
    } else {
        &[
            "0. FORMAT PANGGILAN TOOL WAJIB persis seperti contoh. DILARANG <tool_call>, XML, ```json.",
            "1. PAHAMI DULU. Pertanyaan tanpa file/perintah dijawab LANGSUNG — tanpa tool.",
            "2. Kalau butuh data, panggil tool. Maks SATU kalimat rencana sebelum baris TOOL:.",
            "3. Tiap '[hasil tool] …' WAJIB dilanjutkan: tool berikutnya atau jawaban final. JANGAN ulang tool+arg sama. Kalau GAGAL, coba pendekatan lain.",
            "4. Jawaban FINAL: apa yang dikerjakan + temuan penting. Maks ~4 kalimat, tanpa markdown.",
            "5. Tugas selesai → berhenti memanggil tool. Kalau tak jelas, tanya SEKALI yang spesifik.",
            "6. DILARANG menulis/menghapus di luar kebutuhan atau perintah merusak — tool mutating minta izin.",
            "9. MEMORY: preferensi/keputusan penting → remember (key singkat). Butuh konteks → recall.",
        ]
    };
    let final_line = if en {
        "Reply in the SAME language the user used. Technical terms (file names, commands) stay as-is."
    } else {
        "Balas dalam bahasa yang SAMA dengan user. Sebutan teknis (nama file, perintah) tetap apa adanya."
    };
    let mut lines = vec![head.to_string(), String::new(), format!("Folder kerja: {work_dir}"), String::new(), tools_hdr.to_string()];
    for t in TOOLS {
        lines.push(format!("TOOL: {} {{{}}} — level: {}", t.name, t.params, t.level));
    }
    lines.push(String::new());
    lines.push(if en { "RULES:".into() } else { "ATURAN:".into() });
    for r in rules {
        lines.push(r.to_string());
    }
    lines.push(String::new());
    lines.push(final_line.to_string());
    lines.join("\n")
}

/// Deteksi tool call dari balasan LLM (format bebas → parse longgar).
/// Padanan detectToolCall.
pub fn detect_tool_call(reply: &str) -> Option<(String, Value)> {
    // buang code fence + bold
    let clean = reply.replace("```", "\n").replace("**", "");
    let lower = clean.to_lowercase();
    // nama tool terpanjang dulu (hindari "recall" match sebelum "read_file" dst).
    let mut names: Vec<&str> = TOOLS.iter().map(|t| t.name).collect();
    names.sort_by_key(|n| std::cmp::Reverse(n.len()));
    for name in names {
        if let Some(idx) = lower.find(name) {
            let after_start = idx + name.len();
            let window: String = clean[after_start..].chars().take(160).collect();
            // objek {...} pertama tanpa nested (regex \{[^{}]*\})
            if let Some(obj) = first_brace_obj(&window) {
                if let Ok(v) = serde_json::from_str::<Value>(&obj) {
                    return Some((name.to_string(), v));
                }
                // loose: kunci tanpa quote + kutip tunggal
                let loose = looseify(&obj);
                if let Ok(v) = serde_json::from_str::<Value>(&loose) {
                    return Some((name.to_string(), v));
                }
            }
        }
    }
    None
}

fn first_brace_obj(s: &str) -> Option<String> {
    let start = s.find('{')?;
    let rest = &s[start..];
    let end = rest.find('}')?;
    Some(rest[..=end].to_string())
}

fn looseify(obj: &str) -> String {
    // {key: → {"key":  dan ' → "
    let mut out = String::with_capacity(obj.len() + 8);
    let bytes: Vec<char> = obj.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == '{' || c == ',' {
            out.push(c);
            // skip ws
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_whitespace() {
                out.push(bytes[j]);
                j += 1;
            }
            // ident diikuti ':' → quote
            let ks = j;
            while j < bytes.len() && (bytes[j].is_alphanumeric() || bytes[j] == '_') {
                j += 1;
            }
            if j > ks && j < bytes.len() && {
                let mut k = j;
                while k < bytes.len() && bytes[k].is_whitespace() {
                    k += 1;
                }
                k < bytes.len() && bytes[k] == ':'
            } {
                out.push('"');
                out.extend(&bytes[ks..j]);
                out.push('"');
                i = j;
                continue;
            } else {
                out.extend(&bytes[ks..j]);
                i = j;
                continue;
            }
        } else if c == '\'' {
            out.push('"');
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

/// Eksekusi satu tool. `root` = app root (utk memory), `work_dir` = folder kerja.
/// Selalu mengembalikan String (ERROR: … bukan panic) — sesuai kontrak loop.
pub fn exec_tool(root: &Path, work_dir: &Path, name: &str, args: &Value) -> String {
    let s = |v: &Value, k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let res: Result<String, String> = match name {
        "list_dir" => tools::list_dir(work_dir, &s(args, "path")),
        "read_file" => tools::read_file(work_dir, &s(args, "path")),
        "search_code" => {
            let path = args.get("path").and_then(|x| x.as_str()).unwrap_or(".");
            tools::search_code(work_dir, &s(args, "query"), path)
        }
        "git_diff" => tools::git_diff(work_dir),
        "write_file" => tools::write_file(work_dir, &s(args, "path"), &s(args, "content")),
        "edit_file" => tools::edit_file(work_dir, &s(args, "path"), &s(args, "old"), &s(args, "new")),
        "delete_file" => tools::delete_file(work_dir, &s(args, "path")),
        "run_command" => Ok(tools::run_command(work_dir, &s(args, "command"))),
        "remember" => Ok(memory::remember(root, &s(args, "key"), &s(args, "value"))),
        "recall" => {
            let key = args.get("key").and_then(|x| x.as_str());
            Ok(memory::recall(root, key))
        }
        other => Err(format!("tool belum diport ke core: {other}")),
    };
    match res {
        Ok(t) => t,
        Err(e) => {
            if e.starts_with("ERROR") {
                e
            } else {
                format!("ERROR: {e}")
            }
        }
    }
}

/// Argumen tool yang aman ditampilkan ke UI (redaksi ringan). Padanan publicToolArgs.
pub fn public_tool_args(name: &str, args: &Value) -> Value {
    if is_browser_tool(name) {
        return crate::browser::public_args(name, args);
    }
    args.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_berbagai_format() {
        let (n, a) = detect_tool_call("baiklah. TOOL: read_file {\"path\": \"a.txt\"}").unwrap();
        assert_eq!(n, "read_file");
        assert_eq!(a["path"], "a.txt");
        // loose: kunci tanpa quote
        let (n2, a2) = detect_tool_call("list_dir {path: 'src'}").unwrap();
        assert_eq!(n2, "list_dir");
        assert_eq!(a2["path"], "src");
        // tanpa tool
        assert!(detect_tool_call("halo, apa kabar?").is_none());
    }

    #[test]
    fn exec_dispatch_fs_dan_run() {
        let d = std::env::temp_dir().join(format!("l2dloop-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&d).unwrap();
        let out = exec_tool(&d, &d, "write_file", &json!({ "path": "x.txt", "content": "hai" }));
        assert!(out.contains("char"));
        assert_eq!(exec_tool(&d, &d, "read_file", &json!({ "path": "x.txt" })), "hai");
        // run_command aman (echo)
        let echo = if cfg!(windows) { "echo halo" } else { "echo halo" };
        let r = exec_tool(&d, &d, "run_command", &json!({ "command": echo }));
        assert!(r.to_lowercase().contains("halo"), "{r}");
        // tool belum diport
        assert!(exec_tool(&d, &d, "browser_open", &json!({})).contains("belum diport"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn system_prompt_muat_tool() {
        let p = build_system("id", "/proj");
        assert!(p.contains("Folder kerja: /proj"));
        assert!(p.contains("TOOL: run_command"));
        assert!(p.contains("level: mutating"));
    }
}
