//! Simple token-bucket rate limiter for DoS prevention

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Simple token-bucket rate limiter
pub struct RateLimiter {
    max_per_minute: usize,
    timestamps: Mutex<Vec<Instant>>,
}

impl RateLimiter {
    /// Create a new rate limiter with the given maximum requests per minute.
    pub fn new(max_per_minute: usize) -> Self {
        Self {
            max_per_minute,
            timestamps: Mutex::new(Vec::new()),
        }
    }

    /// Check if action is allowed. Returns true if under limit.
    pub fn check(&self) -> bool {
        let mut ts = self.timestamps.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        let one_minute_ago = now - Duration::from_secs(60);

        // Remove old timestamps
        ts.retain(|t| *t > one_minute_ago);

        if ts.len() >= self.max_per_minute {
            false
        } else {
            ts.push(now);
            true
        }
    }

    /// Get current count in window
    pub fn current_count(&self) -> usize {
        let ts = self.timestamps.lock().unwrap_or_else(|e| e.into_inner());
        let one_minute_ago = Instant::now() - Duration::from_secs(60);
        ts.iter().filter(|t| **t > one_minute_ago).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_allows_under_limit() {
        let limiter = RateLimiter::new(5);
        for _ in 0..5 {
            assert!(limiter.check(), "Should allow requests under limit");
        }
    }

    #[test]
    fn test_rate_limiter_blocks_over_limit() {
        let limiter = RateLimiter::new(3);
        assert!(limiter.check());
        assert!(limiter.check());
        assert!(limiter.check());
        assert!(!limiter.check(), "Should block 4th request");
    }

    #[test]
    fn test_rate_limiter_current_count() {
        let limiter = RateLimiter::new(10);
        assert_eq!(limiter.current_count(), 0);
        limiter.check();
        limiter.check();
        assert_eq!(limiter.current_count(), 2);
    }

    #[test]
    fn test_rate_limiter_zero_limit() {
        let limiter = RateLimiter::new(0);
        assert!(!limiter.check(), "Zero limit should block everything");
    }
}
