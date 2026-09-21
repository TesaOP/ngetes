//! Config — port BACA `src/shared/config.ts` (byte-compatible dengan
//! `data/config.json`). Stage 2: baru jalur BACA (load + mask + GET /api/config).
//! Tulis (saveConnections/saveTTS/dst) menyusul; sampai itu, server Bun tetap
//! pemilik tulisan selama transisi.
//!
//! Strategi byte-compat: pakai `serde_json::Value` mentah, TIDAK memodelkan tiap
//! field — merge section-level ({...DEFAULT, ...base}) persis seperti TS, mask
//! apiKey untuk UI, normalisasi roles. Nilai user tak pernah ditimpa default.

use std::path::Path;

use serde_json::{json, Value};

/// Role LLM yang dikenal (padanan LLM_ROLES di llm-client.ts).
const LLM_ROLES: &[&str] = &["chat", "motion", "sheet", "assistant"];

/// DEFAULT_CONFIG — cermin `src/shared/config.ts` (untuk backfill section baru).
pub fn default_config() -> Value {
    json!({
        "activeId": null,
        "overlay": { "enabled": true, "alpha": 0.9, "size": 1 },
        "connections": [],
        "tts": { "provider": "supertonic", "endpoint": "", "voice": "F1" },
        "events": {
            "idleSpeak": true, "idleMs": 1_800_000, "idleRepeatMs": 1_800_000,
            "awaySpeak": true, "returnSpeak": true, "awayHiddenMs": 10_000,
            "quietMs": 1_800_000
        },
        "camera": {
            "enabled": false, "fps": 0.4, "presenceThreshold": 0.4,
            "device": "webgpu", "model": "Xenova/facial_emotions_image_detection",
            "moodGraceMs": 20_000, "moodDebounceMs": 5000, "moodStableTicks": 2
        },
        "motion": { "enabled": false, "gain": 1.5 },
        "i18n": { "lang": "auto" },
        "stt": {
            "provider": "local", "engineModel": "base", "model": "Xenova/whisper-base",
            "language": "indonesian", "autoSend": true, "silenceMs": 1500,
            "maxMs": 30_000, "device": ""
        }
    })
}

/// Muat config dari `path`, merge section-level dengan default (base menang per
/// key top-level, key default yang absen diisi) — padanan `{...DEFAULT, ...base}`.
/// File hilang/rusak → default utuh (TS ensureDefault menulis default saat itu).
pub fn load(path: &Path) -> Value {
    let mut merged = default_config();
    if let Ok(text) = std::fs::read_to_string(path) {
        if let Ok(base) = serde_json::from_str::<Value>(&text) {
            if let (Some(m), Some(b)) = (merged.as_object_mut(), base.as_object()) {
                for (k, v) in b {
                    m.insert(k.clone(), v.clone()); // base menang (juga isi key ekstra)
                }
            }
        }
    }
    merged
}

/// Mask apiKey untuk UI (padanan maskKey): kosong / "MASUKKAN…" → apa adanya;
/// selain itu 6 char awal + 8 bullet + 4 char akhir. Berbasis char (bukan byte).
pub fn mask_key(k: &str) -> String {
    if k.is_empty() || k.starts_with("MASUKKAN") {
        return k.to_string();
    }
    let chars: Vec<char> = k.chars().collect();
    let head: String = chars.iter().take(6).collect();
    let tail: String = if chars.len() >= 4 {
        chars[chars.len() - 4..].iter().collect()
    } else {
        chars.iter().collect()
    };
    format!("{head}••••••••{tail}")
}

/// Normalisasi roles: array string, lowercase-trim, hanya role dikenal, unik.
pub fn normalize_roles(raw: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(arr) = raw.as_array() {
        for r in arr {
            if let Some(s) = r.as_str() {
                let k = s.trim().to_lowercase();
                if k.is_empty() || !LLM_ROLES.contains(&k.as_str()) {
                    continue;
                }
                if !out.contains(&k) {
                    out.push(k);
                }
            }
        }
    }
    out
}

/// Bangun payload GET /api/config (apiKey dimask, roles dinormalisasi) — persis
/// bentuk yang dikembalikan handler TS.
pub fn api_config_response(path: &Path) -> Value {
    let cfg = load(path);
    let empty_arr = Value::Array(vec![]);
    let conns = cfg.get("connections").unwrap_or(&empty_arr);
    let conns_out: Vec<Value> = conns
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|c| {
                    let mut o = c.clone();
                    if let Some(obj) = o.as_object_mut() {
                        if let Some(k) = obj.get("apiKey").and_then(|v| v.as_str()) {
                            if !k.is_empty() && !k.starts_with("MASUKKAN") {
                                obj.insert("apiKey".into(), json!(mask_key(k)));
                            }
                        }
                        let roles = obj.get("roles").cloned().unwrap_or(Value::Null);
                        obj.insert("roles".into(), json!(normalize_roles(&roles)));
                    }
                    o
                })
                .collect()
        })
        .unwrap_or_default();

    let mut tts_out = cfg.get("tts").cloned().unwrap_or_else(|| json!({}));
    if let Some(obj) = tts_out.as_object_mut() {
        if let Some(k) = obj.get("apiKey").and_then(|v| v.as_str()) {
            if !k.is_empty() {
                obj.insert("apiKey".into(), json!(mask_key(k)));
            }
        }
    }

    let sect = |k: &str| cfg.get(k).cloned().unwrap_or_else(|| json!({}));
    json!({
        "activeId": cfg.get("activeId").cloned().unwrap_or(Value::Null),
        "connections": conns_out,
        "tts": tts_out,
        "events": sect("events"),
        "camera": sect("camera"),
        "motion": sect("motion"),
        "stt": sect("stt"),
        "i18n": sect("i18n"),
        "overlay": sect("overlay"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_key_seperti_ts() {
        assert_eq!(mask_key(""), "");
        assert_eq!(mask_key("MASUKKAN_API_KEY"), "MASUKKAN_API_KEY");
        assert_eq!(mask_key("sk-abcdefghijklmnop"), "sk-abc••••••••mnop");
    }

    #[test]
    fn roles_hanya_yang_dikenal_unik() {
        let r = json!(["chat", "CHAT", " Motion ", "bogus", "assistant"]);
        assert_eq!(normalize_roles(&r), vec!["chat", "motion", "assistant"]);
        assert_eq!(normalize_roles(&json!("bukan-array")), Vec::<String>::new());
    }

    #[test]
    fn load_backfill_default_saat_file_hilang() {
        let cfg = load(Path::new("/tak/ada/config.json"));
        assert_eq!(cfg["tts"]["provider"], "supertonic");
        assert_eq!(cfg["stt"]["provider"], "local");
        assert_eq!(cfg["activeId"], Value::Null);
    }

    #[test]
    fn response_mask_apikey_koneksi() {
        // tulis config sementara
        let dir = std::env::temp_dir().join(format!("l2dcfgtest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("config.json");
        std::fs::write(
            &f,
            r#"{"activeId":"a","connections":[{"id":"a","apiKey":"sk-secretsecret1234","roles":["chat","bogus"]}],"tts":{"provider":"supertonic","apiKey":"tts-secretkey99"}}"#,
        )
        .unwrap();
        let resp = api_config_response(&f);
        let c0 = &resp["connections"][0];
        assert_eq!(c0["apiKey"], "sk-sec••••••••1234"); // dimask
        assert_eq!(c0["roles"], json!(["chat"])); // bogus dibuang
        assert_eq!(resp["tts"]["apiKey"], "tts-se••••••••ey99"); // tts dimask
        let _ = std::fs::remove_dir_all(&dir);
    }
}
