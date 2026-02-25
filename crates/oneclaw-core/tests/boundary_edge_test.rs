//! TIP-021: Boundary Condition Tests
//!
//! Validates behavior at edges:
//! - Zero-length chains, single-step chains, many-step chains
//! - Empty tool registry, tool with missing params
//! - Memory with 0 entries, limit=0, limit=MAX
//! - Event bus saturation, rapid publish/drain cycles
//! - Rate limiter boundary (at limit, over limit)

use async_trait::async_trait;
use oneclaw_core::config::OneClawConfig;
use oneclaw_core::runtime::Runtime;
use oneclaw_core::memory::{MemoryMeta, MemoryQuery, Priority};
use oneclaw_core::event_bus::{Event, EventBus, DefaultEventBus, EventPriority};
use oneclaw_core::orchestrator::chain::{
    Chain, ChainStep, ChainExecutor, DefaultChainExecutor, NoopChainExecutor, ChainContext,
};
use oneclaw_core::orchestrator::ProviderManager;
use oneclaw_core::memory::NoopMemory;
use oneclaw_core::event_bus::NoopEventBus;
use oneclaw_core::tool::{ToolRegistry, NoopTool};
use oneclaw_core::channel::{Channel, IncomingMessage, OutgoingMessage};
use std::collections::HashMap;
use std::sync::Mutex;

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
    let provider_mgr = Box::leak(Box::new(ProviderManager::new("noop")));
    let memory = Box::leak(Box::new(NoopMemory::new()));
    let event_bus = Box::leak(Box::new(NoopEventBus::new()));
    ChainContext {
        provider_mgr,
        provider_name: "noop",
        model: "noop",
        memory,
        event_bus,
        system_prompt: "Test",
        tool_registry: None,
    }
}

// ==================== CHAIN BOUNDARY TESTS ====================

#[tokio::test]
async fn test_zero_step_chain() {
    let executor = DefaultChainExecutor::new();
    let chain = Chain::new("empty-chain");
    let ctx = make_test_context();

    let result = executor.execute(&chain, "input data", &ctx).await.unwrap();
    // Zero steps: final_output should be the initial input
    assert_eq!(result.final_output, "input data");
    assert_eq!(result.steps.len(), 0);
}

#[tokio::test]
async fn test_single_step_chain() {
    let executor = DefaultChainExecutor::new();
    let chain = Chain::new("single")
        .add_step(ChainStep::transform("only-step", "Result: {input}"));
    let ctx = make_test_context();

    let result = executor.execute(&chain, "hello", &ctx).await.unwrap();
    assert_eq!(result.final_output, "Result: hello");
    assert_eq!(result.steps.len(), 1);
}

#[tokio::test]
async fn test_many_step_chain() {
    let executor = DefaultChainExecutor::new();
    let mut chain = Chain::new("long-chain");
    for i in 0..50 {
        chain = chain.add_step(ChainStep::transform(
            format!("step-{}", i),
            "{input}+".to_string(),
        ));
    }
    let ctx = make_test_context();

    let result = executor.execute(&chain, "start", &ctx).await.unwrap();
    assert_eq!(result.steps.len(), 50);
    // Each step appends "+"
    assert!(result.final_output.starts_with("start"));
    assert_eq!(result.final_output.matches('+').count(), 50);
}

#[tokio::test]
async fn test_noop_executor_with_zero_steps() {
    let executor = NoopChainExecutor::new();
    let chain = Chain::new("empty");
    let ctx = make_test_context();

    let result = executor.execute(&chain, "data", &ctx).await.unwrap();
    assert_eq!(result.final_output, "data");
}

// ==================== TOOL BOUNDARY TESTS ====================

