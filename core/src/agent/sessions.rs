//! Multi-session assistant — port `src/server/agent/sessions.ts`.
//! Store `data/assistant-sessions.json`: {active, sessions:[{id,name,workDir,
//! ts,messages}]}. cap 20 sesi (FIFO), history cap 60. Migrasi sekali dari
//! `data/assistant-history.json` (format lama) → satu sesi.
//!
//! State: dibaca/ditulis per-panggil (tak ada cache lintas-proses seperti TS,
//! karena core bisa single-run — tiap operasi load→mutate→save atomik).

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

const MAX_SESSIONS: usize = 20;
const MAX_HISTORY: usize = 60;

fn store_path(data_dir: &Path) -> PathBuf {
    data_dir.join("assistant-sessions.json")
}
fn legacy_path(data_dir: &Path) -> PathBuf {
    data_dir.join("assistant-history.json")
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn session_id() -> String {
    let ms = now_ms() as u128;
    let rnd: u32 = {
        // pseudo-acak sederhana dari waktu (cukup untuk id sesi lokal).
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
        n ^ (ms as u32)
    };
    format!("s_{}_{}", crate::config::base36_pub(ms), crate::config::base36_pub(rnd as u128))
}

fn take_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Nama sesi: pesan user pertama bermakna, else "Sesi YYYY-MM-DD".
fn derive_name(messages: &[Value]) -> String {
    let first = messages.iter().find(|m| {
        m.get("role").and_then(|r| r.as_str()) == Some("user")
            && m.get("content").and_then(|c| c.as_str()).map(|s| {
                let t = s.trim();
                !t.is_empty() && t != "Lanjutkan tugas berdasarkan hasil tool di atas."
            }).unwrap_or(false)
    });
    if let Some(m) = first {
        let raw = m.get("content").and_then(|c| c.as_str()).unwrap_or("");
        let collapsed: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        if !collapsed.is_empty() {
            return take_chars(&collapsed, 40);
        }
    }
    let now = chrono::Local::now();
    format!("Sesi {}", now.format("%Y-%m-%d"))
}

fn normalize(f: &Value) -> Value {
    let mut sessions: Vec<Value> = f
        .get("sessions")
        .and_then(|s| s.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|s| s.get("id").and_then(|i| i.as_str()).is_some())
                .map(|s| {
                    let msgs: Vec<Value> = s.get("messages").and_then(|m| m.as_array()).map(|a| {
                        let start = a.len().saturating_sub(MAX_HISTORY);
                        a[start..].to_vec()
                    }).unwrap_or_default();
                    json!({
                        "id": s.get("id").and_then(|i| i.as_str()).unwrap_or(""),
                        "name": take_chars(s.get("name").and_then(|n| n.as_str()).unwrap_or("Sesi"), 80),
                        "workDir": s.get("workDir").and_then(|w| w.as_str()).unwrap_or(""),
                        "ts": s.get("ts").and_then(|t| t.as_i64()).unwrap_or_else(now_ms),
                        "messages": msgs
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    if sessions.len() > MAX_SESSIONS {
        let start = sessions.len() - MAX_SESSIONS;
        sessions = sessions[start..].to_vec();
    }
    let active_in = f.get("active").and_then(|a| a.as_str()).unwrap_or("");
    let active = if sessions.iter().any(|s| s.get("id").and_then(|i| i.as_str()) == Some(active_in)) {
        active_in.to_string()
    } else {
        sessions.last().and_then(|s| s.get("id").and_then(|i| i.as_str())).unwrap_or("").to_string()
    };
    json!({ "active": active, "sessions": sessions })
}

/// Muat store (dengan migrasi sekali dari format lama bila perlu).
pub fn load(data_dir: &Path) -> Value {
    if let Ok(raw) = std::fs::read_to_string(store_path(data_dir)) {
        if let Ok(j) = serde_json::from_str::<Value>(&raw) {
            return normalize(&j);
        }
    }
    // migrasi legacy assistant-history.json
    if let Ok(raw) = std::fs::read_to_string(legacy_path(data_dir)) {
        if let Ok(j) = serde_json::from_str::<Value>(&raw) {
            if let Some(hist) = j.get("history").and_then(|h| h.as_array()) {
                if !hist.is_empty() {
                    let start = hist.len().saturating_sub(MAX_HISTORY);
                    let msgs = hist[start..].to_vec();
                    let rec = json!({
                        "id": session_id(),
                        "name": derive_name(&msgs),
                        "workDir": j.get("workDir").and_then(|w| w.as_str()).unwrap_or(""),
                        "ts": now_ms(),
                        "messages": msgs
                    });
                    let store = normalize(&json!({ "active": rec["id"], "sessions": [rec] }));
                    let _ = save(data_dir, &store);
                    let _ = std::fs::rename(legacy_path(data_dir), legacy_path(data_dir).with_extension("json.bak"));
                    return store;
                }
            }
        }
    }
    json!({ "active": "", "sessions": [] })
}

fn save(data_dir: &Path, store: &Value) -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    let path = store_path(data_dir);
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string(store).unwrap_or_else(|_| "{}".into()))?;
    std::fs::rename(&tmp, &path)
}

/// Ringkasan untuk UI (tanpa messages, +count).
pub fn list(data_dir: &Path) -> Value {
    let f = load(data_dir);
    let sessions: Vec<Value> = f.get("sessions").and_then(|s| s.as_array()).map(|arr| {
        arr.iter().map(|s| json!({
            "id": s["id"], "name": s["name"], "workDir": s["workDir"], "ts": s["ts"],
            "count": s.get("messages").and_then(|m| m.as_array()).map(|a| a.len()).unwrap_or(0)
        })).collect()
    }).unwrap_or_default();
    json!({ "active": f["active"], "sessions": sessions })
}

/// Buat sesi baru (jadi aktif). Return rekaman.
pub fn create(data_dir: &Path, work_dir: &str) -> Value {
    let mut f = load(data_dir);
    let rec = json!({
        "id": session_id(),
        "name": format!("Sesi {}", chrono::Local::now().format("%Y-%m-%d %H:%M")),
        "workDir": work_dir,
        "ts": now_ms(),
        "messages": []
    });
    let active = f["active"].as_str().unwrap_or("").to_string();
    let arr = f["sessions"].as_array_mut().unwrap();
    arr.push(rec.clone());
    if arr.len() > MAX_SESSIONS {
        if let Some(idx) = arr.iter().position(|s| s.get("id").and_then(|i| i.as_str()) != Some(active.as_str())) {
            arr.remove(idx);
        } else {
            arr.remove(0);
        }
    }
    f["active"] = rec["id"].clone();
    let _ = save(data_dir, &f);
    rec
}

/// Pindah sesi aktif. Return rekaman atau None.
pub fn switch_to(data_dir: &Path, id: &str) -> Option<Value> {
    let mut f = load(data_dir);
    let found = f["sessions"].as_array().unwrap().iter().find(|s| s.get("id").and_then(|i| i.as_str()) == Some(id)).cloned();
    if found.is_some() {
        f["active"] = json!(id);
        let _ = save(data_dir, &f);
    }
    found
}

/// Hapus sesi. Return (ok, new_active).
pub fn remove(data_dir: &Path, id: &str) -> (bool, String) {
    let mut f = load(data_dir);
    let arr = f["sessions"].as_array_mut().unwrap();
    let idx = arr.iter().position(|s| s.get("id").and_then(|i| i.as_str()) == Some(id));
    let idx = match idx {
        Some(i) => i,
        None => return (false, String::new()),
    };
    arr.remove(idx);
    if f["active"].as_str() == Some(id) {
        let next = f["sessions"].as_array().unwrap().last().and_then(|s| s.get("id").and_then(|i| i.as_str())).unwrap_or("").to_string();
        f["active"] = json!(next);
    }
    let new_active = f["active"].as_str().unwrap_or("").to_string();
    let _ = save(data_dir, &f);
    (true, new_active)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_switch_remove() {
        let data = std::env::temp_dir().join(format!("l2dsess-{}-{}", std::process::id(), now_ms()));
        std::fs::create_dir_all(&data).unwrap();

        assert_eq!(list(&data)["sessions"].as_array().unwrap().len(), 0);
        let a = create(&data, "/proj/a");
        let aid = a["id"].as_str().unwrap().to_string();
        assert!(aid.starts_with("s_"));
        assert_eq!(list(&data)["active"], json!(aid));
        let b = create(&data, "/proj/b");
        let bid = b["id"].as_str().unwrap().to_string();
        assert_eq!(list(&data)["active"], json!(bid));
        // switch balik ke a
        assert!(switch_to(&data, &aid).is_some());
        assert_eq!(list(&data)["active"], json!(aid));
        assert!(switch_to(&data, "nope").is_none());
        // hapus aktif (a) → aktif pindah ke sisa (b)
        let (ok, new_active) = remove(&data, &aid);
        assert!(ok);
        assert_eq!(new_active, bid);
        assert_eq!(list(&data)["sessions"].as_array().unwrap().len(), 1);

        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn migrasi_legacy() {
        let data = std::env::temp_dir().join(format!("l2dsessleg-{}-{}", std::process::id(), now_ms()));
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(legacy_path(&data), r#"{"workDir":"/x","history":[{"role":"user","content":"halo dunia"}]}"#).unwrap();
        let f = load(&data);
        assert_eq!(f["sessions"].as_array().unwrap().len(), 1);
        assert_eq!(f["sessions"][0]["name"], "halo dunia");
        assert!(store_path(&data).exists());
        let _ = std::fs::remove_dir_all(&data);
    }
}
