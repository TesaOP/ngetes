//! Agent (assistant) — diport bertahap dari `src/server/agent/`.
//! Batch 3d-1: memory (long-term lintas sesi). Menyusul: sessions, undo,
//! tools, loop + permission gate + SSE.

pub mod assistant;
pub mod bus;
pub mod memory;
pub mod plan;
pub mod sessions;
pub mod tools;
#[path = "loop_.rs"]
pub mod loop_;
