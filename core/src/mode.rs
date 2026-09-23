//! Mode manager — GET/POST `/api/mode`. Satu mode aktif (stage/vtuber/assistant/
//! pet). Sub-status vtuber & pet nyata; assistant ringkas di sini (status penuh
//! di `/api/assistant/status`). GET membawa kunci "active" — dipakai probe shell
//! Tauri (is_our_server) untuk mengenali server ini, dan mode-runtime.js saat boot.

use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};

fn active() -> &'static Mutex<String> {
    static A: OnceLock<Mutex<String>> = OnceLock::new();
    A.get_or_init(|| Mutex::new("stage".to_string()))
}

const VALID: &[&str] = &["stage", "vtuber", "assistant", "pet"];

/// Status mode (kunci "active" wajib ada — dipakai probe shell). Sub-status
/// vtuber SUDAH nyata (runtime diport); assistant/pet masih stub sampai diport.
pub fn status() -> Value {
    let cur = active().lock().map(|g| g.clone()).unwrap_or_else(|_| "stage".into());
    json!({
        "active": cur,
        "vtuber": crate::vtuber::status(),
        "assistant": { "running": false, "busy": false, "workDir": null, "historyCount": 0, "pendingApprovals": [], "plan": [], "tools": [] },
        "pet": crate::pet::status(),
        "note": "core: sub-status assistant di /api/assistant/status (mode manager sinkron)"
    })
}

/// Teardown runtime mode lama (padanan teardownMode TS). Saat ini hanya VTuber
/// yang benar-benar satu-aktif; assistant/pet layanan mandiri.
fn teardown(mode: &str) {
    if mode == "vtuber" {
        crate::vtuber::stop();
    }
}

/// Set mode aktif in-memory + teardown mode lama bila berganti. Dipakai
/// post_mode dan post_vtuber_start (yang mengunci "vtuber").
pub fn set_active(mode: &str) {
    let prev = active().lock().map(|g| g.clone()).unwrap_or_else(|_| "stage".into());
    if prev == mode {
        return;
    }
    teardown(&prev);
    if let Ok(mut g) = active().lock() {
        *g = mode.to_string();
    }
}

/// POST /api/mode {mode} — set mode aktif (in-memory). Return status.
pub fn set_mode(body: &Value) -> (u16, Value) {
    let mode = body.get("mode").and_then(|v| v.as_str()).unwrap_or("");
    if !VALID.contains(&mode) {
        return (400, json!({ "error": format!("mode tidak dikenal: {mode}") }));
    }
    set_active(mode);
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
