//! Provider module — v1.5 multi-provider foundation
//!
//! This module defines the Provider trait and supporting types for
//! OneClaw's multi-provider strategy. 6 providers, 4 API formats:
//!
//!   Format 1 — Anthropic:  POST /v1/messages, x-api-key header
//!   Format 2 — OpenAI:     POST /v1/chat/completions, Bearer header
//!   Format 3 — Ollama:     POST /api/chat, no auth
//!   Format 4 — Gemini:     POST /v1beta/models/{model}:generateContent, ?key= query

pub mod traits;
pub mod anthropic;
pub mod openai_compat;
pub mod ollama;
pub mod gemini;
pub mod chain_builder;
pub mod embedding;
pub mod embedding_ollama;
pub mod embedding_openai;

pub use traits::{
    Provider, ProviderConfig, ProviderResponse,
    ChatMessage, MessageRole, TokenUsage,
    ReliableProvider, FallbackChain,
    NoopTestProvider, FailingTestProvider, CountingTestProvider,
};
pub use anthropic::{AnthropicProvider, resolve_provider};
pub use openai_compat::{
    OpenAICompatibleProvider,
    ProviderPreset, PRESET_OPENAI, PRESET_DEEPSEEK, PRESET_GROQ,
    default_model_for_provider,
};
pub use ollama::OllamaProvider;
pub use gemini::GeminiProvider;
pub use chain_builder::{build_provider_chain, describe_chain};
pub use embedding::{EmbeddingConfig, EmbeddingProvider, build_embedding_provider, parse_embedding_config};
pub use embedding_ollama::OllamaEmbedding;
pub use embedding_openai::OpenAIEmbedding;
