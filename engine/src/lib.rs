//! live2d-engine — library inferensi native (TTS SuperTonic + STT Whisper).
//!
//! Modul di sini dipakai dua konsumen:
//!   - `src/main.rs` (bin) — sidecar HTTP loopback untuk debug/standalone.
//!   - Rust core Tauri (Stage 4 migrasi) — memanggil langsung in-process,
//!     menghilangkan sidecar + port 8330.
//!
//! Lihat docs/ARCHITECTURE-TAURI-RUST.md.

pub mod text;
pub mod tts;
pub mod wav;
#[cfg(feature = "stt")]
pub mod stt;
