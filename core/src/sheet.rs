//! Rute sheet: get/post + sanitizeKey. Sheet = cache scan karakter per-model
//! di `data/sheets/<key>.json`.
//!
//! Invarian dipertahankan: `sanitizeKey` mempertahankan huruf/angka SEMUA aksara
//! (kanji/kana/latin) → `_` untuk sisanya; stamp `scannerVersion`; sheet dgn
//! versi lama ditandai `_stale`; tulis atomik (tmp+rename).

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// Versi scanner sheet (padanan SHEET_SCANNER_VERSION di index.ts).
pub const SHEET_SCANNER_VERSION: i64 = 2;

/// Sanitasi kunci sheet: huruf/angka Unicode + `_` dipertahankan, sisanya `_`.
/// Padanan `sanitizeKey` (regex `[^\p{L}\p{N}_]`); Rust `is_alphanumeric()`
/// sudah Unicode-aware (mencakup \p{L}\p{N}).
pub fn sanitize_key(name: &str) -> String {
    let base = if name.is_empty() { "default" } else { name };
    base.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

fn sheet_path_for(sheets_dir: &Path, name: &str) -> PathBuf {
    sheets_dir.join(format!("{}.json", sanitize_key(name)))
}

/// Path relatif ke `data/` (separator "/") untuk hasil balasan POST.
fn rel_to_data(data_dir: &Path, p: &Path) -> String {
    p.strip_prefix(data_dir)
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| p.to_string_lossy().replace('\\', "/"))
}

/// GET /api/sheet?name=X → (status, body JSON string). 404 bila tak ada.
/// Sheet dgn scannerVersion lama ditandai `_stale`.
pub fn get_sheet(sheets_dir: &Path, name: &str) -> (u16, String) {
    let p = sheet_path_for(sheets_dir, name);
    let raw = match std::fs::read_to_string(&p) {
        Ok(s) => s,
        Err(_) => return (404, json!({ "error": "no sheet" }).to_string()),
    };
    let trimmed = raw.strip_prefix('\u{feff}').unwrap_or(&raw); // strip BOM
    match serde_json::from_str::<Value>(trimmed) {
        Ok(mut parsed) if parsed.is_object() => {
            let ver = parsed.get("scannerVersion").and_then(|v| v.as_i64());
            if ver != Some(SHEET_SCANNER_VERSION) {
                if let Some(obj) = parsed.as_object_mut() {
                    obj.insert(
                        "_stale".into(),
                        json!({
                            "reason": "scannerVersion",
                            "have": parsed_have(ver),
                            "want": SHEET_SCANNER_VERSION
                        }),
                    );
                }
            }
            (200, parsed.to_string())
        }
        // JSON tak valid / bukan objek: kembalikan mentah (konsumen memvalidasi).
        _ => (200, raw),
    }
}

fn parsed_have(ver: Option<i64>) -> Value {
    match ver {
        Some(v) => json!(v),
        None => Value::Null,
    }
}

/// POST /api/sheet — stamp scannerVersion, tulis atomik. `body` = objek sheet
/// (atau {modelName, sheet}). Return (status, body JSON).
pub fn save_sheet(sheets_dir: &Path, data_dir: &Path, body: &Value) -> (u16, String) {
    let name = body
        .get("modelName")
        .and_then(|v| v.as_str())
        .unwrap_or("default")
        .to_string();
    // sheet = body.sheet bila ada, else body sendiri.
    let sheet_src = body.get("sheet").filter(|v| v.is_object()).unwrap_or(body);
    if !sheet_src.is_object() {
        return (400, json!({ "error": "sheet kosong" }).to_string());
    }
    let mut sheet = sheet_src.clone();
    sheet
        .as_object_mut()
        .unwrap()
        .insert("scannerVersion".into(), json!(SHEET_SCANNER_VERSION));

    let target = sheet_path_for(sheets_dir, &name);
    match write_json_atomic(&target, &sheet) {
        Ok(()) => (
            200,
            json!({ "ok": true, "path": rel_to_data(data_dir, &target) }).to_string(),
        ),
        Err(e) => (500, json!({ "error": e.to_string() }).to_string()),
    }
}

/// Tulis atomik tmp+rename (padanan writeJsonAtomic). Buat folder bila perlu.
pub fn write_json_atomic(target: &Path, value: &Value) -> std::io::Result<()> {
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".into());
    let name = target.file_name().and_then(|s| s.to_str()).unwrap_or("out");
    let tmp = target.with_file_name(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, target)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_pertahankan_cjk_dan_kana() {
        assert_eq!(sanitize_key(""), "default");
        assert_eq!(sanitize_key("神宫白子模型"), "神宫白子模型"); // kanji utuh
        assert_eq!(sanitize_key("レンー"), "レンー"); // kana utuh
        assert_eq!(sanitize_key("a b/c"), "a_b_c"); // spasi & slash → _
    }

    #[test]
    fn roundtrip_save_get_dan_stale() {
        let dir = std::env::temp_dir().join(format!("l2dsheet-{}", std::process::id()));
        let data = dir.clone();
        let sheets = data.join("sheets");
        std::fs::create_dir_all(&sheets).unwrap();

        // simpan
        let (st, _) = save_sheet(&sheets, &data, &json!({ "modelName": "hana", "sheet": { "userNote": "hi" } }));
        assert_eq!(st, 200);
        // baca → scannerVersion terstamp, tak stale
        let (st2, body) = get_sheet(&sheets, "hana");
        assert_eq!(st2, 200);
        let v: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["scannerVersion"], SHEET_SCANNER_VERSION);
        assert_eq!(v["userNote"], "hi");
        assert!(v.get("_stale").is_none());

        // tulis sheet versi lama manual → GET menandai _stale
        std::fs::write(sheets.join("old.json"), r#"{"scannerVersion":1,"x":1}"#).unwrap();
        let (_, body2) = get_sheet(&sheets, "old");
        let v2: Value = serde_json::from_str(&body2).unwrap();
        assert_eq!(v2["_stale"]["want"], SHEET_SCANNER_VERSION);
        assert_eq!(v2["_stale"]["have"], 1);

        // tak ada → 404
        let (st404, _) = get_sheet(&sheets, "tidakada");
        assert_eq!(st404, 404);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
