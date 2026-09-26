//! Rute model READ-ONLY: `findModel3` + `/api/models` + `/api/model/path`.
//! Folder tanpa `.model3.json` dilayani lewat blueprint Auto-Rescue (rescue.rs)
//! — ikut list_models + model_path menunjuk manifest virtual __rescue__.

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

/// Cari avatar model — padanan `findModelAvatar`: file preferred dulu, lalu
/// kandidat bernama folder model (`lumine_icon.png` / `lumine.png`), lalu
/// gambar DI ROOT (subfolder TIDAK boleh mendahului — kasus nyata: tekstur UV
/// `lumine.8192/texture_00.png` 8192×8192 menang urut alfabet dan diserahkan
/// sebagai avatar, memakan 25 MB + tampil sebagai potongan atlas), terakhir
/// walk subfolder (kedalaman ≤ 2, isi "texture" dikecualikan).
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
    // Kandidat bernama stem folder — format umum ekspor model
    // (lumine_icon.png, hana.png, avatar_hana.png), dicek di root.
    for ext in ["png", "jpg", "jpeg", "webp"] {
        for cand in [
            format!("{name}_icon.{ext}"),
            format!("{name}.{ext}"),
            format!("{name}_avatar.{ext}"),
        ] {
            let p = dir.join(&cand);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    fn is_img(n: &str) -> bool {
        let l = n.to_lowercase();
        l.ends_with(".png") || l.ends_with(".jpg") || l.ends_with(".jpeg") || l.ends_with(".webp") || l.ends_with(".gif")
    }
    fn is_asset(r: &str) -> bool {
        let l = r.to_lowercase();
        l.contains("model3") || l.contains("cdi3") || l.contains("physics")
            || l.contains("moc3") || l.contains("texture")
    }
    // Root dulu: gambar apa pun di root menang sebelum menyusur subfolder.
    let mut root: Vec<_> = std::fs::read_dir(&dir).ok()?.flatten().collect();
    root.retain(|e| !e.file_type().map(|t| t.is_dir()).unwrap_or(false));
    root.sort_by_key(|e| e.file_name());
    for e in &root {
        let n = e.file_name().to_string_lossy().into_owned();
        if is_img(&n) && !is_asset(&n) {
            return Some(e.path());
        }
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

/// Hapus folder model (rekursif) — padanan handleModelDelete. (status, body).
pub fn delete_model(model_dir: &Path, name: &str) -> (u16, String) {
    if name.split(['\\', '/']).any(|s| s == "..") {
        return (400, serde_json::json!({ "error": "not found" }).to_string());
    }
    let dir = model_dir.join(name);
    if !dir.starts_with(model_dir) || !dir.exists() {
        return (400, serde_json::json!({ "error": "not found" }).to_string());
    }
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => (200, serde_json::json!({ "ok": true }).to_string()),
        Err(e) => (400, serde_json::json!({ "error": e.to_string() }).to_string()),
    }
}

/// Nama model valid: char pertama bukan /,\,.,whitespace; sisanya bukan /,\.
fn valid_model_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c0) if !matches!(c0, '/' | '\\' | '.') && !c0.is_whitespace() => {}
        _ => return false,
    }
    !name.chars().skip(1).any(|c| c == '/' || c == '\\')
}

