//! motion_analysis.rs — Analisis kemampuan motion model dari DISK.
//!
//! Port gagasan `tools/analyze_model.py` (live2d-add-motion-sample-web-ui):
//! - range nilai aman per parameter, diobservasi dari semua .motion3.json
//!   milik model (keyframe endpoint; bezier di-collapse seperti decodeCurve);
//! - destination output physics3.json (★ — jangan dianimasikan langsung;
//!   gerakkan penyebabnya, output akan mengikut lewat simulasi fisika);
//! - estimasi base pose per parameter (modus nilai keyframe pertama);
//! - proyeksi role opsional dari peta role→paramId milik klien. Inferensi
//!   role tetap SATU sumber di `role-mapping.ts` (klien) — Rust tidak
//!   menebak role, hanya melipat peta yang diberikan.
//!
//! Semua angka dihitung dari file di disk (engine), bukan dari LLM.
//! Read-only: tidak pernah menulis ke disk.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{json, Value};

use crate::model::find_model3;

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// Sanitasi ala klien `currentModelKey()`: karakter non-alfanumerik (unicode)
/// selain `_` → `_`. Dipakai untuk mencocokkan key klien ke folder nyata.
fn client_key(s: &str) -> String {
    s.chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' }).collect()
}

/// Resolve identitas model → nama folder nyata di `model_dir`. Menerima nama
/// folder langsung ATAU key ala klien (mis. "model_Mao_mao_model3_json" dari
/// `currentModelKey()` yang menyanitasi path model3). Model-agnostic: cocokkan
/// dengan menghitung ulang key tiap folder yang benar-benar ada, bukan menebak
/// dari nama. Bila ambigu, folder dengan token cocok terpanjang menang.
pub fn resolve_model_folder(model_dir: &Path, name: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    // 1) Nama folder langsung.
    if model_dir.join(name).is_dir() {
        return Some(name.to_string());
    }
    // 2) Key ala klien: folder yang key-nya jadi token di dalam `name`.
    let key = client_key(name);
    let mut best: Option<(usize, String)> = None;
    for entry in std::fs::read_dir(model_dir).ok()?.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let folder = entry.file_name().to_string_lossy().to_string();
        let fkey = client_key(&folder);
        if fkey.is_empty() {
            continue;
        }
        let hit = key == fkey
            || key.contains(&format!("_{fkey}_"))
            || key.starts_with(&format!("{fkey}_"))
            || key.ends_with(&format!("_{fkey}"));
        if hit && best.as_ref().map(|(l, _)| fkey.len() > *l).unwrap_or(true) {
            best = Some((fkey.len(), folder));
        }
    }
    best.map(|(_, f)| f)
}

/// Decode stream `Segments` datar → keyframe endpoint (t, v).
/// Segment bezier (type 1) di-collapse ke endpoint-nya — sama dengan
/// decodeCurve di motion-taxonomy.ts dan repo referensi: titik kontrol
/// hanya pembentuk kurva, keyframe asli tetap endpoint. Segment
/// linear/stepped/inverse-stepped (type 0/2/3) berpasangan (t, v).
pub fn curve_keyframes(segments: &Value) -> Vec<(f64, f64)> {
    let arr = match segments.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };
    let nums: Vec<f64> = arr.iter().filter_map(|v| v.as_f64()).collect();
    if nums.len() < 2 {
        return Vec::new();
    }
    let mut pts = vec![(nums[0], nums[1])];
    let mut i = 2;
    while i < nums.len() {
        if nums[i] == 1.0 {
            // bezier: [1, c1t, c1v, c2t, c2v, t, v]
            if i + 7 > nums.len() {
                break;
            }
            pts.push((nums[i + 5], nums[i + 6]));
            i += 7;
        } else {
            // linear/stepped: [type, t, v]
            if i + 3 > nums.len() {
                break;
            }
            pts.push((nums[i + 1], nums[i + 2]));
            i += 3;
        }
    }
    pts
}

/// Kumpulkan semua *.motion3.json di bawah `dir` (rekursif, depth ≤ 6).
fn collect_motion3(dir: &Path, out: &mut Vec<std::path::PathBuf>, depth: usize) {
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
            collect_motion3(&full, out, depth + 1);
        } else if e
            .file_name()
            .to_string_lossy()
            .to_lowercase()
            .ends_with(".motion3.json")
        {
            out.push(full);
        }
    }
}

