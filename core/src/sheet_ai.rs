//! Endpoint LLM-role "sheet" — port handleClassifyParams (`/api/model/
//! classify-params`). LLM mengklasifikasi parameter rig → role baku + grup +
//! label. Model-Agnostic: hanya role dari KNOWN_ROLES yang lolos; LLM tak
//! pernah mengirim angka range (validasi ketat di sini).

use std::path::Path;

use serde_json::{json, Value};

use crate::{jsonx, llm};

/// Role baku (padanan KNOWN_ROLES index.ts).
pub const KNOWN_ROLES: &[&str] = &[
    "angleX", "angleY", "angleZ", "eyeBallX", "eyeBallY", "eyeLOpen", "eyeROpen",
    "eyeLSmile", "eyeRSmile", "eyeForm", "mouthOpenY", "mouthForm", "mouthOpenX",
    "bodyAngleX", "bodyAngleY", "bodyAngleZ", "breath", "browLForm", "browRForm",
    "browLY", "browRY", "browLAngle", "browRAngle", "blush",
];

fn str_cap(v: &Value, cap: usize) -> String {
    v.as_str()
        .map(|s| {
            s.chars()
                .filter(|&c| (c as u32) > 0x1F && (c as u32) != 0x7F)
                .collect::<String>()
                .trim()
                .chars()
                .take(cap)
                .collect()
        })
        .unwrap_or_default()
}

/// POST /api/model/classify-params — return {classifications:[...]}.
pub async fn classify_params(config_path: &Path, body: &Value) -> Value {
    let unclassified = body.get("params").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    if unclassified.is_empty() {
        return json!({ "classifications": [] });
    }
    let known_roles = body.get("currentRoles").cloned().unwrap_or(json!({}));

    let param_lines = unclassified
        .iter()
        .map(|u| {
            format!(
                "- ID: \"{}\", Range: [{}, {}], Default: {}",
                u.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                u.get("min").map(|v| v.to_string()).unwrap_or_default(),
                u.get("max").map(|v| v.to_string()).unwrap_or_default(),
                u.get("def").map(|v| v.to_string()).unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mapped = known_roles
        .as_object()
        .map(|o| {
            if o.is_empty() {
                "(belum ada)".to_string()
            } else {
                o.iter()
                    .map(|(r, id)| format!("  {r} -> {}", id.as_str().unwrap_or("")))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        })
        .unwrap_or_else(|| "(belum ada)".to_string());

    let prompt = format!(
        "Kamu adalah pakar Live2D Cubism rigging & parameter modeling.\n\
Berikut daftar parameter model yang BELUM memiliki mapping role baku:\n{param_lines}\n\n\
Parameter yang SUDAH ter-mapping:\n{mapped}\n\n\
Daftar semantic roles yang tersedia:\n[{roles}]\n\n\
TUGAS: Analisis setiap parameter (nama ID, range, konvensi JP/CN/EN, fungsinya di Live2D).\n\
Tentukan: id, role (salah satu di atas, atau null jika aksesoris/parts/fisika),\n\
group (\"Sudut (Angle)\"/\"Mata (Eye)\"/\"Alis (Eyebrow)\"/\"Mulut (Mouth)\"/\"Badan (Body)\"/\
\"Rambut (Hair)\"/\"Aksesoris (Accessory)\"/\"Physics\"/\"Kustom\"), label ringkas, isAccessory (bool).\n\n\
KEMBALIKAN HANYA JSON array valid tanpa markdown.\n\
Format:\n[\n  {{ \"id\": \"ParamX\", \"role\": \"angleX\", \"group\": \"Sudut (Angle)\", \"label\": \"Kepala X\", \"isAccessory\": false }}\n]",
        roles = KNOWN_ROLES.join(", "),
    );

    let msgs = vec![llm::ChatMessage { role: "user".into(), content: prompt }];
    let reply = match llm::llm_for_role(config_path, "sheet", &msgs, "").await {
        Ok(ok) => ok.reply,
        Err((_, msg)) => return json!({ "classifications": [], "warning": msg }),
    };

    let parsed = jsonx::extract_json_array_loose(&reply);
    let requested: std::collections::HashSet<String> = unclassified
        .iter()
        .filter_map(|u| u.get("id").and_then(|v| v.as_str()).map(String::from))
        .collect();
    let allowed: std::collections::HashSet<&str> = KNOWN_ROLES.iter().cloned().collect();

    let mut out: Vec<Value> = Vec::new();
    for it in &parsed {
        let id = it.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if !requested.contains(&id) {
            continue;
        }
        let role = it
            .get("role")
            .and_then(|v| v.as_str())
            .filter(|r| allowed.contains(r))
            .map(|r| json!(r))
            .unwrap_or(Value::Null);
        out.push(json!({
            "id": id,
            "role": role,
            "group": str_cap(it.get("group").unwrap_or(&Value::Null), 40),
            "label": str_cap(it.get("label").unwrap_or(&Value::Null), 60),
            "isAccessory": it.get("isAccessory") == Some(&json!(true))
        }));
    }
    json!({ "classifications": out })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn classify_kosong_dan_fallback() {
        let dir = std::env::temp_dir().join(format!("l2dsai-{}-{}", std::process::id(), now()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("config.json");
        std::fs::write(&f, r#"{"activeId":"m","connections":[{"id":"m","provider":"mock"}]}"#).unwrap();
        // params kosong → classifications []
        let empty = classify_params(&f, &json!({ "params": [] })).await;
        assert_eq!(empty["classifications"].as_array().unwrap().len(), 0);
        // mock meng-echo prompt (berisi contoh [{id:ParamX}]); id kita berbeda
        // ("ParamHairFront") → tak ada di requested → tersaring habis → [].
        // Ini juga menguji validasi requested-id.
        let out = classify_params(&f, &json!({ "params": [{ "id": "ParamHairFront", "min": 0, "max": 1, "def": 0 }] })).await;
        assert_eq!(out["classifications"].as_array().unwrap().len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn now() -> u128 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
    }
}
