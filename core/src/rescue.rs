//! Auto-Rescue — port `src/server/rescue.ts`. Folder model tanpa `.model3.json`
//! (hanya .moc3 + tekstur + motion/exp yatim) dirakit jadi manifest virtual DI
//! MEMORI (read-only, tak menulis folder), disajikan via jalur virtual
//! `model/<folder>/__rescue__.model3.json`. Model-agnostic: dari struktur folder,
//! urutan tekstur natural, idle dari vtube.json / pola nama.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

pub const RESCUE_FILENAME: &str = "__rescue__.model3.json";

fn walk_files(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 6 {
        return;
    }
    if let Ok(rd) = std::fs::read_dir(dir) {
        let mut items: Vec<_> = rd.flatten().collect();
        items.sort_by_key(|e| e.file_name());
        for e in items {
            let full = e.path();
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                walk_files(&full, out, depth + 1);
            } else {
                out.push(full);
            }
        }
    }
}

fn find_manifest(dir: &Path, depth: usize) -> bool {
    if depth > 6 {
        return false;
    }
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let full = e.path();
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                if find_manifest(&full, depth + 1) {
                    return true;
                }
            } else if e.file_name().to_string_lossy().to_lowercase().ends_with(".model3.json") {
                return true;
            }
        }
    }
    false
}

/// Urutan natural (texture_02 < texture_10) — split pada run digit.
fn natural_compare(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let split = |s: &str| -> Vec<String> {
        let s = s.to_lowercase();
        let mut parts: Vec<String> = Vec::new();
        let mut cur = String::new();
        let mut in_digit: Option<bool> = None;
        for c in s.chars() {
            let d = c.is_ascii_digit();
            if Some(d) != in_digit && !cur.is_empty() {
                parts.push(std::mem::take(&mut cur));
            }
            cur.push(c);
            in_digit = Some(d);
        }
        if !cur.is_empty() {
            parts.push(cur);
        }
        parts
    };
    let ra = split(a);
    let rb = split(b);
    for i in 0..ra.len().max(rb.len()) {
        match (ra.get(i), rb.get(i)) {
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(xa), Some(xb)) => {
                let na = xa.parse::<u64>().ok();
                let nb = xb.parse::<u64>().ok();
                if let (Some(na), Some(nb)) = (na, nb) {
                    if na != nb {
                        return na.cmp(&nb);
                    }
                } else if xa != xb {
                    return xa.cmp(xb);
                }
            }
        }
    }
    Ordering::Equal
}

fn ends_with_ci(p: &Path, suffix: &str) -> bool {
    p.to_string_lossy().to_lowercase().ends_with(suffix)
}

fn first_by(dir: &Path, suffix: &str) -> Option<PathBuf> {
    let mut hits: Vec<PathBuf> = Vec::new();
    let mut all = Vec::new();
    walk_files(dir, &mut all, 0);
    for f in all {
        if ends_with_ci(&f, suffix) {
            hits.push(f);
        }
    }
    hits.sort_by(|a, b| natural_compare(&a.to_string_lossy(), &b.to_string_lossy()));
    hits.into_iter().next()
}

