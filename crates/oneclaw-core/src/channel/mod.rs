//! Layer 5: Channel — Ears & Mouth

pub mod traits;
pub mod manager;

pub use traits::{Channel, NoopChannel, IncomingMessage, OutgoingMessage};
pub use manager::ChannelManager;
