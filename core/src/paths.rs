//! Resolusi path aplikasi — padanan `src/shared/paths.ts::appRoot()`.
//!
//! `static/` & `data/` hidup di samping akar app. Saat jadi satu exe (Tauri),
//! akar = folder exe; di dev = root repo. Kontrak folder dipertahankan supaya
//! kompatibel dengan server Bun selama transisi.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub root: PathBuf,
    pub static_dir: PathBuf,
    pub data_dir: PathBuf,
    pub model_dir: PathBuf,
    pub sheets_dir: PathBuf,
    pub motions_dir: PathBuf,
}

impl AppPaths {
    /// Bangun dari akar app. `static/` & `data/` relatif ke akar (sama dgn TS).
    pub fn from_root(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        let data_dir = root.join("data");
        Self {
            static_dir: root.join("static"),
            model_dir: data_dir.join("model"),
            sheets_dir: data_dir.join("sheets"),
            motions_dir: data_dir.join("motions"),
            data_dir,
            root,
        }
    }

    /// Deteksi akar seperti appRoot() TS: folder exe bila ada static/index.html
    /// di sampingnya (portable/compiled), selain itu cwd (dev).
    pub fn detect() -> Self {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                if dir.join("static").join("index.html").exists() {
                    return Self::from_root(dir);
                }
            }
        }
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::from_root(cwd)
    }
}
