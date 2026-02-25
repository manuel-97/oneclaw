//! Layer 1: LLM Orchestrator — Heart (MOAT)
//! Smart routing, chain execution, context management, graceful degradation.

pub mod router;
pub mod context;
pub mod chain;

// Re-exports
pub use router::{ModelRouter, DefaultRouter, Complexity, ModelChoice};
pub use context::{ContextManager, DefaultContextManager};
pub use chain::{ChainExecutor, NoopChainExecutor, DefaultChainExecutor, Chain, ChainStep, ChainContext, ChainResult, StepResult, StepAction};
