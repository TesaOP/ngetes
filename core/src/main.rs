//! live2d-core (bin) — server HTTP produksi Rust in-process, pengganti
//! live2d-agent.exe (Bun). Menyajikan frontend + rute yang sudah diport
//! (statis, config, models, sheet, ekspresi, motions, chat/LLM, TTS in-process).
//! Rute yang belum diport (assistant/vtuber/browser/pet/stt) menjawab 404 —
//! degrade anggun sampai diport. Tujuan akhir: melepas runtime JS sepenuhnya.
//!
//! PORT dari env PORT (default 8310, sama dengan Bun) supaya shell Tauri &
//! frontend tidak perlu berubah.

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("PORT").ok().and_then(|s| s.parse().ok()).unwrap_or(8310);
    let paths = live2d_core::paths::AppPaths::detect();
    eprintln!(
        "[live2d-core] server Rust in-process — root={} static={}",
        paths.root.display(),
        paths.static_dir.display()
    );
    if let Err(e) = live2d_core::serve(port, paths).await {
        eprintln!("[live2d-core] gagal: {e}");
        std::process::exit(1);
    }
}
