//! Animation director — port `handleAnimateText` (`/api/animate-text`).
//! LLM role "motion": pecah teks jadi segment + emosi/gesture/motion/intensity.
//! Validasi ketat: hanya emosi/gesture/motion yang didukung model yang lolos
//! (LLM tak boleh mengarang di luar daftar — Model-Agnostic).

use std::path::Path;

use serde_json::{json, Value};

use crate::{jsonx, llm};

const DEFAULT_EMOTIONS: &[&str] = &["senang", "sedih", "malu", "kaget", "normal"];
const DEFAULT_GESTURES: &[&str] = &[
    "nod", "shake", "tilt_curious", "lean_excited", "recoil_surprised",
    "look_away_shy", "laugh_bounce", "think", "wave_hi",
];

/// Buang karakter kontrol (padanan regex TS) + trim + potong.
fn clean_ctrl(s: &str, cap: usize) -> String {
    let cleaned: String = s
        .chars()
        .filter(|&c| {
            let u = c as u32;
            !((u <= 0x08) || u == 0x0B || u == 0x0C || (0x0E..=0x1F).contains(&u) || u == 0x7F)
        })
        .collect();
    cleaned.trim().chars().take(cap).collect()
}

/// Padanan sanitizePersonaText.
pub fn sanitize_persona_text(raw: &Value, cap: usize) -> String {
    raw.as_str().map(|s| clean_ctrl(s, cap)).unwrap_or_default()
}

/// Padanan formatParamNotes: {id:penjelasan} → baris "- \"id\": penjelasan".
pub fn format_param_notes(raw: &Value) -> String {
    let obj = match raw.as_object() {
        Some(o) => o,
        None => return String::new(),
    };
    let mut lines = Vec::new();
    for (id, val) in obj.iter().take(24) {
        if id.is_empty() {
            continue;
        }
        if let Some(s) = val.as_str() {
            let clean = clean_ctrl(s, 200);
            if !clean.is_empty() {
                let id_short: String = id.chars().take(60).collect();
                lines.push(format!("- \"{id_short}\": {clean}"));
            }
        }
    }
    lines.join("\n")
}

