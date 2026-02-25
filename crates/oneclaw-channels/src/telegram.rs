//! Telegram Bot Channel — Long-polling based Telegram Bot API channel
//!
//! Uses the Telegram Bot API via reqwest (async). Receives messages via
//! getUpdates long-polling, sends responses via sendMessage.
//!
//! Design: Lazy connection (no network call in constructor).
//! Whitelist support: if allowed_chat_ids is non-empty, only those chats are accepted.
//! Messages >4000 chars are auto-split at newline boundaries.

use oneclaw_core::channel::{Channel, IncomingMessage, OutgoingMessage};
use oneclaw_core::error::{OneClawError, Result};
use async_trait::async_trait;
use tokio::sync::Mutex;
use std::collections::HashSet;
use tracing::{info, debug, warn};

/// Maximum Telegram message length (API limit is 4096, we use 4000 for safety).
const MAX_MESSAGE_LEN: usize = 4000;

/// A Telegram Bot API channel using long-polling.
pub struct TelegramChannel {
    client: reqwest::Client,
    bot_token: String,
    /// Whitelist of allowed chat IDs (empty = allow all)
    whitelist: HashSet<i64>,
    /// Offset for getUpdates (last_update_id + 1)
    last_update_id: Mutex<Option<i64>>,
    /// Buffered incoming messages
    buffer: Mutex<Vec<IncomingMessage>>,
    /// Last chat_id we received from (for reply routing)
    last_chat_id: Mutex<Option<i64>>,
    /// Long-polling timeout in seconds
    polling_timeout: u64,
}

/// Internal representation of a Telegram update.
#[derive(Debug)]
struct TelegramUpdate {
    update_id: i64,
    chat_id: i64,
    text: String,
}

impl TelegramChannel {
    /// Create a new Telegram channel. No network call — connection is lazy.
    pub fn new(bot_token: &str, allowed_chat_ids: &[i64], polling_timeout: u64) -> Self {
        let whitelist: HashSet<i64> = allowed_chat_ids.iter().copied().collect();
        info!(
            allowed = whitelist.len(),
            timeout = polling_timeout,
            "Telegram channel created (lazy)"
        );
        Self {
            client: reqwest::Client::new(),
            bot_token: bot_token.to_string(),
            whitelist,
            last_update_id: Mutex::new(None),
            buffer: Mutex::new(Vec::new()),
            last_chat_id: Mutex::new(None),
            polling_timeout,
        }
    }

    /// Build a Telegram Bot API URL for the given method.
    fn api_url(&self, method: &str) -> String {
        format!("https://api.telegram.org/bot{}/{}", self.bot_token, method)
    }

    /// Check if a chat_id is allowed by the whitelist.
    fn is_allowed(&self, chat_id: i64) -> bool {
        self.whitelist.is_empty() || self.whitelist.contains(&chat_id)
    }

