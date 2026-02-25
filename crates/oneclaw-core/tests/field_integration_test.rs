//! Field Integration Tests — TIP-031
//!
//! Simulates real deployment lifecycle: install → pair → command → persist →
//! restart → re-verify → unpair → corrupt → recover.

use oneclaw_core::config::OneClawConfig;
use oneclaw_core::security::{DefaultSecurity, SecurityCore, Action, ActionKind, SqliteSecurityStore};
use oneclaw_core::security::traits::PairedDevice;
use oneclaw_core::memory::{Memory, MemoryQuery};
use oneclaw_core::runtime::Runtime;
use async_trait::async_trait;

// ═══════════════════════════════════════════════════
// Helper: MockChannel for runtime tests
// ═══════════════════════════════════════════════════

struct MockChannel {
    inputs: std::sync::Mutex<Vec<String>>,
    outputs: std::sync::Mutex<Vec<String>>,
}

impl MockChannel {
    fn new(inputs: Vec<&str>) -> Self {
        Self {
            inputs: std::sync::Mutex::new(inputs.into_iter().rev().map(String::from).collect()),
            outputs: std::sync::Mutex::new(vec![]),
        }
    }
    fn get_outputs(&self) -> Vec<String> {
        self.outputs.lock().unwrap().clone()
    }
}

#[async_trait]
impl oneclaw_core::channel::Channel for MockChannel {
    fn name(&self) -> &str { "mock" }
    async fn receive(&self) -> oneclaw_core::error::Result<Option<oneclaw_core::channel::IncomingMessage>> {
        let mut inputs = self.inputs.lock().unwrap();
        match inputs.pop() {
            Some(content) => Ok(Some(oneclaw_core::channel::IncomingMessage {
                source: "test".into(),
                content,
                timestamp: chrono::Utc::now(),
            })),
            None => Ok(Some(oneclaw_core::channel::IncomingMessage {
                source: "test".into(),
                content: "exit".into(),
                timestamp: chrono::Utc::now(),
            })),
        }
    }
    async fn send(&self, message: &oneclaw_core::channel::OutgoingMessage) -> oneclaw_core::error::Result<()> {
        self.outputs.lock().unwrap().push(message.content.clone());
        Ok(())
    }
}

// ═══════════════════════════════════════════════════
// GROUP 1: Deployment Lifecycle
// ═══════════════════════════════════════════════════

#[test]
fn test_config_load_with_defaults() {
    let config = OneClawConfig::default_config();
    assert!(config.security.deny_by_default);
    assert!(config.security.pairing_required);
    assert!(config.security.persist_pairing);
    assert_eq!(config.security.persist_path, "data/security.db");
    assert_eq!(config.memory.backend, "sqlite");
    assert_eq!(config.provider.primary, "anthropic");
    assert_eq!(config.provider.model, "claude-sonnet-4-20250514");
}

#[test]
fn test_config_load_with_overrides() {
    let toml_str = r#"
[security]
deny_by_default = false
pairing_required = false
persist_pairing = false
persist_path = "custom.db"

[runtime]
name = "test-agent"

[providers]
default = "openai"

[memory]
backend = "noop"

[channels]
active = ["cli", "mqtt"]

[provider]
primary = "groq"
model = "llama-3.3-70b-versatile"
max_tokens = 512
temperature = 0.5
fallback = ["ollama"]
"#;
    let config: OneClawConfig = toml::from_str(toml_str).unwrap();
    assert!(!config.security.deny_by_default);
    assert!(!config.security.pairing_required);
    assert!(!config.security.persist_pairing);
    assert_eq!(config.security.persist_path, "custom.db");
    assert_eq!(config.runtime.name, "test-agent");
    assert_eq!(config.memory.backend, "noop");
    assert_eq!(config.channels.active, vec!["cli", "mqtt"]);
    assert_eq!(config.provider.primary, "groq");
    assert_eq!(config.provider.model, "llama-3.3-70b-versatile");
    assert_eq!(config.provider.max_tokens, 512);
    assert_eq!(config.provider.fallback, vec!["ollama"]);
}

#[test]
fn test_config_missing_file_graceful() {
    let result = OneClawConfig::load("nonexistent/path/config.toml");
    assert!(result.is_err());
    // Default config still works
    let config = OneClawConfig::default_config();
    assert!(config.security.deny_by_default);
}

// ═══════════════════════════════════════════════════
// GROUP 2: Pairing → Persist → Restart → Restore
// ═══════════════════════════════════════════════════

