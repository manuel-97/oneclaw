//! Async event bus using tokio broadcast channels.
//!
//! Events are processed immediately upon publish — no drain() needed.
//! Supports multiple concurrent subscribers via tokio::sync::broadcast.
//!
//! DefaultEventBus (sync, drain-based) remains the default.
//! AsyncEventBus is opt-in for consumer apps needing realtime events.

use std::collections::VecDeque;
use std::sync::{Mutex, RwLock};
use tokio::sync::broadcast;
use tracing::debug;

use crate::error::{OneClawError, Result};
use crate::event_bus::traits::{Event, EventBus, EventHandler};

/// Subscription with pattern matching (for EventBus trait compat).
struct AsyncSubscription {
    id: String,
    pattern: String,
    handler: EventHandler,
}

/// Async event bus backed by tokio broadcast channels.
///
/// Key differences from DefaultEventBus:
/// - publish() sends to all subscribers immediately (no drain needed)
/// - Multiple async subscribers can receive same event concurrently
/// - History kept for late-joining subscribers and recent_events()
/// - drain() is a no-op returning 0 (events already processed on publish)
/// - pending_count() always returns 0 (no queue)
pub struct AsyncEventBus {
    sender: broadcast::Sender<Event>,
    /// Event history ring buffer.
    history: Mutex<VecDeque<Event>>,
    history_capacity: usize,
    /// Subscriptions with pattern matching (for EventBus trait compat).
    subscriptions: RwLock<Vec<AsyncSubscription>>,
}

impl AsyncEventBus {
    /// Create a new async event bus.
    ///
    /// `capacity`: broadcast channel capacity. Events are dropped if a subscriber
    /// falls behind by this many events. 256 is good for most edge use cases.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            history: Mutex::new(VecDeque::with_capacity(capacity)),
            history_capacity: capacity,
            subscriptions: RwLock::new(Vec::new()),
        }
    }

    /// Get a broadcast receiver for async event consumption.
    ///
    /// This is the preferred way for consumer apps to receive events.
    /// Call this BEFORE boxing the bus as `Box<dyn EventBus>`.
    pub fn subscribe_channel(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    /// Get a clone of the broadcast sender.
    ///
    /// Consumer apps can use this to create receivers later:
    /// `sender.subscribe()` returns a new `Receiver<Event>`.
    pub fn sender(&self) -> broadcast::Sender<Event> {
        self.sender.clone()
    }

    /// Check if an event topic matches a subscription pattern.
    ///
    /// Pattern matching rules (same as DefaultEventBus):
    /// - `"*"` matches all events
    /// - `"topic.*"` or `"topic*"` matches prefix
    /// - exact string matches exact topic
    fn topic_matches(pattern: &str, topic: &str) -> bool {
        if pattern == "*" {
            return true;
        }
        if let Some(prefix) = pattern.strip_suffix(".*") {
            return topic.starts_with(prefix);
        }
        if let Some(prefix) = pattern.strip_suffix('*') {
            return topic.starts_with(prefix);
        }
        pattern == topic
    }
}

impl EventBus for AsyncEventBus {
    fn publish(&self, event: Event) -> Result<()> {
        debug!(topic = %event.topic, source = %event.source, "AsyncEventBus: event published");

        // 1. Process through sync subscriptions (for EventBus trait compat)
        let mut derived_events: Vec<Event> = Vec::new();
        {
            let subs = self.subscriptions.read()
                .unwrap_or_else(|e| { tracing::warn!("Subscriptions lock poisoned, recovering"); e.into_inner() });
            for sub in subs.iter() {
                if Self::topic_matches(&sub.pattern, &event.topic)
                    && let Some(new_event) = (sub.handler)(&event)
                {
                    derived_events.push(new_event);
                }
            }
        }

        // 2. Add to history
        {
            let mut history = self.history.lock()
                .unwrap_or_else(|e| { tracing::warn!("History lock poisoned, recovering"); e.into_inner() });
            if history.len() >= self.history_capacity {
                history.pop_front();
            }
            history.push_back(event.clone());
        }

        // 3. Broadcast to async subscribers (Err = 0 receivers, that's OK)
        let _ = self.sender.send(event);

        // 4. Publish derived events from sync handlers
        for derived in derived_events {
            // Add derived to history + broadcast (avoid infinite recursion by not re-matching)
            {
                let mut history = self.history.lock()
                    .unwrap_or_else(|e| e.into_inner());
                if history.len() >= self.history_capacity {
                    history.pop_front();
                }
                history.push_back(derived.clone());
            }
            let _ = self.sender.send(derived);
        }

        Ok(())
    }