    /// Verify bot token by calling getMe. Returns bot username on success.
    pub async fn verify_token(&self) -> Result<String> {
        let url = self.api_url("getMe");
        let resp = self.client.get(&url).send().await
            .map_err(|e| OneClawError::Channel(format!("Telegram getMe failed: {}", e)))?;

        let body: serde_json::Value = resp.json().await
            .map_err(|e| OneClawError::Channel(format!("Telegram getMe parse error: {}", e)))?;

        if body["ok"].as_bool() != Some(true) {
            return Err(OneClawError::Channel(
                format!("Telegram getMe returned error: {}", body)
            ));
        }

        let username = body["result"]["username"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        info!(username = %username, "Telegram bot verified");
        Ok(username)
    }

    /// Fetch new updates from Telegram via long-polling.
    async fn get_updates(&self) -> Result<Vec<TelegramUpdate>> {
        let offset = {
            let guard = self.last_update_id.lock().await;
            guard.map(|id| id + 1)
        };

        let mut params = serde_json::json!({
            "timeout": self.polling_timeout,
            "allowed_updates": ["message"],
        });
        if let Some(off) = offset {
            params["offset"] = serde_json::json!(off);
        }

        let url = self.api_url("getUpdates");
        let resp = self.client.post(&url).json(&params).send().await
            .map_err(|e| OneClawError::Channel(format!("Telegram getUpdates failed: {}", e)))?;

        let body: serde_json::Value = resp.json().await
            .map_err(|e| OneClawError::Channel(format!("Telegram getUpdates parse error: {}", e)))?;

        if body["ok"].as_bool() != Some(true) {
            return Err(OneClawError::Channel(
                format!("Telegram getUpdates error: {}", body)
            ));
        }

        let mut updates = Vec::new();
        if let Some(results) = body["result"].as_array() {
            for item in results {
                let update_id = match item["update_id"].as_i64() {
                    Some(id) => id,
                    None => continue,
                };
                let chat_id = match item["message"]["chat"]["id"].as_i64() {
                    Some(id) => id,
                    None => continue,
                };
                let text = match item["message"]["text"].as_str() {
                    Some(t) => t.to_string(),
                    None => continue, // Skip non-text messages
                };
                updates.push(TelegramUpdate { update_id, chat_id, text });
            }
        }

        // Update last_update_id
        if let Some(last) = updates.last() {
            let mut guard = self.last_update_id.lock().await;
            *guard = Some(last.update_id);
        }

        Ok(updates)
    }

    /// Send a message to a specific chat_id, auto-splitting if too long.
    async fn send_message(&self, chat_id: i64, text: &str) -> Result<()> {
        let parts = split_message(text, MAX_MESSAGE_LEN);

        for part in &parts {
            let params = serde_json::json!({
                "chat_id": chat_id,
                "text": part,
            });

            let url = self.api_url("sendMessage");
            let resp = self.client.post(&url).json(&params).send().await
                .map_err(|e| OneClawError::Channel(format!("Telegram sendMessage failed: {}", e)))?;

            let body: serde_json::Value = resp.json().await
                .map_err(|e| OneClawError::Channel(format!("Telegram sendMessage parse error: {}", e)))?;

            if body["ok"].as_bool() != Some(true) {
                warn!(chat_id = chat_id, "Telegram sendMessage error: {}", body);
            }
        }

        Ok(())
    }
}

#[async_trait]
impl Channel for TelegramChannel {
    fn name(&self) -> &str { "telegram" }

    async fn receive(&self) -> Result<Option<IncomingMessage>> {
        // Check buffer first
        {
            let mut buffer = self.buffer.lock().await;
            if let Some(msg) = buffer.pop() {
                return Ok(Some(msg));
            }
        }

        // Long-poll for new updates
        let updates = self.get_updates().await?;

        let mut messages = Vec::new();
        for update in updates {
            if !self.is_allowed(update.chat_id) {
                debug!(chat_id = update.chat_id, "Telegram message from non-whitelisted chat, ignoring");
                continue;
            }

            // Track last chat_id for reply routing
            {
                let mut last = self.last_chat_id.lock().await;
                *last = Some(update.chat_id);
            }

            messages.push(IncomingMessage {
                source: format!("telegram:{}", update.chat_id),
                content: update.text,
                timestamp: chrono::Utc::now(),
            });
        }

        if messages.is_empty() {
            return Ok(None);
        }

        // Return first, buffer rest (reversed for pop order)
        let first = messages.remove(0);
        {
            let mut buffer = self.buffer.lock().await;
            for msg in messages.into_iter().rev() {
                buffer.push(msg);
            }
        }

        Ok(Some(first))
    }

    async fn send(&self, msg: &OutgoingMessage) -> Result<()> {
        // Try to parse chat_id from destination ("telegram:12345")
        let chat_id = if msg.destination.starts_with("telegram:") {
            msg.destination["telegram:".len()..].parse::<i64>().ok()
        } else {
            None
        };

        // Fall back to last_chat_id
        let chat_id = match chat_id {
            Some(id) => id,
            None => {
                let last = self.last_chat_id.lock().await;
                match *last {
                    Some(id) => id,
                    None => {
                        debug!("No Telegram chat_id available, message dropped");
                        return Ok(());
                    }
                }
            }
        };

        self.send_message(chat_id, &msg.content).await
    }
}

/// Split a long message at newline boundaries, respecting max_len.
/// If a single line exceeds max_len, it is hard-cut at max_len.
pub fn split_message(text: &str, max_len: usize) -> Vec<String> {
    if text.len() <= max_len {
        return vec![text.to_string()];
    }

    let mut parts = Vec::new();
    let mut current = String::new();

    for line in text.split('\n') {
        // If adding this line (+ newline) would exceed max_len
        if !current.is_empty() && current.len() + 1 + line.len() > max_len {
            parts.push(current);
            current = String::new();
        }

        // Handle lines that are themselves longer than max_len
        if line.len() > max_len {
            if !current.is_empty() {
                parts.push(current);
                current = String::new();
            }
            // Hard-cut the long line
            let mut remaining = line;
            while remaining.len() > max_len {
                parts.push(remaining[..max_len].to_string());
                remaining = &remaining[max_len..];
            }
            if !remaining.is_empty() {
                current = remaining.to_string();
            }
        } else if current.is_empty() {
            current = line.to_string();
        } else {
            current.push('\n');
            current.push_str(line);
        }
    }

    if !current.is_empty() {
        parts.push(current);
    }

    parts
}

/// Send a one-shot alert message to a Telegram chat. Standalone async function
/// for use from event handlers or pipelines without a full TelegramChannel.
pub async fn send_telegram_alert(
    bot_token: &str,
    chat_id: i64,
    message: &str,
) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("https://api.telegram.org/bot{}/sendMessage", bot_token);