#[tokio::test]
async fn test_empty_tool_registry() {
    let reg = ToolRegistry::new();
    assert_eq!(reg.count(), 0);
    assert!(reg.list_tools().is_empty());

    // Execute on empty registry
    let result = reg.execute("anything", &HashMap::new(), None);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_tool_with_missing_required_params() {
    use oneclaw_core::tool::{Tool, ToolInfo, ToolParam, ToolResult};

    struct RequiredParamTool;
    impl Tool for RequiredParamTool {
        fn info(&self) -> ToolInfo {
            ToolInfo {
                name: "required-tool".into(),
                description: "needs params".into(),
                params: vec![
                    ToolParam { name: "url".into(), description: "URL".into(), required: true },
                    ToolParam { name: "method".into(), description: "HTTP method".into(), required: true },
                    ToolParam { name: "timeout".into(), description: "Timeout".into(), required: false },
                ],
                category: "test".into(),
            }
        }
        fn execute(&self, params: &HashMap<String, String>) -> oneclaw_core::error::Result<ToolResult> {
            Ok(ToolResult::ok(format!("ok: {}", params.len())))
        }
    }

    let mut reg = ToolRegistry::new();
    reg.register(Box::new(RequiredParamTool));

    // No params → fail
    assert!(reg.execute("required-tool", &HashMap::new(), None).is_err());

    // Only one required → fail
    let mut p = HashMap::new();
    p.insert("url".into(), "http://example.com".into());
    assert!(reg.execute("required-tool", &p, None).is_err());

    // Both required → pass
    p.insert("method".into(), "GET".into());
    let result = reg.execute("required-tool", &p, None).unwrap();
    assert!(result.success);

    // Both required + optional → pass
    p.insert("timeout".into(), "30".into());
    let result = reg.execute("required-tool", &p, None).unwrap();
    assert!(result.success);
}

#[tokio::test]
async fn test_tool_overwrite_on_re_register() {
    let mut reg = ToolRegistry::new();
    reg.register(Box::new(NoopTool::new()));
    assert_eq!(reg.count(), 1);

    // Re-register with same name
    reg.register(Box::new(NoopTool::new()));
    assert_eq!(reg.count(), 1); // Should overwrite, not duplicate
}

// ==================== MEMORY BOUNDARY TESTS ====================

#[tokio::test]
async fn test_memory_search_on_empty() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    // Search with no entries
    let results = runtime.memory.search(&MemoryQuery::new("anything")).unwrap();
    assert!(results.is_empty());

    assert_eq!(runtime.memory.count().unwrap(), 0);
}

#[tokio::test]
async fn test_memory_search_limit_zero() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    runtime.memory.store("entry 1", MemoryMeta::default()).unwrap();
    runtime.memory.store("entry 2", MemoryMeta::default()).unwrap();

    let results = runtime.memory.search(&MemoryQuery::new("entry").with_limit(0)).unwrap();
    assert!(results.is_empty(), "Limit=0 should return 0 results");
}

#[tokio::test]
async fn test_memory_search_limit_very_large() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    for i in 0..10 {
        runtime.memory.store(&format!("entry {}", i), MemoryMeta::default()).unwrap();
    }

    // Limit far exceeds count
    let results = runtime.memory.search(&MemoryQuery::new("entry").with_limit(999_999)).unwrap();
    assert_eq!(results.len(), 10, "Should return all entries, not panic");
}

#[tokio::test]
async fn test_memory_delete_nonexistent() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    let deleted = runtime.memory.delete("nonexistent-uuid-1234").unwrap();
    assert!(!deleted, "Delete of nonexistent should return false");
}

#[tokio::test]
async fn test_memory_get_nonexistent() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    let entry = runtime.memory.get("nonexistent-uuid-1234").unwrap();
    assert!(entry.is_none());
}

#[tokio::test]
async fn test_memory_all_priority_levels() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    let priorities = vec![
        (Priority::Low, "low entry"),
        (Priority::Medium, "medium entry"),
        (Priority::High, "high entry"),
        (Priority::Critical, "critical entry"),
    ];

    for (priority, content) in &priorities {
        runtime.memory.store(content, MemoryMeta {
            priority: *priority,
            ..Default::default()
        }).unwrap();
    }

    assert_eq!(runtime.memory.count().unwrap(), 4);
}

// ==================== EVENT BUS BOUNDARY TESTS ====================

#[tokio::test]
async fn test_event_bus_drain_empty() {
    let bus = DefaultEventBus::new();
    let drained = bus.drain().unwrap();
    assert_eq!(drained, 0);
    assert_eq!(bus.pending_count(), 0);
}