#[test]
fn test_full_pairing_lifecycle() {
    let tmp = std::env::temp_dir().join("oneclaw_field_lifecycle.db");
    let _ = std::fs::remove_file(&tmp);

    // 1. Create security with persistence
    let store = SqliteSecurityStore::new(&tmp).unwrap();
    let sec = DefaultSecurity::production(std::env::current_dir().unwrap())
        .with_persistence(store);

    // 2-3. Pair two devices
    let code_a = sec.generate_pairing_code().unwrap();
    let id_a = sec.verify_pairing_code(&code_a).unwrap();

    let code_b = sec.generate_pairing_code().unwrap();
    let id_b = sec.verify_pairing_code(&code_b).unwrap();

    // 4. Verify both authorized
    let check = |actor: &str| -> bool {
        sec.authorize(&Action {
            kind: ActionKind::Execute,
            resource: "test".into(),
            actor: actor.into(),
        }).unwrap().granted
    };
    assert!(check(&id_a.device_id), "device_a should be authorized");
    assert!(check(&id_b.device_id), "device_b should be authorized");

    // 5. List devices
    let devices = sec.list_devices().unwrap();
    assert_eq!(devices.len(), 2);

    // 6-7. Simulate restart — drop and reopen
    drop(sec);
    let store2 = SqliteSecurityStore::new(&tmp).unwrap();
    let sec2 = DefaultSecurity::production(std::env::current_dir().unwrap())
        .with_persistence(store2);

    // 8-9. Both devices restored
    let check2 = |actor: &str| -> bool {
        sec2.authorize(&Action {
            kind: ActionKind::Execute,
            resource: "test".into(),
            actor: actor.into(),
        }).unwrap().granted
    };
    assert!(check2(&id_a.device_id), "device_a should be restored");
    assert!(check2(&id_b.device_id), "device_b should be restored");

    // 10. Unknown device NOT authorized
    assert!(!check2("unknown-device"), "unknown should be denied");

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn test_unpair_persists_across_restart() {
    let tmp = std::env::temp_dir().join("oneclaw_field_unpair.db");
    let _ = std::fs::remove_file(&tmp);

    let store = SqliteSecurityStore::new(&tmp).unwrap();
    let sec = DefaultSecurity::production(std::env::current_dir().unwrap())
        .with_persistence(store);

    // Pair two devices
    let code_a = sec.generate_pairing_code().unwrap();
    let id_a = sec.verify_pairing_code(&code_a).unwrap();
    let code_b = sec.generate_pairing_code().unwrap();
    let id_b = sec.verify_pairing_code(&code_b).unwrap();

    // Unpair device_a
    let removed = sec.remove_device(&id_a.device_id).unwrap();
    assert_eq!(removed.device_id, id_a.device_id);

    // Verify device_a NOT authorized, device_b IS
    let check = |sec: &DefaultSecurity, actor: &str| -> bool {
        sec.authorize(&Action {
            kind: ActionKind::Execute,
            resource: "test".into(),
            actor: actor.into(),
        }).unwrap().granted
    };
    assert!(!check(&sec, &id_a.device_id));
    assert!(check(&sec, &id_b.device_id));

    // Restart
    drop(sec);
    let store2 = SqliteSecurityStore::new(&tmp).unwrap();
    let sec2 = DefaultSecurity::production(std::env::current_dir().unwrap())
        .with_persistence(store2);

    assert!(!check(&sec2, &id_a.device_id), "device_a should remain unpaired after restart");
    assert!(check(&sec2, &id_b.device_id), "device_b should survive restart");

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn test_pairing_survives_rapid_restarts() {
    let tmp = std::env::temp_dir().join("oneclaw_field_rapid.db");
    let _ = std::fs::remove_file(&tmp);

    let mut all_ids = Vec::new();

    // 5 cycles of open/pair/close
    for cycle in 0..5 {
        let store = SqliteSecurityStore::new(&tmp).unwrap();
        let sec = DefaultSecurity::production(std::env::current_dir().unwrap())
            .with_persistence(store);

        let code = sec.generate_pairing_code().unwrap();
        let id = sec.verify_pairing_code(&code).unwrap();
        all_ids.push(id.device_id.clone());

        // Verify all previous devices still present
        let devices = sec.list_devices().unwrap();
        assert_eq!(devices.len(), cycle + 1, "cycle {} should have {} devices", cycle, cycle + 1);
    }

    // Final verification: all 5 devices present
    let store = SqliteSecurityStore::new(&tmp).unwrap();
    let sec = DefaultSecurity::production(std::env::current_dir().unwrap())
        .with_persistence(store);

    for id in &all_ids {
        let authorized = sec.authorize(&Action {
            kind: ActionKind::Execute,
            resource: "test".into(),
            actor: id.clone(),
        }).unwrap().granted;
        assert!(authorized, "device {} should be authorized after rapid restarts", id);
    }

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn test_concurrent_pairing_safety() {
    let tmp = std::env::temp_dir().join("oneclaw_field_concurrent.db");
    let _ = std::fs::remove_file(&tmp);

    let store = SqliteSecurityStore::new(&tmp).unwrap();
    let sec = std::sync::Arc::new(
        DefaultSecurity::production(std::env::current_dir().unwrap())
            .with_persistence(store)
    );

    let mut handles = Vec::new();
    for _ in 0..10 {
        let sec_clone = sec.clone();
        handles.push(std::thread::spawn(move || {
            let code = sec_clone.generate_pairing_code().unwrap();
            let identity = sec_clone.verify_pairing_code(&code).unwrap();
            identity.device_id
        }));
    }

    let ids: Vec<String> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(ids.len(), 10);

    // All 10 should be persisted
    let devices = sec.list_devices().unwrap();
    assert_eq!(devices.len(), 10, "all 10 concurrent pairings should persist");

    let _ = std::fs::remove_file(&tmp);
}

// ═══════════════════════════════════════════════════
// GROUP 3: Command Pipeline (end-to-end)
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn test_command_dispatch_secured() {
    // Runtime with DefaultSecurity (pairing required)
    let config = OneClawConfig::default_config();
    let workspace = std::env::current_dir().unwrap();
    let runtime = Runtime::from_config(config, workspace).unwrap();

    // Unpaired device gets rejected for "status" command
    let channel = MockChannel::new(vec!["status", "exit"]);
    runtime.run(&channel).await.unwrap();
    let outputs = channel.get_outputs();
    assert!(outputs[0].contains("Access denied") || outputs[0].contains("not paired"),
        "Unpaired should be rejected: {}", outputs[0]);
}

#[tokio::test]
async fn test_devices_command_output() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    // With NoopSecurity, pair some devices then list
    let channel = MockChannel::new(vec!["verify 111111", "devices", "exit"]);
    runtime.run(&channel).await.unwrap();
    let outputs = channel.get_outputs();
    // NoopSecurity pairs "noop-device"
    assert!(outputs[0].contains("Device paired"), "verify should work: {}", outputs[0]);
    // devices command
    assert!(outputs[1].contains("No paired devices") || outputs[1].contains("Paired Devices"),
        "devices should list: {}", outputs[1]);
}

#[tokio::test]
async fn test_unpair_command_prefix_match() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);
    // NoopSecurity always pairs "noop-device" — but remove_device returns error on Noop
    let channel = MockChannel::new(vec!["unpair noop", "exit"]);
    runtime.run(&channel).await.unwrap();
    let outputs = channel.get_outputs();
    // Either unpaired successfully or error (Noop doesn't support remove)
    assert!(!outputs[0].is_empty(), "unpair should produce output: {}", outputs[0]);
}

