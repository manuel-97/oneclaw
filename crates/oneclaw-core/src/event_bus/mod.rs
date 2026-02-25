//! Layer 3: Event Bus — Nervous System
//! Pub/Sub + Pipeline Engine for reactive event processing.

pub mod traits;
pub mod bus;
pub mod pipeline;

pub use traits::*;
pub use bus::DefaultEventBus;
pub use pipeline::{Pipeline, PipelineStep, FilterOp};
