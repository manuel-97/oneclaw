//! TIP-021: Error Recovery Tests
//!
//! Validates graceful error handling:
//! - Tool that returns Err (not panic)
//! - Tool that returns ToolResult::err (application error)
//! - Chain with failing LLM step (noop fallback)
//! - Chain with failing tool step
//! - Runtime processes errors without crashing
//! - Error metrics increment correctly

use async_trait::async_trait;
use oneclaw_core::config::OneClawConfig;
use oneclaw_core::runtime::Runtime;
use oneclaw_core::orchestrator::chain::{
    Chain, ChainStep, ChainExecutor, DefaultChainExecutor, ChainContext,
};
use oneclaw_core::provider::NoopTestProvider;
use oneclaw_core::memory::NoopMemory;
use oneclaw_core::event_bus::NoopEventBus;
use oneclaw_core::tool::{Tool, ToolInfo, ToolResult, ToolRegistry, NoopTool};
use oneclaw_core::error::OneClawError;
use oneclaw_core::channel::{Channel, IncomingMessage, OutgoingMessage};
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

// ==================== TEST CHANNEL ====================

struct TestCh {
    inputs: Mutex<Vec<String>>,
    outputs: Mutex<Vec<String>>,
}
impl TestCh {
    fn new(inputs: Vec<&str>) -> Self {
        Self {
            inputs: Mutex::new(inputs.into_iter().rev().map(String::from).collect()),
            outputs: Mutex::new(vec![]),
        }
    }
    fn outputs(&self) -> Vec<String> { self.outputs.lock().unwrap().clone() }
}
#[async_trait]
impl Channel for TestCh {
    fn name(&self) -> &str { "test" }
    async fn receive(&self) -> oneclaw_core::error::Result<Option<IncomingMessage>> {
        let mut i = self.inputs.lock().unwrap();
        match i.pop() {
            Some(c) => Ok(Some(IncomingMessage { source: "t".into(), content: c, timestamp: chrono::Utc::now() })),
            None => Ok(Some(IncomingMessage { source: "t".into(), content: "exit".into(), timestamp: chrono::Utc::now() })),
        }
    }
    async fn send(&self, msg: &OutgoingMessage) -> oneclaw_core::error::Result<()> {
        self.outputs.lock().unwrap().push(msg.content.clone()); Ok(())
    }
}

fn make_test_context() -> ChainContext<'static> {
    let provider: &'static dyn oneclaw_core::provider::Provider = Box::leak(Box::new(NoopTestProvider::available()));
    let memory = Box::leak(Box::new(NoopMemory::new()));
    let event_bus = Box::leak(Box::new(NoopEventBus::new()));
    ChainContext {
        provider: Some(provider),
        memory,
        event_bus,
        system_prompt: "Test",
        tool_registry: None,
    }
}

// ==================== FAILING TOOLS ====================

/// Tool that returns Err (hard failure)
struct ErrorTool;
impl Tool for ErrorTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "error-tool".into(),
            description: "Always returns Err".into(),
            params: vec![],
            category: "test".into(),
        }
    }
    fn execute(&self, _params: &HashMap<String, String>) -> oneclaw_core::error::Result<ToolResult> {
        Err(OneClawError::Tool("Simulated hardware failure".into()))
    }
}

/// Tool that returns ToolResult::err (soft failure)
struct SoftFailTool;
impl Tool for SoftFailTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "soft-fail".into(),
            description: "Returns ToolResult::err".into(),
            params: vec![],
            category: "test".into(),
        }
    }
    fn execute(&self, _params: &HashMap<String, String>) -> oneclaw_core::error::Result<ToolResult> {
        Ok(ToolResult::err("Sensor offline — no reading available"))
    }
}

/// Tool that produces very large output
struct BigOutputTool;
impl Tool for BigOutputTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "big-output".into(),
            description: "Returns large output".into(),
            params: vec![],
            category: "test".into(),
        }
    }
    fn execute(&self, _params: &HashMap<String, String>) -> oneclaw_core::error::Result<ToolResult> {
        // 100KB of output
        let output = "x".repeat(100_000);
        Ok(ToolResult::ok(output))
    }
}

