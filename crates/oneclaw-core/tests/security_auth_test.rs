//! TIP-039: Security Authorization Tests
//!
//! Validates per-command authorization:
//! - Unpaired device denied for remember, recall, ask, tool, status
//! - Paired device allowed for remember, recall
//! - Development mode allows all commands
//! - All commands have authorization checks (audit completeness)

use async_trait::async_trait;
use oneclaw_core::config::OneClawConfig;
use oneclaw_core::runtime::Runtime;
use oneclaw_core::channel::{Channel, IncomingMessage, OutgoingMessage};
use oneclaw_core::error::Result;
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
    async fn receive(&self) -> Result<Option<IncomingMessage>> {
        let mut i = self.inputs.lock().unwrap();
        match i.pop() {
            Some(c) => Ok(Some(IncomingMessage {
                source: "unpaired-device".into(),
                content: c,
                timestamp: chrono::Utc::now(),
            })),
            None => Ok(Some(IncomingMessage {
                source: "unpaired-device".into(),
                content: "exit".into(),
                timestamp: chrono::Utc::now(),
            })),
        }
    }
    async fn send(&self, msg: &OutgoingMessage) -> Result<()> {
        self.outputs.lock().unwrap().push(msg.content.clone()); Ok(())
    }
}

/// Create production-mode runtime (deny_by_default=true)
fn production_runtime() -> Runtime {
    let config = OneClawConfig::default_config();
    let workspace = std::env::current_dir().unwrap();
    let runtime = Runtime::from_config(config, workspace).unwrap();
    runtime.boot().unwrap();
    runtime
}

/// Create development-mode runtime (deny_by_default=false)
fn development_runtime() -> Runtime {
    let toml_str = r#"
[security]
deny_by_default = false

[runtime]
name = "dev-agent"
"#;
    let config: OneClawConfig = toml::from_str(toml_str).unwrap();
    let workspace = std::env::current_dir().unwrap();
    let runtime = Runtime::from_config(config, workspace).unwrap();
    runtime.boot().unwrap();
    runtime
}

// ==================== UNPAIRED DEVICE DENIAL TESTS ====================

