//! motion_files.rs — DISCOVERY klip .motion3.json di folder model.
//!
//! Padanan /api/model/expressions untuk motion: sumber data adopsi klip
//! yatim — file .motion3.json di disk yang TIDAK dideklarasikan di
//! model3.json (`FileReferences.Motions`). Klasifikasi semantik tetap
//! milik klien (`src/client/engine/motion-taxonomy.ts`); modul ini hanya
//! data disk. Tidak pernah menulis ke disk.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{json, Value};

use crate::expressions::rel_fwd;
use crate::model::find_model3;

/// Peta File (forward-slash) → (grup, index) dari FileReferences.Motions.
/// Grup "" (nama kosong) adalah grup sah di Cubism — tetap dicatat supaya
/// klipnya bisa diadopsi ke registry per-klip, bukan hilang.
fn declared_motion_map(fr: &Value) -> BTreeMap<String, (String, usize)> {
    let mut out = BTreeMap::new();
    if let Some(groups) = fr.get("Motions").and_then(|m| m.as_object()) {
        for (group, clips) in groups {
            if let Some(arr) = clips.as_array() {
                for (i, c) in arr.iter().enumerate() {
                    if let Some(file) = c.get("File").and_then(|v| v.as_str()) {
                        out.entry(file.replace('\\', "/"))
                            .or_insert((group.clone(), i));
                    }
                }
            }
        }
    }
    out
}

fn walk_motion3(
    dir: &Path,
    base_dir: &Path,
    declared: &BTreeMap<String, (String, usize)>,
    out: &mut Vec<Value>,
    depth: usize,
) {
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
            walk_motion3(&full, base_dir, declared, out, depth + 1);
            continue;
        }
        let fname = e.file_name().to_string_lossy().to_string();
        if !fname.to_lowercase().ends_with(".motion3.json") {
            continue;
        }
        let rel = match rel_fwd(base_dir, &full) {
            Some(r) if !r.starts_with("..") => r,
            _ => continue,
        };
        // Durasi & loop dari Meta (data engine) — rusak → null, bukan error.
        let mut duration: Option<f64> = None;
        let mut looped: Option<bool> = None;
        if let Ok(txt) = std::fs::read_to_string(&full) {
            let clean = txt.strip_prefix('\u{feff}').unwrap_or(&txt);
            if let Ok(j) = serde_json::from_str::<Value>(clean) {
                duration = j
                    .get("Meta")
                    .and_then(|m| m.get("Duration"))
                    .and_then(|v| v.as_f64());
                looped = j
                    .get("Meta")
                    .and_then(|m| m.get("Loop"))
                    .and_then(|v| v.as_bool());
            }
        }
        let name = fname
            .strip_suffix(".motion3.json")
            .or_else(|| fname.strip_suffix(".motion3.JSON"))
            .unwrap_or(&fname)
            .to_string();
        let decl = declared.get(&rel);
        out.push(json!({
            "Name": name,
            "File": rel,
            "declared": decl.is_some(),
            "group": decl.map(|(g, _)| json!(g)).unwrap_or(Value::Null),
            "index": decl.map(|(_, i)| json!(i)).unwrap_or(Value::Null),
            "duration": duration.map(|d| json!(d)).unwrap_or(Value::Null),
            "loop": looped.map(|l| json!(l)).unwrap_or(Value::Null),
        }));
    }
}

