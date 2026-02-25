//! Unified error types for OneClaw Core

use thiserror::Error;

/// Unified error type for all OneClaw subsystems.
#[derive(Error, Debug)]
pub enum OneClawError {
    /// Security subsystem error.
    #[error("Security: {0}")]
    Security(String),

    /// Orchestrator subsystem error.
    #[error("Orchestrator: {0}")]
    Orchestrator(String),

    /// Memory subsystem error.
    #[error("Memory: {0}")]
    Memory(String),

    /// Event bus subsystem error.
    #[error("EventBus: {0}")]
    EventBus(String),

    /// Tool subsystem error.
    #[error("Tool: {0}")]
    Tool(String),

    /// Channel subsystem error.
    #[error("Channel: {0}")]
    Channel(String),

    /// Provider subsystem error.
    #[error("Provider: {0}")]
    Provider(String),

    /// Configuration error.
    #[error("Config: {0}")]
    Config(String),

    /// I/O error.
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization or deserialization error.
    #[error("Serialization: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Convenience result type for OneClaw operations.
pub type Result<T> = std::result::Result<T, OneClawError>;
