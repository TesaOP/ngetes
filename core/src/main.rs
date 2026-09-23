//! live2d-core (bin) — host DEV/EKSTERNAL adapter HTTP, bukan produksi.
//! Produksi = in-process di dalam Companion.exe. Binary ini untuk: dev
//! browser (`cargo run -p live2d-core` / `bun run dev`), CLI, dan integrasi
//! eksternal (OBS). Wire & logika identik (router yang sama).
//!
//! PORT dari env PORT (default 8310).

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