/// POST /api/animate-text — return {segments:[...]}. Selalu 200 (fallback aman).
pub async fn handle_animate_text(config_path: &Path, body: &Value) -> Value {
    let text = body.get("text").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if text.is_empty() {
        return json!({ "segments": [] });
    }
    let caps = body.get("capabilities").cloned().unwrap_or(json!({}));
    let emotions: Vec<String> = caps
        .get("emotions")
        .and_then(|v| v.as_array())
        .filter(|a| !a.is_empty())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_else(|| DEFAULT_EMOTIONS.iter().map(|s| s.to_string()).collect());
    let gestures: Vec<String> = caps
        .get("gestures")
        .and_then(|v| v.as_array())
        .filter(|a| !a.is_empty())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_else(|| DEFAULT_GESTURES.iter().map(|s| s.to_string()).collect());
    // motions: array {id, description?, compatibleEmotions?}
    let motions: Vec<Value> = caps
        .get("motions")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter(|m| m.get("id").and_then(|i| i.as_str()).is_some()).cloned().collect())
        .unwrap_or_default();

    let fallback = || json!({ "segments": [{ "text": text, "emotion": "normal", "gesture": "nod", "intensity": 0.7 }] });

    let note_lines = format_param_notes(&body.get("paramNotes").cloned().unwrap_or(Value::Null));
    let persona_lines = sanitize_persona_text(&body.get("persona").cloned().unwrap_or(Value::Null), 800);
    let char_name = sanitize_persona_text(&body.get("characterName").cloned().unwrap_or(Value::Null), 60);

    let motion_block = if !motions.is_empty() {
        let lines = motions.iter().take(24).map(|m| {
            let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let desc = m.get("description").and_then(|v| v.as_str()).unwrap_or(id);
            let compat = m.get("compatibleEmotions").and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", "))
                .filter(|s| !s.is_empty())
                .map(|s| format!(" (cocok saat: {s})")).unwrap_or_default();
            format!("- {id}: {desc}{compat}")
        }).collect::<Vec<_>>().join("\n");
        format!("Gerakan buatan user (Motion Studio) — pakai field \"motion\" dengan id PERSIS:\n{lines}\nGerakan ini dirancang user sendiri; utamakan bila maknanya pas. Jangan mengarang id.\n")
    } else {
        String::new()
    };
    let name_line = if char_name.is_empty() { String::new() } else { format!("\nKarakter yang kamu animasikan: {char_name}\n") };
    let note_block = if note_lines.is_empty() { String::new() } else { format!("\nPENJELASAN PARAMETER DARI USER (otoritatif — hormati makna ini):\n{note_lines}\n") };
    let persona_block = if persona_lines.is_empty() { String::new() } else { format!("\nKEPRIBADIAN KARAKTER (ditulis user — pilih emosi, gesture, dan intensity yang konsisten dengan kepribadian ini, jangan generik):\n{persona_lines}\n") };
    let motion_field = if !motions.is_empty() { "\n   - \"motion\": id gerakan user bila ada yang sangat pas (atau null)" } else { "" };

    let prompt = format!(
        "Kamu adalah animation director untuk karakter Live2D Anime yang hidup dan ekspresif.\n\
Karakter baru saja berbicara teks berikut:\n\"{text}\"\n{name_line}\
Daftar Emosi yang didukung model: [{emo}]\n\
Daftar Gesture yang tersedia: [{ges}]\n\
{motion_block}{note_block}{persona_block}\
TUGAS:\n\
1. Pecah teks di atas menjadi beberapa segment (per klausa atau per kalimat) agar karakter bergerak seirama omongannya secara hidup (jangan diam selama bicara!).\n\
2. Sebelum menentukan emotion/gesture, analisis dulu makna & nada tiap segment secara independen.\n\
3. Untuk setiap segment, tentukan:\n\
   - \"text\": teks klausa/kalimat tersebut (harus sama persis dengan teks asli bila digabung kembali)\n\
   - \"emotion\": emosi yang SANGAT SESUAI (dari daftar). Emosi WAJIB berubah mengikuti pergeseran nada teks.\n\
   - \"gesture\": nama gesture yang pas (atau null jika netral){motion_field}\n\
   - \"intensity\": angka 0.3 s/d 1.0 — sesuaikan naik-turun.\n\n\
ATURAN PENTING:\n\
- Nilai HARUS berdasarkan analisis makna teks asli, BUKAN meniru contoh format di bawah.\n\
- Jangan mengarang nama emotion/gesture di luar daftar.\n\n\
KEMBALIKAN HANYA JSON array valid tanpa markdown atau kata pengantar.\n\n\
Skema (bukan contoh isi — hanya struktur):\n\
[\n  {{ \"text\": \"<klausa 1>\", \"emotion\": \"<dari daftar>\", \"gesture\": \"<dari daftar atau null>\", \"intensity\": <0.3-1.0> }}\n]",
        emo = emotions.join(", "),
        ges = gestures.join(", "),
    );

    let msgs = vec![llm::ChatMessage { role: "user".into(), content: prompt }];
    let reply = match llm::llm_for_role(config_path, "motion", &msgs, "").await {
        Ok(ok) => ok.reply,
        Err(_) => return fallback(),
    };

    let parsed = jsonx::extract_json_array_loose(&reply);
    let ok_emotion: std::collections::HashSet<&str> = emotions.iter().map(String::as_str).collect();
    let ok_gesture: std::collections::HashSet<&str> = gestures.iter().map(String::as_str).collect();
    let ok_motion: std::collections::HashSet<String> =
        motions.iter().filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from)).collect();

    let mut segments: Vec<Value> = Vec::new();
    for s in &parsed {
        let t = s.get("text").and_then(|v| v.as_str()).unwrap_or("");
        if t.trim().is_empty() {
            continue;
        }
        let mut inten = s.get("intensity").and_then(|v| v.as_f64()).unwrap_or(0.7);
        if !inten.is_finite() {
            inten = 0.7;
        }
        inten = inten.clamp(0.3, 1.0);
        let emotion = s.get("emotion").and_then(|v| v.as_str()).filter(|e| ok_emotion.contains(e)).unwrap_or("normal");
        let gesture = s.get("gesture").and_then(|v| v.as_str()).filter(|g| ok_gesture.contains(g));
        let motion = s.get("motion").and_then(|v| v.as_str()).filter(|m| ok_motion.contains(*m));
        segments.push(json!({
            "text": t,
            "emotion": emotion,
            "gesture": gesture,
            "motion": motion,
            "intensity": inten
        }));
    }
    if segments.is_empty() {
        fallback()
    } else {
        json!({ "segments": segments })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn param_notes_dan_persona() {
        let notes = json!({ "ParamMouth": "kendali mulut", "x": "" });
        assert_eq!(format_param_notes(&notes), "- \"ParamMouth\": kendali mulut");
        assert_eq!(sanitize_persona_text(&json!("Lumine\u{0007} ceria"), 60), "Lumine ceria");
    }

    #[tokio::test]
    async fn animate_text_fallback_dan_validasi() {
        let dir = std::env::temp_dir().join(format!("l2ddir-{}-{}", std::process::id(), now()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("config.json");
        // mock LLM → reply bukan JSON array → fallback 1 segment
        std::fs::write(&f, r#"{"activeId":"m","connections":[{"id":"m","provider":"mock"}]}"#).unwrap();
        let out = handle_animate_text(&f, &json!({ "text": "Halo dunia" })).await;
        let segs = out["segments"].as_array().unwrap();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0]["emotion"], "normal");
        // teks kosong → segments []
        let empty = handle_animate_text(&f, &json!({ "text": "  " })).await;
        assert_eq!(empty["segments"].as_array().unwrap().len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn now() -> u128 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
    }
}
