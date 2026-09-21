//! Rute ekspresi — port `discoverExpressions` + adoption GET/POST dari
//! `src/server/index.ts`. Guard `test-overlay-gate` menuntut tiap ekspresi
//! membawa `params` (Id dari file .exp3.json) untuk gate overlay-vs-native.
//!
//! Gap sengaja (Stage 2): folder tanpa `.model3.json` (Auto-Rescue) belum
//! ditangani → 404 seperti "no model3" (Bun pemilik selama transisi).

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::{json, Value};

use crate::model::find_model3;

/// Nama folder/berkas aman — padanan `sanitizeModelFolderName`: huruf/angka
/// Unicode + `_`/`-` dipertahankan; run karakter asing → satu `_`; kosong →
/// `model_<base36 ts>`.
pub fn sanitize_model_folder_name(name: &str) -> String {
    let trimmed = name.trim();
    let mut out = String::new();
    let mut in_run = false;
    for c in trimmed.chars() {
        if c.is_alphanumeric() || c == '_' || c == '-' {
            out.push(c);
            in_run = false;
        } else if !in_run {
            out.push('_');
            in_run = true;
        }
    }
    if out.is_empty() {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        format!("model_{}", crate::config::base36_pub(ts))
    } else {
        out
    }
}

fn rel_fwd(base: &Path, full: &Path) -> Option<String> {
    let rel = full.strip_prefix(base).ok()?;
    Some(rel.to_string_lossy().replace('\\', "/"))
}

fn walk_exp3(dir: &Path, base_dir: &Path, declared: &BTreeSet<String>, out: &mut Vec<Value>, depth: usize) {
    if depth > 6 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for e in entries.flatten() {
        let full = e.path();
        if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            walk_exp3(&full, base_dir, declared, out, depth + 1);
            continue;
        }
        let fname = e.file_name().to_string_lossy().to_string();
        if !fname.to_lowercase().ends_with(".exp3.json") {
            continue;
        }
        let rel = match rel_fwd(base_dir, &full) {
            Some(r) if !r.starts_with("..") => r,
            _ => continue,
        };
        // params = Id yang ditulis file (data rigger). Rusak → kosong, bukan error.
        let mut params: Vec<String> = Vec::new();
        if let Ok(txt) = std::fs::read_to_string(&full) {
            let clean = txt.strip_prefix('\u{feff}').unwrap_or(&txt);
            if let Ok(j) = serde_json::from_str::<Value>(clean) {
                if let Some(arr) = j.get("Parameters").and_then(|v| v.as_array()) {
                    for p in arr {
                        if let Some(id) = p.get("Id").and_then(|v| v.as_str()) {
                            if !params.contains(&id.to_string()) {
                                params.push(id.to_string());
                            }
                        }
                        if params.len() >= 64 {
                            break;
                        }
                    }
                }
            }
        }
        let name = fname
            .strip_suffix(".exp3.json")
            .or_else(|| fname.strip_suffix(".exp3.JSON"))
            .unwrap_or(&fname)
            .to_string();
        out.push(json!({
            "Name": name,
            "File": rel,
            "declared": declared.contains(&rel),
            "params": params
        }));
    }
}

/// discoverExpressions(name) → Ok(Value) atau Err(msg) (→ 404).
pub fn discover(model_dir: &Path, data_dir: &Path, name: &str) -> Result<Value, String> {
    if name.split(['\\', '/']).any(|s| s == "..") {
        return Err("not found".into());
    }
    let dir = model_dir.join(name);
    if !dir.starts_with(model_dir) || !dir.exists() {
        return Err("not found".into());
    }
    let model3 = find_model3(&dir, 0).ok_or_else(|| "no model3.json in folder".to_string())?;
    let base_dir = model3.parent().unwrap_or(&dir).to_path_buf();

    // declared expressions dari model3 (File → forward slash).
    let mut declared: BTreeSet<String> = BTreeSet::new();
    if let Ok(txt) = std::fs::read_to_string(&model3) {
        let clean = txt.strip_prefix('\u{feff}').unwrap_or(&txt);
        if let Ok(mj) = serde_json::from_str::<Value>(clean) {
            if let Some(ex) = mj
                .get("FileReferences")
                .and_then(|f| f.get("Expressions"))
                .and_then(|e| e.as_array())
            {
                for e in ex {
                    if let Some(file) = e.get("File").and_then(|v| v.as_str()) {
                        declared.insert(file.replace('\\', "/"));
                    }
                }
            }
        }
    }

    let mut found: Vec<Value> = Vec::new();
    walk_exp3(&dir, &base_dir, &declared, &mut found, 0);
    // TS memakai localeCompare (case-insensitive untuk ASCII); tiru dgn
    // banding lowercase supaya urutan sama (mis. "collar_blue" < "X_change").
    found.sort_by(|a, b| {
        let an = a["Name"].as_str().unwrap_or("").to_lowercase();
        let bn = b["Name"].as_str().unwrap_or("").to_lowercase();
        an.cmp(&bn)
    });
    let orphan = found.iter().filter(|f| f["declared"] == json!(false)).count();

    Ok(json!({
        "model3": rel_fwd(data_dir, &model3).unwrap_or_default(),
        "declaredCount": declared.len(),
        "expressions": found,
        "orphanCount": orphan
    }))
}

