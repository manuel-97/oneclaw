//! Configuration loader for OneClaw

use serde::Deserialize;
use std::path::Path;

/// Top-level configuration for the OneClaw agent.
#[derive(Debug, Deserialize, Default, Clone)]
pub struct OneClawConfig {
    /// The security configuration section.
    #[serde(default)]
    pub security: SecurityConfig,
    /// The runtime configuration section.
    #[serde(default)]
    pub runtime: RuntimeConfig,
    /// The LLM providers configuration section.
    #[serde(default)]
    pub providers: ProvidersConfig,
    /// The memory backend configuration section.
    #[serde(default)]
    pub memory: MemoryConfig,
    /// The channels configuration section.
    #[serde(default)]
    pub channels: ChannelsConfig,
    /// Telegram bot configuration (optional).
    #[serde(default)]
    pub telegram: Option<TelegramConfig>,
    /// MQTT broker configuration (optional).
    #[serde(default)]
    pub mqtt: Option<MqttConfig>,
    /// Provider configuration (v1.5 multi-provider foundation).
    #[serde(default)]
    pub provider: ProviderConfigToml,
}

/// Security layer configuration.
#[derive(Debug, Deserialize, Clone)]
pub struct SecurityConfig {
    /// Whether to deny all unauthorized requests by default.
    #[serde(default = "default_true")]
    pub deny_by_default: bool,
    /// Whether device pairing is required before interaction.
    #[serde(default = "default_true")]
    pub pairing_required: bool,
    /// Whether to restrict operations to the workspace directory.
    #[serde(default = "default_true")]
    pub workspace_only: bool,
    /// Path to persist paired device registry (empty = in-memory only) \[LEGACY\]
    #[serde(default)]
    pub registry_path: String,
    /// Path to SQLite database for persistent device pairing
    #[serde(default = "default_persist_path")]
    pub persist_path: String,
    /// Whether to persist pairing state to SQLite
    #[serde(default = "default_true")]
    pub persist_pairing: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            deny_by_default: true,
            pairing_required: true,
            workspace_only: true,
            registry_path: String::new(),
            persist_path: default_persist_path(),
            persist_pairing: true,
        }
    }
}

/// Runtime configuration (agent name, logging).
#[derive(Debug, Deserialize, Clone)]
pub struct RuntimeConfig {
    /// The agent instance name.
    #[serde(default = "default_name")]
    pub name: String,
    /// The log level filter (e.g. "debug", "info").
    #[serde(default)]
    pub log_level: String,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            name: default_name(),
            log_level: String::new(),
        }
    }
}

/// LLM provider configuration (default provider, timeouts, per-provider settings).
#[derive(Debug, Deserialize, Clone)]
pub struct ProvidersConfig {
    /// Default provider: "ollama", "openai", "noop"
    #[serde(default = "default_noop")]
    pub default: String,

    /// LLM call timeout threshold in seconds (monitoring, not cancellation)
    #[serde(default = "default_llm_timeout")]
    pub llm_timeout_secs: u64,

    /// The Ollama provider configuration.
    #[serde(default)]
    pub ollama: OllamaConfig,

    /// The OpenAI provider configuration.
    #[serde(default)]
    pub openai: OpenAIConfig,
}

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            default: default_noop(),
            llm_timeout_secs: default_llm_timeout(),
            ollama: OllamaConfig::default(),
            openai: OpenAIConfig::default(),
        }
    }
}

/// Ollama provider configuration.
#[derive(Debug, Deserialize, Clone)]
pub struct OllamaConfig {
    /// The Ollama server URL.
    #[serde(default = "default_ollama_url")]
    pub url: String,
    /// The Ollama model name to use.
    #[serde(default = "default_ollama_model")]
    pub model: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            url: default_ollama_url(),
            model: default_ollama_model(),
        }
    }
}

/// OpenAI provider configuration.
#[derive(Deserialize, Clone)]
pub struct OpenAIConfig {
    /// The OpenAI API key.
    #[serde(default)]
    pub api_key: String,
    /// The OpenAI model name to use.
    #[serde(default = "default_openai_model")]
    pub model: String,
    /// The OpenAI API base URL.
    #[serde(default = "default_openai_url")]
    pub base_url: String,
}

impl std::fmt::Debug for OpenAIConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAIConfig")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &mask_key(&self.api_key))
            .finish()
    }
}