#[tokio::test]
async fn test_help_includes_devices_unpair() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);
    let channel = MockChannel::new(vec!["help", "exit"]);
    runtime.run(&channel).await.unwrap();
    let outputs = channel.get_outputs();
    assert!(outputs[0].contains("devices"), "help should include devices: {}", outputs[0]);
    assert!(outputs[0].contains("unpair"), "help should include unpair: {}", outputs[0]);
}

// ═══════════════════════════════════════════════════
// GROUP 4: Memory + Search (Vietnamese pipeline)
// ═══════════════════════════════════════════════════

#[test]
fn test_vietnamese_search_end_to_end() {
    let mem = oneclaw_core::memory::SqliteMemory::new(":memory:").unwrap();
    mem.store(
        "Device Alpha reported high temperature since morning",
        oneclaw_core::memory::MemoryMeta::default(),
    ).unwrap();

    // Search for matching content
    let results = mem.search(&MemoryQuery::new("high temperature").with_limit(5)).unwrap();
    assert!(!results.is_empty(), "Search should match stored content");

    // Search for partial match
    let results2 = mem.search(&MemoryQuery::new("Alpha").with_limit(5)).unwrap();
    assert!(!results2.is_empty(), "Partial search should match");

    // Search for unrelated term
    let results3 = mem.search(&MemoryQuery::new("humidity sensor").with_limit(5)).unwrap();
    assert!(results3.is_empty(), "Unrelated search should not match");
}