/// GET /api/model/motions?name=X — daftar klip .motion3.json di folder model
/// plus flag declared + (grup, index) untuk yang terdeklarasi.
pub fn discover(model_dir: &Path, data_dir: &Path, name: &str) -> Result<Value, String> {
    if name.split(['\\', '/']).any(|s| s == "..") {
        return Err("not found".into());
    }
    let dir = model_dir.join(name);
    if !dir.starts_with(model_dir) || !dir.exists() {
        return Err("not found".into());
    }
    // Parity expressions::discover: folder tanpa manifest → blueprint rescue
    // in-memory; file user di disk tak tersentuh.
    let (model3, base_dir, blueprint) = match find_model3(&dir, 0) {
        Some(m3) => {
            let base = m3.parent().unwrap_or(&dir).to_path_buf();
            (m3, base, None)
        }
        None => {
            let bp = crate::rescue::build_rescue_blueprint(&dir)
                .ok_or_else(|| "no model3.json in folder".to_string())?;
            (dir.join(crate::rescue::RESCUE_FILENAME), dir.clone(), Some(bp))
        }
    };

    let fr = match &blueprint {
        Some(bp) => bp.get("FileReferences").cloned().unwrap_or(Value::Null),
        None => std::fs::read_to_string(&model3)
            .ok()
            .map(|txt| {
                let clean = txt.strip_prefix('\u{feff}').unwrap_or(&txt);
                serde_json::from_str::<Value>(clean).unwrap_or(Value::Null)
            })
            .and_then(|mj| mj.get("FileReferences").cloned())
            .unwrap_or(Value::Null),
    };
    let declared = declared_motion_map(&fr);

    let mut found: Vec<Value> = Vec::new();
    walk_motion3(&dir, &base_dir, &declared, &mut found, 0);
    // Urut sama dengan expressions: banding lowercase (localeCompare ASCII).
    found.sort_by(|a, b| {
        let an = a["Name"].as_str().unwrap_or("").to_lowercase();
        let bn = b["Name"].as_str().unwrap_or("").to_lowercase();
        an.cmp(&bn)
    });
    let orphan = found
        .iter()
        .filter(|f| f["declared"] == json!(false))
        .count();

    Ok(json!({
        "model3": rel_fwd(data_dir, &model3).unwrap_or_default(),
        "declaredCount": declared.len(),
        "motions": found,
        "orphanCount": orphan
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    }

    fn temp_model(tag: &str) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
        let data = std::env::temp_dir().join(format!(
            "l2dmtn-{}-{}-{}",
            tag,
            std::process::id(),
            crate::config::base36_pub(now())
        ));
        let model_dir = data.join("model");
        let m = model_dir.join("hana");
        std::fs::create_dir_all(&m).unwrap();
        (data, model_dir, m)
    }

    #[test]
    fn discover_declared_grup_kosong_dan_orphan() {
        let (data, model_dir, m) = temp_model("utama");
        // Idle: 1 klip; grup "" : 1 klip; orphan di subfolder dengan durasi.
        std::fs::write(
            m.join("hana.model3.json"),
            r#"{"FileReferences":{"Motions":{"Idle":[{"File":"motions/mtn_01.motion3.json"}],"":[{"File":"motions/mtn_02.motion3.json"}]}}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(m.join("motions")).unwrap();
        std::fs::write(
            m.join("motions").join("mtn_01.motion3.json"),
            r#"{"Meta":{"Duration":3.5,"Loop":false},"Curves":[]}"#,
        )
        .unwrap();
        std::fs::write(
            m.join("motions").join("mtn_02.motion3.json"),
            r#"{"Meta":{"Duration":1.2,"Loop":true},"Curves":[]}"#,
        )
        .unwrap();
        std::fs::create_dir_all(m.join("extra")).unwrap();
        std::fs::write(
            m.join("extra").join("lompat.motion3.json"),
            r#"{"Meta":{"Duration":2.0},"Curves":[]}"#,
        )
        .unwrap();

        let info = discover(&model_dir, &data, "hana").unwrap();
        assert_eq!(info["declaredCount"], 2);
        assert_eq!(info["orphanCount"], 1);
        let mo = info["motions"].as_array().unwrap();
        assert_eq!(mo.len(), 3);
        // sorted lowercase: lompat, mtn_01, mtn_02
        assert_eq!(mo[0]["Name"], "lompat");
        assert_eq!(mo[0]["declared"], false);
        assert_eq!(mo[0]["group"], Value::Null);
        assert_eq!(mo[0]["duration"], 2.0);
        let m1 = mo.iter().find(|x| x["Name"] == "mtn_01").unwrap();
        assert_eq!(m1["declared"], true);
        assert_eq!(m1["group"], "Idle");
        assert_eq!(m1["index"], 0);
        assert_eq!(m1["duration"], 3.5);
        assert_eq!(m1["loop"], false);
        let m2 = mo.iter().find(|x| x["Name"] == "mtn_02").unwrap();
        assert_eq!(m2["group"], "");
        assert_eq!(m2["index"], 0);
        assert_eq!(m2["loop"], true);

        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn traversal_ditolak_dan_folder_tanpa_model3() {
        let (data, model_dir, m) = temp_model("traversal");
        let _ = &m;
        assert!(discover(&model_dir, &data, "..").is_err());
        assert!(discover(&model_dir, &data, "a/b").is_err());
        // folder tanpa model3 → error "no model3.json in folder" (bukan panic)
        std::fs::create_dir_all(model_dir.join("kosong")).unwrap();
        let err = discover(&model_dir, &data, "kosong").unwrap_err();
        assert!(err.contains("model3"), "err: {err}");
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn bom_dan_cjk_filename() {
        let (data, model_dir, m) = temp_model("cjk");
        std::fs::write(m.join("hana.model3.json"), r#"{"FileReferences":{}}"#).unwrap();
        std::fs::write(
            m.join("跳びはねる.motion3.json"),
            "\u{feff}{\"Meta\":{\"Duration\":4.25},\"Curves\":[]}",
        )
        .unwrap();
        let info = discover(&model_dir, &data, "hana").unwrap();
        let mo = info["motions"].as_array().unwrap();
        assert_eq!(mo.len(), 1);
        assert_eq!(mo[0]["Name"], "跳びはねる");
        assert_eq!(mo[0]["duration"], 4.25);
        assert_eq!(mo[0]["declared"], false);
        let _ = std::fs::remove_dir_all(&data);
    }
}
