//! Layer 4: Tool — Hands
//! Sandboxed tool execution with registry and security gating.

pub mod traits;
pub mod registry;

pub use traits::*;
pub use registry::ToolRegistry;
