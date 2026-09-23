//! Long-term memory lintas sesi.
//! Disimpan di `<root>/.agent-memory/memory.json` (BUKAN di data/ — kontrak path
//! dipertahankan). Model memanggil remember/recall eksplisit; ringkasan disuntik
//! ke system prompt. Cap 100 entri × 1200 char.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

const MAX_ENTRIES: usize = 100;
const MAX_VALUE: usize = 1200;

fn memory_file(root: &Path) -> PathBuf {
    root.join(".agent-memory").join("memory.json")
}

fn load_all(root: &Path) -> Vec<Value> {
    std::fs::read_to_string(memory_file(root))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|j| j.get("entries").and_then(|e| e.as_array()).cloned())
        .unwrap_or_default()
}

fn save_all(root: &Path, entries: &[Value]) -> std::io::Result<()> {
    let dir = root.join(".agent-memory");
    std::fs::create_dir_all(&dir)?;
    let text = serde_json::to_string_pretty(&json!({ "entries": entries })).unwrap_or_else(|_| "{\"entries\":[]}".into());
    std::fs::write(memory_file(root), text)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn take_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// remember(key, value) — simpan/timpa. Return pesan status (padanan TS).
pub fn remember(root: &Path, key: &str, value: &str) -> String {
    let k = take_chars(key.trim(), 60);
    let v = take_chars(value.trim(), MAX_VALUE);
    if k.is_empty() || v.is_empty() {
        return "ERROR: key dan value wajib diisi".into();
    }
    let mut entries: Vec<Value> = load_all(root).into_iter().filter(|e| e.get("key").and_then(|x| x.as_str()) != Some(k.as_str())).collect();
    entries.push(json!({ "key": k, "value": v, "ts": now_ms() }));
    while entries.len() > MAX_ENTRIES {
        entries.remove(0);
    }
    let _ = save_all(root, &entries);
    let head = take_chars(&v, 80);
    let ell = if v.chars().count() > 80 { "…" } else { "" };
    format!("Tersimpan di memory: [{k}] {head}{ell}")
}

/// recall(key?) — semua atau filter substring key.
pub fn recall(root: &Path, key: Option<&str>) -> String {
    let entries = load_all(root);
    if entries.is_empty() {
        return "(memory kosong)".into();
    }
    let fmt = |e: &Value| format!("[{}] {}", e.get("key").and_then(|x| x.as_str()).unwrap_or(""), e.get("value").and_then(|x| x.as_str()).unwrap_or(""));
    match key {
        Some(k) if !k.trim().is_empty() => {
            let kl = k.trim().to_lowercase();
            let hits: Vec<String> = entries.iter().filter(|e| e.get("key").and_then(|x| x.as_str()).map(|s| s.to_lowercase().contains(&kl)).unwrap_or(false)).map(fmt).collect();
            if hits.is_empty() {
                format!("(tidak ada memory dengan key \"{k}\")")
            } else {
                hits.join("\n")
            }
        }
        _ => entries.iter().map(fmt).collect::<Vec<_>>().join("\n"),
    }
}

/// Blok ringkasan memory untuk system prompt (24 terakhir, value ≤200).
pub fn memory_prompt_block(root: &Path) -> String {
    let entries = load_all(root);
    if entries.is_empty() {
        return String::new();
    }
    let start = entries.len().saturating_sub(24);
    let lines: Vec<String> = entries[start..]
        .iter()
        .map(|e| format!("- [{}] {}", e.get("key").and_then(|x| x.as_str()).unwrap_or(""), take_chars(e.get("value").and_then(|x| x.as_str()).unwrap_or(""), 200)))
        .collect();
    format!("\n=== MEMORY LINTAS SESI (ingatan eksplisit dari sesi-sesi sebelumnya) ===\n{}\n", lines.join("\n"))
}

/// GET /api/assistant/memory — daftar entri.
pub fn memory_list(root: &Path) -> Vec<Value> {
    load_all(root)
}

/// POST /api/assistant/memory/forget {key}.
pub fn memory_delete(root: &Path, key: &str) -> (u16, Value) {
    let k = key.trim();
    let entries = load_all(root);
    let next: Vec<Value> = entries.iter().filter(|e| e.get("key").and_then(|x| x.as_str()) != Some(k)).cloned().collect();
    if next.len() == entries.len() {
        return (200, json!({ "ok": false, "error": format!("memory tidak ditemukan: {k}") }));
    }
    if next.is_empty() {
        let _ = std::fs::remove_file(memory_file(root));
    } else {
        let _ = save_all(root, &next);
    }
    (200, json!({ "ok": true }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remember_recall_forget() {
        let root = std::env::temp_dir().join(format!("l2dmem-{}-{}", std::process::id(), now_ms()));
        std::fs::create_dir_all(&root).unwrap();

        assert_eq!(recall(&root, None), "(memory kosong)");
        let msg = remember(&root, "nama_user", "Farel");
        assert!(msg.contains("Farel"));
        remember(&root, "proyek", "live2d-agent");
        assert!(recall(&root, Some("nama")).contains("Farel"));
        assert!(recall(&root, Some("zzz")).contains("tidak ada"));
        assert_eq!(memory_list(&root).len(), 2);
        // timpa key sama → tetap 2
        remember(&root, "nama_user", "Yuda");
        assert_eq!(memory_list(&root).len(), 2);
        assert!(recall(&root, Some("nama")).contains("Yuda"));
        // forget
        assert_eq!(memory_delete(&root, "proyek").0, 200);
        assert_eq!(memory_list(&root).len(), 1);
        // prompt block
        assert!(memory_prompt_block(&root).contains("Yuda"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn remember_tolak_kosong() {
        let root = std::env::temp_dir().join(format!("l2dmem2-{}-{}", std::process::id(), now_ms()));
        assert!(remember(&root, "", "x").starts_with("ERROR"));
        assert!(remember(&root, "k", "  ").starts_with("ERROR"));
    }
}