// ==================== TOOL ERROR TESTS ====================

#[tokio::test]
async fn test_tool_hard_error() {
    let mut reg = ToolRegistry::new();
    reg.register(Box::new(ErrorTool));

    let result = reg.execute("error-tool", &HashMap::new(), None);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("Simulated hardware failure"));
}

#[tokio::test]
async fn test_tool_soft_failure() {
    let mut reg = ToolRegistry::new();
    reg.register(Box::new(SoftFailTool));

    let result = reg.execute("soft-fail", &HashMap::new(), None).unwrap();
    assert!(!result.success);
    assert!(result.output.contains("Sensor offline"));
}

#[tokio::test]
async fn test_tool_big_output() {
    let mut reg = ToolRegistry::new();
    reg.register(Box::new(BigOutputTool));

    let result = reg.execute("big-output", &HashMap::new(), None).unwrap();
    assert!(result.success);
    assert_eq!(result.output.len(), 100_000);
}

#[tokio::test]
async fn test_tool_error_emits_high_priority_event() {
    use oneclaw_core::event_bus::{DefaultEventBus, EventBus};

    let mut reg = ToolRegistry::new();
    reg.register(Box::new(SoftFailTool));

    let bus = DefaultEventBus::new();
    let result = reg.execute("soft-fail", &HashMap::new(), Some(&bus)).unwrap();
    assert!(!result.success);

    bus.drain().unwrap();
    let recent = bus.recent_events(1).unwrap();
    assert_eq!(recent[0].topic, "tool.soft-fail");
    assert_eq!(recent[0].data.get("success").unwrap(), "false");
}

// ==================== CHAIN ERROR RECOVERY TESTS ====================

#[tokio::test]
async fn test_chain_with_hard_failing_tool() {
    let executor = DefaultChainExecutor::new();
    let chain = Chain::new("error-chain")
        .add_step(ChainStep::tool_call("call-error", "error-tool", HashMap::new()));

    let mut reg = ToolRegistry::new();
    reg.register(Box::new(ErrorTool));
    let registry = Box::leak(Box::new(reg));

    let mut ctx = make_test_context();
    ctx.tool_registry = Some(registry);

    let result = executor.execute(&chain, "input", &ctx).await.unwrap();
    // Should not panic, should wrap error
    assert!(
        result.final_output.contains("[Tool error]"),
        "Should contain error message: {}",
        result.final_output
    );
}

#[tokio::test]
async fn test_chain_with_soft_failing_tool() {
    let executor = DefaultChainExecutor::new();
    let chain = Chain::new("soft-fail-chain")
        .add_step(ChainStep::tool_call("call-soft", "soft-fail", HashMap::new()));

    let mut reg = ToolRegistry::new();
    reg.register(Box::new(SoftFailTool));
    let registry = Box::leak(Box::new(reg));

    let mut ctx = make_test_context();
    ctx.tool_registry = Some(registry);

    let result = executor.execute(&chain, "input", &ctx).await.unwrap();
    assert!(
        result.final_output.contains("[Tool failed]"),
        "Should contain failure message: {}",
        result.final_output
    );
}

#[tokio::test]
async fn test_chain_error_does_not_stop_subsequent_steps() {
    let executor = DefaultChainExecutor::new();
    let chain = Chain::new("multi-step-with-error")
        .add_step(ChainStep::tool_call("call-error", "error-tool", HashMap::new()))
        .add_step(ChainStep::transform("format", "After error: {input}"));

    let mut reg = ToolRegistry::new();
    reg.register(Box::new(ErrorTool));
    let registry = Box::leak(Box::new(reg));

    let mut ctx = make_test_context();
    ctx.tool_registry = Some(registry);

    let result = executor.execute(&chain, "start", &ctx).await.unwrap();
    // Both steps should execute
    assert_eq!(result.steps.len(), 2);
    // Second step should transform the error output
    assert!(result.final_output.starts_with("After error:"));
}

