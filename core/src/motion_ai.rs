//! Endpoint LLM-role "motion" — port `handleMotionsAnalyze`
//! (`POST /api/motions/analyze`): tebak makna satu motion → deskripsi + tag +
//! kompatibilitas emosi. Echo-retry + validasi emosi (hanya dari daftar).
//!
//! CATATAN: `/api/motions/generate` BELUM diport — butuh `sanitizeMotionAsset`
//! dari client motion-dsl (belum ada di Rust). Bun pemilik jalur itu.

use std::path::Path;

use serde_json::{json, Value};

use crate::{jsonx, llm};

const DEFAULT_EMOTIONS: &[&str] = &["senang", "sedih", "malu", "kaget", "normal"];

fn strip_fences(text: &str) -> String {
    text.replace("```json", "").replace("```JSON", "").replace("```", "").trim().to_string()
}

/// POST /api/motions/analyze — (status, body JSON). Selalu 200 kecuali input
/// invalid (400) / tak ada koneksi (503, ditangani llm_for_role → warning).
pub async fn analyze_motion(config_path: &Path, body: &Value) -> (u16, String) {
    let m = body.get("motion").cloned().unwrap_or(json!({}));
    let emotions: Vec<String> = body
        .get("emotions")
        .and_then(|v| v.as_array())
        .filter(|a| !a.is_empty())
        .map(|a| a.iter().take(12).map(|x| x.as_str().unwrap_or("").to_string()).collect())
        .unwrap_or_else(|| DEFAULT_EMOTIONS.iter().map(|s| s.to_string()).collect());

    // tracks: {target, range[min,max], keyframes}
    struct Tr {
        target: String,
        lo: f64,
        hi: f64,
        keyframes: usize,
    }
    let mut tracks: Vec<Tr> = Vec::new();
    if let Some(arr) = m.get("tracks").and_then(|v| v.as_array()) {
        for tr in arr {
            let target = tr
                .get("label").and_then(|v| v.as_str())
                .or_else(|| tr.get("param").and_then(|v| v.as_str()))
                .or_else(|| tr.get("target").and_then(|v| v.as_str()))
                .or_else(|| tr.get("field").and_then(|v| v.as_str()))
                .unwrap_or("")
                .to_string();
            if target.is_empty() {
                continue;
            }
            let keys = tr.get("keys").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let vals: Vec<f64> = keys.iter().filter_map(|k| k.get("v").and_then(|v| v.as_f64())).collect();
            let (lo, hi) = if vals.is_empty() {
                (0.0, 0.0)
            } else {
                (vals.iter().cloned().fold(f64::INFINITY, f64::min), vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max))
            };
            tracks.push(Tr { target, lo, hi, keyframes: keys.len() });
        }
    }
    if tracks.is_empty() {
        return (400, json!({ "error": "motion tanpa track" }).to_string());
    }

    let duration = m.get("duration").and_then(|v| v.as_f64()).filter(|d| *d > 0.0).unwrap_or(1.0);
    let track_lines = tracks
        .iter()
        .map(|t| format!("- {}: rentang {}..{}, {} keyframe", t.target, t.lo, t.hi, t.keyframes))
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = format!(
        "Kamu menganalisa satu gerakan (motion) karakter Live2D.\n\
Data gerakan (peran semantik, bukan parameter mentah):\n\
durasi: {duration} detik\n{track_lines}\n\n\
Nama track bisa peran singkat atau nama parameter rig:\n\
ax=kepala kiri/kanan, ay=kepala atas/bawah, bodyZ=badan miring, bodyX/bodyY=badan geser,\n\
ex/ey=arah bola mata, mouthForm=bentuk mulut. Nama lain = parameter rig (tebak dari namanya).\n\
Baca rentang tiap track, jangan asumsikan derajat.\n\n\
TUGAS: tebak gerakan ini menyampaikan apa, lalu balas JSON:\n\
{{\n  \"description\": \"satu kalimat Indonesia, maks 120 karakter\",\n  \"tags\": [\"3-5 tag Indonesia satu kata\"],\n  \"emotionCompatibility\": {{ \"<emosi>\": 0.0-1.0 }}\n}}\n\
Emosi yang boleh HANYA: [{emo}]\n\
KEMBALIKAN HANYA JSON. MULAI dengan {{ dan AKHIRI dengan }}. JANGAN mengulang instruksi.",
        emo = emotions.join(", "),
    );

    let echoed = |clean: &str| -> bool {
        let low = clean.to_lowercase();
        low.contains("balas json") || low.contains("kamu menganalisa")
    };

    let msgs1 = vec![llm::ChatMessage { role: "user".into(), content: prompt.clone() }];
    let reply = match llm::llm_for_role(config_path, "motion", &msgs1, "").await {
        Ok(ok) => ok.reply,
        Err((_, msg)) => return (200, json!({ "warning": msg }).to_string()),
    };
    let mut clean = strip_fences(&reply);
    let mut parsed = jsonx::extract_json_object_loose(&reply);

    if parsed.is_none() || echoed(&clean) {
        let msgs2 = vec![
            llm::ChatMessage { role: "user".into(), content: prompt.clone() },
            llm::ChatMessage { role: "assistant".into(), content: clean.chars().take(2000).collect() },
            llm::ChatMessage {
                role: "user".into(),
                content: "Balasanmu tadi salah: kamu mengulang instruksi. Balas HANYA objek JSON {description, tags, emotionCompatibility} — mulai dengan { langsung.".into(),
            },
        ];
        if let Ok(ok2) = llm::llm_for_role(config_path, "motion", &msgs2, "").await {
            let p2 = jsonx::extract_json_object_loose(&ok2.reply);
            clean = strip_fences(&ok2.reply);
            if p2.is_some() {
                parsed = p2;
            } else if echoed(&clean) {
                parsed = None;
            }
        }
    }

    let parsed = match parsed {
        Some(p) => p,
        None => {
            let extra = if echoed(&clean) { " (model mengulang instruksi dua kali — coba lagi / pakai model lain)" } else { "" };
            return (200, json!({ "warning": format!("AI tidak mengembalikan JSON valid{extra}") }).to_string());
        }
    };

    let ok_emo: std::collections::HashSet<&str> = emotions.iter().map(String::as_str).collect();
    let mut emo = serde_json::Map::new();
    if let Some(ec) = parsed.get("emotionCompatibility").and_then(|v| v.as_object()) {
        for (k, v) in ec {
            if ok_emo.contains(k.as_str()) {
                if let Some(n) = v.as_f64() {
                    emo.insert(k.clone(), json!(n.clamp(0.0, 1.0)));
                }
            }
        }
    }
    let tags: Vec<String> = parsed
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .take(5)
                .filter_map(|t| t.as_str())
                .map(|s| s.trim().to_lowercase().chars().take(30).collect::<String>())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let description: String = parsed.get("description").and_then(|v| v.as_str()).unwrap_or("").trim().chars().take(200).collect();

    (200, json!({ "description": description, "tags": tags, "emotionCompatibility": emo, "source": "ai" }).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn analyze_tanpa_track_400() {
        let dir = std::env::temp_dir().join(format!("l2dmai-{}-{}", std::process::id(), now()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("config.json");
        std::fs::write(&f, r#"{"activeId":"m","connections":[{"id":"m","provider":"mock"}]}"#).unwrap();
        let (st, _) = analyze_motion(&f, &json!({ "motion": { "tracks": [] } })).await;
        assert_eq!(st, 400);
        // ada track → mock echo → 200 warning (bukan crash)
        let (st2, body) = analyze_motion(&f, &json!({ "motion": { "duration": 1.2, "tracks": [{ "target": "ay", "keys": [{ "t": 0, "v": 0 }, { "t": 0.4, "v": 8 }] }] } })).await;
        assert_eq!(st2, 200);
        assert!(body.contains("warning") || body.contains("description"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn now() -> u128 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
    }
}
