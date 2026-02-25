//! Integration test: Memory + Runtime full flow

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

#[tokio::test]
async fn test_remember_and_recall_via_runtime() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    let ch = TestChannel::new(vec![
        "remember blood pressure 140/90",
        "remember temperature 37.5",
        "recall blood pressure",
        "exit",
    ]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();

    assert!(out[0].contains("Remembered"), "Store should succeed: {}", out[0]);
    assert!(out[1].contains("Remembered"), "Store should succeed: {}", out[1]);
    assert!(out[2].contains("blood pressure") || out[2].contains("140/90"),
        "Recall should find BP entry: {}", out[2]);
}

#[tokio::test]
async fn test_sqlite_memory_remember_recall() {
    let db_path = std::env::temp_dir().join("oneclaw_test_mem_integration.db");
    let _ = std::fs::remove_file(&db_path);

    let toml_str = format!(
        r#"
[security]
deny_by_default = false

[memory]
backend = "sqlite"
db_path = "{}"
"#,
        db_path.display()
    );

    let config: OneClawConfig = toml::from_str(&toml_str).unwrap();
    let workspace = std::env::current_dir().unwrap();
    let runtime = Runtime::from_config(config, workspace).unwrap();

    let ch = TestChannel::new(vec![
        "remember Huyet ap ba Nguyen 140/90 sang nay",
        "recall Nguyen",
        "exit",
    ]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();

    assert!(out[0].contains("Remembered"), "SQLite store: {}", out[0]);
    assert!(out[1].contains("Nguyen") || out[1].contains("140/90"),
        "SQLite recall: {}", out[1]);

    let _ = std::fs::remove_file(&db_path);
}

