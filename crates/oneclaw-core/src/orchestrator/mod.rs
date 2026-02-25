//! Layer 1: LLM Orchestrator — Heart (MOAT)
//! Smart routing, chain execution, context management, graceful degradation.

pub mod router;
pub mod context;
pub mod chain;
pub mod fallback;
pub mod provider;
pub mod provider_manager;

// Re-exports
pub use router::{ModelRouter, DefaultRouter, Complexity, ModelChoice};
pub use context::{ContextManager, DefaultContextManager};
pub use chain::{ChainExecutor, NoopChainExecutor, DefaultChainExecutor, Chain, ChainStep, ChainContext, ChainResult, StepResult, StepAction};
pub use fallback::DegradationMode;
pub use provider::{LlmProvider, LlmRequest, LlmResponse, ChatMessage, MessageRole, NoopProvider, TokenUsage};
pub use provider_manager::ProviderManager;