    let parts = split_message(message, MAX_MESSAGE_LEN);
    for part in &parts {
        let params = serde_json::json!({
            "chat_id": chat_id,
            "text": part,
        });

        let resp = client.post(&url).json(&params).send().await
            .map_err(|e| OneClawError::Channel(format!("Telegram alert send failed: {}", e)))?;

        let body: serde_json::Value = resp.json().await
            .map_err(|e| OneClawError::Channel(format!("Telegram alert parse error: {}", e)))?;

        if body["ok"].as_bool() != Some(true) {
            return Err(OneClawError::Channel(
                format!("Telegram alert error: {}", body)
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- split_message tests ---

    #[test]
    fn test_split_message_short() {
        let parts = split_message("hello world", 100);
        assert_eq!(parts, vec!["hello world"]);
    }

    #[test]
    fn test_split_message_at_newline_boundary() {
        let text = "line1\nline2\nline3";
        let parts = split_message(text, 11);
        // "line1\nline2" = 11 chars, fits; "line3" goes to next
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], "line1\nline2");
        assert_eq!(parts[1], "line3");
    }

    #[test]
    fn test_split_message_hard_cut() {
        let text = "a".repeat(10);
        let parts = split_message(&text, 4);
        assert_eq!(parts, vec!["aaaa", "aaaa", "aa"]);
    }

    // --- TelegramUpdate parsing tests ---

    #[test]
    fn test_parse_telegram_update_json() {
        let json = serde_json::json!({
            "update_id": 100,
            "message": {
                "chat": { "id": 12345 },
                "text": "hello bot"
            }
        });

        let update_id = json["update_id"].as_i64().unwrap();
        let chat_id = json["message"]["chat"]["id"].as_i64().unwrap();
        let text = json["message"]["text"].as_str().unwrap();

        assert_eq!(update_id, 100);
        assert_eq!(chat_id, 12345);
        assert_eq!(text, "hello bot");
    }

    #[test]
    fn test_parse_telegram_update_no_text_skipped() {
        let json = serde_json::json!({
            "update_id": 101,
            "message": {
                "chat": { "id": 12345 },
                "sticker": { "file_id": "abc" }
            }
        });

        // No "text" field → should be skipped (as_str returns None)
        assert!(json["message"]["text"].as_str().is_none());
    }

    // --- Whitelist tests ---

    #[test]
    fn test_whitelist_empty_allows_all() {
        let ch = TelegramChannel::new("fake:token", &[], 30);
        assert!(ch.is_allowed(12345));
        assert!(ch.is_allowed(99999));
    }

    #[test]
    fn test_whitelist_restricts() {
        let ch = TelegramChannel::new("fake:token", &[100, 200], 30);
        assert!(ch.is_allowed(100));
        assert!(ch.is_allowed(200));
        assert!(!ch.is_allowed(300));
    }

    // --- API URL test ---

    #[test]
    fn test_api_url_format() {
        let ch = TelegramChannel::new("123:ABC", &[], 30);
        assert_eq!(ch.api_url("getMe"), "https://api.telegram.org/bot123:ABC/getMe");
        assert_eq!(ch.api_url("sendMessage"), "https://api.telegram.org/bot123:ABC/sendMessage");
    }
}
