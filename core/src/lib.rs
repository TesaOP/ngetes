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
pub mod expressions;
pub mod model;
pub mod motions;
pub mod paths;
pub mod rescue;
pub mod sheet;
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
        .route("/api/config", get(get_config).post(post_config))
        .route("/api/models", get(get_models))
        .route("/api/model/path", get(get_model_path))
        .route("/api/sheet", get(get_sheet_h).post(post_sheet_h))
        .route("/api/model/expressions", get(get_expressions))
        .route(
            "/api/model/expressions-adoption",
            get(get_adoption).post(post_adoption),
        )
        .route("/api/model/files", get(get_model_files))
        .route("/api/model/avatar", get(get_model_avatar))
        .route("/api/model/upload", axum::routing::post(post_model_upload))
        .route("/api/model/import-zip", axum::routing::post(post_import_zip))
        .route("/api/model/{name}", axum::routing::delete(delete_model_h))
        .route("/api/motions", get(get_motions_list))
        .route("/api/motions/{id}", get(get_motion_h).delete(del_motion_h))
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

/// POST /api/config — action add/update/delete/setActive/saveEvents/saveTTS/
/// saveI18n/save (padanan handleConfigPost). data/config.json gitignored.
async fn post_config(State(paths): State<AppPaths>, body: axum::body::Bytes) -> Response {
    let parsed: Option<serde_json::Value> = serde_json::from_slice(&body).ok();
    match parsed {
        Some(v) => {
            let (status, out) = config::handle_config_post(&paths.data_dir.join("config.json"), &v);
            json_raw(status, out)
        }
        None => json_status(StatusCode::BAD_REQUEST, json!({ "error": "body JSON rusak" })),
    }
}

/// GET /api/models — daftar folder model yang punya `.model3.json` (terurut).
async fn get_models(State(paths): State<AppPaths>) -> Json<serde_json::Value> {
    Json(json!({ "models": model::list_models(&paths.model_dir) }))
}

/// GET /api/model/path?name=X — path .model3.json relatif ke data/ (atau 404).
async fn get_model_path(
    State(paths): State<AppPaths>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let name = q.get("name").map(String::as_str).unwrap_or("");
    match model::model_path_rel(&paths.data_dir, &paths.model_dir, name) {
        Some(rel) => json_status(StatusCode::OK, json!({ "path": rel })),
        None => json_status(StatusCode::NOT_FOUND, json!({ "error": "not found" })),
    }
}

/// GET /api/sheet?name=X — cache sheet karakter (tandai _stale bila versi lama).
async fn get_sheet_h(
    State(paths): State<AppPaths>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let name = q.get("name").map(String::as_str).unwrap_or("default");
    let (status, body) = sheet::get_sheet(&paths.sheets_dir, name);
    json_raw(status, body)
}

/// POST /api/sheet — stamp scannerVersion + tulis atomik.
async fn post_sheet_h(State(paths): State<AppPaths>, body: axum::body::Bytes) -> Response {
    let parsed: Option<serde_json::Value> = serde_json::from_slice(&body).ok();
    match parsed {
        Some(v) => {
            let (status, out) = sheet::save_sheet(&paths.sheets_dir, &paths.data_dir, &v);
            json_raw(status, out)
        }
        None => json_status(StatusCode::BAD_REQUEST, json!({ "error": "sheet kosong" })),
    }
}

/// GET /api/model/expressions?name=X — discovery ekspresi (+params per exp).
async fn get_expressions(
    State(paths): State<AppPaths>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let name = q.get("name").map(String::as_str).unwrap_or("");
    match expressions::discover(&paths.model_dir, &paths.data_dir, name) {
        Ok(v) => json_status(StatusCode::OK, v),
        Err(e) => json_status(StatusCode::NOT_FOUND, json!({ "error": e })),
    }
}

/// GET /api/model/expressions-adoption?name=X — ekspresi + flag enabled.
async fn get_adoption(
    State(paths): State<AppPaths>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let name = q.get("name").map(String::as_str).unwrap_or("");
    let (status, body) = expressions::adoption_get(&paths.model_dir, &paths.data_dir, &paths.sheets_dir, name);
    json_raw(status, body)
}

/// POST /api/model/expressions-adoption {name, disabled:[]}.
async fn post_adoption(State(paths): State<AppPaths>, body: axum::body::Bytes) -> Response {
    match serde_json::from_slice::<serde_json::Value>(&body).ok() {
        Some(v) => {
            let (status, out) = expressions::adoption_post(&paths.sheets_dir, &v);
            json_raw(status, out)
        }
        None => json_status(StatusCode::BAD_REQUEST, json!({ "error": "body JSON rusak" })),
    }
}

/// GET /api/model/files?name=X — semua file relatif di folder model.
async fn get_model_files(
    State(paths): State<AppPaths>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let name = q.get("name").map(String::as_str).unwrap_or("");
    match model::list_model_files(&paths.model_dir, name) {
        Some(files) => json_status(StatusCode::OK, json!({ "name": name, "files": files })),
        None => json_status(StatusCode::NOT_FOUND, json!({ "error": "not found" })),
    }
}

