//! Media TTS in-process — memanggil `live2d_engine::tts` (SuperTonic) LANGSUNG,
//! tanpa sidecar HTTP. Ini bagian dari tujuan single-exe: TTS dijalankan di
//! proses Rust yang sama, bukan diproxy ke engine.exe.
//!
//! Model diambil dari `~/.cache/supertonic3` (berbagi dengan Python) atau
//! `engines/models/supertonic3`. STT (whisper) di belakang feature engine-stt.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use live2d_engine::tts::SuperTonic;
use live2d_engine::wav;

use crate::config;
use crate::paths::AppPaths;

// Model TTS dimuat sekali (4 ONNX mahal) lalu di-cache. Synth = kerja blocking
// CPU → dijalankan di spawn_blocking; Mutex menjaga akses serial.
static TTS_ENGINE: OnceLock<Mutex<Option<SuperTonic>>> = OnceLock::new();

/// Direktori model SuperTonic: cache Python bila lengkap, else engines/models.
fn tts_model_dir(paths: &AppPaths) -> PathBuf {
    if let Some(home) = dirs_home() {
        let shared = home.join(".cache").join("supertonic3");
        if shared.join("onnx").join("vocoder.onnx").exists() {
            return shared;
        }
    }
    paths.root.join("engines").join("models").join("supertonic3")
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// True bila model TTS sudah ada di disk (siap dipakai in-process).
pub fn tts_model_ready(paths: &AppPaths) -> bool {
    tts_model_dir(paths).join("onnx").join("vocoder.onnx").exists()
}

/// Sintesis TTS in-process (SuperTonic). Return (WAV bytes, mime) atau error.
/// Blocking di-offload ke spawn_blocking.
pub async fn synth_tts(paths: &AppPaths, text: &str, voice: &str, lang: &str) -> Result<(Vec<u8>, &'static str), String> {
    if text.trim().is_empty() {
        return Err("teks kosong".into());
    }
    let model_dir = tts_model_dir(paths);
    if !model_dir.join("onnx").join("vocoder.onnx").exists() {
        return Err(format!(
            "model SuperTonic belum ada di {} — unduh dulu (on-demand belum diport ke core)",
            model_dir.display()
        ));
    }
    let text = text.to_string();
    let voice = voice.to_string();
    let lang = lang.to_string();
    tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, &'static str), String> {
        let cell = TTS_ENGINE.get_or_init(|| Mutex::new(None));
        let mut guard = cell.lock().map_err(|_| "lock TTS")?;
        if guard.is_none() {
            *guard = Some(SuperTonic::load(&model_dir)?);
        }
        let eng = guard.as_mut().unwrap();
        let style = SuperTonic::load_style(&model_dir, &voice)?;
        let lang_opt = if lang.is_empty() { None } else { Some(lang.as_str()) };
        let samples = eng.synthesize(&text, &style, 8, 1.05, 0.3, lang_opt)?;
        Ok((wav::encode_pcm16(&samples, eng.sample_rate()), "audio/wav"))
    })
    .await
    .map_err(|e| format!("task TTS gagal: {e}"))?
}

/// Voice + lang default dari config.tts (fallback F1 / id).
pub fn tts_voice_lang(config_path: &Path) -> (String, String) {
    let cfg = config::load(config_path);
    let tts = cfg.get("tts").cloned().unwrap_or_default();
    let voice = tts.get("voice").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).unwrap_or("F1").to_string();
    let lang = tts.get("lang").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).unwrap_or("id").to_string();
    (voice, lang)
}

// ── STT in-process (whisper) — di belakang feature engine-stt (cmake+LLVM) ──
#[cfg(feature = "engine-stt")]
static STT_ENGINE: OnceLock<Mutex<Option<live2d_engine::stt::Whisper>>> = OnceLock::new();

