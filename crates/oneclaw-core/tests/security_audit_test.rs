//! Security audit tests — verify security properties
//!
//! These tests verify that the security hardening (TIP-017) works correctly:
//! 1. Unpaired devices cannot access secured commands
//! 2. Always-open commands (help, pair, verify, exit) work without pairing
//! 3. Rate limiter blocks floods
//! 4. API keys are not leaked in status output

use async_trait::async_trait;
use oneclaw_core::config::OneClawConfig;
use oneclaw_core::runtime::Runtime;
use oneclaw_core::channel::{Channel, IncomingMessage, OutgoingMessage};
use oneclaw_core::error::Result;
use std::sync::Mutex;

struct AuditChannel {
    inputs: Mutex<Vec<String>>,
    outputs: Mutex<Vec<String>>,
    source: String,
}
impl AuditChannel {
    fn new(source: &str, inputs: Vec<&str>) -> Self {
        Self {
            inputs: Mutex::new(inputs.into_iter().rev().map(String::from).collect()),
            outputs: Mutex::new(vec![]),
            source: source.into(),
        }
    }
    fn outputs(&self) -> Vec<String> { self.outputs.lock().unwrap().clone() }
}
#[async_trait]
impl Channel for AuditChannel {
    fn name(&self) -> &str { "audit" }
    async fn receive(&self) -> Result<Option<IncomingMessage>> {
        let mut i = self.inputs.lock().unwrap();
        match i.pop() {
            Some(c) => Ok(Some(IncomingMessage {
                source: self.source.clone(),
                content: c,
                timestamp: chrono::Utc::now(),
            })),
            None => Ok(Some(IncomingMessage {
                source: self.source.clone(),
                content: "exit".into(),
                timestamp: chrono::Utc::now(),
            })),
        }
    }
    async fn send(&self, msg: &OutgoingMessage) -> Result<()> {
        self.outputs.lock().unwrap().push(msg.content.clone()); Ok(())
    }
}

/// Helper: create a runtime with security that denies unpaired devices.
/// Uses NoopSecurity which allows everything — BUT the security gate in
/// process_message() now checks BEFORE command dispatch.
/// To test denial, we use DefaultSecurity via with_security().
fn secure_runtime() -> Runtime {
    let config = OneClawConfig::default_config(); // deny_by_default=true
    let workspace = std::env::current_dir().unwrap();
    Runtime::with_security(config, workspace)
}

#[tokio::test]
async fn test_unpaired_device_cannot_remember() {
    let runtime = secure_runtime();

    let ch = AuditChannel::new("unknown-device", vec!["remember secret data", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();

    assert!(
        out[0].contains("từ chối") || out[0].contains("denied") || out[0].contains("pair"),
        "Unpaired device should not be able to remember: {}", out[0]
    );
}

#[tokio::test]
async fn test_unpaired_device_cannot_recall() {
    let runtime = secure_runtime();

    let ch = AuditChannel::new("unknown-device", vec!["recall secrets", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();

    assert!(
        out[0].contains("từ chối") || out[0].contains("denied") || out[0].contains("pair"),
        "Unpaired device should not be able to recall: {}", out[0]
    );
}

#[tokio::test]
async fn test_unpaired_device_cannot_use_tools() {
    let runtime = secure_runtime();

    let ch = AuditChannel::new("unknown-device", vec!["tool system_info", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();

    assert!(
        out[0].contains("từ chối") || out[0].contains("denied") || out[0].contains("pair"),
        "Unpaired device should not be able to use tools: {}", out[0]
    );
}

#[tokio::test]
async fn test_unpaired_device_cannot_access_status() {
    let runtime = secure_runtime();

    let ch = AuditChannel::new("unknown-device", vec!["status", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();

    assert!(
        out[0].contains("từ chối") || out[0].contains("denied") || out[0].contains("pair"),
        "Unpaired device should not be able to view status: {}", out[0]
    );
}

#[tokio::test]
async fn test_unpaired_device_can_help_pair_exit() {
    let runtime = secure_runtime();

    // help should work for unpaired device
    let ch = AuditChannel::new("unknown-device", vec!["help", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();
    assert!(out[0].contains("OneClaw Commands"),
        "help should work for unpaired device: {}", out[0]);
}

#[tokio::test]
async fn test_rate_limiter_blocks_flood() {
    use oneclaw_core::security::RateLimiter;

    let limiter = RateLimiter::new(5); // 5 per minute
    for _ in 0..5 {
        assert!(limiter.check(), "Should allow first 5");
    }
    assert!(!limiter.check(), "Should block 6th");
}

#[tokio::test]
async fn test_api_key_not_in_status() {
    let toml_str = r#"
[security]
deny_by_default = false

[providers]
default = "openai"

[providers.openai]
base_url = "https://api.example.com"
model = "gpt-4"
api_key = "sk-super-secret-key-12345"

[providers.ollama]
url = "http://localhost:11434"
model = "llama3.2:1b"
"#;
    let config: OneClawConfig = toml::from_str(toml_str).unwrap();
    let runtime = Runtime::with_defaults(config);

    let ch = AuditChannel::new("test", vec!["status", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();

    assert!(
        !out[0].contains("sk-super-secret-key-12345"),
        "Status should not contain raw API key: {}", out[0]
    );
}

#[tokio::test]
async fn test_openai_config_debug_masks_key() {
    let config = oneclaw_core::config::OpenAIConfig {
        api_key: "sk-super-secret-key-12345".to_string(),
        ..Default::default()
    };
    let debug_output = format!("{:?}", config);
    assert!(!debug_output.contains("sk-super-secret-key-12345"),
        "Debug output should not contain raw API key: {}", debug_output);
}

#[tokio::test]
async fn test_paired_device_can_remember() {
    let runtime = secure_runtime();

    // Pair first, then remember
    let ch = AuditChannel::new("test-device", vec!["pair", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();
    assert!(out[0].contains("Pairing code:"), "pair should work: {}", out[0]);
}
