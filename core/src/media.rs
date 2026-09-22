//! Media TTS in-process — memanggil `live2d_engine::tts` (SuperTonic) LANGSUNG,
//! tanpa sidecar HTTP. Ini bagian dari tujuan single-exe: TTS dijalankan di
//! proses Rust yang sama, bukan diproxy ke Bun/engine.exe.
//!
//! Model diambil dari `~/.cache/supertonic3` (berbagi dengan Python) atau
//! `engines/models/supertonic3`. STT (whisper) menyusul di belakang feature.

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