/// Upload file model (base64) — padanan handleModelUpload. `files` = array
/// {path, base64}. Wajib mengandung *.model3.json. (status, body).
pub fn upload_model(model_dir: &Path, name: &str, files: &serde_json::Value) -> (u16, String) {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde_json::json;
    if !valid_model_name(name) {
        return (400, json!({ "error": "nama model invalid" }).to_string());
    }
    let arr = match files.as_array() {
        Some(a) if !a.is_empty() => a,
        _ => return (400, json!({ "error": "tidak ada file" }).to_string()),
    };
    let dest = model_dir.join(name);
    if std::fs::create_dir_all(&dest).is_err() {
        return (400, json!({ "error": "gagal buat folder" }).to_string());
    }
    let mut wrote_model3 = false;
    for f in arr {
        let raw = f.get("path").and_then(|v| v.as_str()).unwrap_or("");
        // buang prefix ../ dan absolut; tolak traversal.
        let rel = raw.replace('\\', "/");
        let rel = rel.trim_start_matches("../").trim_start_matches("./");
        if rel.is_empty() || rel.contains("..") || rel.starts_with('/') {
            continue;
        }
        if rel.to_lowercase().ends_with("model3.json") {
            wrote_model3 = true;
        }
        let target = dest.join(rel);
        if !target.starts_with(&dest) {
            continue;
        }
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let b64 = f.get("base64").and_then(|v| v.as_str()).unwrap_or("");
        if let Ok(bytes) = STANDARD.decode(b64) {
            let _ = std::fs::write(&target, bytes);
        }
    }
    if !wrote_model3 {
        return (400, json!({ "error": "folder tidak mengandung *.model3.json" }).to_string());
    }
    (200, json!({ "ok": true, "name": name }).to_string())
}

/// Import model dari zip base64 — padanan handleImportZip. Ekstrak via crate
/// `zip` (bukan shell unzip). Wajib mengandung *.model3.json. (status, body).
pub fn import_zip(model_dir: &Path, data_dir: &Path, name: &str, base64_zip: &str) -> (u16, String) {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde_json::json;
    use std::io::Read;

    if base64_zip.is_empty() {
        return (400, json!({ "error": "zip kosong" }).to_string());
    }
    let clean = crate::expressions::sanitize_model_folder_name(name);
    let dest = model_dir.join(&clean);
    if std::fs::create_dir_all(&dest).is_err() {
        return (400, json!({ "error": "gagal buat folder" }).to_string());
    }
    let bytes = match STANDARD.decode(base64_zip) {
        Ok(b) => b,
        Err(_) => return (400, json!({ "error": "base64 zip tidak valid" }).to_string()),
    };
    let mut archive = match zip::ZipArchive::new(std::io::Cursor::new(bytes)) {
        Ok(a) => a,
        Err(e) => return (400, json!({ "error": format!("gagal buka zip: {e}") }).to_string()),
    };
    for i in 0..archive.len() {
        let mut entry = match archive.by_index(i) {
            Ok(e) => e,
            Err(_) => continue,
        };
        // enclosed_name menolak traversal (../, absolut).
        let rel = match entry.enclosed_name() {
            Some(p) => p,
            None => continue,
        };
        let target = dest.join(&rel);
        if !target.starts_with(&dest) {
            continue;
        }
        if entry.is_dir() {
            let _ = std::fs::create_dir_all(&target);
            continue;
        }
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut buf = Vec::new();
        if entry.read_to_end(&mut buf).is_ok() {
            let _ = std::fs::write(&target, buf);
        }
    }
    match find_model3(&dest, 0) {
        Some(abs) => {
            let rel = abs
                .strip_prefix(data_dir)
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            (200, json!({ "ok": true, "name": clean, "path": rel }).to_string())
        }
        None => (400, json!({ "error": "zip tidak mengandung *.model3.json" }).to_string()),
    }
}

/// Buang akhiran ".model3" dari stem file (case-insensitive, ASCII — aman
/// dipotong per-byte karena akhirannya ASCII).
fn strip_model3_suffix(stem: &str) -> &str {
    let b = stem.as_bytes();
    if b.len() >= 7 && b[b.len() - 7..].eq_ignore_ascii_case(b".model3") {
        &stem[..stem.len() - 7]
    } else {
        stem
    }
}

