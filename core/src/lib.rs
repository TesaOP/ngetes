//! live2d-core — Rust application core.
//!
//! Menyediakan **server HTTP in-process** (axum) yang, di dalam Tauri, melayani
//! `/api/*` + aset statis menggantikan server Bun rute demi rute — jalur menuju
//! SATU exe (`Companion.exe`) tanpa proses Bun terpisah. Wire tetap HTTP
//! loopback, jadi frontend & klien mandiri (CLI/OBS/HP) tak perlu berubah.
//!
//! Renderer Live2D/PixiJS TETAP di frontend TypeScript. Lihat
//! docs/ARCHITECTURE-TAURI-RUST.md.
//!
//! Stage 2: static serving + /api/version. Rute Bun lain diport bertahap.

pub mod config;
pub mod paths;
pub mod static_serve;

use axum::{
    body::Body,
    extract::State,
    http::{header, StatusCode, Uri},
    response::Response,
    routing::get,
    Json, Router,
};
use serde_json::json;

use paths::AppPaths;
use static_serve::{mime_for, safe_join, Resolved};

/// Versi core — dipakai command Tauri `app_info` + endpoint `/api/version`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Placeholder Stage 0: bukti crate ter-link.
pub fn core_ready() -> bool {
    true
}

/// Bangun router HTTP core. `/health` + `/api/version` + penyajian statis
/// (fallback) dari `static/` & `data/` sesuai AppPaths.
pub fn router(paths: AppPaths) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/version", get(version))
        .route("/api/config", get(get_config))
        .fallback(static_handler)
        .with_state(paths)
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok", "core": VERSION }))
}

async fn version() -> Json<serde_json::Value> {
    Json(json!({ "core_version": VERSION, "engine": "rust-in-process" }))
}

/// GET /api/config — apiKey dimask, roles dinormalisasi (padanan handler TS).
async fn get_config(State(paths): State<AppPaths>) -> Json<serde_json::Value> {
    Json(config::api_config_response(&paths.data_dir.join("config.json")))
}

/// Penyajian statis + SPA fallback (padanan blok fetch static di index.ts).
async fn static_handler(State(paths): State<AppPaths>, uri: Uri) -> Response {
    let mut pathname = uri.path().to_string();
    if pathname == "/" {
        pathname = "/index.html".to_string();
    }
    match safe_join(&paths, &pathname) {
        Resolved::Forbidden => text(StatusCode::FORBIDDEN, "Forbidden"),
        Resolved::File(f) => serve_file(f).await,
        Resolved::NotFound => {
            // /api/* tak dikenal → 404 JSON (bukan SPA fallback), sama TS.
            if pathname.starts_with("/api/") {
                return json_status(StatusCode::NOT_FOUND, json!({"error":"not found"}));
            }
            // SPA fallback HANYA path tanpa ekstensi (rute UI).
            let last = pathname.rsplit('/').next().unwrap_or("");
            if !last.is_empty() && !last.contains('.') {
                let idx = paths.static_dir.join("index.html");
                if idx.is_file() {
                    return serve_file(idx).await;
                }
            }
            text(StatusCode::NOT_FOUND, "Not Found")
        }
    }
}

async fn serve_file(path: std::path::PathBuf) -> Response {
    let mime = mime_for(&path.to_string_lossy());
    match tokio::fs::read(&path).await {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime)
            .header(header::CACHE_CONTROL, "no-cache")
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .body(Body::from(bytes))
            .unwrap(),
        Err(_) => text(StatusCode::NOT_FOUND, "Not Found"),
    }
}

fn text(status: StatusCode, msg: &str) -> Response {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(Body::from(msg.to_string()))
        .unwrap()
}

fn json_status(status: StatusCode, v: serde_json::Value) -> Response {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json; charset=utf-8")
        .body(Body::from(v.to_string()))
        .unwrap()
}

/// Jalankan server core di loopback `127.0.0.1:<port>`. Blocking sampai shutdown.
pub async fn serve(port: u16, paths: AppPaths) -> std::io::Result<()> {
    let addr = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    eprintln!("[core] server HTTP in-process siap di http://{addr}");
    axum::serve(listener, router(paths)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt; // oneshot

    fn app() -> Router {
        router(AppPaths::from_root("."))
    }

    #[test]
    fn skeleton_siap() {
        assert!(core_ready());
        assert!(!VERSION.is_empty());
    }

    #[tokio::test]
    async fn health_membalas_ok() {
        let resp = app()
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn version_membalas_core_version() {
        let resp = app()
            .oneshot(Request::builder().uri("/api/version").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["core_version"], VERSION);
    }

    #[tokio::test]
    async fn api_tak_dikenal_404_json() {
        let resp = app()
            .oneshot(Request::builder().uri("/api/nope").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let ct = resp.headers().get(header::CONTENT_TYPE).unwrap().to_str().unwrap();
        assert!(ct.contains("application/json"));
    }

    #[tokio::test]
    async fn traversal_403() {
        let resp = app()
            .oneshot(Request::builder().uri("/../secret").body(Body::empty()).unwrap())
            .await
            .unwrap();
        // axum menormalkan sebagian; safe_join tetap menolak segmen "..".
        assert!(matches!(resp.status(), StatusCode::FORBIDDEN | StatusCode::NOT_FOUND));
    }
}
