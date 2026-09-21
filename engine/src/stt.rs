//! STT via whisper-rs (binding whisper.cpp). Hanya dikompilasi saat feature `stt`.
//!
//! Input: sampel f32 mono. whisper.cpp butuh 16 kHz — resample bila perlu.
//! Model GGML (.bin) diambil relatif ke direktori model.

use std::path::Path;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct Whisper {
    ctx: WhisperContext,
}

impl Whisper {
    /// Muat model GGML dari file .bin.
    pub fn load(model_path: &Path) -> Result<Self, String> {
        let path = model_path
            .to_str()
            .ok_or("path model GGML bukan UTF-8")?;
        let ctx = WhisperContext::new_with_params(path, WhisperContextParameters::default())
            .map_err(|e| format!("gagal muat model whisper: {e}"))?;
        Ok(Self { ctx })
    }

    /// Transkripsi sampel mono (sample_rate apa pun) → teks.
    /// `lang`: Some("id") / Some("auto") / None (=auto).
    pub fn transcribe(
        &self,
        samples: &[f32],
        sample_rate: u32,
        lang: Option<&str>,
    ) -> Result<String, String> {
        let audio = if sample_rate != 16_000 {
            resample_linear(samples, sample_rate, 16_000)
        } else {
            samples.to_vec()
        };

        let mut state = self.ctx.create_state().map_err(|e| format!("state: {e}"))?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        let lang = lang.unwrap_or("auto");
        // whisper pakai "auto" untuk deteksi; kode ISO ("id") untuk paksa.
        let lang_code = if lang == "auto" || lang == "indonesian" {
            if lang == "indonesian" { "id" } else { "auto" }
        } else {
            lang
        };
        params.set_language(Some(lang_code));
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        state
            .full(params, &audio)
            .map_err(|e| format!("transkripsi gagal: {e}"))?;

        let n = state.full_n_segments();
        let mut text = String::new();
        for i in 0..n {
            if let Some(seg) = state.get_segment(i) {
                if let Ok(s) = seg.to_str_lossy() {
                    text.push_str(&s);
                }
            }
        }
        Ok(text.trim().to_string())
    }
}

/// Resampler linear sederhana (sepadan konsep resampler voice-input.js).
fn resample_linear(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = to as f64 / from as f64;
    let out_len = ((input.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = i as f64 / ratio;
        let idx = src.floor() as usize;
        let frac = (src - idx as f64) as f32;
        let a = input.get(idx).copied().unwrap_or(0.0);
        let b = input.get(idx + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * frac);
    }
    out
}
