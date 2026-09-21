//! Rute model READ-ONLY — port `findModel3` + `/api/models` + `/api/model/path`
//! dari `src/server/index.ts`.
//!
//! CATATAN (gap sengaja, Stage 2): logika Auto-Rescue (folder tanpa
//! `.model3.json` → manifest virtual) BELUM diport. Selama transisi server Bun
//! masih pemilik jalur itu; folder model normal (punya `.model3.json`) sudah
//! ditangani di sini. Rescue menyusul di grup berikutnya.

use std::path::{Path, PathBuf};

/// Cari `*.model3.json` (atau `model3.json`) rekursif, depth ≤ 6 — padanan
/// `findModel3` TS (DFS, folder dulu).
pub fn find_model3(root: &Path, depth: usize) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    // Kumpulkan lalu proses: TS mengembalikan hasil rekursi folder lebih dulu
    // bila ketemu, else file .model3.json. Kita tiru urutan readdir apa adanya.
    let mut items: Vec<_> = entries.flatten().collect();
    items.sort_by_key(|e| e.file_name());
    for e in &items {
        let full = e.path();
        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            if depth > 6 {
                continue;
            }
            if let Some(r) = find_model3(&full, depth + 1) {
                return Some(r);
            }
        } else {
            let name = e.file_name().to_string_lossy().to_lowercase();
            if name.ends_with(".model3.json") || name == "model3.json" {
                return Some(full);
            }
        }
    }
    None
}

/// Daftar folder model yang dapat dipakai (punya `.model3.json` ATAU bisa
/// dirakit Auto-Rescue), terurut — padanan handleListModels.
pub fn list_models(model_dir: &Path) -> Vec<String> {
    let mut usable: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(model_dir) {
        for e in rd.flatten() {
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let dir = e.path();
                if find_model3(&dir, 0).is_some()
                    || crate::rescue::build_rescue_blueprint(&dir).is_some()
                {
                    usable.push(e.file_name().to_string_lossy().into_owned());
                }
            }
        }
    }
    usable.sort();
    usable
}

/// Path `.model3.json` relatif ke `data/` (separator "/"), untuk `name` di bawah
/// `model_dir`. None bila nama tak aman / folder tak ada / tanpa model3.
pub fn model_path_rel(data_dir: &Path, model_dir: &Path, name: &str) -> Option<String> {
    if name.is_empty() || name.split(['\\', '/']).any(|s| s == ".." || s.is_empty()) {
        return None;
    }
    let dir = model_dir.join(name);
    // pastikan tetap di bawah model_dir
    if !dir.starts_with(model_dir) || !dir.exists() {
        return None;
    }
    // Auto-Rescue: folder tanpa manifest → jalur virtual __rescue__.
    match find_model3(&dir, 0) {
        Some(abs) => {
            let rel = abs.strip_prefix(data_dir).ok()?;
            Some(rel.to_string_lossy().replace('\\', "/"))
        }
        None => {
            if crate::rescue::build_rescue_blueprint(&dir).is_some() {
                Some(format!("model/{name}/{}", crate::rescue::RESCUE_FILENAME))
            } else {
                None
            }
        }
    }
}

/// Semua file (path relatif "/") di bawah `model_dir/name` — padanan
/// `handleModelFiles`. None bila nama tak aman / folder tak ada.
pub fn list_model_files(model_dir: &Path, name: &str) -> Option<Vec<String>> {
    if name.split(['\\', '/']).any(|s| s == "..") {
        return None;
    }
    let dir = model_dir.join(name);
    if !dir.starts_with(model_dir) || !dir.exists() {
        return None;
    }
    let mut out: Vec<String> = Vec::new();
    fn walk(d: &Path, rel: &str, out: &mut Vec<String>) {
        if let Ok(rd) = std::fs::read_dir(d) {
            let mut items: Vec<_> = rd.flatten().collect();
            items.sort_by_key(|e| e.file_name());
            for e in items {
                let name = e.file_name().to_string_lossy().into_owned();
                let r = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
                if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    walk(&e.path(), &r, out);
                } else {
                    out.push(r);
                }
            }
        }
    }
    walk(&dir, "", &mut out);
    Some(out)
}

const AVATAR_PREFERRED: &[&str] = &[
    "avatar.png", "avatar.jpg", "avatar.jpeg", "avatar.webp", "icon.png",
    "thumbnail.png", "preview.png",
];

/// Cari avatar model — padanan `findModelAvatar`: file preferred dulu, lalu walk
/// (kedalaman relatif ≤ 2) untuk gambar yang bukan aset model3/cdi3/physics/moc3.
pub fn find_avatar(model_dir: &Path, name: &str) -> Option<PathBuf> {
    if name.split(['\\', '/']).any(|s| s == "..") {
        return None;
    }
    let dir = model_dir.join(name);
    if !dir.starts_with(model_dir) || !dir.exists() {
        return None;
    }
    for f in AVATAR_PREFERRED {
        let p = dir.join(f);
        if p.exists() {
            return Some(p);
        }
    }
    fn is_img(n: &str) -> bool {
        let l = n.to_lowercase();
        l.ends_with(".png") || l.ends_with(".jpg") || l.ends_with(".jpeg") || l.ends_with(".webp") || l.ends_with(".gif")
    }
    fn is_asset(r: &str) -> bool {
        let l = r.to_lowercase();
        l.contains("model3") || l.contains("cdi3") || l.contains("physics") || l.contains("moc3")
    }
    fn walk(d: &Path, rel: &str) -> Option<PathBuf> {
        let mut items: Vec<_> = std::fs::read_dir(d).ok()?.flatten().collect();
        items.sort_by_key(|e| e.file_name());
        for e in &items {
            let name = e.file_name().to_string_lossy().into_owned();
            let r = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            let depth = r.split('/').count();
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                if depth > 2 {
                    continue;
                }
                if let Some(hit) = walk(&e.path(), &r) {
                    return Some(hit);
                }
            } else if is_img(&name) && !is_asset(&r) && depth <= 2 {
                return Some(e.path());
            }
        }
        None
    }
    walk(&dir, "")
}

/// Content-Type gambar avatar dari ekstensi.
pub fn avatar_mime(path: &Path) -> &'static str {
    let e = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
    match e.as_str() {
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "image/jpeg",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("l2dmodeltest-{}-{}", std::process::id(), rand_suffix()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn rand_suffix() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64
    }

    #[test]
    fn find_dan_list_model3() {
        let data = tmp();
        let model_dir = data.join("model");
        let hana = model_dir.join("hana");
        std::fs::create_dir_all(&hana).unwrap();
        std::fs::write(hana.join("hana.model3.json"), "{}").unwrap();
        // folder tanpa model3 tidak masuk daftar
        std::fs::create_dir_all(model_dir.join("kosong")).unwrap();

        assert!(find_model3(&hana, 0).is_some());
        assert_eq!(list_models(&model_dir), vec!["hana".to_string()]);

        let rel = model_path_rel(&data, &model_dir, "hana").unwrap();
        assert_eq!(rel, "model/hana/hana.model3.json");
        assert_eq!(model_path_rel(&data, &model_dir, "kosong"), None);
        assert_eq!(model_path_rel(&data, &model_dir, "../etc"), None);

        let _ = std::fs::remove_dir_all(&data);
    }
}
