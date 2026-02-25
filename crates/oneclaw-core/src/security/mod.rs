//! Layer 0: Security Core — Immune System
//! Deny-by-default. Every action must be authorized.

pub mod traits;
pub mod default;
pub mod path_guard;
pub mod pairing;
pub mod persistence;
pub mod rate_limit;

// Re-exports
pub use traits::*;
pub use default::DefaultSecurity;
pub use persistence::SqliteSecurityStore;
pub use rate_limit::RateLimiter;