/// GET /api/model/avatar?name=X — gambar avatar (biner) atau 404.
async fn get_model_avatar(
    State(paths): State<AppPaths>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let name = q.get("name").map(String::as_str).unwrap_or("");
    match model::find_avatar(&paths.model_dir, name) {
        Some(fp) => {
            let mime = model::avatar_mime(&fp);
            match tokio::fs::read(&fp).await {
                Ok(bytes) => Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, mime)
                    .header(header::CACHE_CONTROL, "no-cache")
                    .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                    .body(Body::from(bytes))
                    .unwrap(),
                Err(e) => json_status(StatusCode::INTERNAL_SERVER_ERROR, json!({ "error": e.to_string() })),
            }
        }
        None => json_status(StatusCode::NOT_FOUND, json!({ "error": "no avatar" })),
    }
}

/// DELETE /api/model/:name — hapus folder model (rekursif).
async fn delete_model_h(
    State(paths): State<AppPaths>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Response {
    let (status, body) = model::delete_model(&paths.model_dir, &name);
    json_raw(status, body)
}

/// POST /api/model/upload {name, files:[{path,base64}]}.
async fn post_model_upload(State(paths): State<AppPaths>, body: axum::body::Bytes) -> Response {
    match serde_json::from_slice::<serde_json::Value>(&body).ok() {
        Some(v) => {
            let name = v.get("name").and_then(|x| x.as_str()).unwrap_or("");
            let files = v.get("files").cloned().unwrap_or(json!([]));
            let (status, out) = model::upload_model(&paths.model_dir, name, &files);
            json_raw(status, out)
        }
        None => json_status(StatusCode::BAD_REQUEST, json!({ "error": "body JSON rusak" })),
    }
}

/// POST /api/model/import-zip {name, base64}.
async fn post_import_zip(State(paths): State<AppPaths>, body: axum::body::Bytes) -> Response {
    match serde_json::from_slice::<serde_json::Value>(&body).ok() {
        Some(v) => {
            let name = v.get("name").and_then(|x| x.as_str()).unwrap_or("");
            let b64 = v.get("base64").and_then(|x| x.as_str()).unwrap_or("");
            let (status, out) = model::import_zip(&paths.model_dir, &paths.data_dir, name, b64);
            json_raw(status, out)
        }
        None => json_status(StatusCode::BAD_REQUEST, json!({ "error": "zip kosong" })),
    }
}

/// GET /api/motions?model=X — daftar motion buatan user.
async fn get_motions_list(
    State(paths): State<AppPaths>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let model = q.get("model").map(String::as_str).unwrap_or("default");
    json_status(StatusCode::OK, motions::list_motions(&paths.motions_dir, model))
}

/// GET /api/motions/:id?model=X — satu motion.
async fn get_motion_h(
    State(paths): State<AppPaths>,
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let model = q.get("model").map(String::as_str).unwrap_or("default");
    let (status, body) = motions::get_motion(&paths.motions_dir, model, &id);
    json_raw(status, body)
}

/// DELETE /api/motions/:id?model=X — hapus motion.
async fn del_motion_h(
    State(paths): State<AppPaths>,
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let model = q.get("model").map(String::as_str).unwrap_or("default");
    let (status, body) = motions::delete_motion(&paths.motions_dir, model, &id);
    json_raw(status, body)
}

/// Bangun Response JSON dari status u16 + body string (untuk handler yang sudah
/// menghasilkan JSON string + status sendiri).
fn json_raw(status: u16, body: String) -> Response {
    Response::builder()
        .status(StatusCode::from_u16(status).unwrap_or(StatusCode::OK))
        .header(header::CONTENT_TYPE, "application/json; charset=utf-8")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(Body::from(body))
        .unwrap()
}

/// Penyajian statis + SPA fallback (padanan blok fetch static di index.ts).
async fn static_handler(State(paths): State<AppPaths>, uri: Uri) -> Response {
    let mut pathname = uri.path().to_string();
    if pathname == "/" {
        pathname = "/index.html".to_string();
    }
    // Auto-Rescue: manifest virtual model/<folder>/__rescue__.model3.json
    // dirakit di memori (tidak menulis folder), sama seperti index.ts.
    if let Some(folder) = pathname
        .strip_prefix("/model/")
        .and_then(|s| s.strip_suffix(&format!("/{}", rescue::RESCUE_FILENAME)))
    {
        let decoded = static_serve::percent_decode_pub(folder);
        if decoded.split(['/', '\\']).any(|s| s == "..") {
            return text(StatusCode::FORBIDDEN, "Forbidden");
        }
        let dir = paths.model_dir.join(&decoded);
        return match rescue::build_rescue_blueprint(&dir) {
            Some(m) => json_status(StatusCode::OK, m),
            None => json_status(StatusCode::NOT_FOUND, json!({ "error": "tak bisa dirakit" })),
        };
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