#[tokio::test]
async fn test_remember_denied_for_unpaired_device() {
    let runtime = production_runtime();
    let ch = TestCh::new(vec!["remember secret data", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();
    assert!(
        out[0].contains("Access denied"),
        "remember should be denied for unpaired device, got: {}", out[0]
    );
}

#[tokio::test]
async fn test_recall_denied_for_unpaired_device() {
    let runtime = production_runtime();
    let ch = TestCh::new(vec!["recall secret", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();
    assert!(
        out[0].contains("Access denied"),
        "recall should be denied for unpaired device, got: {}", out[0]
    );
}

#[tokio::test]
async fn test_ask_denied_for_unpaired_device() {
    let runtime = production_runtime();
    let ch = TestCh::new(vec!["ask hello world", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();
    assert!(
        out[0].contains("Access denied"),
        "ask should be denied for unpaired device, got: {}", out[0]
    );
}

#[tokio::test]
async fn test_tool_denied_for_unpaired_device() {
    let runtime = production_runtime();
    let ch = TestCh::new(vec!["tool system_info", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();
    assert!(
        out[0].contains("Access denied"),
        "tool should be denied for unpaired device, got: {}", out[0]
    );
}

#[tokio::test]
async fn test_status_denied_for_unpaired_device() {
    let runtime = production_runtime();
    let ch = TestCh::new(vec!["status", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();
    assert!(
        out[0].contains("Access denied"),
        "status should be denied for unpaired device, got: {}", out[0]
    );
}

#[tokio::test]
async fn test_free_text_denied_for_unpaired_device() {
    let runtime = production_runtime();
    let ch = TestCh::new(vec!["what is my blood pressure?", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();
    assert!(
        out[0].contains("Access denied"),
        "free text LLM should be denied for unpaired device, got: {}", out[0]
    );
}

#[tokio::test]
async fn test_denied_counter_increments() {
    let runtime = production_runtime();
    let ch = TestCh::new(vec!["remember secret", "recall data", "ask hello", "exit"]);
    runtime.run(&ch).await.unwrap();

    let denied = runtime.metrics.messages_denied.load(Ordering::Relaxed);
    assert!(denied >= 3, "Should deny at least 3 commands, got {}", denied);
}

// ==================== ALWAYS-OPEN COMMANDS ====================

#[tokio::test]
async fn test_help_always_open_even_unpaired() {
    let runtime = production_runtime();
    let ch = TestCh::new(vec!["help", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();
    assert!(
        out[0].contains("OneClaw Commands"),
        "help should always work, got: {}", out[0]
    );
}

#[tokio::test]
async fn test_pair_always_open_even_unpaired() {
    let runtime = production_runtime();
    let ch = TestCh::new(vec!["pair", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();
    assert!(
        out[0].contains("Pairing code:"),
        "pair should always work, got: {}", out[0]
    );
}

// ==================== PAIRED DEVICE ALLOWED TESTS ====================

#[tokio::test]
async fn test_remember_allowed_for_paired_device() {
    let runtime = production_runtime();

    // Phase 1: pair
    let ch1 = TestCh::new(vec!["pair", "exit"]);
    runtime.run(&ch1).await.unwrap();
    let out1 = ch1.outputs();
    let code: String = out1[0].chars()
        .collect::<Vec<_>>()
        .windows(6)
        .find(|w| w.iter().all(|c| c.is_ascii_digit()))
        .map(|w| w.iter().collect())
        .unwrap_or_default();

    // Phase 2: verify + remember
    let ch2 = TestCh::new(vec![
        &format!("verify {}", code),
        "remember important note for testing",
        "exit",
    ]);
    runtime.run(&ch2).await.unwrap();
    let out2 = ch2.outputs();

    assert!(out2[0].contains("paired successfully"), "Should pair: {}", out2[0]);
    assert!(out2[1].contains("Remembered"), "Paired device should be able to remember: {}", out2[1]);
}

#[tokio::test]
async fn test_recall_allowed_for_paired_device() {
    let runtime = production_runtime();

    // Phase 1: pair
    let ch1 = TestCh::new(vec!["pair", "exit"]);
    runtime.run(&ch1).await.unwrap();
    let out1 = ch1.outputs();
    let code: String = out1[0].chars()
        .collect::<Vec<_>>()
        .windows(6)
        .find(|w| w.iter().all(|c| c.is_ascii_digit()))
        .map(|w| w.iter().collect())
        .unwrap_or_default();

    // Phase 2: verify + remember + recall
    let ch2 = TestCh::new(vec![
        &format!("verify {}", code),
        "remember patient blood pressure 140/90",
        "recall blood pressure",
        "exit",
    ]);
    runtime.run(&ch2).await.unwrap();
    let out2 = ch2.outputs();

    assert!(out2[0].contains("paired successfully"), "Should pair: {}", out2[0]);
    assert!(out2[1].contains("Remembered"), "Should store: {}", out2[1]);
    assert!(
        out2[2].contains("blood pressure") || out2[2].contains("140/90"),
        "Paired device should be able to recall: {}", out2[2]
    );
}

// ==================== DEVELOPMENT MODE TESTS ====================

#[tokio::test]
async fn test_development_mode_allows_remember() {
    let runtime = development_runtime();
    let ch = TestCh::new(vec!["remember test data", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();
    assert!(
        out[0].contains("Remembered"),
        "Dev mode should allow remember without pairing: {}", out[0]
    );
}

#[tokio::test]
async fn test_development_mode_allows_recall() {
    let runtime = development_runtime();
    let ch = TestCh::new(vec!["remember some data", "recall data", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();
    assert!(out[0].contains("Remembered"), "Dev mode remember: {}", out[0]);
    // recall may find "No memories found" (NoopMemory) or the data if SQLite
    assert!(!out[1].contains("Access denied"), "Dev mode should not deny recall: {}", out[1]);
}

#[tokio::test]
async fn test_development_mode_allows_status() {
    let runtime = development_runtime();
    let ch = TestCh::new(vec!["status", "exit"]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();
    assert!(
        out[0].contains("OneClaw Agent"),
        "Dev mode should allow status: {}", out[0]
    );
}

#[tokio::test]
async fn test_development_mode_allows_all_commands() {
    let runtime = development_runtime();
    let ch = TestCh::new(vec![
        "metrics",
        "health",
        "providers",
        "events",
        "channels",
        "devices",
        "tools",
        "exit",
    ]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();

    for (i, output) in out.iter().enumerate() {
        if i < 7 { // skip exit goodbye
            assert!(
                !output.contains("Access denied"),
                "Dev mode command {} should not be denied: {}", i, output
            );
        }
    }
}

// ==================== AUDIT COMPLETENESS ====================

#[tokio::test]
async fn test_all_secured_commands_denied_when_unpaired() {
    // Meta-test: every command that should require auth IS denied for unpaired device
    let commands = vec![
        "status",
        "metrics",
        "health",
        "reload",
        "providers",
        "events",
        "channels",
        "devices",
        "unpair test",
        "tools",
        "tool noop",
        "remember secret",
        "recall data",
        "ask hello",
        "hello world",  // free text → LLM
    ];

    for cmd in &commands {
        let runtime = production_runtime();
        let ch = TestCh::new(vec![cmd, "exit"]);
        runtime.run(&ch).await.unwrap();
        let out = ch.outputs();
        assert!(
            out[0].contains("Access denied"),
            "Command '{}' should be denied for unpaired device, got: {}",
            cmd, out[0]
        );
    }
}