fn adopt_file(sheets_dir: &Path, name: &str) -> std::path::PathBuf {
    sheets_dir.join(format!("exp3-adoption_{}.json", sanitize_model_folder_name(name)))
}

/// GET /api/model/expressions-adoption?name=X → (status, body).
pub fn adoption_get(model_dir: &Path, data_dir: &Path, sheets_dir: &Path, name: &str) -> (u16, String) {
    let info = match discover(model_dir, data_dir, name) {
        Ok(v) => v,
        Err(e) => return (404, json!({ "error": e }).to_string()),
    };
    let mut disabled: BTreeSet<String> = BTreeSet::new();
    if let Ok(txt) = std::fs::read_to_string(adopt_file(sheets_dir, name)) {
        if let Ok(j) = serde_json::from_str::<Value>(&txt) {
            if let Some(arr) = j.get("disabled").and_then(|v| v.as_array()) {
                for d in arr {
                    if let Some(s) = d.as_str() {
                        disabled.insert(s.to_string());
                    }
                }
            }
        }
    }
    let exprs: Vec<Value> = info["expressions"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|mut e| {
            let nm = e["Name"].as_str().unwrap_or("").to_string();
            if let Some(o) = e.as_object_mut() {
                o.insert("enabled".into(), json!(!disabled.contains(&nm)));
            }
            e
        })
        .collect();
    (
        200,
        json!({
            "model3": info["model3"],
            "expressions": exprs,
            "disabled": disabled.iter().cloned().collect::<Vec<_>>()
        })
        .to_string(),
    )
}

/// POST /api/model/expressions-adoption {name, disabled:[]} → (status, body).
pub fn adoption_post(sheets_dir: &Path, body: &Value) -> (u16, String) {
    let name = sanitize_model_folder_name(body.get("name").and_then(|v| v.as_str()).unwrap_or(""));
    if name.is_empty() {
        return (500, json!({ "error": "name kosong" }).to_string());
    }
    let disabled: Vec<Value> = body
        .get("disabled")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter(|x| x.is_string()).cloned().collect())
        .unwrap_or_default();
    let file = adopt_file(sheets_dir, &name);
    match crate::sheet::write_json_atomic(&file, &json!({ "disabled": disabled.clone() })) {
        Ok(()) => (200, json!({ "ok": true, "disabled": disabled }).to_string()),
        Err(e) => (500, json!({ "error": e.to_string() }).to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_folder_name() {
        assert_eq!(sanitize_model_folder_name("神宫白子模型"), "神宫白子模型");
        assert_eq!(sanitize_model_folder_name("ren-official_2"), "ren-official_2");
        assert_eq!(sanitize_model_folder_name("a  b//c"), "a_b_c");
        // "!!!" → run karakter asing runtuh jadi satu "_" (sama TS, bukan fallback).
        assert_eq!(sanitize_model_folder_name("!!!"), "_");
        // benar-benar kosong → fallback bermakna.
        assert!(sanitize_model_folder_name("   ").starts_with("model_"));
    }

    #[test]
    fn discover_dan_adoption() {
        let data = std::env::temp_dir().join(format!("l2dexp-{}-{}", std::process::id(), crate::config::base36_pub(now())));
        let model_dir = data.join("model");
        let sheets = data.join("sheets");
        let m = model_dir.join("hana");
        std::fs::create_dir_all(&m).unwrap();
        std::fs::create_dir_all(&sheets).unwrap();
        // model3 dengan 1 expression declared
        std::fs::write(m.join("hana.model3.json"),
            r#"{"FileReferences":{"Expressions":[{"Name":"senyum","File":"exp/senyum.exp3.json"}]}}"#).unwrap();
        std::fs::create_dir_all(m.join("exp")).unwrap();
        std::fs::write(m.join("exp").join("senyum.exp3.json"),
            r#"{"Parameters":[{"Id":"ParamMouth","Value":1},{"Id":"ParamMouth","Value":1}]}"#).unwrap();
        std::fs::write(m.join("exp").join("marah.exp3.json"), r#"{"Parameters":[{"Id":"ParamBrow"}]}"#).unwrap();

        let info = discover(&model_dir, &data, "hana").unwrap();
        let ex = info["expressions"].as_array().unwrap();
        assert_eq!(ex.len(), 2);
        // sorted by Name: marah, senyum
        assert_eq!(ex[0]["Name"], "marah");
        assert_eq!(ex[0]["declared"], false); // orphan
        assert_eq!(ex[1]["Name"], "senyum");
        assert_eq!(ex[1]["declared"], true);
        assert_eq!(ex[1]["params"], json!(["ParamMouth"])); // deduped
        assert_eq!(info["orphanCount"], 1);

        // adoption: disable "marah"
        let (st, _) = adoption_post(&sheets, &json!({ "name": "hana", "disabled": ["marah"] }));
        assert_eq!(st, 200);
        let (_, body) = adoption_get(&model_dir, &data, &sheets, "hana");
        let v: Value = serde_json::from_str(&body).unwrap();
        let exg = v["expressions"].as_array().unwrap();
        let marah = exg.iter().find(|e| e["Name"] == "marah").unwrap();
        assert_eq!(marah["enabled"], false);
        let senyum = exg.iter().find(|e| e["Name"] == "senyum").unwrap();
        assert_eq!(senyum["enabled"], true);

        let _ = std::fs::remove_dir_all(&data);
    }

    fn now() -> u128 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
    }
}
