//! Agent (assistant): memory, sessions, undo, tools, loop + permission gate + SSE.

pub mod assistant;
pub mod bus;
pub mod memory;
pub mod motion_tools;
pub mod plan;
pub mod sessions;
pub mod subagent;
pub mod tools;
#[path = "loop_.rs"]
pub mod loop_;
