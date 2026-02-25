#![warn(missing_docs)]
//! OneClaw Providers — LLM provider implementations
//!
//! Implements LlmProvider trait for various backends:
//! - Ollama (local inference)
//! - OpenAI-compatible (cloud APIs)

/// Ollama local inference provider.
pub mod ollama;
/// OpenAI-compatible API provider.
pub mod openai_compat;

pub use ollama::OllamaProvider;
pub use openai_compat::OpenAICompatProvider;
