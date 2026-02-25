//! Tests for graceful shutdown, LLM timeout, and enhanced status (TIP-019)
//!
//! Verifies:
//! 1. Shutdown flag stops the event loop
//! 2. Enhanced status shows comprehensive info
//! 3. Metrics track across session
//! 4. Exit returns clean
//! 5. Shutdown flag is externally settable (for signal handlers)

use async_trait::async_trait;
use oneclaw_core::config::OneClawConfig;
use oneclaw_core::runtime::Runtime;
use oneclaw_core::channel::{Channel, IncomingMessage, OutgoingMessage};
use oneclaw_core::error::Result;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

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
    async fn receive(&self) -> Result<Option<IncomingMessage>> {
        let mut i = self.inputs.lock().unwrap();
        match i.pop() {
            Some(c) => Ok(Some(IncomingMessage { source: "t".into(), content: c, timestamp: chrono::Utc::now() })),
            None => Ok(Some(IncomingMessage { source: "t".into(), content: "exit".into(), timestamp: chrono::Utc::now() })),
        }
    }
    async fn send(&self, msg: &OutgoingMessage) -> Result<()> {
        self.outputs.lock().unwrap().push(msg.content.clone()); Ok(())
    }
}

#[tokio::test]
async fn test_shutdown_flag_stops_loop() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    // Set shutdown flag before running
    runtime.shutdown.store(true, Ordering::SeqCst);

    let ch = TestCh::new(vec!["this should not be processed"]);
    runtime.run(&ch).await.unwrap();

    // Should exit immediately without processing messages
    let out = ch.outputs();
    assert!(out.is_empty(), "Should not process after shutdown: {:?}", out);
}

#[tokio::test]
async fn test_status_shows_comprehensive_info() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    let ch = TestCh::new(vec!["status", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();

    assert!(out[0].contains("Uptime"), "Status should show uptime: {}", out[0]);
    assert!(out[0].contains("Memory"), "Status should show memory: {}", out[0]);
    assert!(out[0].contains("Providers"), "Status should show providers: {}", out[0]);
    assert!(out[0].contains("Security"), "Status should show security: {}", out[0]);
    assert!(out[0].contains("Tools"), "Status should show tools: {}", out[0]);
    assert!(out[0].contains("Messages"), "Status should show messages: {}", out[0]);
    assert!(out[0].contains("LLM"), "Status should show LLM: {}", out[0]);
}

#[tokio::test]
async fn test_metrics_track_across_session() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    // Process several commands
    let ch = TestCh::new(vec!["help", "status", "help", "exit"]);
    runtime.run(&ch).await.unwrap();

    let total = runtime.metrics.messages_total.load(Ordering::Relaxed);
    assert!(total >= 4, "Should count at least 4 messages (3 cmds + exit): {}", total);
}

#[tokio::test]
async fn test_exit_returns_clean() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    let ch = TestCh::new(vec!["exit"]);
    let result = runtime.run(&ch).await;
    assert!(result.is_ok(), "Exit should return Ok");
}

#[tokio::test]
async fn test_shutdown_flag_is_arc_clonable() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    // Clone the shutdown flag (simulates what a Ctrl+C handler would do)
    let flag = runtime.shutdown.clone();
    assert!(!flag.load(Ordering::SeqCst));

    // Set from the clone
    flag.store(true, Ordering::SeqCst);
    assert!(runtime.shutdown.load(Ordering::SeqCst));

    // Run should exit immediately
    let ch = TestCh::new(vec!["should not process"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();
    assert!(out.is_empty());
}

#[tokio::test]
async fn test_status_shows_denied_count_after_denial() {
    let config = OneClawConfig::default_config();
    let workspace = std::env::current_dir().unwrap();
    let runtime = Runtime::with_security(config, workspace);

    // Unpaired device tries a secured command → denied
    let ch = TestCh::new(vec!["status", "status", "exit"]);
    runtime.run(&ch).await.unwrap();

    let denied = runtime.metrics.messages_denied.load(Ordering::Relaxed);
    assert!(denied >= 2, "Should count denied messages: {}", denied);
}

#[tokio::test]
async fn test_llm_timeout_config_default() {
    let config = OneClawConfig::default_config();
    assert_eq!(config.providers.llm_timeout_secs, 30);
}

#[tokio::test]
async fn test_llm_timeout_config_custom() {
    let toml_str = r#"
[providers]
llm_timeout_secs = 60
"#;
    let config: OneClawConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.providers.llm_timeout_secs, 60);
}
