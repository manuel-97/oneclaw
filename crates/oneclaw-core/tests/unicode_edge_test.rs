//! TIP-021: Unicode Edge Case Tests
//!
//! Validates zero panics and correct behavior with:
//! - Vietnamese diacritics (acute, grave, hook, tilde, dot-below)
//! - Mixed scripts (Latin + CJK + Emoji)
//! - Empty strings, null bytes, extremely long content
//! - RTL text, combining characters, ZWJ sequences

use async_trait::async_trait;
use oneclaw_core::config::OneClawConfig;
use oneclaw_core::runtime::Runtime;
use oneclaw_core::memory::{MemoryMeta, MemoryQuery, Priority};
use oneclaw_core::event_bus::{Event, EventBus, DefaultEventBus};
use oneclaw_core::channel::{Channel, IncomingMessage, OutgoingMessage};
use std::sync::Mutex;

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
    async fn receive(&self) -> oneclaw_core::error::Result<Option<IncomingMessage>> {
        let mut i = self.inputs.lock().unwrap();
        match i.pop() {
            Some(c) => Ok(Some(IncomingMessage { source: "t".into(), content: c, timestamp: chrono::Utc::now() })),
            None => Ok(Some(IncomingMessage { source: "t".into(), content: "exit".into(), timestamp: chrono::Utc::now() })),
        }
    }
    async fn send(&self, msg: &OutgoingMessage) -> oneclaw_core::error::Result<()> {
        self.outputs.lock().unwrap().push(msg.content.clone()); Ok(())
    }
}

// ==================== UNICODE TESTS ====================

#[tokio::test]
async fn test_vietnamese_diacritics_in_commands() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    // Vietnamese text with all 6 tones — tests diacritics handling
    let ch = TestCh::new(vec![
        "Nhiệt độ phòng khách hôm nay thế nào?",
        "Thiết bị cảm biến cần kiểm tra",
        "Cấp cứu! Hệ thống cảnh báo quá tải",
        "exit",
    ]);
    runtime.run(&ch).await.unwrap();
    let out = ch.outputs();
    // Should not panic, should produce responses
    assert!(out.len() >= 3, "Should have responses for all Vietnamese inputs: got {}", out.len());
}

#[tokio::test]
async fn test_vietnamese_memory_store_and_search() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    // Store Vietnamese content
    let id1 = runtime.memory.store(
        "Nhiệt độ phòng khách: 28°C — bình thường",
        MemoryMeta {
            tags: vec!["nhiệt_độ".into(), "phòng_khách".into()],
            priority: Priority::High,
            source: "cảm_biến".into(),
        },
    ).unwrap();

    let id2 = runtime.memory.store(
        "Độ ẩm tầng trệt: 65% — hơi cao",
        MemoryMeta {
            tags: vec!["độ_ẩm".into(), "tầng_trệt".into()],
            priority: Priority::Medium,
            source: "cảm_biến".into(),
        },
    ).unwrap();

    // Retrieve by ID
    let entry = runtime.memory.get(&id1).unwrap().unwrap();
    assert!(entry.content.contains("phòng khách"));
    assert!(entry.content.contains("28°C"));

    let entry2 = runtime.memory.get(&id2).unwrap().unwrap();
    assert!(entry2.content.contains("tầng trệt"));
}

#[tokio::test]
async fn test_mixed_script_content() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    // Mixed Latin + Vietnamese + CJK + Emoji
    let mixed_content = "Device 設備 thiết bị 🏠 Temp=28°C 💡 Đèn: đang bật";
    let id = runtime.memory.store(mixed_content, MemoryMeta::default()).unwrap();

    let entry = runtime.memory.get(&id).unwrap().unwrap();
    assert_eq!(entry.content, mixed_content);
}

#[tokio::test]
async fn test_emoji_heavy_content() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    // ZWJ sequences, skin tone modifiers, compound emoji
    let emoji_content = "🏠 Smart home 🌡️ results: ✅ Temp normal 💡 Light ❤️‍🔥 OK 🔋";
    let id = runtime.memory.store(emoji_content, MemoryMeta::default()).unwrap();

    let entry = runtime.memory.get(&id).unwrap().unwrap();
    assert_eq!(entry.content, emoji_content);
}

