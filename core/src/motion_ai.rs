//! Endpoint LLM-role "motion": `/api/motions/analyze` (tebak makna satu motion
//! → deskripsi + tag + kompatibilitas emosi) dan `/api/motions/generate` (buat
//! motion dari teks). Echo-retry + validasi emosi + sanitize via motion_dsl.

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

/// POST /api/motions/generate — buat motion dari deskripsi user (role "motion")
/// + echo-retry + sanitize (motion_dsl). Return (status, body JSON). Tidak
/// menulis ke disk — klien menerima {motion} lalu menyimpan lewat PUT.
pub async fn generate_motion(config_path: &Path, body: &Value) -> (u16, String) {
    let desc: String = body.get("prompt").and_then(|v| v.as_str()).unwrap_or("").trim().chars().take(300).collect();
    if desc.is_empty() {
        return (400, json!({ "error": "prompt kosong" }).to_string());
    }
    let emotions: Vec<String> = body
        .get("emotions")
        .and_then(|v| v.as_array())
        .filter(|a| !a.is_empty())
        .map(|a| a.iter().take(12).map(|x| x.as_str().unwrap_or("").to_string()).collect())
        .unwrap_or_else(|| DEFAULT_EMOTIONS.iter().map(|s| s.to_string()).collect());

    let prompt = format!(
        "Kamu membuat gerakan (motion) untuk karakter Live2D dari deskripsi user.\n\
Permintaan user: \"{desc}\"\n\n\
Kamu HANYA boleh memakai nama track berikut. Ini nama PERAN, bukan nama parameter\n\
model — klien yang akan menerjemahkannya ke parameter rig yang sesuai:\n\
ax    = kepala kiri(-)/kanan(+), derajat, batas ±30\n\
ay    = kepala atas(-)/bawah(+), derajat, batas ±30\n\
bodyZ = badan miring, derajat, batas ±30\n\
bodyX = badan geser kiri/kanan, derajat, batas ±30\n\
bodyY = badan naik/turun, derajat, batas ±30\n\
ex    = bola mata kiri(-)/kanan(+), −1..1\n\
ey    = bola mata atas(-)/bawah(+), −1..1\n\
mouthForm = bentuk mulut, −1..1\n\n\
JANGAN menyebut nama parameter model seperti ParamAngleX atau ParamHairFront —\n\
kamu tidak tahu nama parameter rig ini dan menebaknya akan ditolak.\n\n\
Aturan:\n\
- Maksimal 4 track, maksimal 6 keyframe per track.\n\
- t dalam detik, mulai 0, tidak melebihi durasi.\n\
- Durasi 0.6 sampai 3 detik.\n\
- Gerakan yang bagus PULANG ke 0 di keyframe terakhir supaya tidak nyangkut.\n\
- Nilai realistis: ±5..15 derajat untuk kepala, ±0.2..0.6 untuk mata.\n\n\
Balas JSON persis format ini:\n\
{{\n  \"id\": \"nama_id_snake_case\",\n  \"name\": \"Nama Singkat\",\n  \"description\": \"satu kalimat bahasa Indonesia\",\n  \"tags\": [\"dua-empat tag\"],\n  \"duration\": 1.4,\n  \"emotionCompatibility\": {{ \"<emosi>\": 0.0-1.0 }},\n  \"tracks\": [\n    {{ \"target\": \"ay\", \"keys\": [{{ \"t\": 0, \"v\": 0 }}, {{ \"t\": 0.4, \"v\": 8 }}, {{ \"t\": 1.4, \"v\": 0 }}] }}\n  ]\n}}\n\
Emosi yang boleh dipakai HANYA: [{emo}]\n\
KEMBALIKAN HANYA JSON. MULAI balasanmu langsung dengan {{ dan AKHIRI dengan }} — JANGAN mengulang instruksi ini.",
        emo = emotions.join(", "),
    );

    let echoed = |clean: &str| -> bool {
        let low = clean.to_lowercase();
        low.contains("balas json") || low.contains("permintaan user")
    };

    let msgs1 = vec![llm::ChatMessage { role: "user".into(), content: prompt.clone() }];
    let reply = match llm::llm_for_role(config_path, "motion", &msgs1, "").await {
        Ok(ok) => ok.reply,
        Err((_, msg)) => return (200, json!({ "error": msg }).to_string()),
    };
    let mut clean = strip_fences(&reply);
    let mut parsed = jsonx::extract_json_object_loose(&reply);

    if parsed.is_none() || echoed(&clean) {
        let msgs2 = vec![
            llm::ChatMessage { role: "user".into(), content: prompt.clone() },
            llm::ChatMessage { role: "assistant".into(), content: clean.chars().take(2000).collect() },
            llm::ChatMessage {
                role: "user".into(),
                content: "Balasanmu tadi salah: kamu mengulang instruksi. Balas HANYA objek JSON motion (id, name, duration, tracks, emotionCompatibility) — mulai dengan karakter { langsung, tanpa mengulang instruksi.".into(),
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

    let mut parsed = match parsed {
        Some(p) if p.is_object() => p,
        _ => {
            let extra = if echoed(&clean) { " (model mengulang instruksi dua kali — coba lagi / pakai model lain)" } else { "" };
            return (200, json!({ "error": format!("AI tidak mengembalikan JSON valid{extra}") }).to_string());
        }
    };

    // Buang emosi di luar daftar; normalisasi id snake_case.
    let ok_emo: std::collections::HashSet<&str> = emotions.iter().map(String::as_str).collect();
    if let Some(ec) = parsed.get_mut("emotionCompatibility").and_then(|v| v.as_object_mut()) {
        ec.retain(|k, _| ok_emo.contains(k.as_str()));
    }
    let id_src = parsed.get("id").and_then(|v| v.as_str()).map(String::from).unwrap_or_else(|| desc.clone());
    let mut id: String = id_src
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    while id.contains("__") {
        id = id.replace("__", "_");
    }
    let id: String = id.trim_matches('_').chars().take(60).collect();
    let id = if id.is_empty() { "gerakan_ai".to_string() } else { id };
    parsed["id"] = json!(id);

    match crate::motion_dsl::sanitize_motion_asset(&parsed, &crate::motion_dsl::SanitizeOpts { require_tracks: true, source: Some("user".into()), ..Default::default() }) {
        Ok(asset) => (200, json!({ "motion": asset, "source": "ai" }).to_string()),
        Err(errs) => (200, json!({ "error": format!("hasil AI tidak valid: {}", errs.join("; ")) }).to_string()),
    }
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
