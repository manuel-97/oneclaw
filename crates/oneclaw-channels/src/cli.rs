//! CLI Channel — Terminal-based I/O
//! Always included. The simplest channel for testing and direct interaction.

use oneclaw_core::channel::traits::{Channel, IncomingMessage, OutgoingMessage};
use oneclaw_core::error::{OneClawError, Result};
use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// CLI channel that reads from stdin and writes to stdout.
pub struct CliChannel {
    prompt: String,
}

impl CliChannel {
    /// Create a new CLI channel with default prompt "oneclaw> ".
    pub fn new() -> Self {
        Self { prompt: "oneclaw> ".to_string() }
    }

    /// Create a CLI channel with a custom prompt.
    pub fn with_prompt(prompt: impl Into<String>) -> Self {
        Self { prompt: prompt.into() }
    }
}

impl Default for CliChannel {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Channel for CliChannel {
    fn name(&self) -> &str { "cli" }

    async fn receive(&self) -> Result<Option<IncomingMessage>> {
        let mut stdout = tokio::io::stdout();
        stdout.write_all(self.prompt.as_bytes()).await
            .map_err(|e| OneClawError::Channel(format!("stdout write: {}", e)))?;
        stdout.flush().await
            .map_err(|e| OneClawError::Channel(format!("stdout flush: {}", e)))?;

        let stdin = tokio::io::stdin();
        let mut reader = BufReader::new(stdin);
        let mut line = String::new();
        match reader.read_line(&mut line).await {
            Ok(0) => Err(OneClawError::Channel("EOF".into())), // EOF — signal loop to stop
            Ok(_) => {
                let content = line.trim().to_string();
                if content.is_empty() {
                    return Ok(None);
                }
                Ok(Some(IncomingMessage {
                    source: "cli".into(),
                    content,
                    timestamp: chrono::Utc::now(),
                }))
            }
            Err(e) => Err(OneClawError::Channel(format!("stdin read: {}", e))),
        }
    }

    async fn send(&self, message: &OutgoingMessage) -> Result<()> {
        let mut stdout = tokio::io::stdout();
        let data = format!("{}\n", message.content);
        stdout.write_all(data.as_bytes()).await
            .map_err(|e| OneClawError::Channel(format!("stdout write: {}", e)))?;
        stdout.flush().await
            .map_err(|e| OneClawError::Channel(format!("stdout flush: {}", e)))?;
        Ok(())
    }
}