#[tokio::test]
async fn test_empty_string_handling() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    // Empty content in memory
    let id = runtime.memory.store("", MemoryMeta::default()).unwrap();
    let entry = runtime.memory.get(&id).unwrap().unwrap();
    assert_eq!(entry.content, "");

    // Empty search query
    let results = runtime.memory.search(&MemoryQuery::new("")).unwrap();
    // Should return entries (no panic)
    assert!(!results.is_empty());

    // Empty command through channel
    let ch = TestCh::new(vec!["", "exit"]);
    runtime.run(&ch).await.unwrap();
    // Should not panic
}

#[tokio::test]
async fn test_null_bytes_in_content() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    // Content with embedded null bytes
    let content_with_nulls = "data\0with\0null\0bytes";
    let id = runtime.memory.store(content_with_nulls, MemoryMeta::default()).unwrap();
    let entry = runtime.memory.get(&id).unwrap().unwrap();
    assert_eq!(entry.content, content_with_nulls);
}

#[tokio::test]
async fn test_extremely_long_unicode_content() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    // 10KB of Vietnamese text
    let long_content: String = "Nhiệt độ phòng khách rất cao. ".repeat(400);
    assert!(long_content.len() > 10_000);

    let id = runtime.memory.store(&long_content, MemoryMeta::default()).unwrap();
    let entry = runtime.memory.get(&id).unwrap().unwrap();
    assert_eq!(entry.content.len(), long_content.len());
}

#[tokio::test]
async fn test_rtl_and_combining_characters() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    // Arabic RTL text + combining characters
    let rtl_content = "جهاز: درجة الحرارة 28";
    let id = runtime.memory.store(rtl_content, MemoryMeta::default()).unwrap();
    let entry = runtime.memory.get(&id).unwrap().unwrap();
    assert_eq!(entry.content, rtl_content);

    // Combining diacritical marks (separate from base character)
    let combining = "a\u{0301}"; // á as a + combining acute
    let id2 = runtime.memory.store(combining, MemoryMeta::default()).unwrap();
    let entry2 = runtime.memory.get(&id2).unwrap().unwrap();
    assert_eq!(entry2.content, combining);
}

#[tokio::test]
async fn test_unicode_in_event_bus() {
    let bus = DefaultEventBus::new();

    // Vietnamese topic and data
    let event = Event::new("cảm_biến.nhiệt_độ", "thiết_bị_thông_minh")
        .with_data("phòng", "phòng khách")
        .with_data("giá_trị", "28°C");

    bus.publish(event).unwrap();
    assert_eq!(bus.pending_count(), 1);

    let drained = bus.drain().unwrap();
    assert_eq!(drained, 1);

    let recent = bus.recent_events(1).unwrap();
    assert_eq!(recent[0].topic, "cảm_biến.nhiệt_độ");
    assert_eq!(recent[0].data.get("phòng").unwrap(), "phòng khách");
}

#[tokio::test]
async fn test_unicode_in_tags() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    let id = runtime.memory.store(
        "Test entry with unicode tags",
        MemoryMeta {
            tags: vec!["nhiệt_độ".into(), "cảnh_báo".into(), "🏠".into()],
            priority: Priority::Critical,
            source: "thiết_bị".into(),
        },
    ).unwrap();

    let entry = runtime.memory.get(&id).unwrap().unwrap();
    assert_eq!(entry.meta.tags.len(), 3);
    assert!(entry.meta.tags.contains(&"nhiệt_độ".to_string()));
    assert!(entry.meta.tags.contains(&"🏠".to_string()));
    assert_eq!(entry.meta.source, "thiết_bị");
}

#[tokio::test]
async fn test_special_fts_characters() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    // Characters that might break FTS queries: quotes, parens, wildcards
    let tricky_contents = vec![
        "Sensor reading (value/unit): 42.5/celsius",
        "Device's temperature is \"elevated\"",
        "Note: value = 42 AND type = 'temperature'",
        "Wildcard test: * ? + - !",
    ];

    for content in &tricky_contents {
        let id = runtime.memory.store(content, MemoryMeta::default()).unwrap();
        let entry = runtime.memory.get(&id).unwrap().unwrap();
        assert_eq!(&entry.content, content);
    }
}

#[tokio::test]
async fn test_whitespace_only_input() {
    let config = OneClawConfig::default_config();
    let runtime = Runtime::with_defaults(config);

    // Various whitespace-only inputs
    let ch = TestCh::new(vec![" ", "\t", "\n", "  \n\t  ", "exit"]);
    runtime.run(&ch).await.unwrap();
    // Should not panic
    let out = ch.outputs();
    assert!(out.len() >= 4, "Should produce responses for whitespace inputs");
}