fn basename_lower(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn basename_no_ext(p: &Path, ext: &str) -> String {
    let base = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    base[..base.len().saturating_sub(ext.len())].to_string()
}

/// Petunjuk motion Idle dari vtube.json (VTube Studio FileReferences.IdleAnimation).
fn vtube_idle_motion(dir: &Path) -> Option<PathBuf> {
    let mut vts: Vec<PathBuf> = Vec::new();
    let mut all = Vec::new();
    walk_files(dir, &mut all, 0);
    for f in &all {
        if ends_with_ci(f, ".vtube.json") {
            vts.push(f.clone());
        }
    }
    vts.sort_by(|a, b| natural_compare(&a.to_string_lossy(), &b.to_string_lossy()));
    for vt in vts {
        if let Ok(txt) = std::fs::read_to_string(&vt) {
            if let Ok(j) = serde_json::from_str::<Value>(&txt) {
                if let Some(idle) = j.get("FileReferences").and_then(|f| f.get("IdleAnimation")).and_then(|v| v.as_str()) {
                    let idle = idle.trim();
                    if !idle.is_empty() {
                        let wanted = idle.replace('\\', "/").rsplit('/').next().unwrap_or("").to_lowercase();
                        let mut cands: Vec<PathBuf> = all
                            .iter()
                            .filter(|f| ends_with_ci(f, ".motion3.json") && basename_lower(f) == wanted)
                            .cloned()
                            .collect();
                        cands.sort_by(|a, b| natural_compare(&a.to_string_lossy(), &b.to_string_lossy()));
                        if let Some(c) = cands.into_iter().next() {
                            return Some(c);
                        }
                        return Some(vt.parent().unwrap_or(dir).join(idle));
                    }
                }
            }
        }
    }
    None
}

/// Rakit blueprint manifest di memori. None bila folder tak ada / tak ada .moc3
/// / sudah punya manifest (idempoten).
pub fn build_rescue_blueprint(model_dir: &Path) -> Option<Value> {
    if !model_dir.exists() || find_manifest(model_dir, 0) {
        return None;
    }
    let mut all = Vec::new();
    walk_files(model_dir, &mut all, 0);

    let mut moc3: Option<PathBuf> = None;
    let mut pngs: Vec<PathBuf> = Vec::new();
    let mut motions: Vec<(String, PathBuf)> = Vec::new();
    let mut expressions: Vec<(String, PathBuf)> = Vec::new();
    for f in &all {
        let low = f.to_string_lossy().to_lowercase();
        if low.ends_with(".moc3") {
            if moc3.is_none() {
                moc3 = Some(f.clone());
            }
        } else if low.ends_with(".png") {
            pngs.push(f.clone());
        } else if low.ends_with(".motion3.json") {
            motions.push((basename_no_ext(f, ".motion3.json"), f.clone()));
        } else if low.ends_with(".exp3.json") {
            expressions.push((basename_no_ext(f, ".exp3.json"), f.clone()));
        }
    }
    let moc3 = moc3?;

    // tekstur: atlas-like dulu (texture / .8192/.4096/.2048), buang icon.
    let is_icon = |p: &PathBuf| p.to_string_lossy().to_lowercase().contains("icon");
    let atlas: Vec<PathBuf> = pngs
        .iter()
        .filter(|p| {
            let l = p.to_string_lossy().to_lowercase();
            (l.contains("texture") || l.contains(".8192") || l.contains(".4096") || l.contains(".2048")) && !is_icon(p)
        })
        .cloned()
        .collect();
    let mut tex: Vec<PathBuf> = if !atlas.is_empty() {
        atlas
    } else {
        pngs.iter().filter(|p| !is_icon(p)).cloned().collect()
    };
    tex.sort_by(|a, b| natural_compare(&a.to_string_lossy(), &b.to_string_lossy()));
    motions.sort_by(|a, b| natural_compare(&a.1.to_string_lossy(), &b.1.to_string_lossy()));
    expressions.sort_by(|a, b| natural_compare(&a.1.to_string_lossy(), &b.1.to_string_lossy()));
    let idle_hint = vtube_idle_motion(model_dir);

    let rel = |full: &Path| -> String {
        full.strip_prefix(model_dir)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| full.to_string_lossy().replace('\\', "/"))
    };

    let mut file_refs = serde_json::Map::new();
    file_refs.insert("Moc".into(), json!(rel(&moc3)));
    file_refs.insert("Textures".into(), json!(tex.iter().map(|t| rel(t)).collect::<Vec<_>>()));
    if let Some(p) = first_by(model_dir, ".physics3.json") {
        file_refs.insert("Physics".into(), json!(rel(&p)));
    }
    if let Some(p) = first_by(model_dir, ".cdi3.json") {
        file_refs.insert("DisplayInfo".into(), json!(rel(&p)));
    }
    if let Some(p) = first_by(model_dir, ".pose3.json") {
        file_refs.insert("Pose".into(), json!(rel(&p)));
    }

    // motion grouping: Idle (dari hint atau /idle/i) vs Motion.
    let mut idle_files: BTreeSet<PathBuf> = BTreeSet::new();
    if let Some(h) = &idle_hint {
        idle_files.insert(h.clone());
    }
    let mut group_idle: Vec<Value> = Vec::new();
    let mut group_motion: Vec<Value> = Vec::new();
    for (name, file) in &motions {
        let is_idle = idle_files.contains(file) || name.to_lowercase().contains("idle");
        if is_idle {
            group_idle.push(json!({ "File": rel(file) }));
        } else {
            group_motion.push(json!({ "File": rel(file) }));
        }
    }
    if !group_idle.is_empty() || !group_motion.is_empty() {
        let mut m = serde_json::Map::new();
        if !group_idle.is_empty() {
            m.insert("Idle".into(), json!(group_idle));
        }
        if !group_motion.is_empty() {
            m.insert("Motion".into(), json!(group_motion));
        }
        file_refs.insert("Motions".into(), Value::Object(m));
    }

    // ekspresi yatim: nama unik (duplikat → " 2", " 3", …).
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut exprs: Vec<Value> = Vec::new();
    for (name, file) in &expressions {
        let mut nm = name.clone();
        if seen.contains(&nm) {
            let mut k = 2;
            while seen.contains(&format!("{nm} {k}")) {
                k += 1;
            }
            nm = format!("{nm} {k}");
        }
        seen.insert(nm.clone());
        exprs.push(json!({ "Name": nm, "File": rel(file) }));
    }
    if !exprs.is_empty() {
        file_refs.insert("Expressions".into(), json!(exprs));
    }

    Some(json!({
        "Version": 3,
        "FileReferences": Value::Object(file_refs),
        "AutoRescued": {
            "by": "live2d-agent auto-rescue (core)",
            "textures": tex.len(),
            "motions": motions.len(),
            "expressions": expressions.len()
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order() {
        use std::cmp::Ordering;
        assert_eq!(natural_compare("t2.png", "t10.png"), Ordering::Less);
        assert_eq!(natural_compare("a.png", "b.png"), Ordering::Less);
    }

    #[test]
    fn rakit_manifest_dari_moc3() {
        let dir = std::env::temp_dir().join(format!("l2dresc-{}-{}", std::process::id(), now()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("m.moc3"), "x").unwrap();
        std::fs::write(dir.join("texture_00.png"), "x").unwrap();
        std::fs::write(dir.join("idle_01.motion3.json"), "{}").unwrap();
        std::fs::write(dir.join("wave.motion3.json"), "{}").unwrap();

        let bp = build_rescue_blueprint(&dir).unwrap();
        assert_eq!(bp["Version"], 3);
        assert_eq!(bp["FileReferences"]["Moc"], "m.moc3");
        assert_eq!(bp["FileReferences"]["Textures"][0], "texture_00.png");
        // idle_01 → grup Idle; wave → Motion
        assert_eq!(bp["FileReferences"]["Motions"]["Idle"][0]["File"], "idle_01.motion3.json");
        assert_eq!(bp["FileReferences"]["Motions"]["Motion"][0]["File"], "wave.motion3.json");

        // folder dengan manifest asli → None (idempoten)
        std::fs::write(dir.join("real.model3.json"), "{}").unwrap();
        assert!(build_rescue_blueprint(&dir).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atlas_dulu_icon_dibuang_hint_vtube_ekspresi_dedup() {
        // Port test-auto-rescue.js: tekstur atlas (.8192) menang atas png liar,
        // "icon" dikecualikan, idle dari pola nama + hint vtube.json, ekspresi
        // tabrakan nama dapat suffix, tanpa .moc3 → None.
        let dir = std::env::temp_dir().join(format!("l2dresc2-{}-{}", std::process::id(), now()));
        std::fs::create_dir_all(dir.join("lumine.8192")).unwrap();
        std::fs::create_dir_all(dir.join("mothion")).unwrap();
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        std::fs::write(dir.join("lumine.moc3"), "MOC3-fake").unwrap();
        std::fs::write(dir.join("lumine.8192/texture_01.png"), "p1").unwrap();
        std::fs::write(dir.join("lumine.8192/texture_00.png"), "p0").unwrap();
        std::fs::write(dir.join("lumine_icon.png"), "icon").unwrap(); // harus dibuang
        std::fs::write(dir.join("stray.png"), "liar").unwrap(); // kalah oleh atlas
        std::fs::write(dir.join("lumine.physics3.json"), "{}").unwrap();
        std::fs::write(dir.join("lumine.cdi3.json"), "{}").unwrap();
        std::fs::write(dir.join("mothion/idle.motion3.json"), "{\"Meta\":{}}").unwrap();
        std::fs::write(dir.join("mothion/wave.motion3.json"), "{\"Meta\":{}}").unwrap();
        std::fs::write(dir.join("exp_heart.exp3.json"), "{\"Parameters\":[]}").unwrap();
        std::fs::write(dir.join("nested/exp_heart.exp3.json"), "{\"Parameters\":[]}").unwrap();
        // vtube.json: IdleAnimation → wave masuk grup Idle (mengalahkan pola nama)
        std::fs::write(dir.join("lumine.vtube.json"), r#"{"FileReferences":{"IdleAnimation":"mothion/wave.motion3.json"}}"#).unwrap();

        let bp = build_rescue_blueprint(&dir).unwrap();
        let tex = bp["FileReferences"]["Textures"].as_array().unwrap();
        assert_eq!(tex.len(), 2, "icon & stray tidak boleh ikut: {tex:?}");
        assert_eq!(tex[0], "lumine.8192/texture_00.png");
        assert_eq!(tex[1], "lumine.8192/texture_01.png");
        assert_eq!(bp["FileReferences"]["Physics"], "lumine.physics3.json");
        assert_eq!(bp["FileReferences"]["DisplayInfo"], "lumine.cdi3.json");
        // hint vtube: wave masuk Idle (nama file mengandung "idle" juga ikut
        // Idle oleh pola nama — keduanya sah, yang penting wave TIDAK dibuang)
        let idle_grp = bp["FileReferences"]["Motions"]["Idle"].as_array().unwrap();
        assert!(idle_grp.iter().any(|m| m["File"] == "mothion/wave.motion3.json"), "{idle_grp:?}");
        // ekspresi tabrakan nama → dedup suffix
        let ex = bp["FileReferences"]["Expressions"].as_array().unwrap();
        let names: Vec<&str> = ex.iter().map(|e| e["Name"].as_str().unwrap()).collect();
        assert!(names.contains(&"exp_heart"));
        assert!(names.iter().any(|n| n.starts_with("exp_heart ")), "{names:?}");

        // tanpa .moc3 → None (bukan rescue)
        let empty = std::env::temp_dir().join(format!("l2dresc3-{}-{}", std::process::id(), now()));
        std::fs::create_dir_all(&empty).unwrap();
        std::fs::write(empty.join("a.png"), "x").unwrap();
        assert!(build_rescue_blueprint(&empty).is_none());

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&empty);
    }

    fn now() -> u128 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
    }
}