/// Mask API key for logging — show first 4 + last 4 chars
pub fn mask_key(key: &str) -> String {
    if key.is_empty() {
        "(empty)".to_string()
    } else if key.len() <= 8 {
        "****".to_string()
    } else {
        format!("{}...{}", &key[..4], &key[key.len()-4..])
    }
}

impl Default for OpenAIConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: default_openai_model(),
            base_url: default_openai_url(),
        }
    }
}

/// Memory backend configuration.
#[derive(Debug, Deserialize, Clone)]
pub struct MemoryConfig {
    /// Backend: "sqlite", "noop". Default: "sqlite" for data persistence.
    #[serde(default = "default_sqlite")]
    pub backend: String,
    /// Path to database file
    #[serde(default = "default_db_path")]
    pub db_path: String,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            backend: default_sqlite(),
            db_path: default_db_path(),
        }
    }
}

/// Telegram bot configuration.
#[derive(Deserialize, Clone)]
pub struct TelegramConfig {
    /// Bot token from @BotFather
    pub bot_token: String,
    /// Allowed chat IDs (empty = allow all)
    #[serde(default)]
    pub allowed_chat_ids: Vec<i64>,
    /// Long-polling timeout in seconds
    #[serde(default = "default_polling_timeout")]
    pub polling_timeout: u64,
}

impl std::fmt::Debug for TelegramConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelegramConfig")
            .field("bot_token", &mask_key(&self.bot_token))
            .field("allowed_chat_ids", &self.allowed_chat_ids)
            .field("polling_timeout", &self.polling_timeout)
            .finish()
    }
}

/// MQTT broker configuration.
#[derive(Deserialize, Clone)]
pub struct MqttConfig {
    /// Broker host (e.g., "localhost", "mqtt.example.com")
    pub host: String,
    /// Broker port (default 1883)
    #[serde(default = "default_mqtt_port")]
    pub port: u16,
    /// Client ID (unique per device)
    #[serde(default = "default_mqtt_client_id")]
    pub client_id: String,
    /// Topics to subscribe (e.g., ["sensors/#", "devices/+/data"])
    #[serde(default)]
    pub subscribe_topics: Vec<String>,
    /// Topic prefix for publishing responses/alerts
    #[serde(default = "default_mqtt_publish_prefix")]
    pub publish_prefix: String,
    /// Username (optional)
    #[serde(default)]
    pub username: Option<String>,
    /// Password (optional, masked in Debug)
    #[serde(default)]
    pub password: Option<String>,
    /// Keep-alive interval in seconds
    #[serde(default = "default_mqtt_keepalive")]
    pub keep_alive_secs: u64,
}

impl std::fmt::Debug for MqttConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MqttConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("client_id", &self.client_id)
            .field("subscribe_topics", &self.subscribe_topics)
            .field("publish_prefix", &self.publish_prefix)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "***"))
            .field("keep_alive_secs", &self.keep_alive_secs)
            .finish()
    }
}

/// Channel configuration (which communication channels are active).
#[derive(Debug, Deserialize, Clone)]
pub struct ChannelsConfig {
    /// Active channels: ["cli"], ["cli", "mqtt"], etc.
    #[serde(default = "default_channels")]
    pub active: Vec<String>,
}

impl Default for ChannelsConfig {
    fn default() -> Self {
        Self {
            active: default_channels(),
        }
    }
}

/// Provider configuration — v1.5 multi-provider with FallbackChain.
///
/// Supports: anthropic, openai, deepseek, groq, google/gemini, ollama.
/// FallbackChain: primary → fallback\[0\] → fallback\[1\] → ... with per-provider retry.
#[derive(Debug, Deserialize, Clone)]
pub struct ProviderConfigToml {
    /// Primary provider ID: "anthropic", "openai", "google", "deepseek", "groq", "ollama"
    #[serde(default = "default_provider")]
    pub primary: String,
    /// Model name (e.g., "claude-sonnet-4-20250514", "gpt-4o", "gemini-2.0-flash")
    #[serde(default = "default_provider_model")]
    pub model: String,
    /// Max tokens for response
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// Temperature (0.0 - 1.0)
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    /// API key for primary provider (also checked: ONECLAW_API_KEY env)
    #[serde(default)]
    pub api_key: Option<String>,
    /// Fallback provider chain (tried in order after primary fails)
    #[serde(default)]
    pub fallback: Vec<String>,
    /// Max retries per provider before moving to next in chain
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Per-provider API keys (overrides primary api_key for specific provider)
    /// Example: [provider.keys]
    ///          openai = "sk-..."
    ///          google = "AIza..."
    #[serde(default)]
    pub keys: std::collections::HashMap<String, String>,
    /// Ollama endpoint override (default: http://localhost:11434)
    #[serde(default)]
    pub ollama_endpoint: Option<String>,
    /// Ollama model override (separate from primary model)
    #[serde(default)]
    pub ollama_model: Option<String>,
}

