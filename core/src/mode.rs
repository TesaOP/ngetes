//! Mode manager minimal — GET/POST `/api/mode`. Satu mode aktif (stage/vtuber/
//! assistant/pet). Padanan modeStatus()/handleModePost tapi RINGKAS: runtime
//! vtuber/assistant/pet belum diport ke core, jadi sub-status di sini stub
//! (running:false). Yang penting: GET membawa kunci "active" — dipakai probe
//! shell Tauri (is_our_server) untuk mengenali server ini, dan mode-runtime.js
//! saat boot. Diisi lengkap saat runtime mode diport.

use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};

fn active() -> &'static Mutex<String> {
    static A: OnceLock<Mutex<String>> = OnceLock::new();
    A.get_or_init(|| Mutex::new("stage".to_string()))
}

const VALID: &[&str] = &["stage", "vtuber", "assistant", "pet"];

/// Status mode (kunci "active" wajib ada — dipakai probe shell).
pub fn status() -> Value {
    let cur = active().lock().map(|g| g.clone()).unwrap_or_else(|_| "stage".into());
    json!({
        "active": cur,
        "vtuber": { "running": false, "provider": null, "channel": null, "respond": false, "eventCount": 0 },
        "assistant": { "running": false, "busy": false, "workDir": null, "historyCount": 0, "pendingApprovals": [], "plan": [], "tools": [] },
        "pet": { "running": false, "clickThrough": false, "shell": null },
        "note": "core: runtime mode (vtuber/assistant/pet) belum diport — sub-status stub"
    })
}

/// POST /api/mode {mode} — set mode aktif (in-memory). Return status.
pub fn set_mode(body: &Value) -> (u16, Value) {
    let mode = body.get("mode").and_then(|v| v.as_str()).unwrap_or("");
    if !VALID.contains(&mode) {
        return (400, json!({ "error": format!("mode tidak dikenal: {mode}") }));
    }
    if let Ok(mut g) = active().lock() {
        *g = mode.to_string();
    }
    (200, status())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_dan_status() {
        assert_eq!(status()["active"], "stage");
        let (st, _) = set_mode(&json!({ "mode": "vtuber" }));
        assert_eq!(st, 200);
        assert_eq!(status()["active"], "vtuber");
        assert_eq!(set_mode(&json!({ "mode": "bogus" })).0, 400);
        // balik ke stage utk test lain deterministik
        set_mode(&json!({ "mode": "stage" }));
    }
}
