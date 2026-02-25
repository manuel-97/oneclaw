//! Integration test: LLM Pipeline — ask command, memory-aware context

use async_trait::async_trait;
use oneclaw_core::config::OneClawConfig;
use oneclaw_core::runtime::Runtime;
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
        let mut inputs = self.inputs.lock().unwrap();
        match inputs.pop() {
            Some(c) => Ok(Some(IncomingMessage { source: "test".into(), content: c, timestamp: chrono::Utc::now() })),
            None => Ok(Some(IncomingMessage { source: "test".into(), content: "exit".into(), timestamp: chrono::Utc::now() })),
        }
    }
    async fn send(&self, msg: &OutgoingMessage) -> Result<()> {
        self.outputs.lock().unwrap().push(msg.content.clone()); Ok(())
    }
}

/// Test 1: `ask` command sends question to LLM pipeline (noop provider)
#[tokio::test]
async fn test_ask_command_reaches_llm_pipeline() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    let ch = TestChannel::new(vec![
        "ask what is the current sensor reading?",
        "exit",
    ]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();

    assert!(!out[0].is_empty(), "ask should produce a response");
    // Noop provider returns something — not an error, not help text
    assert!(!out[0].contains("OneClaw Commands"), "ask should not show help");
}

/// Test 2: LLM pipeline uses memory context when available
#[tokio::test]
async fn test_llm_pipeline_with_memory_context() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    // Pre-load memory with sensor data
    runtime.memory.store(
        "sensor_01 | temperature | value = 22.5",
        oneclaw_core::memory::MemoryMeta {
            tags: vec!["sensor".into(), "temperature".into()],
            priority: oneclaw_core::memory::Priority::High,
            source: "device".into(),
        },
    ).unwrap();

    let ch = TestChannel::new(vec![
        "ask sensor temperature readings",
        "exit",
    ]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();

    // Pipeline should produce a response (noop, but not empty)
    assert!(!out[0].is_empty(), "Pipeline should produce response with memory context");
}

/// Test 3: Non-command messages go through LLM pipeline
#[tokio::test]
async fn test_non_command_goes_to_llm() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    let ch = TestChannel::new(vec![
        "hello how are you today",
        "exit",
    ]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();

    // Messages should go through LLM pipeline
    assert!(!out[0].is_empty(), "LLM pipeline should produce a response");
}

/// Test 4: Graceful fallback when all providers fail
#[tokio::test]
async fn test_graceful_fallback_on_provider_failure() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    // With only noop provider, the pipeline should still produce a response
    let ch = TestChannel::new(vec![
        "ask give me a detailed analysis of all sensor data",
        "exit",
    ]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();

    // Should get a response — either noop content or offline fallback
    assert!(!out[0].is_empty(), "Should always produce a response, never panic");
    // Must not contain error stack trace or panic message
    assert!(!out[0].contains("panicked"), "Should never panic: {}", out[0]);
}
