//! Chain Executor — Multi-step LLM reasoning
//!
//! A Chain is a sequence of steps. Each step can:
//! - Call LLM with context from previous steps
//! - Search memory
//! - Transform/format data
//! - Emit events
//!
//! The executor runs steps sequentially, passing output forward.

use crate::error::Result;
use async_trait::async_trait;
use std::collections::HashMap;

/// What a chain step does
#[derive(Debug, Clone)]
pub enum StepAction {
    /// Call LLM with given prompt template
    /// Template uses {input} for previous step output, {memory} for memory context
    LlmCall {
        /// The prompt template with {input} and {memory} placeholders
        prompt_template: String,
        /// Maximum number of tokens in the response
        max_tokens: u32,
        /// Sampling temperature for the LLM call
        temperature: f32,
    },
    /// Search memory with query derived from previous step
    MemorySearch {
        /// The query template with {input} placeholder
        query_template: String,
        /// Maximum number of memory entries to return
        limit: usize,
    },
    /// Format/transform output using a template
    /// {input} = previous step output, {step_N} = output of step N
    Transform {
        /// The output format template with {input} and {step_N} placeholders
        template: String,
    },
    /// Emit an event to the bus
    EmitEvent {
        /// The event topic to publish to
        topic: String,
    },
    /// Call a registered tool with params
    /// Template values in param values use {input}/{step_N} substitution
    ToolCall {
        /// The name of the tool to invoke
        tool_name: String,
        /// The parameters to pass to the tool
        params: std::collections::HashMap<String, String>,
    },
}

/// A single step in a chain
#[derive(Debug, Clone)]
pub struct ChainStep {
    /// The name of this step
    pub name: String,
    /// The action to perform in this step
    pub action: StepAction,
}

impl ChainStep {
    /// Create an LLM call step with default max_tokens and temperature
    pub fn llm(name: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            action: StepAction::LlmCall {
                prompt_template: prompt.into(),
                max_tokens: 300,
                temperature: 0.3,
            },
        }
    }

    /// Create a memory search step with the given query template and result limit
    pub fn memory_search(name: impl Into<String>, query: impl Into<String>, limit: usize) -> Self {
        Self {
            name: name.into(),
            action: StepAction::MemorySearch {
                query_template: query.into(),
                limit,
            },
        }
    }

    /// Create a transform step that formats output using a template
    pub fn transform(name: impl Into<String>, template: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            action: StepAction::Transform {
                template: template.into(),
            },
        }
    }

    /// Create an event emission step that publishes to the given topic
    pub fn emit_event(name: impl Into<String>, topic: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            action: StepAction::EmitEvent {
                topic: topic.into(),
            },
        }
    }

    /// Create a tool call step that invokes a registered tool with parameters
    pub fn tool_call(name: impl Into<String>, tool_name: impl Into<String>, params: HashMap<String, String>) -> Self {
        Self {
            name: name.into(),
            action: StepAction::ToolCall {
                tool_name: tool_name.into(),
                params,
            },
        }
    }
}

/// A complete chain definition
#[derive(Debug, Clone)]
pub struct Chain {
    /// The name of this chain
    pub name: String,
    /// The ordered list of steps in this chain
    pub steps: Vec<ChainStep>,
}

impl Chain {
    /// Create a new empty chain with the given name
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), steps: Vec::new() }
    }

    /// Append a step to the chain and return self for builder chaining
    pub fn add_step(mut self, step: ChainStep) -> Self {
        self.steps.push(step);
        self
    }
}

/// Result of executing one step
#[derive(Debug, Clone)]
pub struct StepResult {
    /// The name of the executed step
    pub step_name: String,
    /// The output produced by this step
    pub output: String,
    /// The execution time of this step in milliseconds
    pub latency_ms: u64,
}

/// Result of executing an entire chain
#[derive(Debug, Clone)]
pub struct ChainResult {
    /// The name of the executed chain
    pub chain_name: String,
    /// The results of each step in execution order
    pub steps: Vec<StepResult>,
    /// The output of the last step in the chain
    pub final_output: String,
    /// The total execution time of the chain in milliseconds
    pub total_latency_ms: u64,
}

