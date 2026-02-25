//! Model Router — Smart routing based on task complexity
//!
//! 4-factor routing:
//! 1. Message length (short → simple, long → complex)
//! 2. Keyword detection (medical terms → complex, greetings → simple)
//! 3. Whether memory context is needed
//! 4. Explicit complexity hints from caller

use crate::error::Result;

/// The complexity level of a task for routing decisions
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Complexity {
    /// Quick response, use cheapest/fastest model
    Simple,
    /// Standard conversation, balanced model
    Medium,
    /// Analysis needed, use best available model
    Complex,
    /// Life/safety critical, use best model and verify
    Critical,
}

/// The result of a routing decision
#[derive(Debug, Clone)]
pub struct ModelChoice {
    /// The selected provider name
    pub provider: String,
    /// The selected model identifier
    pub model: String,
    /// The reason for this routing decision
    pub reason: String,
}

/// Trait for routing requests to the appropriate model based on complexity
pub trait ModelRouter: Send + Sync {
    /// Select a provider and model for the given complexity level
    fn route(&self, complexity: Complexity) -> Result<ModelChoice>;
}

/// Noop: always returns a placeholder
pub struct NoopRouter;
impl ModelRouter for NoopRouter {
    fn route(&self, _complexity: Complexity) -> Result<ModelChoice> {
        Ok(ModelChoice {
            provider: "noop".into(),
            model: "noop".into(),
            reason: "noop router".into(),
        })
    }
}

/// Smart router that maps complexity to provider/model
pub struct DefaultRouter {
    /// Provider configs: (complexity_level, provider_name, model_name)
    routes: Vec<(Complexity, String, String)>,
    /// Fallback provider when nothing matches
    fallback_provider: String,
    fallback_model: String,
}

impl DefaultRouter {
    /// Create with explicit route mapping
    pub fn new(routes: Vec<(Complexity, String, String)>) -> Self {
        Self {
            routes,
            fallback_provider: "noop".into(),
            fallback_model: "noop".into(),
        }
    }

    /// Create from config — maps complexity to configured providers
    pub fn from_config(config: &crate::config::ProvidersConfig) -> Self {
        let default_provider = config.default.clone();
        let default_model = match default_provider.as_str() {
            "ollama" => config.ollama.model.clone(),
            "openai" => config.openai.model.clone(),
            _ => "noop".into(),
        };

        // Simple routing: all complexities go to default provider
        // Sprint 7-8 will add multi-provider routing (simple→local, complex→cloud)
        let routes = vec![
            (Complexity::Simple, default_provider.clone(), default_model.clone()),
            (Complexity::Medium, default_provider.clone(), default_model.clone()),
            (Complexity::Complex, default_provider.clone(), default_model.clone()),
            (Complexity::Critical, default_provider.clone(), default_model.clone()),
        ];

        Self {
            routes,
            fallback_provider: "noop".into(),
            fallback_model: "noop".into(),
        }
    }
}

impl ModelRouter for DefaultRouter {
    fn route(&self, complexity: Complexity) -> Result<ModelChoice> {
        for (level, provider, model) in &self.routes {
            if *level == complexity {
                return Ok(ModelChoice {
                    provider: provider.clone(),
                    model: model.clone(),
                    reason: format!("Complexity {:?} → {}:{}", complexity, provider, model),
                });
            }
        }

        // Fallback
        Ok(ModelChoice {
            provider: self.fallback_provider.clone(),
            model: self.fallback_model.clone(),
            reason: format!("No route for {:?}, using fallback", complexity),
        })
    }
}

/// Analyze a message to determine complexity
pub fn analyze_complexity(message: &str, has_memory_context: bool) -> Complexity {
    let word_count = message.split_whitespace().count();
    let lower = message.to_lowercase();

    // Critical: emergency/safety keywords
    let critical_keywords = ["emergency", "critical", "urgent", "danger", "alert",
        "shutdown", "failure", "fatal"];
    if critical_keywords.iter().any(|k| lower.contains(k)) {
        return Complexity::Critical;
    }

    // Complex: analysis, comparison, reasoning
    let complex_keywords = ["analyze", "compare", "why", "explain", "trend",
        "diagnose", "recommend", "evaluate", "summarize", "correlate"];
    if complex_keywords.iter().any(|k| lower.contains(k)) {
        return Complexity::Complex;
    }

    // If memory context is involved, at least Medium
    if has_memory_context && word_count > 5 {
        return Complexity::Medium;
    }

    // Simple: short messages, greetings
    if word_count <= 5 {
        return Complexity::Simple;
    }

    Complexity::Medium
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_router_routes() {
        let router = DefaultRouter::new(vec![
            (Complexity::Simple, "ollama".into(), "llama3.2:1b".into()),
            (Complexity::Complex, "openai".into(), "gpt-4o".into()),
        ]);
        let choice = router.route(Complexity::Simple).unwrap();
        assert_eq!(choice.provider, "ollama");
        let choice = router.route(Complexity::Complex).unwrap();
        assert_eq!(choice.provider, "openai");
    }

    #[test]
    fn test_router_fallback() {
        let router = DefaultRouter::new(vec![]);
        let choice = router.route(Complexity::Simple).unwrap();
        assert_eq!(choice.provider, "noop");
    }

    #[test]
    fn test_analyze_critical() {
        assert_eq!(analyze_complexity("emergency alert detected!", false), Complexity::Critical);
        assert_eq!(analyze_complexity("critical system failure", false), Complexity::Critical);
    }

    #[test]
    fn test_analyze_complex() {
        assert_eq!(analyze_complexity("analyze trend data over 7 days", false), Complexity::Complex);
        assert_eq!(analyze_complexity("Why is the sensor reading increasing?", false), Complexity::Complex);
    }

    #[test]
    fn test_analyze_simple() {
        assert_eq!(analyze_complexity("hello", false), Complexity::Simple);
        assert_eq!(analyze_complexity("status check", false), Complexity::Simple);
    }

    #[test]
    fn test_analyze_medium_with_context() {
        assert_eq!(
            analyze_complexity("what are the sensor readings from device today", true),
            Complexity::Medium
        );
    }

    #[test]
    fn test_router_from_config() {
        let config = crate::config::ProvidersConfig {
            default: "ollama".into(),
            llm_timeout_secs: 30,
            ollama: crate::config::OllamaConfig {
                url: "http://localhost:11434".into(),
                model: "llama3.2:1b".into(),
            },
            openai: crate::config::OpenAIConfig::default(),
        };
        let router = DefaultRouter::from_config(&config);
        let choice = router.route(Complexity::Simple).unwrap();
        assert_eq!(choice.provider, "ollama");
        assert_eq!(choice.model, "llama3.2:1b");
    }
}