impl Default for ProviderConfigToml {
    fn default() -> Self {
        Self {
            primary: default_provider(),
            model: default_provider_model(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            api_key: None,
            fallback: vec![],
            max_retries: default_max_retries(),
            keys: std::collections::HashMap::new(),
            ollama_endpoint: None,
            ollama_model: None,
        }
    }
}

fn default_max_retries() -> u32 { 1 }

impl std::fmt::Display for ProviderConfigToml {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{} (max_tokens={}, temp={})", self.primary, self.model, self.max_tokens, self.temperature)
    }
}

fn default_provider() -> String { "anthropic".into() }
fn default_provider_model() -> String { "claude-sonnet-4-20250514".into() }
fn default_max_tokens() -> u32 { 1024 }
fn default_temperature() -> f32 { 0.3 }

fn default_true() -> bool { true }
fn default_persist_path() -> String { "data/security.db".to_string() }
fn default_name() -> String { "oneclaw".to_string() }
fn default_noop() -> String { "noop".to_string() }
fn default_sqlite() -> String { "sqlite".to_string() }
fn default_ollama_url() -> String { "http://localhost:11434".to_string() }
fn default_ollama_model() -> String { "llama3.2:1b".to_string() }
fn default_openai_model() -> String { "gpt-4o-mini".to_string() }
fn default_openai_url() -> String { "https://api.openai.com/v1".to_string() }
fn default_llm_timeout() -> u64 { 30 }
fn default_db_path() -> String { "data/oneclaw.db".to_string() }
fn default_channels() -> Vec<String> { vec!["cli".to_string()] }
fn default_polling_timeout() -> u64 { 30 }
fn default_mqtt_port() -> u16 { 1883 }
fn default_mqtt_client_id() -> String { format!("oneclaw-{}", std::process::id()) }
fn default_mqtt_publish_prefix() -> String { "oneclaw/alerts".into() }
fn default_mqtt_keepalive() -> u64 { 30 }

