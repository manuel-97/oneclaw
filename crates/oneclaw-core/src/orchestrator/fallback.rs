//! Degradation modes — graceful fallback when connectivity drops

/// Degradation modes when connectivity drops
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DegradationMode {
    /// Full connectivity: use cloud for complex tasks
    #[default]
    FullOnline,
    /// Limited: only cloud for Critical tasks
    Metered,
    /// No internet: 100% local models
    Offline,
    /// No models at all: rule-based fallback
    Emergency,
}

