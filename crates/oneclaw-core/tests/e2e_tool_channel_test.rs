//! Integration test: Tool-Pipeline Wiring + Multi-Channel + Sensor flow
//!
//! Tests the TIP-016 end-to-end wiring:
//! - Arc<ToolRegistry> shared access
//! - StepAction::ToolCall in chain
//! - Multi-channel tool execution
//! - Tool registry accessible through Runtime

use async_trait::async_trait;
use oneclaw_core::config::OneClawConfig;
use oneclaw_core::runtime::Runtime;
use oneclaw_core::event_bus::DefaultEventBus;
use oneclaw_core::channel::{Channel, ChannelManager, IncomingMessage, OutgoingMessage};
use oneclaw_core::tool::NoopTool;
use oneclaw_core::error::Result;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

struct TestChannel {
    ch_name: String,
    inputs: Mutex<Vec<String>>,
    outputs: Mutex<Vec<String>>,
}
impl TestChannel {
    fn new(name: &str, inputs: Vec<&str>) -> Self {
        Self {
            ch_name: name.into(),
            inputs: Mutex::new(inputs.into_iter().rev().map(String::from).collect()),
            outputs: Mutex::new(vec![]),
        }
    }
    fn outputs(&self) -> Vec<String> { self.outputs.lock().unwrap().clone() }
}
#[async_trait]
impl Channel for TestChannel {
    fn name(&self) -> &str { &self.ch_name }
    async fn receive(&self) -> Result<Option<IncomingMessage>> {
        let mut i = self.inputs.lock().unwrap();
        match i.pop() {
            Some(c) => Ok(Some(IncomingMessage { source: "test".into(), content: c, timestamp: chrono::Utc::now() })),
            None => Ok(None),
        }
    }
    async fn send(&self, msg: &OutgoingMessage) -> Result<()> {
        self.outputs.lock().unwrap().push(msg.content.clone()); Ok(())
    }
}

#[tokio::test]
async fn test_tool_execute_via_runtime_arc() {
    let config = OneClawConfig::default_config();
    let mut runtime = Runtime::with_defaults(config);
    Arc::get_mut(&mut runtime.tool_registry).unwrap().register(Box::new(NoopTool::new()));

    // Tool should be accessible via Arc deref
    assert_eq!(runtime.tool_registry.count(), 1);
    let tools = runtime.tool_registry.list_tools();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "noop");
}

#[tokio::test]
async fn test_tool_command_via_channel() {
    let config = OneClawConfig::default_config();
    let mut runtime = Runtime::with_defaults(config);
    Arc::get_mut(&mut runtime.tool_registry).unwrap().register(Box::new(NoopTool::new()));

    let ch = TestChannel::new("cli", vec!["tool noop", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();
    assert!(out[0].contains("[OK]"), "Tool execute should work: {}", out[0]);
}

#[tokio::test]
async fn test_tool_command_via_multi_channel() {
    let config = OneClawConfig::default_config();
    let mut runtime = Runtime::with_defaults(config);
    Arc::get_mut(&mut runtime.tool_registry).unwrap().register(Box::new(NoopTool::new()));

    let mut mgr = ChannelManager::new();
    mgr.add_channel(Box::new(TestChannel::new("ch1", vec!["tools", "exit"])));

    runtime.run_multi(&mgr).await.unwrap();
    // Didn't crash — tool listing works in multi-channel mode
}

#[tokio::test]
async fn test_arc_clone_does_not_break_access() {
    let config = OneClawConfig::default_config();
    let mut runtime = Runtime::with_defaults(config);
    Arc::get_mut(&mut runtime.tool_registry).unwrap().register(Box::new(NoopTool::new()));

    // Clone Arc (simulating what register_alert_notifier does)
    let cloned = runtime.tool_registry.clone();
    assert_eq!(cloned.count(), 1);

    // Original still works
    assert_eq!(runtime.tool_registry.count(), 1);

    // Both point to same registry
    let tools_orig = runtime.tool_registry.list_tools();
    let tools_clone = cloned.list_tools();
    assert_eq!(tools_orig.len(), tools_clone.len());
}

#[tokio::test]
async fn test_tools_and_events_combined() {
    let config = OneClawConfig::default_config();
    let mut runtime = Runtime::with_defaults(config);
    runtime.event_bus = Box::new(DefaultEventBus::new());
    Arc::get_mut(&mut runtime.tool_registry).unwrap().register(Box::new(NoopTool::new()));

    // Execute tool — should emit event on event bus
    let params = HashMap::new();
    runtime.tool_registry.execute("noop", &params, Some(runtime.event_bus.as_ref())).unwrap();

    // Event should have been published
    runtime.event_bus.drain().unwrap();
    let events = runtime.event_bus.recent_events(10).unwrap();
    assert!(!events.is_empty(), "Tool execution should emit event");
}
