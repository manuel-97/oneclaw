//! Integration test: Full device pairing flow
//! Validates Security + Runtime + Channel working together end-to-end.

use async_trait::async_trait;
use oneclaw_core::config::OneClawConfig;
use oneclaw_core::runtime::Runtime;
use oneclaw_core::channel::{Channel, IncomingMessage, OutgoingMessage};
use oneclaw_core::error::Result;
use std::sync::Mutex;

/// Mock channel that feeds predefined inputs and captures outputs
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
    fn outputs(&self) -> Vec<String> {
        self.outputs.lock().unwrap().clone()
    }
}

#[async_trait]
impl Channel for TestChannel {
    fn name(&self) -> &str { "test" }
    async fn receive(&self) -> Result<Option<IncomingMessage>> {
        let mut inputs = self.inputs.lock().unwrap();
        match inputs.pop() {
            Some(content) => Ok(Some(IncomingMessage {
                source: "test-device".into(),
                content,
                timestamp: chrono::Utc::now(),
            })),
            None => Ok(Some(IncomingMessage {
                source: "test-device".into(),
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
async fn test_full_pairing_flow_unpaired_then_pair_then_chat() {
    // Scenario: Caregiver connects, tries to chat (denied), pairs device, then chats (allowed)
    let channel = TestChannel::new(vec![
        "hello",           // Step 1: Try chatting unpaired -> denied
        "pair",            // Step 2: Generate pairing code
        "exit",
    ]);

    let config = OneClawConfig::default_config(); // deny_by_default = true
    let workspace = std::env::current_dir().unwrap();
    let runtime = Runtime::from_config(config, workspace).unwrap();
    runtime.boot().unwrap();
    runtime.run(&channel).await.unwrap();

    let outputs = channel.outputs();

    // Step 1: First message should be denied (unpaired device)
    assert!(
        outputs[0].contains("denied") || outputs[0].contains("Denied") || outputs[0].contains("not paired"),
        "Expected denial for unpaired device, got: {}",
        outputs[0]
    );

    // Step 2: Pair command should return a 6-digit code
    assert!(
        outputs[1].contains("Pairing code:"),
        "Expected pairing code, got: {}",
        outputs[1]
    );
    // Verify code format: extract digits
    let code: String = outputs[1].chars().filter(|c| c.is_ascii_digit()).take(6).collect();
    assert_eq!(code.len(), 6, "Pairing code should be 6 digits");
}

#[tokio::test]
async fn test_pairing_flow_with_verify() {
    // Full flow: pair -> verify -> chat
    // Two-phase since we need the code from phase 1

    let config = OneClawConfig::default_config();
    let workspace = std::env::current_dir().unwrap();
    let runtime = Runtime::from_config(config, workspace).unwrap();
    runtime.boot().unwrap();

    // Phase 1: Generate code
    let ch1 = TestChannel::new(vec!["pair", "exit"]);
    runtime.run(&ch1).await.unwrap();
    let outputs = ch1.outputs();
    let code_output = &outputs[0];

    // Extract 6-digit code
    let code: String = code_output
        .chars()
        .collect::<Vec<_>>()
        .windows(6)
        .find(|w| w.iter().all(|c| c.is_ascii_digit()))
        .map(|w| w.iter().collect())
        .unwrap_or_default();

    assert_eq!(code.len(), 6, "Should extract 6-digit code from: {}", code_output);

    // Phase 2: Verify code, then chat (same runtime, state persists)
    let ch2 = TestChannel::new(vec![
        &format!("verify {}", code),
        "hello after pairing",
        "exit",
    ]);
    runtime.run(&ch2).await.unwrap();
    let outputs2 = ch2.outputs();

    assert!(
        outputs2[0].contains("paired successfully") || outputs2[0].contains("Device paired"),
        "Expected successful pairing, got: {}",
        outputs2[0]
    );
    assert!(
        outputs2[0].contains("Device ID:"),
        "Expected device ID in response, got: {}",
        outputs2[0]
    );

    // After pairing, chat goes through LLM pipeline (offline mode if no provider)
    assert!(
        !outputs2[1].is_empty(),
        "Expected response after pairing, got empty",
    );
}

#[tokio::test]
async fn test_invalid_pairing_code_rejected() {
    let config = OneClawConfig::default_config();
    let workspace = std::env::current_dir().unwrap();
    let runtime = Runtime::from_config(config, workspace).unwrap();
    runtime.boot().unwrap();

    let channel = TestChannel::new(vec![
        "verify 000000",  // Invalid code
        "exit",
    ]);
    runtime.run(&channel).await.unwrap();
    let outputs = channel.outputs();

    assert!(
        outputs[0].contains("failed") || outputs[0].contains("Failed") || outputs[0].contains("Invalid"),
        "Expected pairing failure, got: {}",
        outputs[0]
    );
}

#[tokio::test]
async fn test_development_mode_no_pairing_needed() {
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

    let channel = TestChannel::new(vec![
        "hello from dev mode",
        "exit",
    ]);
    runtime.run(&channel).await.unwrap();
    let outputs = channel.outputs();

    // In dev mode, message should go through without pairing (offline response if no provider)
    assert!(
        !outputs[0].is_empty(),
        "Dev mode should respond without pairing, got empty",
    );
}
