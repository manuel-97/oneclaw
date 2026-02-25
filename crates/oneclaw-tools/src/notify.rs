//! Notify Tool — Send notifications via file-based alert log
//!
//! Appends timestamped entries to an alert log file and logs via tracing.
//! Provides durable notification records for system alerts.

use oneclaw_core::tool::{Tool, ToolInfo, ToolParam, ToolResult};
use oneclaw_core::error::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::info;

/// Tool that sends notifications via file-based alert log.
pub struct NotifyTool {
    /// Path to the alert log file
    alert_log_path: PathBuf,
}

impl NotifyTool {
    /// Create a new `NotifyTool` with default alert log path.
    pub fn new() -> Self {
        Self {
            alert_log_path: PathBuf::from("data/alerts.log"),
        }
    }

    /// Create a new `NotifyTool` with a custom alert log path.
    pub fn with_log_path(path: impl Into<PathBuf>) -> Self {
        Self {
            alert_log_path: path.into(),
        }
    }

    /// Append an alert entry to the log file.
    fn append_to_log(&self, urgency: &str, recipient: &str, message: &str) {
        use std::io::Write;
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let entry = format!("[{}] [{}] {}: {}\n", timestamp, urgency, recipient, message);

        if let Some(parent) = self.alert_log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        match std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.alert_log_path)
        {
            Ok(mut file) => {
                if let Err(e) = file.write_all(entry.as_bytes()) {
                    tracing::warn!(path = %self.alert_log_path.display(), "Failed to write alert log: {}", e);
                }
            }
            Err(e) => {
                tracing::warn!(path = %self.alert_log_path.display(), "Failed to open alert log: {}", e);
            }
        }
    }
}

impl Default for NotifyTool {
    fn default() -> Self { Self::new() }
}

impl Tool for NotifyTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "notify".into(),
            description: "Send a notification (file log + tracing)".into(),
            params: vec![
                ToolParam { name: "message".into(), description: "Notification message".into(), required: true },
                ToolParam { name: "urgency".into(), description: "low, normal, high, critical".into(), required: false },
                ToolParam { name: "recipient".into(), description: "Who to notify (default: system)".into(), required: false },
            ],
            category: "notify".into(),
        }
    }

    fn execute(&self, params: &HashMap<String, String>) -> Result<ToolResult> {
        let message = params.get("message")
            .ok_or_else(|| oneclaw_core::error::OneClawError::Tool("Missing 'message' param".into()))?;
        let urgency = params.get("urgency").map(|s| s.as_str()).unwrap_or("normal");
        let recipient = params.get("recipient").map(|s| s.as_str()).unwrap_or("system");

        // Log via tracing
        info!(
            urgency = %urgency,
            recipient = %recipient,
            "NOTIFICATION: {}",
            message
        );

        // Append to persistent alert log file
        self.append_to_log(urgency, recipient, message);

        Ok(ToolResult::ok(format!(
            "Notification sent to {} [{}]: {} (logged to {})",
            recipient, urgency, message, self.alert_log_path.display(),
        ))
        .with_meta("urgency", urgency)
        .with_meta("recipient", recipient)
        .with_meta("log_path", self.alert_log_path.to_string_lossy()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notify_basic() {
        let tmp = std::env::temp_dir().join("oneclaw_test_notify.log");
        let tool = NotifyTool::with_log_path(&tmp);
        let mut params = HashMap::new();
        params.insert("message".into(), "Sensor threshold exceeded: 105.3".into());

        let result = tool.execute(&params).unwrap();
        assert!(result.success);
        assert!(result.output.contains("Notification sent"));

        // Verify file was written
        let content = std::fs::read_to_string(&tmp).unwrap_or_default();
        assert!(content.contains("Sensor threshold exceeded"));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_notify_with_urgency() {
        let tmp = std::env::temp_dir().join("oneclaw_test_notify_urg.log");
        let tool = NotifyTool::with_log_path(&tmp);
        let mut params = HashMap::new();
        params.insert("message".into(), "Emergency!".into());
        params.insert("urgency".into(), "critical".into());
        params.insert("recipient".into(), "doctor".into());

        let result = tool.execute(&params).unwrap();
        assert!(result.output.contains("doctor"));
        assert!(result.output.contains("critical"));

        let content = std::fs::read_to_string(&tmp).unwrap_or_default();
        assert!(content.contains("[critical]"));
        assert!(content.contains("doctor"));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_notify_missing_message() {
        let tool = NotifyTool::new();
        let result = tool.execute(&HashMap::new());
        assert!(result.is_err());
    }
}
