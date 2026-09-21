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

/// Daftar folder model yang dapat dipakai (punya `.model3.json`), terurut.
/// (Rescue-only folders belum termasuk — lihat catatan modul.)
pub fn list_models(model_dir: &Path) -> Vec<String> {
    let mut usable: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(model_dir) {
        for e in rd.flatten() {
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let dir = e.path();
                if find_model3(&dir, 0).is_some() {
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
    let abs = find_model3(&dir, 0)?;
    let rel = abs.strip_prefix(data_dir).ok()?;
    Some(rel.to_string_lossy().replace('\\', "/"))
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