fn read_json(path: &Path) -> Option<Value> {
    let txt = std::fs::read_to_string(path).ok()?;
    let clean = txt.strip_prefix('\u{feff}').unwrap_or(&txt);
    serde_json::from_str::<Value>(clean).ok()
}

#[derive(Default)]
struct ParamStat {
    min: f64,
    max: f64,
    usage: u64,
    /// Counter nilai keyframe pertama (dibulatkan 3 desimal) untuk base pose.
    first: BTreeMap<i64, u64>,
}

impl ParamStat {
    fn observe(&mut self, pts: &[(f64, f64)]) {
        self.usage += 1;
        for &(_, v) in pts {
            self.min = self.min.min(v);
            self.max = self.max.max(v);
        }
        let k = (round3(pts[0].1) * 1000.0).round() as i64;
        *self.first.entry(k).or_insert(0) += 1;
    }
}

fn mode_first(first: &BTreeMap<i64, u64>) -> f64 {
    let mut best_key = 0i64;
    let mut best_n = 0u64;
    for (k, n) in first {
        if *n > best_n {
            best_n = *n;
            best_key = *k;
        }
    }
    (best_key as f64) / 1000.0
}

/// GET /api/model/motion-analysis?name=X&roles={"ax":"ParamAngleX",...}
///
/// `roles` (opsional) = peta role→paramId dari klien (state.caps.ids hasil
/// role-mapping.ts). Output: params[], physicsOutputs[], roles{} (bila
/// peta dikirim), hasReference (ada motion ber-kurva parameter untuk
/// dipakai sebagai referensi range).
pub fn analyze(
    model_dir: &Path,
    data_dir: &Path,
    name: &str,
    roles: Option<&Value>,
) -> Result<Value, String> {
    if name.split(['\\', '/']).any(|s| s == "..") {
        return Err("not found".into());
    }
    // Terima nama folder ATAU key ala klien (path model3 disanitasi). Klien
    // mengirim `currentModelKey()` = modelPath yang [^\p{L}\p{N}_] → "_"; itu
    // BUKAN nama folder, jadi join langsung gagal "not found". Resolve dulu.
    let folder = resolve_model_folder(model_dir, name).ok_or_else(|| "not found".to_string())?;
    let dir = model_dir.join(&folder);
    if !dir.starts_with(model_dir) || !dir.exists() {
        return Err("not found".into());
    }

    // Nama physics3/cdi3 dari manifest bila ada; folder tanpa manifest
    // (blueprint rescue) difallback ke satu file senama di folder model.
    let model3 = find_model3(&dir, 0);
    let base = model3
        .as_ref()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| dir.clone());
    let fr = model3
        .as_ref()
        .and_then(|p| read_json(p))
        .and_then(|mj| mj.get("FileReferences").cloned())
        .unwrap_or(Value::Null);
    let physics_rel = fr.get("Physics").and_then(|v| v.as_str());
    let cdi_rel = fr.get("DisplayInfo").and_then(|v| v.as_str());
    let find_single = |suffix: &str| -> Option<std::path::PathBuf> {
        let mut hits: Vec<std::path::PathBuf> = Vec::new();
        collect_by_suffix(&dir, suffix, &mut hits, 0);
        hits.into_iter().next()
    };
    let physics_path = physics_rel
        .map(|r| base.join(r))
        .filter(|p| p.exists())
        .or_else(|| find_single(".physics3.json"));
    let cdi_path = cdi_rel
        .map(|r| base.join(r))
        .filter(|p| p.exists())
        .or_else(|| find_single(".cdi3.json"));

    // Destination output physics = parameter yang DIDORONG fisika.
    let mut physics_outputs: BTreeSet<String> = BTreeSet::new();
    if let Some(pp) = physics_path {
        if let Some(ph) = read_json(&pp) {
            if let Some(settings) = ph.get("PhysicsSettings").and_then(|v| v.as_array()) {
                for s in settings {
                    if let Some(outputs) = s.get("Output").and_then(|v| v.as_array()) {
                        for o in outputs {
                            if let Some(id) = o
                                .get("Destination")
                                .and_then(|d| d.get("Id"))
                                .and_then(|v| v.as_str())
                            {
                                physics_outputs.insert(id.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    // Label tampilan dari cdi3 (murni label, tidak dipakai menyimpulkan role).
    let mut labels: BTreeMap<String, String> = BTreeMap::new();
    if let Some(cp) = cdi_path {
        if let Some(cdi) = read_json(&cp) {
            if let Some(ps) = cdi.get("Parameters").and_then(|v| v.as_array()) {
                for p in ps {
                    if let (Some(id), Some(nm)) = (
                        p.get("Id").and_then(|v| v.as_str()),
                        p.get("Name").and_then(|v| v.as_str()),
                    ) {
                        labels.insert(id.to_string(), nm.to_string());
                    }
                }
            }
        }
    }

    // Agregasi semua motion di folder model.
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    collect_motion3(&dir, &mut files, 0);
    files.sort();
    let mut stats: BTreeMap<String, ParamStat> = BTreeMap::new();
    for f in &files {
        let Some(d) = read_json(f) else { continue };
        let Some(curves) = d.get("Curves").and_then(|v| v.as_array()) else {
            continue;
        };
        for c in curves {
            if c.get("Target").and_then(|v| v.as_str()) != Some("Parameter") {
                continue;
            }
            let Some(pid) = c.get("Id").and_then(|v| v.as_str()) else {
                continue;
            };
            let pts = curve_keyframes(c.get("Segments").unwrap_or(&Value::Null));
            if pts.is_empty() {
                continue;
            }
            stats.entry(pid.to_string()).or_default().observe(&pts);
        }
    }

    let has_reference = !stats.is_empty();
    let mut params_obj = serde_json::Map::new();
    for (pid, s) in &stats {
        let mut e = json!({
            "min": round3(s.min),
            "max": round3(s.max),
            "base": round3(mode_first(&s.first)),
            "usage": s.usage,
        });
        if let Some(lbl) = labels.get(pid) {
            e["label"] = json!(lbl);
        }
        if physics_outputs.contains(pid) {
            e["physics"] = json!(true);
        }
        params_obj.insert(pid.clone(), e);
    }

    // Proyeksi role: fold min/max dari peta klien (string atau array 1 elemen).
    let roles_json = roles.and_then(|r| r.as_object()).map(|m| {
        let mut out = serde_json::Map::new();
        for (role, pv) in m {
            let pid = match pv {
                Value::String(s) => Some(s.clone()),
                Value::Array(a) => a.first().and_then(|v| v.as_str()).map(String::from),
                _ => None,
            };
            let Some(pid) = pid else { continue };
            let Some(s) = stats.get(&pid) else { continue };
            out.insert(
                role.clone(),
                json!({
                    "min": round3(s.min),
                    "max": round3(s.max),
                    "base": round3(mode_first(&s.first)),
                    "param": pid,
                    "physics": physics_outputs.contains(&pid),
                }),
            );
        }
        Value::Object(out)
    });

    Ok(json!({
        "model3": model3
            .as_ref()
            .and_then(|p| crate::expressions::rel_fwd(data_dir, p))
            .unwrap_or_default(),
        "motionCount": files.len(),
        "hasReference": has_reference,
        "params": Value::Object(params_obj),
        "physicsOutputs": physics_outputs.into_iter().collect::<Vec<_>>(),
        "roles": roles_json.unwrap_or(Value::Null),
    }))
}

fn collect_by_suffix(dir: &Path, suffix: &str, out: &mut Vec<std::path::PathBuf>, depth: usize) {
    if depth > 3 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for e in entries.flatten() {
        let full = e.path();
        if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            collect_by_suffix(&full, suffix, out, depth + 1);
        } else if e
            .file_name()
            .to_string_lossy()
            .to_lowercase()
            .ends_with(suffix)
        {
            out.push(full);
        }
    }
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

    #[test]
    fn resolve_folder_langsung_dan_dari_client_key() {
        let (_data, model_dir, _m) = temp_model("resolve");
        // folder "hana" sudah dibuat oleh temp_model.
        std::fs::create_dir_all(model_dir.join("Mao")).unwrap();
        // Nama folder langsung.
        assert_eq!(resolve_model_folder(&model_dir, "hana").as_deref(), Some("hana"));
        assert_eq!(resolve_model_folder(&model_dir, "Mao").as_deref(), Some("Mao"));
        // Key ala klien currentModelKey(): path model3 disanitasi → "_".
        assert_eq!(
            resolve_model_folder(&model_dir, "model_Mao_mao_model3_json").as_deref(),
            Some("Mao"),
            "key path-disanitasi harus dipetakan ke folder"
        );
        assert_eq!(
            resolve_model_folder(&model_dir, "model_hana_hana_model3_json").as_deref(),
            Some("hana")
        );
        // Tak dikenal → None.
        assert!(resolve_model_folder(&model_dir, "model_tidakada_x_json").is_none());
        let _ = std::fs::remove_dir_all(&_data);
    }

    fn temp_model(tag: &str) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
        let data = std::env::temp_dir().join(format!(
            "l2dma-{}-{}-{}",
            tag,
            std::process::id(),
            crate::config::base36_pub(now())
        ));
        let model_dir = data.join("model");
        let m = model_dir.join("hana");
        std::fs::create_dir_all(&m).unwrap();
        (data, model_dir, m)
    }

    /// Fixture: model + physics + cdi + 2 motion (bezier + linear).
    fn isi_fixture(m: &Path, id_angle: &str, id_hair: &str) {
        std::fs::write(
            m.join("hana.model3.json"),
            r#"{"FileReferences":{"Physics":"hana.physics3.json","DisplayInfo":"hana.cdi3.json"}}"#,
        )
        .unwrap();
        std::fs::write(
            m.join("hana.physics3.json"),
            format!(
                r#"{{"PhysicsSettings":[{{"Output":[{{"Destination":{{"Id":"{id_hair}"}}}},{{"Destination":{{"Id":"{id_hair}2"}}}}]}}]}}"#
            ),
        )
        .unwrap();
        std::fs::write(
            m.join("hana.cdi3.json"),
            format!(r#"{{"Parameters":[{{"Id":"{id_angle}","Name":"角度X"}}]}}"#),
        )
        .unwrap();
        std::fs::create_dir_all(m.join("motions")).unwrap();
        // m1: bezier (collapse ke endpoint) + linear; basepose AngleX = 0.
        std::fs::write(
            m.join("motions").join("m1.motion3.json"),
            format!(
                r#"{{"Meta":{{"Duration":2.0}},"Curves":[
                  {{"Target":"Parameter","Id":"{id_angle}","Segments":[0,0, 1,0.3,5,0.6,5, 0.7,20, 1,1.2,-8,1.5,-8, 2.0,0]}},
                  {{"Target":"Parameter","Id":"{id_hair}","Segments":[0,0, 0,1.0,4, 0,2.0,0]}},
                  {{"Target":"Parameter","Id":"{id_hair}2","Segments":[0,0, 0,2.0,3]}}
                ]}}"#
            ),
        )
        .unwrap();
        // m2: AngleX sampai -30; basepose kedua juga 0 → mode = 0.
        std::fs::write(
            m.join("motions").join("m2.motion3.json"),
            format!(
                r#"{{"Meta":{{"Duration":1.0}},"Curves":[
                  {{"Target":"Parameter","Id":"{id_angle}","Segments":[0,0, 0,0.5,-30, 0,1.0,0]}}
                ]}}"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn agregasi_range_base_dan_physics() {
        let (data, model_dir, m) = temp_model("utama");
        isi_fixture(&m, "ParamAngleX", "ParamHairFront");

        let a = analyze(&model_dir, &data, "hana", None).unwrap();
        assert_eq!(a["motionCount"], 2);
        assert_eq!(a["hasReference"], true);

        let ax = &a["params"]["ParamAngleX"];
        assert_eq!(ax["min"], -30.0);
        assert_eq!(ax["max"], 20.0);
        assert_eq!(ax["base"], 0.0);
        assert_eq!(ax["usage"], 2);
        assert_eq!(ax["label"], "角度X");
        // ParamHairFront adalah output physics → bertanda + tak berlabel.
        let hair = &a["params"]["ParamHairFront"];
        assert_eq!(hair["physics"], true);
        assert!(hair.get("label").is_none());
        let po = a["physicsOutputs"].as_array().unwrap();
        assert!(po.iter().any(|v| v == "ParamHairFront"));
        assert!(po.iter().any(|v| v == "ParamHairFront2"));

        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn proyeksi_role_dari_peta_klien() {
        let (data, model_dir, m) = temp_model("role");
        isi_fixture(&m, "ParamAngleX", "ParamHairFront");

        let roles = json!({ "ax": "ParamAngleX", "zz": "ParamTidakAda" });
        let a = analyze(&model_dir, &data, "hana", Some(&roles)).unwrap();
        let r = &a["roles"];
        assert_eq!(r["ax"]["min"], -30.0);
        assert_eq!(r["ax"]["max"], 20.0);
        assert_eq!(r["ax"]["param"], "ParamAngleX");
        assert_eq!(r["ax"]["physics"], false);
        assert!(r.get("zz").is_none()); // param tak terobservasi → tanpa data
        // Tanpa peta → roles null.
        let b = analyze(&model_dir, &data, "hana", None).unwrap();
        assert!(b["roles"].is_null());

        let _ = std::fs::remove_dir_all(&data);
    }

    /// Invariansi nama: statistik identik untuk id berbeda (m_001 dsb.).
    #[test]
    fn invarian_terhadap_rename_param() {
        let (data1, dir1, m1) = temp_model("nama1");
        let (data2, dir2, m2) = temp_model("nama2");
        isi_fixture(&m1, "ParamAngleX", "ParamHairFront");
        isi_fixture(&m2, "m_001", "m_002");

        let a = analyze(&dir1, &data1, "hana", None).unwrap();
        let b = analyze(&dir2, &data2, "hana", None).unwrap();

        let ax1 = &a["params"]["ParamAngleX"];
        let ax2 = &b["params"]["m_001"];
        for k in ["min", "max", "base", "usage"] {
            assert_eq!(ax1[k], ax2[k], "kolom {k} berubah saat rename");
        }
        assert_eq!(a["physicsOutputs"].as_array().unwrap().len(), 2);
        assert_eq!(b["physicsOutputs"].as_array().unwrap().len(), 2);

        let _ = std::fs::remove_dir_all(&data1);
        let _ = std::fs::remove_dir_all(&data2);
    }

    #[test]
    fn traversal_tanpa_motion_dan_bom() {
        let (data, model_dir, m) = temp_model("tj");
        assert!(analyze(&model_dir, &data, "..", None).is_err());
        assert!(analyze(&model_dir, &data, "a/b", None).is_err());

        // Model tanpa motion sama sekali → hasReference false, params kosong.
        std::fs::write(m.join("hana.model3.json"), r#"{"FileReferences":{}}"#).unwrap();
        let a = analyze(&model_dir, &data, "hana", None).unwrap();
        assert_eq!(a["hasReference"], false);
        assert_eq!(a["params"].as_object().unwrap().len(), 0);
        assert_eq!(a["motionCount"], 0);

        // BOM di motion file tetap terbaca.
        std::fs::create_dir_all(m.join("motions")).unwrap();
        std::fs::write(
            m.join("motions").join("b.motion3.json"),
            "\u{feff}{\"Meta\":{\"Duration\":1.0},\"Curves\":[{\"Target\":\"Parameter\",\"Id\":\"P\",\"Segments\":[0,0, 0,1.0,5]}]}",
        )
        .unwrap();
        let b = analyze(&model_dir, &data, "hana", None).unwrap();
        assert_eq!(b["params"]["P"]["max"], 5.0);

        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn fallback_physics_cdi_tanpa_manifest() {
        // Folder tanpa model3.json (kasus rescue): physics/cdi dicari per sufiks.
        let (data, model_dir, m) = temp_model("rescue");
        std::fs::write(
            m.join("ana.physics3.json"),
            r#"{"PhysicsSettings":[{"Output":[{"Destination":{"Id":"Rambut"}}]}]}"#,
        )
        .unwrap();
        std::fs::write(
            m.join("m1.motion3.json"),
            r#"{"Meta":{"Duration":1.0},"Curves":[{"Target":"Parameter","Id":"Rambut","Segments":[0,0, 0,1.0,2]}]}"#,
        )
        .unwrap();
        let a = analyze(&model_dir, &data, "hana", None).unwrap();
        assert_eq!(a["params"]["Rambut"]["physics"], true);
        assert_eq!(a["physicsOutputs"].as_array().unwrap().len(), 1);

        let _ = std::fs::remove_dir_all(&data);
    }
}