    fn subscribe(&self, topic_pattern: &str, handler: EventHandler) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let sub = AsyncSubscription {
            id: id.clone(),
            pattern: topic_pattern.to_string(),
            handler,
        };

        let mut subs = self.subscriptions.write()
            .map_err(|e| OneClawError::EventBus(format!("Subscription lock error: {}", e)))?;
        subs.push(sub);

        debug!(pattern = %topic_pattern, id = %&id[..8], "AsyncEventBus: subscribed");
        Ok(id)
    }

    fn unsubscribe(&self, subscription_id: &str) -> Result<bool> {
        let mut subs = self.subscriptions.write()
            .map_err(|e| OneClawError::EventBus(format!("Subscription lock error: {}", e)))?;
        let before = subs.len();
        subs.retain(|s| s.id != subscription_id);
        Ok(subs.len() < before)
    }

    fn pending_count(&self) -> usize {
        // No queue — events are processed immediately on publish.
        0
    }

    fn drain(&self) -> Result<usize> {
        // No-op for async bus — events already processed on publish.
        debug!("AsyncEventBus::drain() is a no-op — events processed immediately");
        Ok(0)
    }

    fn recent_events(&self, limit: usize) -> Result<Vec<Event>> {
        let history = self.history.lock()
            .unwrap_or_else(|e| { tracing::warn!("History lock poisoned, recovering"); e.into_inner() });
        let events: Vec<Event> = history.iter().rev().take(limit).cloned().collect();
        // Reverse back to chronological order (oldest first)
        Ok(events.into_iter().rev().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::traits::EventPriority;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn test_event(topic: &str, content: &str) -> Event {
        Event::new(topic, "test").with_data("content", content)
    }

    // ── Basic publish/history ──

    #[test]
    fn test_async_bus_publish() {
        let bus = AsyncEventBus::new(256);
        bus.publish(test_event("test.topic", "hello")).unwrap();

        let recent = bus.recent_events(10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].data.get("content"), Some(&"hello".to_string()));
    }

    #[test]
    fn test_async_bus_history_capacity() {
        let bus = AsyncEventBus::new(5);
        for i in 0..10 {
            bus.publish(test_event("test", &format!("event-{}", i))).unwrap();
        }
        let recent = bus.recent_events(100).unwrap();
        assert_eq!(recent.len(), 5); // only last 5 kept
        assert_eq!(recent[0].data.get("content"), Some(&"event-5".to_string()));
        assert_eq!(recent[4].data.get("content"), Some(&"event-9".to_string()));
    }

    // ── Subscription pattern matching ──

    #[test]
    fn test_async_bus_subscribe_wildcard() {
        let bus = AsyncEventBus::new(256);
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();

        bus.subscribe("*", Box::new(move |_event| {
            c.fetch_add(1, Ordering::SeqCst);
            None
        })).unwrap();

        bus.publish(test_event("a.b", "msg1")).unwrap();
        bus.publish(test_event("c.d", "msg2")).unwrap();

        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_async_bus_subscribe_pattern() {
        let bus = AsyncEventBus::new(256);
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();

        bus.subscribe("sensor.*", Box::new(move |_event| {
            c.fetch_add(1, Ordering::SeqCst);
            None
        })).unwrap();

        bus.publish(test_event("sensor.temperature", "32")).unwrap();
        bus.publish(test_event("system.status", "ok")).unwrap();

        assert_eq!(counter.load(Ordering::SeqCst), 1); // only sensor event
    }

    #[test]
    fn test_async_bus_topic_matching() {
        assert!(AsyncEventBus::topic_matches("*", "anything"));
        assert!(AsyncEventBus::topic_matches("sensor.*", "sensor.temp"));
        assert!(AsyncEventBus::topic_matches("sensor.*", "sensor.motion"));
        assert!(!AsyncEventBus::topic_matches("sensor.*", "alert.critical"));
        assert!(AsyncEventBus::topic_matches("sensor.temp", "sensor.temp"));
        assert!(!AsyncEventBus::topic_matches("sensor.temp", "sensor.motion"));
    }

    // ── Unsubscribe ──

    #[test]
    fn test_async_bus_unsubscribe() {
        let bus = AsyncEventBus::new(256);
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();

        let id = bus.subscribe("*", Box::new(move |_| {
            c.fetch_add(1, Ordering::SeqCst);
            None
        })).unwrap();

        bus.publish(test_event("test", "before")).unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        assert!(bus.unsubscribe(&id).unwrap());
        assert!(!bus.unsubscribe(&id).unwrap()); // already removed

        bus.publish(test_event("test", "after")).unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 1); // no increase
    }

    // ── drain() is no-op ──

    #[test]
    fn test_async_bus_drain_noop() {
        let bus = AsyncEventBus::new(256);
        bus.publish(test_event("test", "msg")).unwrap();
        let drained = bus.drain().unwrap();
        assert_eq!(drained, 0); // no-op for async bus
        // But event still in history
        assert_eq!(bus.recent_events(10).unwrap().len(), 1);
    }

    #[test]
    fn test_async_bus_pending_count_always_zero() {
        let bus = AsyncEventBus::new(256);
        bus.publish(test_event("test", "msg")).unwrap();
        assert_eq!(bus.pending_count(), 0);
    }

    // ── Handler generates response event ──

    #[test]
    fn test_async_bus_handler_generates_event() {
        let bus = AsyncEventBus::new(256);
        let alert_count = Arc::new(AtomicUsize::new(0));
        let ac = alert_count.clone();

        // Handler that generates alert when value > 100
        bus.subscribe("sensor.*", Box::new(|event| {
            if let Some(value) = event.data.get("value")
                && value.parse::<f64>().unwrap_or(0.0) > 100.0
            {
                return Some(Event::new("alert.threshold", "pipeline")
                    .with_data("source_topic", event.topic.clone())
                    .with_priority(EventPriority::Critical));
            }
            None
        })).unwrap();

        // Subscribe to alerts
        bus.subscribe("alert.*", Box::new(move |_| {
            ac.fetch_add(1, Ordering::SeqCst);
            None
        })).unwrap();

        // Publish high-value event
        bus.publish(
            Event::new("sensor.temperature", "temp-sensor")
                .with_data("value", "105.5")
        ).unwrap();

        // Unlike DefaultEventBus (needs 2 drains), derived events NOT re-matched
        // to avoid infinite recursion. alert.* handler won't fire from derived events.
        // But the derived event IS in history and broadcast channel.
        let recent = bus.recent_events(10).unwrap();
        assert_eq!(recent.len(), 2); // original + derived
        assert_eq!(recent[1].topic, "alert.threshold");
    }

    // ── Async channel ──

    #[tokio::test]
    async fn test_async_bus_channel_recv() {
        let bus = AsyncEventBus::new(256);
        let mut rx = bus.subscribe_channel();

        bus.publish(test_event("test", "async-msg")).unwrap();

        let event = rx.recv().await.unwrap();
        assert_eq!(event.topic, "test");
        assert_eq!(event.data.get("content"), Some(&"async-msg".to_string()));
    }

    #[tokio::test]
    async fn test_async_bus_multiple_receivers() {
        let bus = AsyncEventBus::new(256);
        let mut rx1 = bus.subscribe_channel();
        let mut rx2 = bus.subscribe_channel();

        bus.publish(test_event("test", "broadcast")).unwrap();

        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();
        // Both receivers get the same event
        assert_eq!(e1.topic, e2.topic);
        assert_eq!(e1.data.get("content"), e2.data.get("content"));
    }

    #[tokio::test]
    async fn test_async_bus_realtime_latency() {
        let bus = AsyncEventBus::new(256);
        let mut rx = bus.subscribe_channel();

        let start = std::time::Instant::now();
        bus.publish(test_event("test", "latency")).unwrap();
        let _ = rx.recv().await.unwrap();
        let elapsed = start.elapsed();

        assert!(elapsed.as_millis() < 10, "Event latency too high: {:?}", elapsed);
    }

    #[tokio::test]
    async fn test_async_bus_sender_clone() {
        let bus = AsyncEventBus::new(256);
        let sender = bus.sender();
        let mut rx = bus.subscribe_channel();

        // Create receiver from cloned sender
        let mut rx2 = sender.subscribe();

        bus.publish(test_event("test", "via-bus")).unwrap();

        let e1 = rx.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();
        assert_eq!(e1.topic, e2.topic);
    }

    // ── EventBus trait compatibility ──

    #[test]
    fn test_async_bus_is_event_bus() {
        let bus: Box<dyn EventBus> = Box::new(AsyncEventBus::new(256));
        bus.publish(test_event("test", "trait-compat")).unwrap();
        let drained = bus.drain().unwrap();
        assert_eq!(drained, 0); // no-op but doesn't crash
        assert_eq!(bus.recent_events(10).unwrap().len(), 1);
    }
}
