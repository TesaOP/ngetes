//! live2d-core — Rust application core (skeleton, Stage 0).
//!
//! Belum berisi logika. Backend TypeScript/Bun (config, persistence, LLM,
//! agent, media, browser) akan dipindah ke sini bertahap sesuai
//! docs/ARCHITECTURE-TAURI-RUST.md — dimulai Stage 2 (infrastruktur).
//!
//! Renderer Live2D/PixiJS TETAP di frontend TypeScript; core hanya mengirim
//! directive semantik lewat Tauri IPC (bukan parameter per-frame).

/// Versi core — dipakai command Tauri `app_info` (Stage 1).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Placeholder Stage 0: bukti crate ter-link & lolos test workspace.
pub fn core_ready() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_siap() {
        assert!(core_ready());
        assert!(!VERSION.is_empty());
    }
}