#[tokio::test]
async fn test_event_bus_saturation() {
    let bus = DefaultEventBus::new();

    // Publish 10,000 events
    for i in 0..10_000 {
        bus.publish(Event::new(format!("sat.{}", i % 100), "stress")).unwrap();
    }

    assert_eq!(bus.pending_count(), 10_000);
    let drained = bus.drain().unwrap();
    assert_eq!(drained, 10_000);
    assert_eq!(bus.pending_count(), 0);
}

#[tokio::test]
async fn test_event_bus_rapid_publish_drain_cycles() {
    let bus = DefaultEventBus::new();

    for cycle in 0..100 {
        bus.publish(Event::new(format!("rapid.{}", cycle), "test")).unwrap();
        let drained = bus.drain().unwrap();
        assert_eq!(drained, 1, "Cycle {}: should drain exactly 1", cycle);
    }

    let recent = bus.recent_events(100).unwrap();
    assert_eq!(recent.len(), 100);
}

#[tokio::test]
async fn test_event_bus_history_overflow() {
    let bus = DefaultEventBus::new().with_max_history(5);

    for i in 0..20 {
        bus.publish(Event::new(format!("overflow.{}", i), "test")).unwrap();
    }
    bus.drain().unwrap();

    let recent = bus.recent_events(100).unwrap();
    assert_eq!(recent.len(), 5, "History should cap at max_history=5");
    // Should have the LAST 5 events
    assert_eq!(recent[0].topic, "overflow.15");
    assert_eq!(recent[4].topic, "overflow.19");
}

#[tokio::test]
async fn test_event_bus_all_priority_levels() {
    let bus = DefaultEventBus::new();

    let priorities = vec![
        EventPriority::Low,
        EventPriority::Normal,
        EventPriority::High,
        EventPriority::Critical,
    ];

    for priority in priorities {
        bus.publish(
            Event::new("priority.test", "test").with_priority(priority)
        ).unwrap();
    }

    assert_eq!(bus.pending_count(), 4);
    let drained = bus.drain().unwrap();
    assert_eq!(drained, 4);
}

#[tokio::test]
async fn test_event_bus_unsubscribe_invalid() {
    let bus = DefaultEventBus::new();
    let removed = bus.unsubscribe("nonexistent-sub-id-12345").unwrap();
    assert!(!removed);
}

#[tokio::test]
async fn test_event_bus_recent_events_more_than_history() {
    let bus = DefaultEventBus::new();

    bus.publish(Event::new("only.one", "test")).unwrap();
    bus.drain().unwrap();

    // Request more than exist
    let recent = bus.recent_events(1000).unwrap();
    assert_eq!(recent.len(), 1);
}

// ==================== RUNTIME BOUNDARY TESTS ====================

#[tokio::test]
async fn test_runtime_immediate_exit() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    // Immediately exit
    let ch = TestCh::new(vec!["exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();
    // Should get goodbye message
    assert!(!out.is_empty());
}

#[tokio::test]
async fn test_runtime_many_commands_sequentially() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    // 100 help commands
    let mut inputs: Vec<&str> = vec!["help"; 100];
    inputs.push("exit");
    let ch = TestCh::new(inputs);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();
    // Should have 100 help responses + exit
    assert!(out.len() >= 100, "Should handle 100 sequential commands: got {}", out.len());
}

#[tokio::test]
async fn test_chain_tool_call_to_nonexistent_tool() {
    let executor = DefaultChainExecutor::new();
    let chain = Chain::new("ghost-tool-chain")
        .add_step(ChainStep::tool_call("call-ghost", "nonexistent_tool", HashMap::new()));

    let registry = Box::leak(Box::new(ToolRegistry::new()));
    let mut ctx = make_test_context();
    ctx.tool_registry = Some(registry);

    let result = executor.execute(&chain, "input", &ctx).await.unwrap();
    assert!(result.final_output.contains("[Tool error]"), "Should wrap error: {}", result.final_output);
}

#[tokio::test]
async fn test_chain_tool_call_without_registry() {
    let executor = DefaultChainExecutor::new();
    let chain = Chain::new("no-registry-chain")
        .add_step(ChainStep::tool_call("call-tool", "noop", HashMap::new()));

    let ctx = make_test_context(); // tool_registry: None
    let result = executor.execute(&chain, "input", &ctx).await.unwrap();
    assert!(result.final_output.contains("[No tools]"), "Should report no registry: {}", result.final_output);
}
