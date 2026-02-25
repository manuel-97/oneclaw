//! Runtime Metrics — Operational telemetry for monitoring
//!
//! Thread-safe counters using AtomicU64 for all agent operations.
//! Designed for Edge: no external dependencies, minimal overhead.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Thread-safe operational metrics using atomic counters.
pub struct Metrics {
    boot_time: Instant,

    /// Total messages received.
    pub messages_total: AtomicU64,
    /// Messages that passed security authorization.
    pub messages_secured: AtomicU64,
    /// Messages denied by security.
    pub messages_denied: AtomicU64,
    /// Messages rejected by rate limiter.
    pub messages_rate_limited: AtomicU64,

    /// Total LLM API calls made.
    pub llm_calls_total: AtomicU64,
    /// LLM API calls that failed.
    pub llm_calls_failed: AtomicU64,
    /// Total tokens consumed across all LLM calls.
    pub llm_tokens_total: AtomicU64,
    /// Cumulative LLM latency in milliseconds.
    pub llm_latency_total_ms: AtomicU64,

    /// Total memory store operations.
    pub memory_stores: AtomicU64,
    /// Total memory search operations.
    pub memory_searches: AtomicU64,

    /// Total tool execution calls.
    pub tool_calls_total: AtomicU64,
    /// Tool executions that failed.
    pub tool_calls_failed: AtomicU64,

    /// Total events published to the event bus.
    pub events_published: AtomicU64,
    /// Total events processed (drained) from the event bus.
    pub events_processed: AtomicU64,
    /// Total alerts triggered by event handlers.
    pub alerts_triggered: AtomicU64,

    /// Total chains executed.
    pub chains_executed: AtomicU64,
    /// Total chain steps executed across all chains.
    pub chain_steps_total: AtomicU64,

    /// Total errors encountered.
    pub errors_total: AtomicU64,
}

impl Metrics {
    /// Create a new metrics instance with all counters at zero.
    pub fn new() -> Self {
        Self {
            boot_time: Instant::now(),
            messages_total: AtomicU64::new(0),
            messages_secured: AtomicU64::new(0),
            messages_denied: AtomicU64::new(0),
            messages_rate_limited: AtomicU64::new(0),
            llm_calls_total: AtomicU64::new(0),
            llm_calls_failed: AtomicU64::new(0),
            llm_tokens_total: AtomicU64::new(0),
            llm_latency_total_ms: AtomicU64::new(0),
            memory_stores: AtomicU64::new(0),
            memory_searches: AtomicU64::new(0),
            tool_calls_total: AtomicU64::new(0),
            tool_calls_failed: AtomicU64::new(0),
            events_published: AtomicU64::new(0),
            events_processed: AtomicU64::new(0),
            alerts_triggered: AtomicU64::new(0),
            chains_executed: AtomicU64::new(0),
            chain_steps_total: AtomicU64::new(0),
            errors_total: AtomicU64::new(0),
        }
    }

    /// Uptime in seconds
    pub fn uptime_secs(&self) -> u64 {
        self.boot_time.elapsed().as_secs()
    }

    /// Formatted uptime string
    pub fn uptime_display(&self) -> String {
        let secs = self.uptime_secs();
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        let s = secs % 60;
        if hours > 0 {
            format!("{}h {}m {}s", hours, mins, s)
        } else if mins > 0 {
            format!("{}m {}s", mins, s)
        } else {
            format!("{}s", s)
        }
    }

    /// Average LLM latency in ms (0 if no calls)
    pub fn avg_llm_latency_ms(&self) -> u64 {
        let total = self.llm_calls_total.load(Ordering::Relaxed);
        if total == 0 { return 0; }
        self.llm_latency_total_ms.load(Ordering::Relaxed) / total
    }

    /// Increment a counter
    pub fn inc(counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }

    /// Add to a counter
    pub fn add(counter: &AtomicU64, value: u64) {
        counter.fetch_add(value, Ordering::Relaxed);
    }

    /// Format all metrics as a report string
    pub fn report(&self) -> String {
        let o = Ordering::Relaxed;
        format!(
            "OneClaw Metrics:\n\
             \n  Uptime: {}\
             \n\
             \n  Messages:\
             \n    Total: {} | Secured: {} | Denied: {} | Rate-limited: {}\
             \n\
             \n  LLM:\
             \n    Calls: {} | Failed: {} | Tokens: {} | Avg latency: {}ms\
             \n\
             \n  Memory:\
             \n    Stores: {} | Searches: {}\
             \n\
             \n  Tools:\
             \n    Calls: {} | Failed: {}\
             \n\
             \n  Events:\
             \n    Published: {} | Processed: {} | Alerts: {}\
             \n\
             \n  Chains:\
             \n    Executed: {} | Steps: {}\
             \n\
             \n  Errors: {}",
            self.uptime_display(),
            self.messages_total.load(o),
            self.messages_secured.load(o),
            self.messages_denied.load(o),
            self.messages_rate_limited.load(o),
            self.llm_calls_total.load(o),
            self.llm_calls_failed.load(o),
            self.llm_tokens_total.load(o),
            self.avg_llm_latency_ms(),
            self.memory_stores.load(o),
            self.memory_searches.load(o),
            self.tool_calls_total.load(o),
            self.tool_calls_failed.load(o),
            self.events_published.load(o),
            self.events_processed.load(o),
            self.alerts_triggered.load(o),
            self.chains_executed.load(o),
            self.chain_steps_total.load(o),
            self.errors_total.load(o),
        )
    }
}

impl Default for Metrics {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_new() {
        let m = Metrics::new();
        assert_eq!(m.messages_total.load(Ordering::Relaxed), 0);
        assert!(m.uptime_secs() < 2);
    }

    #[test]
    fn test_metrics_increment() {
        let m = Metrics::new();
        Metrics::inc(&m.messages_total);
        Metrics::inc(&m.messages_total);
        assert_eq!(m.messages_total.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_metrics_add() {
        let m = Metrics::new();
        Metrics::add(&m.llm_tokens_total, 100);
        Metrics::add(&m.llm_tokens_total, 250);
        assert_eq!(m.llm_tokens_total.load(Ordering::Relaxed), 350);
    }

    #[test]
    fn test_metrics_avg_latency() {
        let m = Metrics::new();
        Metrics::inc(&m.llm_calls_total);
        Metrics::inc(&m.llm_calls_total);
        Metrics::add(&m.llm_latency_total_ms, 100);
        Metrics::add(&m.llm_latency_total_ms, 200);
        assert_eq!(m.avg_llm_latency_ms(), 150);
    }

    #[test]
    fn test_metrics_avg_latency_zero_calls() {
        let m = Metrics::new();
        assert_eq!(m.avg_llm_latency_ms(), 0);
    }

    #[test]
    fn test_metrics_report() {
        let m = Metrics::new();
        Metrics::inc(&m.messages_total);
        Metrics::inc(&m.messages_secured);
        Metrics::inc(&m.tool_calls_total);
        let report = m.report();
        assert!(report.contains("OneClaw Metrics"));
        assert!(report.contains("Total: 1"));
        assert!(report.contains("Secured: 1"));
    }

    #[test]
    fn test_uptime_display() {
        let m = Metrics::new();
        let display = m.uptime_display();
        assert!(display.contains('s'));
    }
}