/// Path model GGML whisper: engines/models/ggml-<name>.bin (release) atau
/// engine/models/ggml-<name>.bin (dev). None bila tak ada.
#[cfg(feature = "engine-stt")]
fn stt_model_path(paths: &AppPaths, name: &str) -> Option<PathBuf> {
    let clean: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-').collect();
    for base in [paths.root.join("engines").join("models"), paths.root.join("engine").join("models")] {
        let p = base.join(format!("ggml-{clean}.bin"));
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// Transkripsi STT in-process (whisper). `audio_wav` = byte WAV; lang mis "id".
/// Hanya tersedia bila di-compile dgn feature engine-stt.
#[cfg(feature = "engine-stt")]
pub async fn transcribe_stt(paths: &AppPaths, audio_wav: Vec<u8>, lang: String, model_name: String) -> Result<String, String> {
    let model_path = stt_model_path(paths, &model_name)
        .ok_or_else(|| format!("model whisper ggml-{model_name}.bin belum ada (unduh dulu)"))?;
    tokio::task::spawn_blocking(move || -> Result<String, String> {
        let (samples, sr) = live2d_engine::wav::decode_pcm16(&audio_wav)?;
        let cell = STT_ENGINE.get_or_init(|| Mutex::new(None));
        let mut guard = cell.lock().map_err(|_| "lock STT")?;
        if guard.is_none() {
            *guard = Some(live2d_engine::stt::Whisper::load(&model_path)?);
        }
        let lang_opt = if lang.is_empty() { None } else { Some(lang.as_str()) };
        guard.as_ref().unwrap().transcribe(&samples, sr, lang_opt)
    })
    .await
    .map_err(|e| format!("task STT gagal: {e}"))?
}

/// STT provider + model dari config.stt (fallback local / base).
pub fn stt_provider_model(config_path: &Path) -> (String, String, String) {
    let stt = stt_section(config_path);
    let provider = stt.get("provider").and_then(|v| v.as_str()).unwrap_or("local").to_string();
    let model = stt.get("engineModel").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).unwrap_or("base").to_string();
    (provider, model, stt_lang_of(&stt))
}

/// Section stt dari config (config::load sudah backfill default per key).
fn stt_section(config_path: &Path) -> serde_json::Value {
    config::load(config_path).get("stt").cloned().unwrap_or_default()
}

/// Bahasa whisper dari config.stt: "indonesian" → "id", selain itu apa adanya
/// ("auto" berarti biarkan deteksi sendiri).
fn stt_lang_of(stt: &serde_json::Value) -> String {
    let raw = stt.get("language").and_then(|v| v.as_str()).unwrap_or("auto");
    if raw == "indonesian" { "id" } else { raw }.to_string()
}

/// Konfigurasi provider "openai" dari config.stt: (endpoint, api_key, model, lang).
/// endpoint kosong → resmi OpenAI (lihat `transcription_url`); model dari
/// `stt.apiModel` (fallback "whisper-1") — field `model` milik provider browser.
pub fn stt_openai_config(config_path: &Path) -> (String, String, String, String) {
    let stt = stt_section(config_path);
    let endpoint = stt.get("endpoint").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let api_key = stt.get("apiKey").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let model = stt.get("apiModel").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).unwrap_or("whisper-1").to_string();
    (endpoint, api_key, model, stt_lang_of(&stt))
}

/// URL endpoint transkripsi dari base URL `stt.endpoint`. Kosong → resmi OpenAI.
/// Base yang sudah memuat path `/audio/transcriptions` dipakai apa adanya (user
/// tempel URL lengkap), selain itu base dianggap memuat `/v1` dan path ditempel.
pub fn transcription_url(endpoint: &str) -> String {
    let base = endpoint.trim().trim_end_matches('/');
    if base.is_empty() {
        return "https://api.openai.com/v1/audio/transcriptions".into();
    }
    if base.ends_with("/audio/transcriptions") {
        return base.to_string();
    }
    format!("{base}/audio/transcriptions")
}

/// Rakit body multipart/form-data untuk /audio/transcriptions: part file (WAV)
/// + model + language. Part language DILEWATI bila "auto"/kosong (API OpenAI
/// tidak mengenal "auto" — tanpa field berarti deteksi sendiri). Return
/// (content_type, body).
pub fn build_transcription_body(boundary: &str, model: &str, lang: &str, wav: &[u8]) -> (String, Vec<u8>) {
    let mut b = Vec::with_capacity(wav.len() + 512);
    let push = |b: &mut Vec<u8>, s: &str| b.extend_from_slice(s.as_bytes());
    push(&mut b, &format!("--{boundary}\r\n"));
    push(&mut b, "Content-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\n");
    push(&mut b, "Content-Type: audio/wav\r\n\r\n");
    b.extend_from_slice(wav);
    push(&mut b, &format!("\r\n--{boundary}\r\n"));
    push(&mut b, "Content-Disposition: form-data; name=\"model\"\r\n\r\n");
    push(&mut b, &format!("{model}\r\n"));
    if !lang.is_empty() && lang != "auto" {
        push(&mut b, &format!("--{boundary}\r\n"));
        push(&mut b, "Content-Disposition: form-data; name=\"language\"\r\n\r\n");
        push(&mut b, &format!("{lang}\r\n"));
    }
    push(&mut b, &format!("--{boundary}--\r\n"));
    (format!("multipart/form-data; boundary={boundary}"), b)
}

/// Parse respons JSON transcription: `{"text": "..."}` → teks ter-trim.
pub fn parse_transcription_response(body: &str) -> Result<String, String> {
    let j: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| format!("respon bukan JSON: {}", body.chars().take(200).collect::<String>()))?;
    match j.get("text").and_then(|v| v.as_str()) {
        Some(t) => Ok(t.trim().to_string()),
        None => Err(format!("respon tanpa \"text\": {}", body.chars().take(200).collect::<String>())),
    }
}

