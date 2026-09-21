//! Penyajian file statis — port `serveStatic`/`safeJoinStatic` dari
//! `src/server/index.ts`. Menjaga invarian:
//!   - guard traversal ".." (dicek pada string mentah, bukan cuma hasil normalize)
//!   - `config.json`/`.bak` (berisi apiKey plaintext) TIDAK PERNAH disajikan
//!   - path `model/...` diresolve di bawah `data/`, bukan `static/`
//!   - MIME by suffix terpanjang (mis. `.model3.json` → application/json)
//!
//! Inilah fondasi single-exe: Tauri menyajikan frontend dari Rust in-process,
//! bukan dari server Bun. Lihat docs/ARCHITECTURE-TAURI-RUST.md.

use std::path::{Component, Path, PathBuf};

use crate::paths::AppPaths;

/// Hasil resolusi path statis (padanan {file, forbidden} di TS).
#[derive(Debug, PartialEq)]
pub enum Resolved {
    /// File aman untuk disajikan.
    File(PathBuf),
    /// Traversal / file sensitif → 403.
    Forbidden,
    /// Tak ada file (boleh SPA fallback).
    NotFound,
}

// config.json / .bak menyimpan apiKey plaintext — tak boleh disajikan statis.
const SENSITIVE: &[&str] = &["config.json", "config.json.bak"];

/// True bila ada segmen ".." pada string path (dicek sebelum normalisasi).
fn has_dot_dot(s: &str) -> bool {
    s.split(['\\', '/']).any(|seg| seg == "..")
}

/// Normalisasi lexical: buang komponen "." (CurDir). ".." dikembalikan sebagai
/// sinyal gagal via `None`.
fn normalize_lexical(p: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::ParentDir => return None, // ".." → keluar → tolak
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    Some(out)
}

/// Gabung `rel` di bawah `base` dan pastikan hasilnya tetap di dalam `base`.
/// Kedua sisi dinormalisasi lexical supaya "./static" vs "static/…" konsisten.
fn resolve_under(base: &Path, rel: &str) -> Option<PathBuf> {
    let out = normalize_lexical(&base.join(rel))?;
    let base_norm = normalize_lexical(base)?;
    if out == base_norm || out.starts_with(&base_norm) {
        Some(out)
    } else {
        None
    }
}

/// Resolusi path request statis. `req_path` = pathname URL (mis. "/js/app.js").
pub fn safe_join(paths: &AppPaths, req_path: &str) -> Resolved {
    // buang query + decode sederhana (%20 dst. sudah didecode oleh layer HTTP;
    // di sini kita defensif terhadap "..").
    let raw = req_path.split('?').next().unwrap_or("");
    let decoded = percent_decode(raw);
    if has_dot_dot(raw) || has_dot_dot(&decoded) {
        return Resolved::Forbidden;
    }
    let rel = decoded.trim_start_matches(['/', '\\']).to_string();

    // model files hidup di bawah DATA, bukan STATIC.
    let low = rel.replace('\\', "/");
    if low.starts_with("model/") {
        return match resolve_under(&paths.data_dir, &rel) {
            Some(f) => file_or_missing(f),
            None => Resolved::Forbidden,
        };
    }

    // config.json / .bak sensitif.
    if SENSITIVE.contains(&low.as_str()) {
        return Resolved::Forbidden;
    }

    let full = match resolve_under(&paths.static_dir, &rel) {
        Some(f) => f,
        None => return Resolved::Forbidden,
    };
    if is_file(&full) {
        return Resolved::File(full);
    }
    // DATA fallback untuk path polos yang tak ada di STATIC.
    if let Some(alt) = resolve_under(&paths.data_dir, &rel) {
        if is_file(&alt) {
            return Resolved::File(alt);
        }
    }
    // ada resolve tapi file tak ada → NotFound (boleh SPA fallback).
    if full.exists() {
        Resolved::File(full)
    } else {
        Resolved::NotFound
    }
}

fn is_file(p: &Path) -> bool {
    p.is_file()
}

fn file_or_missing(f: PathBuf) -> Resolved {
    if is_file(&f) {
        Resolved::File(f)
    } else {
        Resolved::NotFound
    }
}

/// Decode persen publik (dipakai lib.rs untuk jalur rescue virtual).
pub fn percent_decode_pub(s: &str) -> String {
    percent_decode(s)
}

/// Decode persen minimal (%XX) — cukup untuk deteksi ".." ter-encode.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// MIME by suffix terpanjang (padanan tabel MIME + logika suffix di TS).
pub fn mime_for(path: &str) -> &'static str {
    let p = path.to_lowercase();
    // urutan penting: suffix panjang dulu.
    const TABLE: &[(&str, &str)] = &[
        (".model3.json", "application/json; charset=utf-8"),
        (".physics3.json", "application/json; charset=utf-8"),
        (".exp3.json", "application/json; charset=utf-8"),
        (".cdi3.json", "application/json; charset=utf-8"),
        (".html", "text/html; charset=utf-8"),
        (".css", "text/css; charset=utf-8"),
        (".mjs", "text/javascript; charset=utf-8"),
        (".js", "text/javascript; charset=utf-8"),
        (".json", "application/json; charset=utf-8"),
        (".png", "image/png"),
        (".jpeg", "image/jpeg"),
        (".jpg", "image/jpeg"),
        (".gif", "image/gif"),
        (".svg", "image/svg+xml"),
        (".ico", "image/x-icon"),
        (".moc3", "application/octet-stream"),
        (".woff2", "font/woff2"),
        (".woff", "font/woff"),
        (".mp3", "audio/mpeg"),
        (".wav", "audio/wav"),
    ];
    // pilih match dgn suffix terpanjang (bukan yang pertama).
    let mut best: (&str, &str) = ("", "application/octet-stream");
    for (ext, mime) in TABLE {
        if p.ends_with(ext) && ext.len() > best.0.len() {
            best = (ext, mime);
        }
    }
    best.1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths() -> AppPaths {
        AppPaths::from_root("/app")
    }

    #[test]
    fn tolak_traversal() {
        assert_eq!(safe_join(&paths(), "/../secret"), Resolved::Forbidden);
        assert_eq!(safe_join(&paths(), "/js/../../etc"), Resolved::Forbidden);
        // ".." ter-encode juga ditolak.
        assert_eq!(safe_join(&paths(), "/%2e%2e/x"), Resolved::Forbidden);
    }

    #[test]
    fn tolak_config_sensitif() {
        assert_eq!(safe_join(&paths(), "/config.json"), Resolved::Forbidden);
        assert_eq!(safe_join(&paths(), "config.json.bak"), Resolved::Forbidden);
    }

    #[test]
    fn model_diresolve_di_bawah_data() {
        // file tak ada di FS test → NotFound, tapi path di bawah data/model,
        // bukan Forbidden (membuktikan routing model→data).
        let r = safe_join(&paths(), "/model/foo/bar.moc3");
        assert_eq!(r, Resolved::NotFound);
    }

    #[test]
    fn mime_suffix_terpanjang() {
        assert_eq!(mime_for("hana.model3.json"), "application/json; charset=utf-8");
        assert_eq!(mime_for("app.js"), "text/javascript; charset=utf-8");
        assert_eq!(mime_for("view.mjs"), "text/javascript; charset=utf-8");
        assert_eq!(mime_for("a.png"), "image/png");
        assert_eq!(mime_for("x.unknown"), "application/octet-stream");
    }
}
