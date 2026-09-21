//! live2d-core — Rust application core.
//!
//! Menyediakan **server HTTP in-process** (axum) yang, di dalam Tauri, melayani
//! `/api/*` menggantikan server Bun rute demi rute — jalur menuju SATU exe
//! (`Companion.exe`) tanpa proses Bun terpisah. Wire tetap HTTP loopback, jadi
//! frontend & klien mandiri (CLI/OBS/HP) tak perlu berubah.
//!
//! Renderer Live2D/PixiJS TETAP di frontend TypeScript. Lihat
//! docs/ARCHITECTURE-TAURI-RUST.md.
//!
//! Stage 2+ mengisi `router()` dengan port rute Bun (config, model, sheet, dst).

use axum::{routing::get, Json, Router};
use serde_json::json;

/// Versi core — dipakai command Tauri `app_info` + endpoint `/api/version`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Placeholder Stage 0: bukti crate ter-link.
pub fn core_ready() -> bool {
    true
}

/// Bangun router HTTP core. Stage 1: baru `/health` + `/api/version` (bukti
/// pola in-process). Rute Bun diport ke sini bertahap (Stage 2+).
pub fn router() -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/version", get(version))
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok", "core": VERSION }))
}

async fn version() -> Json<serde_json::Value> {
    Json(json!({ "core_version": VERSION, "engine": "rust-in-process" }))
}

/// Jalankan server core di loopback `127.0.0.1:<port>`. Dipanggil dari Tauri
/// (atau CLI dev) untuk menyajikan backend tanpa Bun. Blocking sampai shutdown.
pub async fn serve(port: u16) -> std::io::Result<()> {
    let addr = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    eprintln!("[core] server HTTP in-process siap di http://{addr}");
    axum::serve(listener, router()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt; // oneshot

    #[test]
    fn skeleton_siap() {
        assert!(core_ready());
        assert!(!VERSION.is_empty());
    }

    #[tokio::test]
    async fn health_membalas_ok() {
        let resp = router()
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn version_membalas_core_version() {
        let resp = router()
            .oneshot(Request::builder().uri("/api/version").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["core_version"], VERSION);
    }
}
