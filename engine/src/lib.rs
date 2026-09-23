//! live2d-engine — library inferensi native (TTS SuperTonic + STT Whisper).
//!
//! Dipakai IN-PROCESS oleh Rust core (Companion.exe) — tanpa sidecar, tanpa
//! port 8330. Bin sidecar lama sudah dihapus.
//!
//! Lihat docs/ARCHITECTURE-TAURI-RUST.md.

pub mod text;
pub mod tts;
pub mod wav;
#[cfg(feature = "stt")]
pub mod stt;