/// Import folder model dari disk LOKAL (jalur dialog folder native di shell —
/// IPC `import_model_dialog`): salin rekursif ke `model_dir/<nama>` TANPA
/// upload base64 lewat WebView. Folder model ber-tekstur 4K berukuran
/// puluhan-ratusan MB; JSON base64-nya melampaui batas body HTTP dan WebView2
/// memutus koneksi ("Failed to fetch") — menyalin langsung di disk menghindari
/// semuanya. Nama: `preferred_name` bila diisi, else stem file `*.model3.json`,
/// else nama folder sumber (semuanya disanitasi). Wajib mengandung
/// `*.model3.json`. (status, body).
pub fn import_model_folder(
    model_dir: &Path,
    data_dir: &Path,
    src: &Path,
    preferred_name: &str,
) -> (u16, String) {
    use serde_json::json;

    let is_dir = std::fs::metadata(src).map(|m| m.is_dir()).unwrap_or(false);
    if !is_dir {
        return (400, json!({ "ok": false, "error": "folder sumber tidak ada" }).to_string());
    }
    let model3 = match find_model3(src, 0) {
        Some(p) => p,
        None => {
            return (
                400,
                json!({ "ok": false, "error": "folder tidak mengandung *.model3.json" })
                    .to_string(),
            )
        }
    };

    // Nama: preferensi user → stem model3.json ("mao_pro.model3.json" →
    // "mao_pro") → nama folder sumber. Pilih nama mentah dulu, sanitasi SEKALI
    // di akhir — sanitize("") menghasilkan "model_<ts>", bukan kosong, jadi
    // cabang prioritas tidak boleh mengandalkan cek kosong pasca-sanitize.
    let mut raw = preferred_name.trim().to_string();
    if raw.is_empty() {
        raw = strip_model3_suffix(
            &model3.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
        )
        .trim()
        .to_string();
    }
    if raw.is_empty() {
        raw = src
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
    let name = crate::expressions::sanitize_model_folder_name(&raw);
    if name.is_empty() {
        return (
            400,
            json!({ "ok": false, "error": "tidak bisa menentukan nama model" }).to_string(),
        );
    }

    let dest = model_dir.join(&name);
    if !dest.starts_with(model_dir) {
        return (400, json!({ "ok": false, "error": "nama model invalid" }).to_string());
    }
    if src == dest {
        // Sumber sudah di galeri model — idempoten, anggap selesai.
        let rel = model_path_rel(data_dir, model_dir, &name).unwrap_or_default();
        return (200, json!({ "ok": true, "name": name, "path": rel }).to_string());
    }
    if dest.starts_with(src) {
        // User memilih folder yang memuat folder tujuan (mis. data/model itu
        // sendiri) — menyalin folder ke dalam dirinya sendiri; tolak.
        return (
            400,
            json!({ "ok": false, "error": "folder sumber memuat folder tujuan" }).to_string(),
        );
    }
    if std::fs::create_dir_all(&dest).is_err() {
        return (400, json!({ "ok": false, "error": "gagal buat folder" }).to_string());
    }

    fn walk(src_dir: &Path, dest_dir: &Path) {
        let Ok(rd) = std::fs::read_dir(src_dir) else { return };
        for e in rd.flatten() {
            let target = dest_dir.join(e.file_name());
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let _ = std::fs::create_dir_all(&target);
                walk(&e.path(), &target);
            } else {
                let _ = std::fs::copy(e.path(), &target);
            }
        }
    }
    walk(src, &dest);

    match find_model3(&dest, 0) {
        Some(abs) => {
            let rel = abs
                .strip_prefix(data_dir)
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            (200, json!({ "ok": true, "name": name, "path": rel }).to_string())
        }
        None => (
            400,
            json!({ "ok": false, "error": "gagal menyalin folder model" }).to_string(),
        ),
    }
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
    fn import_zip_ekstrak_dan_temukan_model3() {
        use base64::{engine::general_purpose::STANDARD, Engine};
        use std::io::Write;
        // rakit zip in-memory: 1 file sub/hana.model3.json
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zw = zip::ZipWriter::new(&mut buf);
            let opts: zip::write::FileOptions<()> =
                zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            zw.start_file("sub/hana.model3.json", opts).unwrap();
            zw.write_all(b"{\"Version\":3}").unwrap();
            // entri traversal harus diabaikan
            zw.finish().unwrap();
        }
        let b64 = STANDARD.encode(buf.into_inner());

        let data = tmp();
        let model_dir = data.join("model");
        std::fs::create_dir_all(&model_dir).unwrap();
        let (st, body) = import_zip(&model_dir, &data, "MyModel!", &b64);
        assert_eq!(st, 200, "{body}");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["name"], "MyModel_"); // sanitize: '!' → '_'
        assert!(v["path"].as_str().unwrap().ends_with("hana.model3.json"));
        assert!(model_dir.join("MyModel_").join("sub").join("hana.model3.json").exists());
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn impor_folder_valid_dari_disk_menantukan_nama_stem() {
        let data = tmp();
        let model_dir = data.join("model");
        // Sumber di luar model_dir, seperti folder hasil dialog user: nama
        // folder generik ("runtime"), manifest bernama "mao_pro.model3.json".
        let src = data.join("sumber").join("runtime");
        std::fs::create_dir_all(src.join("textures")).unwrap();
        std::fs::write(src.join("mao_pro.model3.json"), "{\"Version\":3}").unwrap();
        std::fs::write(src.join("mao_pro.moc3"), "MOC").unwrap();
        std::fs::write(src.join("textures").join("00.png"), "PNG").unwrap();

        // Tanpa nama preferensi → stem manifest ("mao_pro"), bukan "runtime".
        let (st, body) = import_model_folder(&model_dir, &data, &src, "");
        assert_eq!(st, 200, "{body}");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["name"], "mao_pro");
        assert_eq!(v["path"], "model/mao_pro/mao_pro.model3.json");
        assert!(model_dir.join("mao_pro").join("textures").join("00.png").exists());
        assert_eq!(list_models(&model_dir), vec!["mao_pro".to_string()]);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn impor_folder_nama_preferred_menang_dan_disanitasi() {
        let data = tmp();
        let model_dir = data.join("model");
        let src = data.join("sumber").join("runtime");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("mao_pro.model3.json"), "{}").unwrap();

        let (st, body) = import_model_folder(&model_dir, &data, &src, "Mao (EN)!");
        assert_eq!(st, 200, "{body}");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["name"], "Mao_EN_");
        assert!(model_dir.join("Mao_EN_").join("mao_pro.model3.json").exists());
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn impor_folder_tanpa_model3_ditolak() {
        let data = tmp();
        let model_dir = data.join("model");
        let src = data.join("sumber").join("runtime");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("mao_pro.moc3"), "MOC").unwrap();

        let (st, body) = import_model_folder(&model_dir, &data, &src, "");
        assert_eq!(st, 400, "{body}");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["ok"], false);
        assert!(!model_dir.join("runtime").exists());
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn impor_folder_menolak_sumber_yang_memuat_tujuan() {
        let data = tmp();
        let model_dir = data.join("model");
        std::fs::create_dir_all(&model_dir).unwrap();
        let hana = model_dir.join("hana");
        std::fs::create_dir_all(&hana).unwrap();
        std::fs::write(hana.join("hana.model3.json"), "{}").unwrap();

        // User memilih model_dir itu sendiri: dest (model/hana) berada DI DALAM
        // sumber — menyalin folder ke dalam dirinya sendiri; harus ditolak,
        // bukan salin tanpa akhir.
        let (st, body) = import_model_folder(&model_dir, &data, &model_dir, "");
        assert_eq!(st, 400, "{body}");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["ok"], false);
        // Idempoten: memilih folder model yang sudah di galeri → sukses tanpa salin.
        let (st2, body2) = import_model_folder(&model_dir, &data, &hana, "");
        assert_eq!(st2, 200, "{body2}");
        let v2: serde_json::Value = serde_json::from_str(&body2).unwrap();
        assert_eq!(v2["name"], "hana");
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn find_dan_list_model3() {
        let data = tmp();
        let model_dir = data.join("model");
        let hana = model_dir.join("hana");
        std::fs::create_dir_all(&hana).unwrap();
        std::fs::write(hana.join("hana.model3.json"), "{}").unwrap();
        // folder tanpa model3 DAN tanpa .moc3 → tidak terdaftar, path None
        std::fs::create_dir_all(model_dir.join("kosong")).unwrap();

        assert!(find_model3(&hana, 0).is_some());
        assert_eq!(list_models(&model_dir), vec!["hana".to_string()]);

        let rel = model_path_rel(&data, &model_dir, "hana").unwrap();
        assert_eq!(rel, "model/hana/hana.model3.json");
        assert_eq!(model_path_rel(&data, &model_dir, "kosong"), None);
        assert_eq!(model_path_rel(&data, &model_dir, "../etc"), None);

        // padanan wiring guard test-auto-rescue (arsip JS dihapus Batch A):
        // folder tanpa manifest tapi rescue-able (ber-.moc3) ikut terdaftar
        // dan model_path menunjuk manifest virtual __rescue__.
        std::fs::write(model_dir.join("kosong").join("a.moc3"), "M").unwrap();
        assert_eq!(list_models(&model_dir), vec!["hana".to_string(), "kosong".to_string()]);
        assert_eq!(
            model_path_rel(&data, &model_dir, "kosong").as_deref(),
            Some("model/kosong/__rescue__.model3.json")
        );

        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn avatar_root_dahulu_daripada_tekstur_subfolder() {
        let data = tmp();
        let model_dir = data.join("model");

        // (A) Struktur nyata model lumine: ikon di root, tekstur UV di
        // subfolder yang urut alfabetnya mendahului file ikon. Bug lama:
        // walk masuk lumine.8192 dulu → texture_00.png 8192×8192 (25 MB)
        // diserahkan sebagai avatar.
        let m = model_dir.join("lumine");
        std::fs::create_dir_all(m.join("lumine.8192")).unwrap();
        std::fs::write(m.join("lumine.8192").join("texture_00.png"), b"T").unwrap();
        std::fs::write(m.join("lumine.model3.json"), "{}").unwrap();
        std::fs::write(m.join("lumine_icon.png"), b"I").unwrap();
        assert_eq!(
            find_avatar(&model_dir, "lumine").unwrap(),
            m.join("lumine_icon.png")
        );

        // (B) Gambar root menang atas gambar subfolder mana pun (root-first),
        // bukan cuma kasus texture.
        let h = model_dir.join("hana");
        std::fs::create_dir_all(h.join("gallery")).unwrap();
        std::fs::write(h.join("gallery").join("photo.png"), b"G").unwrap();
        std::fs::write(h.join("hana.model3.json"), "{}").unwrap();
        std::fs::write(h.join("potret.png"), b"P").unwrap();
        assert_eq!(find_avatar(&model_dir, "hana").unwrap(), h.join("potret.png"));

        // (C) Kandidat bernama stem folder: hana.png / hana_icon.png dikenali
        // walau bukan nama preferensi generik.
        let s = model_dir.join("sena");
        std::fs::create_dir_all(&s).unwrap();
        std::fs::write(s.join("sena.model3.json"), "{}").unwrap();
        std::fs::write(s.join("sena_icon.png"), b"S").unwrap();
        assert_eq!(find_avatar(&model_dir, "sena").unwrap(), s.join("sena_icon.png"));

        // (D) Hanya tekstur → None (placeholder), tekstur bukan avatar.
        let k = model_dir.join("kosong");
        std::fs::create_dir_all(k.join("tex.8192")).unwrap();
        std::fs::write(k.join("tex.8192").join("texture_00.png"), b"T").unwrap();
        std::fs::write(k.join("kosong.model3.json"), "{}").unwrap();
        assert_eq!(find_avatar(&model_dir, "kosong"), None);

        let _ = std::fs::remove_dir_all(&data);
    }
}
