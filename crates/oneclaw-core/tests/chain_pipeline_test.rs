//! Integration test: Chain Executor + Event Bus pipeline

use async_trait::async_trait;
use oneclaw_core::config::OneClawConfig;
use oneclaw_core::runtime::Runtime;
use oneclaw_core::event_bus::{Event, DefaultEventBus};
use oneclaw_core::channel::{Channel, IncomingMessage, OutgoingMessage};
use oneclaw_core::error::Result;
use std::sync::Mutex;

struct TestChannel {
    inputs: Mutex<Vec<String>>,
    outputs: Mutex<Vec<String>>,
}
impl TestChannel {
    fn new(inputs: Vec<&str>) -> Self {
        Self {
            inputs: Mutex::new(inputs.into_iter().rev().map(String::from).collect()),
            outputs: Mutex::new(vec![]),
        }
    }
    fn outputs(&self) -> Vec<String> { self.outputs.lock().unwrap().clone() }
}
#[async_trait]
impl Channel for TestChannel {
    fn name(&self) -> &str { "test" }
    async fn receive(&self) -> Result<Option<IncomingMessage>> {
        let mut i = self.inputs.lock().unwrap();
        match i.pop() {
            Some(c) => Ok(Some(IncomingMessage { source: "test".into(), content: c, timestamp: chrono::Utc::now() })),
            None => Ok(Some(IncomingMessage { source: "test".into(), content: "exit".into(), timestamp: chrono::Utc::now() })),
        }
    }
    async fn send(&self, msg: &OutgoingMessage) -> Result<()> {
        self.outputs.lock().unwrap().push(msg.content.clone()); Ok(())
    }
}

#[tokio::test]
async fn test_event_bus_drain_in_loop() {
    let config = OneClawConfig::default_config();
    let mut runtime = Runtime::with_defaults(config);
    // Swap in DefaultEventBus so publish/drain actually works
    runtime.event_bus = Box::new(DefaultEventBus::new());

    // Publish an event directly
    runtime.event_bus.publish(
        Event::new("test.event", "test-src")
    ).unwrap();
    assert_eq!(runtime.event_bus.pending_count(), 1);

    // After run() loop processes a message, drain should clear
    let ch = TestChannel::new(vec!["status", "exit"]);
    runtime.run(&ch).await.unwrap();

    // After run completes, events should have been drained
    assert_eq!(runtime.event_bus.pending_count(), 0);
}

#[tokio::test]
async fn test_help_includes_ask() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    let ch = TestChannel::new(vec!["help", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();

    assert!(out[0].contains("ask"), "Help should include ask: {}", out[0]);
}