#[tokio::test]
async fn test_chain_with_llm_no_provider() {
    let executor = DefaultChainExecutor::new();
    let chain = Chain::new("llm-offline")
        .add_step(ChainStep::llm("call-llm", "Analyze: {input}"));

    // No provider configured → offline mode
    let memory = Box::leak(Box::new(NoopMemory::new()));
    let event_bus = Box::leak(Box::new(NoopEventBus::new()));

    let ctx = ChainContext {
        provider: None,
        memory,
        event_bus,
        system_prompt: "Test",
        tool_registry: None,
    };

    let result = executor.execute(&chain, "sensor data", &ctx).await.unwrap();
    // Should produce [Offline] prefix
    assert!(result.final_output.contains("[Offline]"), "Should indicate offline: {}", result.final_output);
}

#[tokio::test]
async fn test_chain_memory_search_on_empty_memory() {
    let executor = DefaultChainExecutor::new();
    let chain = Chain::new("empty-memory-chain")
        .add_step(ChainStep::memory_search("search", "{input}", 10));

    let ctx = make_test_context();
    let result = executor.execute(&chain, "find something", &ctx).await.unwrap();

    // Should return Vietnamese "not found" message, not panic
    assert!(!result.final_output.is_empty());
}

// ==================== RUNTIME ERROR RECOVERY TESTS ====================

#[tokio::test]
async fn test_runtime_unknown_command_does_not_crash() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    let ch = TestCh::new(vec![
        "completely_unknown_command",
        "another garbage input $#!@",
        "SELECT * FROM users; --",
        "exit",
    ]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();
    // Should respond to all 3 inputs without panic
    assert!(out.len() >= 3, "Should handle unknown commands: got {}", out.len());
}

#[tokio::test]
async fn test_runtime_metrics_track_after_errors() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    let ch = TestCh::new(vec!["help", "unknown_stuff", "status", "exit"]);
    runtime.run(&ch).await.unwrap();

    let total = runtime.metrics.messages_total.load(Ordering::Relaxed);
    assert!(total >= 4, "Should count all messages including errors: {}", total);
}

#[tokio::test]
async fn test_runtime_consecutive_sessions() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    // Session 1
    let ch1 = TestCh::new(vec!["help", "exit"]);
    runtime.run(&ch1).await.unwrap();

    // Session 2 — same runtime
    let ch2 = TestCh::new(vec!["status", "exit"]);
    runtime.run(&ch2).await.unwrap();

    // Session 3 — same runtime
    let ch3 = TestCh::new(vec!["help", "help", "exit"]);
    runtime.run(&ch3).await.unwrap();

    // Metrics should accumulate across sessions
    let total = runtime.metrics.messages_total.load(Ordering::Relaxed);
    assert!(total >= 7, "Should accumulate metrics across sessions: {}", total);
}

#[tokio::test]
async fn test_runtime_with_security_denies_unpaired() {
    let config = OneClawConfig::default_config();
    let workspace = std::env::current_dir().unwrap();
    let runtime = Runtime::with_security(config, workspace);

    // Unpaired device tries commands → should be denied
    let ch = TestCh::new(vec!["status", "help", "exit"]);
    runtime.run(&ch).await.unwrap();

    let denied = runtime.metrics.messages_denied.load(Ordering::Relaxed);
    // "help" and "exit" are always open, "status" should be denied
    assert!(denied >= 1, "Should deny at least 1 unpaired command: {}", denied);
}

#[tokio::test]
async fn test_tool_registry_execute_after_error() {
    let mut reg = ToolRegistry::new();
    reg.register(Box::new(ErrorTool));
    reg.register(Box::new(NoopTool::new()));

    // First: error
    let r1 = reg.execute("error-tool", &HashMap::new(), None);
    assert!(r1.is_err());

    // Second: success (registry should not be in broken state)
    let r2 = reg.execute("noop", &HashMap::new(), None).unwrap();
    assert!(r2.success);
}