impl OneClawConfig {
    /// Load config from a TOML file.
    pub fn load(path: impl AsRef<Path>) -> crate::error::Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| crate::error::OneClawError::Config(
                format!("Failed to read config: {}", e)
            ))?;
        let config: Self = toml::from_str(&content)
            .map_err(|e| crate::error::OneClawError::Config(
                format!("Failed to parse config: {}", e)
            ))?;
        Ok(config)
    }

    /// Load with defaults (no file needed).
    pub fn default_config() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = OneClawConfig::default_config();
        assert!(config.security.deny_by_default);
        assert!(config.security.pairing_required);
        assert!(config.security.workspace_only);
    }

    #[test]
    fn test_load_from_toml_string() {
        let toml_str = r#"
[security]
deny_by_default = true
pairing_required = false

[runtime]
name = "test-agent"
log_level = "debug"
"#;
        let config: OneClawConfig = toml::from_str(toml_str).unwrap();
        assert!(config.security.deny_by_default);
        assert!(!config.security.pairing_required);
        assert_eq!(config.runtime.name, "test-agent");
    }

    #[test]
    fn test_full_config_parse() {
        let toml_str = r#"
[security]
deny_by_default = true
pairing_required = true

[runtime]
name = "my-agent"
log_level = "debug"

[providers]
default = "ollama"

[providers.ollama]
url = "http://localhost:11434"
model = "llama3.2:3b"

[providers.openai]
model = "gpt-4o"
base_url = "https://api.openai.com/v1"

[memory]
backend = "sqlite"
db_path = "data/oneclaw.db"

[channels]
active = ["cli", "mqtt"]
"#;
        let config: OneClawConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.providers.default, "ollama");
        assert_eq!(config.providers.ollama.model, "llama3.2:3b");
        assert_eq!(config.memory.backend, "sqlite");
        assert_eq!(config.channels.active, vec!["cli", "mqtt"]);
    }

    #[test]
    fn test_config_defaults_when_sections_missing() {
        let toml_str = r#"
[runtime]
name = "minimal"
"#;
        let config: OneClawConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.providers.default, "noop");
        assert_eq!(config.memory.backend, "sqlite");
        assert_eq!(config.channels.active, vec!["cli"]);
        assert!(config.security.deny_by_default);
    }

    #[test]
    fn test_mask_key_normal() {
        assert_eq!(mask_key("sk-1234567890abcdef"), "sk-1...cdef");
    }

    #[test]
    fn test_mask_key_short() {
        assert_eq!(mask_key("short"), "****");
        assert_eq!(mask_key("12345678"), "****");
    }

    #[test]
    fn test_mask_key_empty() {
        assert_eq!(mask_key(""), "(empty)");
    }

    #[test]
    fn test_telegram_config_parse_and_masked_debug() {
        let toml_str = r#"
[runtime]
name = "test"

[telegram]
bot_token = "123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11"
allowed_chat_ids = [12345, 67890]
polling_timeout = 60
"#;
        let config: OneClawConfig = toml::from_str(toml_str).unwrap();
        let tg = config.telegram.expect("telegram config should be present");
        assert_eq!(tg.bot_token, "123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11");
        assert_eq!(tg.allowed_chat_ids, vec![12345, 67890]);
        assert_eq!(tg.polling_timeout, 60);

        // Debug should mask bot_token
        let debug_output = format!("{:?}", tg);
        assert!(!debug_output.contains("123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11"));
        assert!(debug_output.contains("1234...ew11"));
    }

    #[test]
    fn test_telegram_config_absent_is_none() {
        let toml_str = r#"
[runtime]
name = "minimal"
"#;
        let config: OneClawConfig = toml::from_str(toml_str).unwrap();
        assert!(config.telegram.is_none());
    }

    #[test]
    fn test_mqtt_config_parse_and_defaults() {
        let toml_str = r#"
[runtime]
name = "test"

[mqtt]
host = "mqtt.local"
subscribe_topics = ["sensors/#", "devices/#"]
"#;
        let config: OneClawConfig = toml::from_str(toml_str).unwrap();
        let mqtt = config.mqtt.expect("mqtt config should be present");
        assert_eq!(mqtt.host, "mqtt.local");
        assert_eq!(mqtt.port, 1883);
        assert!(!mqtt.client_id.is_empty());
        assert_eq!(mqtt.subscribe_topics.len(), 2);
        assert_eq!(mqtt.publish_prefix, "oneclaw/alerts");
        assert_eq!(mqtt.keep_alive_secs, 30);
        assert!(mqtt.username.is_none());
        assert!(mqtt.password.is_none());
    }

    #[test]
    fn test_mqtt_config_full_parse() {
        let toml_str = r#"
[mqtt]
host = "mqtt.example.com"
port = 8883
client_id = "oneclaw-01"
subscribe_topics = ["sensors/#"]
publish_prefix = "oneclaw/alerts"
username = "device"
password = "secret"
keep_alive_secs = 60
"#;
        let config: OneClawConfig = toml::from_str(toml_str).unwrap();
        let mqtt = config.mqtt.unwrap();
        assert_eq!(mqtt.port, 8883);
        assert_eq!(mqtt.client_id, "oneclaw-01");
        assert_eq!(mqtt.username.as_deref(), Some("device"));
        assert_eq!(mqtt.keep_alive_secs, 60);
    }

    #[test]
    fn test_mqtt_password_masked_in_debug() {
        let toml_str = r#"
[mqtt]
host = "localhost"
password = "super-secret-mqtt-password"
"#;
        let config: OneClawConfig = toml::from_str(toml_str).unwrap();
        let mqtt = config.mqtt.unwrap();
        let debug_str = format!("{:?}", mqtt);
        assert!(!debug_str.contains("super-secret-mqtt-password"),
            "Password should be masked in debug: {}", debug_str);
        assert!(debug_str.contains("***"));
    }

    #[test]
    fn test_mqtt_config_absent_is_none() {
        let toml_str = r#"
[runtime]
name = "no-mqtt"
"#;
        let config: OneClawConfig = toml::from_str(toml_str).unwrap();
        assert!(config.mqtt.is_none());
    }

    #[test]
    fn test_security_persist_defaults() {
        let config = OneClawConfig::default_config();
        assert!(config.security.persist_pairing);
        assert_eq!(config.security.persist_path, "data/security.db");
    }

    #[test]
    fn test_security_persist_config_parse() {
        let toml_str = r#"
[security]
persist_pairing = false
persist_path = "custom/path.db"
"#;
        let config: OneClawConfig = toml::from_str(toml_str).unwrap();
        assert!(!config.security.persist_pairing);
        assert_eq!(config.security.persist_path, "custom/path.db");
    }

    #[test]
    fn test_provider_config_defaults() {
        let config = OneClawConfig::default_config();
        assert_eq!(config.provider.primary, "anthropic");
        assert_eq!(config.provider.model, "claude-sonnet-4-20250514");
        assert_eq!(config.provider.max_tokens, 1024);
        assert!((config.provider.temperature - 0.3).abs() < f32::EPSILON);
        assert!(config.provider.api_key.is_none());
        assert!(config.provider.fallback.is_empty());
        assert_eq!(config.provider.max_retries, 1);
        assert!(config.provider.keys.is_empty());
        assert!(config.provider.ollama_endpoint.is_none());
        assert!(config.provider.ollama_model.is_none());
    }

    #[test]
    fn test_provider_config_custom() {
        let toml_str = r#"
[provider]
primary = "openai"
model = "gpt-4o"
max_tokens = 2048
temperature = 0.7
api_key = "sk-test-key"
fallback = ["ollama", "deepseek"]
"#;
        let config: OneClawConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.provider.primary, "openai");
        assert_eq!(config.provider.model, "gpt-4o");
        assert_eq!(config.provider.max_tokens, 2048);
        assert!((config.provider.temperature - 0.7).abs() < f32::EPSILON);
        assert_eq!(config.provider.api_key.as_deref(), Some("sk-test-key"));
        assert_eq!(config.provider.fallback, vec!["ollama", "deepseek"]);
    }

    #[test]
    fn test_provider_config_absent_uses_defaults() {
        let toml_str = r#"
[runtime]
name = "no-provider-section"
"#;
        let config: OneClawConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.provider.primary, "anthropic");
        assert_eq!(config.provider.model, "claude-sonnet-4-20250514");
    }

    #[test]
    fn test_provider_config_full_toml_with_keys_and_fallback() {
        let toml_str = r#"
[provider]
primary = "anthropic"
model = "claude-sonnet-4-20250514"
max_tokens = 2048
temperature = 0.5
api_key = "sk-global-key"
fallback = ["openai", "ollama"]
max_retries = 3
ollama_endpoint = "http://192.168.1.100:11434"
ollama_model = "qwen2.5:3b"

[provider.keys]
openai = "sk-openai-key-abc"
google = "AIza-google-key-xyz"
"#;
        let config: OneClawConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.provider.primary, "anthropic");
        assert_eq!(config.provider.model, "claude-sonnet-4-20250514");
        assert_eq!(config.provider.max_tokens, 2048);
        assert!((config.provider.temperature - 0.5).abs() < f32::EPSILON);
        assert_eq!(config.provider.api_key.as_deref(), Some("sk-global-key"));
        assert_eq!(config.provider.fallback, vec!["openai", "ollama"]);
        assert_eq!(config.provider.max_retries, 3);
        assert_eq!(config.provider.ollama_endpoint.as_deref(), Some("http://192.168.1.100:11434"));
        assert_eq!(config.provider.ollama_model.as_deref(), Some("qwen2.5:3b"));
        assert_eq!(config.provider.keys.len(), 2);
        assert_eq!(config.provider.keys.get("openai").unwrap(), "sk-openai-key-abc");
        assert_eq!(config.provider.keys.get("google").unwrap(), "AIza-google-key-xyz");
    }

    #[test]
    fn test_provider_config_display() {
        let config = ProviderConfigToml::default();
        let display = format!("{}", config);
        assert!(display.contains("anthropic"));
        assert!(display.contains("claude-sonnet-4-20250514"));
        assert!(display.contains("1024"));
        assert!(display.contains("0.3"));
    }

    #[test]
    fn test_openai_config_debug_hides_key() {
        let config = OpenAIConfig {
            api_key: "sk-super-secret-key-12345".to_string(),
            ..Default::default()
        };
        let debug_output = format!("{:?}", config);
        assert!(!debug_output.contains("sk-super-secret-key-12345"),
            "Debug should not contain raw key: {}", debug_output);
        assert!(debug_output.contains("sk-s...2345"),
            "Debug should contain masked key: {}", debug_output);
    }
}