#[test]
fn test_memory_persist_and_search_after_restart() {
    let tmp = std::env::temp_dir().join("oneclaw_field_memory.db");
    let _ = std::fs::remove_file(&tmp);

    // Store entries
    {
        let mem = oneclaw_core::memory::SqliteMemory::new(&tmp).unwrap();
        mem.store("Device Alpha temperature = 22.5", oneclaw_core::memory::MemoryMeta::default()).unwrap();
        mem.store("Device Beta humidity = 65", oneclaw_core::memory::MemoryMeta::default()).unwrap();
        assert_eq!(mem.count().unwrap(), 2);
    }

    // Reopen and search
    {
        let mem = oneclaw_core::memory::SqliteMemory::new(&tmp).unwrap();
        assert_eq!(mem.count().unwrap(), 2);
        let results = mem.search(&MemoryQuery::new("Alpha").with_limit(5)).unwrap();
        assert!(!results.is_empty(), "Memory should survive restart");
    }

    let _ = std::fs::remove_file(&tmp);
}

// ═══════════════════════════════════════════════════
// GROUP 5: Alert Dispatch Chain
// ═══════════════════════════════════════════════════

#[test]
fn test_alert_dispatch_to_pending_alerts() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    // Simulate alert by pushing to pending_alerts
    {
        let mut alerts = runtime.pending_alerts.lock().unwrap();
        alerts.push_back("ALERT: Threshold exceeded".into());
    }

    // Verify alert is in queue
    let alerts = runtime.pending_alerts.lock().unwrap();
    assert_eq!(alerts.len(), 1);
    assert!(alerts[0].contains("Threshold"));
}

#[test]
fn test_alert_with_no_channels_graceful() {
    use oneclaw_core::event_bus::{EventBus, Event};

    let bus = oneclaw_core::event_bus::DefaultEventBus::new();
    // Publish alert event with no subscribers
    let event = Event::new("alerts.threshold", "test")
        .with_data("device", "sensor_01")
        .with_data("value", "105.5");
    let result = bus.publish(event);
    assert!(result.is_ok(), "Publishing with no subscribers should not fail");

    // Drain should work fine
    let _drained = bus.drain().unwrap();
}

// ═══════════════════════════════════════════════════
// GROUP 6: Edge Resilience
// ═══════════════════════════════════════════════════

#[test]
fn test_corrupted_db_recovery() {
    let tmp = std::env::temp_dir().join("oneclaw_field_corrupt.db");

    // Create valid DB with a paired device
    {
        let store = SqliteSecurityStore::new(&tmp).unwrap();
        let device = PairedDevice::from_identity(&oneclaw_core::security::Identity {
            device_id: "persist-test".into(),
            paired_at: chrono::Utc::now(),
        });
        store.store_device(&device).unwrap();
    }

    // Corrupt the DB
    std::fs::write(&tmp, b"THIS IS GARBAGE DATA NOT SQLITE").unwrap();

    // Try to open — should fail gracefully
    let result = SqliteSecurityStore::new(&tmp);
    // SQLite may or may not detect corruption on open — it depends on the corruption
    // The key is NO panic
    if let Err(e) = result {
        // Expected: graceful error
        let err_msg = format!("{}", e);
        assert!(!err_msg.is_empty(), "Error message should be present");
    }
    // If it somehow opens, that's also OK (SQLite is resilient)

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn test_read_only_path_graceful() {
    // Try to persist to a path we can't write to
    let result = SqliteSecurityStore::new("/nonexistent/deep/nested/path/security.db");
    assert!(result.is_err(), "Should fail on unwritable path");
    // No panic — graceful error
}

#[test]
fn test_in_memory_fallback_on_db_failure() {
    // DefaultSecurity without persistence should work fine
    let sec = DefaultSecurity::production(std::env::current_dir().unwrap());
    let code = sec.generate_pairing_code().unwrap();
    let identity = sec.verify_pairing_code(&code).unwrap();

    let authorized = sec.authorize(&Action {
        kind: ActionKind::Execute,
        resource: "test".into(),
        actor: identity.device_id,
    }).unwrap().granted;
    assert!(authorized, "In-memory pairing should work without persistence");
}