/// Transkripsi via server OpenAI-compatible (`/v1/audio/transcriptions`).
/// HANYA dipanggil bila user eksplisit menyetel `stt.provider: "openai"` di
/// config.json (cloud, tidak pernah default — audio diunggah ke endpoint itu).
pub async fn transcribe_openai(endpoint: &str, api_key: &str, model: &str, lang: &str, audio_wav: Vec<u8>) -> Result<String, String> {
    // Boundary unik per request (pid + nanos); body WAV tak mungkin memuatnya.
    let boundary = format!(
        "----lumimi-stt-{:x}{:x}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let (content_type, body) = build_transcription_body(&boundary, model, lang, &audio_wav);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("client HTTP: {e}"))?;
    let resp = client
        .post(transcription_url(endpoint))
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", content_type)
        .body(body)
        .send()
        .await
        .map_err(|e| format!("gagal menghubungi endpoint: {e}"))?;
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    if status >= 400 {
        return Err(format!("HTTP {status}: {}", text.chars().take(300).collect::<String>()));
    }
    parse_transcription_response(&text)
}

/// Daftar voice style tersedia (nama file voice_styles/*.json), untuk katalog.
pub fn tts_voices(paths: &AppPaths) -> Vec<String> {
    let dir = tts_model_dir(paths).join("voice_styles");
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    out.push(stem.to_string());
                }
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcription_url_join_benar() {
        assert_eq!(transcription_url(""), "https://api.openai.com/v1/audio/transcriptions");
        assert_eq!(
            transcription_url("https://api.groq.com/openai/v1"),
            "https://api.groq.com/openai/v1/audio/transcriptions"
        );
        // trailing slash dibersihkan sebelum ditempel
        assert_eq!(
            transcription_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/audio/transcriptions"
        );
        // URL lengkap tempelan user dipakai apa adanya
        assert_eq!(
            transcription_url("https://host.example/v1/audio/transcriptions"),
            "https://host.example/v1/audio/transcriptions"
        );
    }

    #[test]
    fn body_multipart_isi_benar() {
        let wav = vec![0x52u8, 0x49, 0x46, 0x46, 0x00, 0x01];
        let (ct, body) = build_transcription_body("BOUNDRY", "whisper-1", "id", &wav);
        assert!(ct.starts_with("multipart/form-data; boundary=BOUNDRY"));
        let s = String::from_utf8_lossy(&body);
        assert!(s.contains("name=\"file\"; filename=\"audio.wav\""));
        assert!(s.contains("name=\"model\"\r\n\r\nwhisper-1\r\n"));
        assert!(s.contains("name=\"language\"\r\n\r\nid\r\n"));
        assert!(s.ends_with("--BOUNDRY--\r\n"));
        // byte WAV utuh ada di body
        let pos = s.find("audio/wav\r\n\r\n").unwrap() + "audio/wav\r\n\r\n".len();
        assert_eq!(&body[pos..pos + wav.len()], &wav[..]);
    }

    #[test]
    fn body_multipart_auto_tanpa_language() {
        let (_ct, body) = build_transcription_body("B", "whisper-1", "auto", &[]);
        let s = String::from_utf8_lossy(&body);
        assert!(!s.contains("name=\"language\""));
        let (_ct, body) = build_transcription_body("B", "whisper-1", "", &[]);
        assert!(!String::from_utf8_lossy(&body).contains("name=\"language\""));
    }

    #[test]
    fn parse_respons_transkripsi() {
        assert_eq!(parse_transcription_response(r#"{"text":" halo dunia "}"#).unwrap(), "halo dunia");
        assert_eq!(parse_transcription_response(r#"{"text":""}"#).unwrap(), "");
        let e = parse_transcription_response("bukan json").unwrap_err();
        assert!(e.contains("bukan JSON"), "{e}");
        let e = parse_transcription_response(r#"{"beda":1}"#).unwrap_err();
        assert!(e.contains("tanpa \"text\""), "{e}");
    }

    #[test]
    fn config_openai_baca_dan_default() {
        let dir = std::env::temp_dir().join(format!("l2dmedtest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("config.json");
        std::fs::write(
            &f,
            r#"{"stt":{"provider":"openai","endpoint":"https://api.groq.com/openai/v1","apiKey":" sk-test123 ","apiModel":"whisper-large-v3-turbo"}}"#,
        )
        .unwrap();
        let (endpoint, key, model, lang) = stt_openai_config(&f);
        assert_eq!(endpoint, "https://api.groq.com/openai/v1");
        assert_eq!(key, "sk-test123"); // trim
        assert_eq!(model, "whisper-large-v3-turbo");
        // file tanpa apiModel → fallback whisper-1; language hilang → auto
        std::fs::write(&f, r#"{"stt":{"provider":"openai","apiKey":"sk-x"}}"#).unwrap();
        let (endpoint, key, model, lang) = stt_openai_config(&f);
        assert_eq!(endpoint, "");
        assert_eq!(key, "sk-x");
        assert_eq!(model, "whisper-1");
        assert_eq!(lang, "auto");
        // file hilang sama sekali → default utuh (default_config: language
        // "indonesian" → "id")
        let (_e, key, model, lang) = stt_openai_config(&dir.join("tak-ada.json"));
        assert_eq!(key, "");
        assert_eq!(model, "whisper-1");
        assert_eq!(lang, "id");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
