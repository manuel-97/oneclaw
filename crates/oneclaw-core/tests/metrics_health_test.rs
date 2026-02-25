//! Metrics, Health, and Reload integration tests (TIP-018)
//!
//! Verifies:
//! 1. `metrics` command returns formatted report
//! 2. `health` command probes all 5 layers
//! 3. `reload` command reports diff or missing file
//! 4. Metrics counters increment correctly through message flow
//! 5. Help text includes new commands

use async_trait::async_trait;
use oneclaw_core::config::OneClawConfig;
use oneclaw_core::runtime::Runtime;
use oneclaw_core::channel::{Channel, IncomingMessage, OutgoingMessage};
use oneclaw_core::error::Result;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

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
            Some(c) => Ok(Some(IncomingMessage {
                source: "test".into(),
                content: c,
                timestamp: chrono::Utc::now(),
            })),
            None => Ok(Some(IncomingMessage {
                source: "test".into(),
                content: "exit".into(),
                timestamp: chrono::Utc::now(),
            })),
        }
    }
    async fn send(&self, msg: &OutgoingMessage) -> Result<()> {
        self.outputs.lock().unwrap().push(msg.content.clone());
        Ok(())
    }
}

#[tokio::test]
async fn test_metrics_command_returns_report() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);
    let ch = TestChannel::new(vec!["metrics", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();
    assert!(out[0].contains("OneClaw Metrics"), "metrics command should return report: {}", out[0]);
    assert!(out[0].contains("Uptime:"), "metrics should show uptime: {}", out[0]);
    assert!(out[0].contains("Messages:"), "metrics should show messages section: {}", out[0]);
    assert!(out[0].contains("LLM:"), "metrics should show LLM section: {}", out[0]);
}

#[tokio::test]
async fn test_health_command_probes_all_layers() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);
    let ch = TestChannel::new(vec!["health", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();
    assert!(out[0].contains("Health Check"), "health should show title: {}", out[0]);
    assert!(out[0].contains("L0 Security"), "health should check L0: {}", out[0]);
    assert!(out[0].contains("L1 Orchestrator"), "health should check L1: {}", out[0]);
    assert!(out[0].contains("L2 Memory"), "health should check L2: {}", out[0]);
    assert!(out[0].contains("L3 Event Bus"), "health should check L3: {}", out[0]);
    assert!(out[0].contains("L4 Tools"), "health should check L4: {}", out[0]);
    assert!(out[0].contains("Uptime:"), "health should show uptime: {}", out[0]);
}

#[tokio::test]
async fn test_reload_command_no_config_file() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);
    let ch = TestChannel::new(vec!["reload", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();
    assert!(
        out[0].contains("No config file found") || out[0].contains("Config reload"),
        "reload should report status: {}", out[0]
    );
}

#[tokio::test]
async fn test_metrics_counters_increment_through_flow() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    let ch = TestChannel::new(vec!["status", "remember test data", "recall test", "exit"]);
    runtime.run(&ch).await.unwrap();

    // messages_total should count all messages (status + remember + recall + exit = 4)
    let total = runtime.metrics.messages_total.load(Ordering::Relaxed);
    assert!(total >= 4, "messages_total should be >= 4, got {}", total);

    // messages_secured should count secured commands
    let secured = runtime.metrics.messages_secured.load(Ordering::Relaxed);
    assert!(secured >= 3, "messages_secured should be >= 3, got {}", secured);

    // memory_stores should count remember
    let stores = runtime.metrics.memory_stores.load(Ordering::Relaxed);
    assert_eq!(stores, 1, "memory_stores should be 1");

    // memory_searches should count recall
    let searches = runtime.metrics.memory_searches.load(Ordering::Relaxed);
    assert_eq!(searches, 1, "memory_searches should be 1");
}

#[tokio::test]
async fn test_metrics_report_in_metrics_command() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    // Do some operations first
    let ch = TestChannel::new(vec!["status", "remember hello", "metrics", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();

    // The metrics output is the 3rd response (index 2)
    let metrics_output = &out[2];
    assert!(metrics_output.contains("Total: "), "should show total counter: {}", metrics_output);
}

#[tokio::test]
async fn test_help_includes_metrics_health_reload() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);
    let ch = TestChannel::new(vec!["help", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();
    assert!(out[0].contains("metrics"), "help should include metrics: {}", out[0]);
    assert!(out[0].contains("health"), "help should include health: {}", out[0]);
    assert!(out[0].contains("reload"), "help should include reload: {}", out[0]);
}

#[tokio::test]
async fn test_health_status_healthy_with_noop() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);
    let ch = TestChannel::new(vec!["health", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();
    // With NoopSecurity + NoopProviders (noop is always "online") — should be HEALTHY or DEGRADED
    assert!(
        out[0].contains("HEALTHY") || out[0].contains("DEGRADED"),
        "health status should show HEALTHY or DEGRADED: {}", out[0]
    );
}

#[tokio::test]
async fn test_metrics_llm_counters_on_ask() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    let ch = TestChannel::new(vec!["ask hello world", "exit"]);
    runtime.run(&ch).await.unwrap();

    let llm_calls = runtime.metrics.llm_calls_total.load(Ordering::Relaxed);
    assert!(llm_calls >= 1, "llm_calls_total should be >= 1, got {}", llm_calls);
}

#[tokio::test]
async fn test_metrics_tool_counters() {
    let config = OneClawConfig::default_config();
    let mut runtime = Runtime::with_defaults(config);
    std::sync::Arc::get_mut(&mut runtime.tool_registry).unwrap()
        .register(Box::new(oneclaw_core::tool::NoopTool::new()));

    let ch = TestChannel::new(vec!["tool noop", "tool ghost", "exit"]);
    runtime.run(&ch).await.unwrap();

    let tool_total = runtime.metrics.tool_calls_total.load(Ordering::Relaxed);
    assert_eq!(tool_total, 2, "tool_calls_total should be 2");

    let tool_failed = runtime.metrics.tool_calls_failed.load(Ordering::Relaxed);
    assert_eq!(tool_failed, 1, "tool_calls_failed should be 1 (ghost)");
}
