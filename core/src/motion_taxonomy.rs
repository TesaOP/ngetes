//! motion_taxonomy.rs — SIMPAN & SAJIKAN cache taksonomi motion (opsi B).
//!
//! Klasifikasi (verb/byVerb/stats) TIDAK dihitung di Rust: modul classifier
//! (`src/client/engine/motion-taxonomy.ts`) dipakai bersama server & bundle
//! browser, jadi klien yang menghitung (punya modulnya) lalu POST hasilnya ke
//! sini untuk dipersist. Server hanya cache-store — hindari implementasi kedua
//! yang bisa drift dari versi browser.
//!
//! Cache: `data/sheets/<sanitize(name)>.motions.json` (sama dgn jalur Bun).

use std::path::Path;

use serde_json::{json, Value};

use crate::sheet::sanitize_key;

fn cache_file(sheets_dir: &Path, name: &str) -> std::path::PathBuf {
    sheets_dir.join(format!("{}.motions.json", sanitize_key(name)))
}

/// GET /api/model/motion-taxonomy?name=X[&force=1] → (status, body JSON).
/// Sajikan cache bila ada (kecuali force). Bila belum ada → payload kosong
/// (`clipCount:0`) supaya klien jatuh ke klasifikasi lokal (name-only) tanpa
/// error — lalu klien boleh POST hasilnya untuk dipersist.
pub fn get(model_dir: &Path, sheets_dir: &Path, name: &str, force: bool) -> (u16, String) {
    if name.is_empty() {
        return (400, json!({ "error": "name kosong" }).to_string());
    }
    let dir = model_dir.join(name);
    if !dir.starts_with(model_dir) || !dir.exists() {
        return (400, json!({ "error": "model not found" }).to_string());
    }
    let cache = cache_file(sheets_dir, name);
    if !force {
        if let Ok(txt) = std::fs::read_to_string(&cache) {
            let clean = txt.strip_prefix('\u{feff}').unwrap_or(&txt);
            if serde_json::from_str::<Value>(clean).is_ok() {
                return (200, clean.to_string());
            }
        }
    }
    // Belum ada cache valid → payload kosong (klien klasifikasi lokal).
    (200, json!({
        "model": name,
        "generatedAt": chrono::Utc::now().to_rfc3339(),
        "clipCount": 0,
        "clips": [],
        "byVerb": {},
        "stats": {},
        "source": "none",
    }).to_string())
}

/// POST /api/model/motion-taxonomy {name, ...payload} → simpan payload sebagai
/// cache. Payload = hasil klasifikasi klien (byVerb/clips/stats/...).
pub fn store(model_dir: &Path, sheets_dir: &Path, body: &Value) -> (u16, String) {
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if name.is_empty() {
        return (400, json!({ "error": "name kosong" }).to_string());
    }
    let dir = model_dir.join(&name);
    if !dir.starts_with(model_dir) || !dir.exists() {
        return (400, json!({ "error": "model not found" }).to_string());
    }
    if std::fs::create_dir_all(sheets_dir).is_err() {
        return (400, json!({ "error": "gagal membuat folder sheets" }).to_string());
    }
    // Simpan payload apa adanya (pastikan berisi model+generatedAt).
    let mut payload = body.clone();
    if let Some(o) = payload.as_object_mut() {
        o.entry("model").or_insert(json!(name));
        o.insert("generatedAt".into(), json!(chrono::Utc::now().to_rfc3339()));
    }
    match crate::sheet::write_json_atomic(&cache_file(sheets_dir, &name), &payload) {
        Ok(()) => (200, json!({ "ok": true }).to_string()),
        Err(e) => (400, json!({ "error": e.to_string() }).to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("l2dtax-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn store_lalu_get() {
        let root = tmp();
        let model_dir = root.join("model");
        let sheets_dir = root.join("sheets");
        std::fs::create_dir_all(model_dir.join("hana")).unwrap();

        // model tak ada → 400
        assert_eq!(get(&model_dir, &sheets_dir, "nihil", false).0, 400);
        // belum ada cache → clipCount 0
        let (st, body) = get(&model_dir, &sheets_dir, "hana", false);
        assert_eq!(st, 200);
        assert!(body.contains("\"clipCount\":0"));

        // store payload klien
        let payload = json!({ "name": "hana", "clipCount": 3, "byVerb": { "nod": ["a"] }, "clips": [], "stats": {} });
        assert_eq!(store(&model_dir, &sheets_dir, &payload).0, 200);
        // get sekarang menyajikan cache (file di-pretty-print → parse, jangan
        // cocokkan substring mentah).
        let (st2, body2) = get(&model_dir, &sheets_dir, "hana", false);
        assert_eq!(st2, 200);
        let parsed: Value = serde_json::from_str(&body2).unwrap();
        assert_eq!(parsed["clipCount"], 3);
        assert!(parsed["byVerb"].get("nod").is_some());
        // force → bypass cache → payload kosong
        let (_stf, bodyf) = get(&model_dir, &sheets_dir, "hana", true);
        assert_eq!(serde_json::from_str::<Value>(&bodyf).unwrap()["clipCount"], 0);

        let _ = std::fs::remove_dir_all(&root);
    }
}
