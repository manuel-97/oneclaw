//! Layer 3: Event Bus — Nervous System
//! Pub/Sub + Pipeline Engine for reactive event processing.
//!
//! Two implementations:
//! - `DefaultEventBus` — sync, drain-based (default)
//! - `AsyncEventBus` — tokio broadcast, realtime (opt-in)

pub mod traits;
pub mod bus;
pub mod async_bus;
pub mod pipeline;

pub use traits::*;
pub use bus::DefaultEventBus;
pub use async_bus::AsyncEventBus;
pub use pipeline::{Pipeline, PipelineStep, FilterOp};
