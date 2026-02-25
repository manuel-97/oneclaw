//! Layer 3: Event Bus — Nervous System
//! Pub/Sub + Pipeline Engine for reactive event processing.

use crate::error::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Event flowing through the bus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Unique event ID
    pub id: String,
    /// Topic/channel: "sensor.temperature", "device.status", "alert.critical"
    pub topic: String,
    /// Event payload as key-value pairs (flexible schema)
    pub data: HashMap<String, String>,
    /// Source of the event
    pub source: String,
    /// Priority level
    pub priority: EventPriority,
    /// When the event was created
    pub timestamp: DateTime<Utc>,
}

/// Priority level for events on the bus.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventPriority {
    /// Low priority event.
    Low,
    /// Normal priority event (default).
    #[default]
    Normal,
    /// High priority event.
    High,
    /// Critical priority event.
    Critical,
}

impl Event {
    /// Create a new event with given topic
    pub fn new(topic: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            topic: topic.into(),
            data: HashMap::new(),
            source: source.into(),
            priority: EventPriority::Normal,
            timestamp: Utc::now(),
        }
    }

    /// Builder: set data field
    pub fn with_data(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.data.insert(key.into(), value.into());
        self
    }

    /// Builder: set priority
    pub fn with_priority(mut self, priority: EventPriority) -> Self {
        self.priority = priority;
        self
    }
}

/// Callback type for event handlers
/// Takes: event reference -> Optional response event (to publish back)
pub type EventHandler = Box<dyn Fn(&Event) -> Option<Event> + Send + Sync>;

/// Layer 3 Trait: Event Bus
pub trait EventBus: Send + Sync {
    /// Publish an event to the bus
    fn publish(&self, event: Event) -> Result<()>;

    /// Subscribe a handler to a topic pattern
    /// Pattern supports prefix matching: "sensor.*" matches "sensor.temperature", "sensor.humidity"
    fn subscribe(&self, topic_pattern: &str, handler: EventHandler) -> Result<String>; // returns subscription ID

    /// Unsubscribe by subscription ID
    fn unsubscribe(&self, subscription_id: &str) -> Result<bool>;

    /// Get count of pending events (for monitoring)
    fn pending_count(&self) -> usize;

    /// Process all pending events (synchronous drain)
    fn drain(&self) -> Result<usize>; // returns number of events processed

    /// Get event history (recent events for debugging)
    fn recent_events(&self, limit: usize) -> Result<Vec<Event>>;
}

/// NoopEventBus — does nothing (for testing without event infrastructure)
pub struct NoopEventBus;

impl NoopEventBus {
    /// Create a new no-op event bus.
    pub fn new() -> Self { Self }
}

impl Default for NoopEventBus {
    fn default() -> Self { Self::new() }
}

impl EventBus for NoopEventBus {
    fn publish(&self, _event: Event) -> Result<()> { Ok(()) }
    fn subscribe(&self, _pattern: &str, _handler: EventHandler) -> Result<String> {
        Ok("noop-sub".into())
    }
    fn unsubscribe(&self, _id: &str) -> Result<bool> { Ok(true) }
    fn pending_count(&self) -> usize { 0 }
    fn drain(&self) -> Result<usize> { Ok(0) }
    fn recent_events(&self, _limit: usize) -> Result<Vec<Event>> { Ok(vec![]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_builder() {
        let event = Event::new("sensor.temperature", "sensor-1")
            .with_data("value", "42.5")
            .with_data("unit", "celsius")
            .with_priority(EventPriority::High);

        assert_eq!(event.topic, "sensor.temperature");
        assert_eq!(event.data.get("value"), Some(&"42.5".to_string()));
        assert_eq!(event.priority, EventPriority::High);
        assert!(!event.id.is_empty());
    }

    #[test]
    fn test_noop_event_bus() {
        let bus = NoopEventBus::new();
        let event = Event::new("test", "test");
        assert!(bus.publish(event).is_ok());
        assert_eq!(bus.pending_count(), 0);
        assert_eq!(bus.drain().unwrap(), 0);
    }
}