/// Context passed to chain execution — provides access to LLM, memory, event bus, tools
pub struct ChainContext<'a> {
    /// The provider manager for making LLM calls
    pub provider_mgr: &'a crate::orchestrator::ProviderManager,
    /// The name of the LLM provider to use
    pub provider_name: &'a str,
    /// The model identifier to use for LLM calls
    pub model: &'a str,
    /// The memory store for search steps
    pub memory: &'a dyn crate::memory::Memory,
    /// The event bus for emitting events
    pub event_bus: &'a dyn crate::event_bus::EventBus,
    /// The system prompt for LLM calls
    pub system_prompt: &'a str,
    /// The optional tool registry for tool call steps
    pub tool_registry: Option<&'a crate::tool::ToolRegistry>,
}

/// Chain Executor trait
#[async_trait]
pub trait ChainExecutor: Send + Sync {
    /// Execute a chain with the given initial input and context
    async fn execute(&self, chain: &Chain, initial_input: &str, context: &ChainContext<'_>) -> Result<ChainResult>;
}

/// Noop chain executor — returns input as output
pub struct NoopChainExecutor;

impl NoopChainExecutor {
    /// Create a new no-op chain executor
    pub fn new() -> Self { Self }
}

impl Default for NoopChainExecutor {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl ChainExecutor for NoopChainExecutor {
    async fn execute(&self, chain: &Chain, initial_input: &str, _context: &ChainContext<'_>) -> Result<ChainResult> {
        Ok(ChainResult {
            chain_name: chain.name.clone(),
            steps: vec![StepResult {
                step_name: "noop".into(),
                output: initial_input.to_string(),
                latency_ms: 0,
            }],
            final_output: initial_input.to_string(),
            total_latency_ms: 0,
        })
    }
}

/// Default chain executor — actually runs steps
pub struct DefaultChainExecutor;

impl DefaultChainExecutor {
    /// Create a new default chain executor
    pub fn new() -> Self { Self }
}

impl Default for DefaultChainExecutor {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl ChainExecutor for DefaultChainExecutor {
    async fn execute(&self, chain: &Chain, initial_input: &str, ctx: &ChainContext<'_>) -> Result<ChainResult> {
        use crate::orchestrator::provider::LlmRequest;
        use tracing::{info, warn};
        use std::time::Instant;

        let mut step_results: Vec<StepResult> = Vec::new();
        let mut current_input = initial_input.to_string();
        let mut step_outputs: HashMap<usize, String> = HashMap::new();
        let total_start = Instant::now();

        info!(chain = %chain.name, steps = chain.steps.len(), "Chain execution started");

        for (i, step) in chain.steps.iter().enumerate() {
            let step_start = Instant::now();
            info!(chain = %chain.name, step = %step.name, index = i, "Executing step");

            let output = match &step.action {
                StepAction::LlmCall { prompt_template, max_tokens, temperature } => {
                    let prompt = substitute_template(prompt_template, &current_input, &step_outputs);

                    let request = LlmRequest::with_system(
                        ctx.model,
                        ctx.system_prompt,
                        &prompt,
                    ).set_max_tokens(*max_tokens).set_temperature(*temperature);

                    match ctx.provider_mgr.chat(ctx.provider_name, &request).await {
                        Ok(resp) => resp.content,
                        Err(e) => {
                            warn!(step = %step.name, error = %e, "LLM call failed in chain");
                            match ctx.provider_mgr.chat("noop", &request).await {
                                Ok(resp) => format!("[Offline] {}", resp.content),
                                Err(_) => format!("[Error] Step '{}' failed: {}", step.name, e),
                            }
                        }
                    }
                }

                StepAction::MemorySearch { query_template, limit } => {
                    let query_text = substitute_template(query_template, &current_input, &step_outputs);
                    let query = crate::memory::MemoryQuery::new(&query_text).with_limit(*limit);
                    match ctx.memory.search(&query) {
                        Ok(entries) if entries.is_empty() => {
                            "Không tìm thấy dữ liệu liên quan.".to_string()
                        }
                        Ok(entries) => {
                            let mut result = String::new();
                            for entry in &entries {
                                result.push_str(&format!(
                                    "[{}] {}\n",
                                    entry.created_at.format("%d/%m %H:%M"),
                                    entry.content,
                                ));
                            }
                            result
                        }
                        Err(e) => format!("[Memory error] {}", e),
                    }
                }

                StepAction::Transform { template } => {
                    substitute_template(template, &current_input, &step_outputs)
                }

                StepAction::EmitEvent { topic } => {
                    let event = crate::event_bus::Event::new(topic.clone(), format!("chain:{}", chain.name))
                        .with_data("chain", &chain.name)
                        .with_data("step", &step.name)
                        .with_data("output", &current_input);
                    let _ = ctx.event_bus.publish(event);
                    current_input.clone() // Pass through
                }

                StepAction::ToolCall { tool_name, params } => {
                    if let Some(registry) = ctx.tool_registry {
                        // Substitute template variables in param values
                        let resolved_params: std::collections::HashMap<String, String> = params.iter()
                            .map(|(k, v)| (k.clone(), substitute_template(v, &current_input, &step_outputs)))
                            .collect();

                        match registry.execute(tool_name, &resolved_params, Some(ctx.event_bus)) {
                            Ok(result) => {
                                if result.success {
                                    result.output
                                } else {
                                    format!("[Tool failed] {}: {}", tool_name, result.output)
                                }
                            }
                            Err(e) => {
                                warn!(step = %step.name, tool = %tool_name, error = %e, "Tool call failed in chain");
                                format!("[Tool error] {}: {}", tool_name, e)
                            }
                        }
                    } else {
                        warn!(step = %step.name, tool = %tool_name, "No tool registry in chain context");
                        format!("[No tools] Tool registry not available for '{}'", tool_name)
                    }
                }
            };

            let latency_ms = step_start.elapsed().as_millis() as u64;

            info!(
                chain = %chain.name,
                step = %step.name,
                latency_ms = latency_ms,
                output_len = output.len(),
                "Step completed"
            );

            step_outputs.insert(i, output.clone());
            current_input = output.clone();
            step_results.push(StepResult {
                step_name: step.name.clone(),
                output,
                latency_ms,
            });
        }

        let total_latency = total_start.elapsed().as_millis() as u64;
        let final_output = current_input;

        info!(chain = %chain.name, total_latency_ms = total_latency, "Chain execution completed");

        Ok(ChainResult {
            chain_name: chain.name.clone(),
            steps: step_results,
            final_output,
            total_latency_ms: total_latency,
        })
    }
}

/// Substitute template variables:
/// {input} -> current input
/// {step_0}, {step_1}, ... -> output of step N
fn substitute_template(template: &str, input: &str, step_outputs: &HashMap<usize, String>) -> String {
    let mut result = template.replace("{input}", input);
    for (i, output) in step_outputs {
        result = result.replace(&format!("{{step_{}}}", i), output);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chain_builder() {
        let chain = Chain::new("test-chain")
            .add_step(ChainStep::memory_search("gather", "{input}", 5))
            .add_step(ChainStep::llm("analyze", "Analyze: {input}\nData: {step_0}"))
            .add_step(ChainStep::transform("format", "Report:\n{input}"));

        assert_eq!(chain.name, "test-chain");
        assert_eq!(chain.steps.len(), 3);
    }

    #[test]
    fn test_substitute_template() {
        let mut outputs = HashMap::new();
        outputs.insert(0, "memory data here".to_string());
        outputs.insert(1, "analysis result".to_string());

        let result = substitute_template(
            "Query: {input}\nMemory: {step_0}\nAnalysis: {step_1}",
            "user question",
            &outputs,
        );

        assert!(result.contains("user question"));
        assert!(result.contains("memory data here"));
        assert!(result.contains("analysis result"));
    }

    #[tokio::test]
    async fn test_noop_chain_executor() {
        let executor = NoopChainExecutor::new();
        let chain = Chain::new("test").add_step(ChainStep::llm("s1", "prompt"));

        let ctx = make_test_context();
        let result = executor.execute(&chain, "hello", &ctx).await.unwrap();

        assert_eq!(result.final_output, "hello");
        assert_eq!(result.total_latency_ms, 0);
    }

    #[tokio::test]
    async fn test_default_executor_memory_step() {
        let executor = DefaultChainExecutor::new();
        let chain = Chain::new("test")
            .add_step(ChainStep::memory_search("search", "{input}", 5));

        let ctx = make_test_context();
        let result = executor.execute(&chain, "test query", &ctx).await.unwrap();
        assert!(!result.final_output.is_empty());
        assert_eq!(result.steps.len(), 1);
    }

    #[tokio::test]
    async fn test_default_executor_transform_step() {
        let executor = DefaultChainExecutor::new();
        let chain = Chain::new("test")
            .add_step(ChainStep::transform("format", "Report: {input}"));

        let ctx = make_test_context();
        let result = executor.execute(&chain, "raw data", &ctx).await.unwrap();
        assert_eq!(result.final_output, "Report: raw data");
    }

    #[tokio::test]
    async fn test_default_executor_multi_step() {
        let executor = DefaultChainExecutor::new();
        let chain = Chain::new("multi")
            .add_step(ChainStep::transform("step1", "processed: {input}"))
            .add_step(ChainStep::transform("step2", "final: {input} | original: {step_0}"));

        let ctx = make_test_context();
        let result = executor.execute(&chain, "start", &ctx).await.unwrap();

        assert!(result.final_output.contains("processed: start"));
        assert_eq!(result.steps.len(), 2);
    }

    #[tokio::test]
    async fn test_default_executor_llm_step_with_noop_provider() {
        let executor = DefaultChainExecutor::new();
        let chain = Chain::new("llm-test")
            .add_step(ChainStep::llm("call-llm", "Analyze this: {input}"));

        let ctx = make_test_context();
        let result = executor.execute(&chain, "sensor data", &ctx).await.unwrap();

        assert!(!result.final_output.is_empty());
        assert_eq!(result.steps.len(), 1);
    }

    #[tokio::test]
    async fn test_default_executor_emit_event_step() {
        let executor = DefaultChainExecutor::new();
        let chain = Chain::new("event-test")
            .add_step(ChainStep::transform("prep", "alert: {input}"))
            .add_step(ChainStep::emit_event("notify", "chain.completed"));

        let ctx = make_test_context();
        let result = executor.execute(&chain, "high bp", &ctx).await.unwrap();

        assert!(result.final_output.contains("alert: high bp"));
    }

    #[tokio::test]
    async fn test_chain_result_latency() {
        let executor = DefaultChainExecutor::new();
        let chain = Chain::new("test")
            .add_step(ChainStep::transform("s1", "{input}"))
            .add_step(ChainStep::transform("s2", "{input}"));

        let ctx = make_test_context();
        let result = executor.execute(&chain, "data", &ctx).await.unwrap();
        assert_eq!(result.steps.len(), 2);
        assert!(result.total_latency_ms < 100);
    }

    #[tokio::test]
    async fn test_default_executor_tool_call_step() {
        use crate::tool::{ToolRegistry, NoopTool};

        let executor = DefaultChainExecutor::new();
        let mut params = HashMap::new();
        params.insert("data".to_string(), "{input}".to_string());

        let chain = Chain::new("tool-test")
            .add_step(ChainStep::tool_call("call-noop", "noop", params));

        let registry = Box::leak(Box::new(ToolRegistry::new()));
        registry.register(Box::new(NoopTool::new()));

        let mut ctx = make_test_context();
        ctx.tool_registry = Some(registry);

        let result = executor.execute(&chain, "test data", &ctx).await.unwrap();
        assert!(!result.final_output.is_empty());
        assert_eq!(result.steps.len(), 1);
    }

    #[tokio::test]
    async fn test_tool_call_no_registry() {
        let executor = DefaultChainExecutor::new();
        let chain = Chain::new("tool-test")
            .add_step(ChainStep::tool_call("call-noop", "noop", HashMap::new()));

        let ctx = make_test_context(); // tool_registry: None
        let result = executor.execute(&chain, "data", &ctx).await.unwrap();
        assert!(result.final_output.contains("[No tools]"));
    }

    #[tokio::test]
    async fn test_tool_call_nonexistent_tool() {
        use crate::tool::ToolRegistry;

        let executor = DefaultChainExecutor::new();
        let chain = Chain::new("tool-test")
            .add_step(ChainStep::tool_call("call-ghost", "ghost_tool", HashMap::new()));

        let registry = Box::leak(Box::new(ToolRegistry::new()));
        let mut ctx = make_test_context();
        ctx.tool_registry = Some(registry);

        let result = executor.execute(&chain, "data", &ctx).await.unwrap();
        assert!(result.final_output.contains("[Tool error]"));
    }

    // Helper: create test context with noop everything
    fn make_test_context() -> ChainContext<'static> {
        use crate::orchestrator::ProviderManager;
        use crate::memory::NoopMemory;
        use crate::event_bus::NoopEventBus;

        let provider_mgr = Box::leak(Box::new(ProviderManager::new("noop")));
        let memory = Box::leak(Box::new(NoopMemory::new()));
        let event_bus = Box::leak(Box::new(NoopEventBus::new()));

        ChainContext {
            provider_mgr,
            provider_name: "noop",
            model: "noop",
            memory,
            event_bus,
            system_prompt: "Test system prompt",
            tool_registry: None,
        }
    }
}
