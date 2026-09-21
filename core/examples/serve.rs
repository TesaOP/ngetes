//! Contoh/smoke: jalankan server core in-process untuk uji manual.
//!   cargo run -p live2d-core --example serve -- <root> <port>
//! <root> = akar app (berisi static/ & data/). Default: cwd, port 8340.

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let root = args.next().unwrap_or_else(|| ".".to_string());
    let port: u16 = args.next().and_then(|s| s.parse().ok()).unwrap_or(8340);
    let paths = live2d_core::paths::AppPaths::from_root(&root);
    eprintln!("[example] root={root} static={:?}", paths.static_dir);
    live2d_core::serve(port, paths).await.unwrap();
}
