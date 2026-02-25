//! Channel trait definitions (async)

use crate::error::Result;
use async_trait::async_trait;

/// A message received from an external source.
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    /// The source identifier of the message.
    pub source: String,
    /// The text content of the message.
    pub content: String,
    /// The timestamp when the message was received.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// A message to be sent to an external destination.
#[derive(Debug, Clone)]
pub struct OutgoingMessage {
    /// The destination identifier for the message.
    pub destination: String,
    /// The text content of the message.
    pub content: String,
}

/// Layer 5 Trait: Channel — async communication interface for sending and receiving messages.
#[async_trait]
pub trait Channel: Send + Sync {
    /// Return the name of this channel.
    fn name(&self) -> &str;
    /// Receive the next incoming message, if any (async).
    async fn receive(&self) -> Result<Option<IncomingMessage>>;
    /// Send an outgoing message through this channel (async).
    async fn send(&self, message: &OutgoingMessage) -> Result<()>;
}

/// No-op channel that discards sends and never receives.
pub struct NoopChannel;
#[async_trait]
impl Channel for NoopChannel {
    fn name(&self) -> &str { "noop" }
    async fn receive(&self) -> Result<Option<IncomingMessage>> { Ok(None) }
    async fn send(&self, _message: &OutgoingMessage) -> Result<()> { Ok(()) }
}
