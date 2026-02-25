//! Channel Manager — Multiplexes multiple channels into a single stream
//!
//! Design: Sequential polling across registered channels.
//! Each channel is polled in order. First channel with a message wins.
//! If no channels have messages, returns None (caller should sleep briefly).

use crate::channel::{Channel, IncomingMessage, OutgoingMessage};
use crate::error::{OneClawError, Result};
use tracing::{info, debug};

/// Multiplexes multiple channels into a single stream with sequential polling.
pub struct ChannelManager {
    channels: Vec<Box<dyn Channel>>,
}

impl ChannelManager {
    /// Create a new empty channel manager.
    pub fn new() -> Self {
        Self { channels: Vec::new() }
    }

    /// Add a channel to the manager
    pub fn add_channel(&mut self, channel: Box<dyn Channel>) {
        info!(channel = channel.name(), "Channel registered");
        self.channels.push(channel);
    }

    /// Poll all channels (sequential), return first message found.
    /// Returns None if no channel has a message ready.
    pub async fn receive_any(&self) -> Result<Option<(usize, IncomingMessage)>> {
        for (i, channel) in self.channels.iter().enumerate() {
            match channel.receive().await {
                Ok(Some(msg)) => {
                    debug!(channel = channel.name(), source = %msg.source, "Message received");
                    return Ok(Some((i, msg)));
                }
                Ok(None) => continue,
                Err(e) => {
                    debug!(channel = channel.name(), error = %e, "Channel receive error");
                    continue;
                }
            }
        }
        Ok(None)
    }

    /// Send a message via a specific channel index
    pub async fn send_to(&self, channel_idx: usize, msg: &OutgoingMessage) -> Result<()> {
        if let Some(channel) = self.channels.get(channel_idx) {
            channel.send(msg).await
        } else {
            Err(OneClawError::Channel(
                format!("Channel index {} out of range", channel_idx)
            ))
        }
    }

    /// Send a message to channel by name
    pub async fn send_by_name(&self, name: &str, msg: &OutgoingMessage) -> Result<()> {
        for channel in &self.channels {
            if channel.name() == name {
                return channel.send(msg).await;
            }
        }
        Err(OneClawError::Channel(
            format!("Channel '{}' not found", name)
        ))
    }

    /// Get number of registered channels
    pub fn count(&self) -> usize {
        self.channels.len()
    }

    /// List channel names
    pub fn list(&self) -> Vec<&str> {
        self.channels.iter().map(|c| c.name()).collect()
    }
}

impl Default for ChannelManager {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct MockChannel {
        ch_name: String,
        messages: Mutex<Vec<String>>,
        sent: Mutex<Vec<String>>,
    }

    impl MockChannel {
        fn new(name: &str, messages: Vec<&str>) -> Self {
            Self {
                ch_name: name.into(),
                messages: Mutex::new(messages.into_iter().rev().map(String::from).collect()),
                sent: Mutex::new(vec![]),
            }
        }
    }

    #[async_trait]
    impl Channel for MockChannel {
        fn name(&self) -> &str { &self.ch_name }
        async fn receive(&self) -> Result<Option<IncomingMessage>> {
            let mut msgs = self.messages.lock().unwrap();
            match msgs.pop() {
                Some(content) => Ok(Some(IncomingMessage {
                    source: self.ch_name.clone(),
                    content,
                    timestamp: chrono::Utc::now(),
                })),
                None => Ok(None),
            }
        }
        async fn send(&self, msg: &OutgoingMessage) -> Result<()> {
            self.sent.lock().unwrap().push(msg.content.clone());
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_channel_manager_basic() {
        let mut mgr = ChannelManager::new();
        mgr.add_channel(Box::new(MockChannel::new("cli", vec!["hello"])));
        mgr.add_channel(Box::new(MockChannel::new("tcp", vec!["world"])));

        assert_eq!(mgr.count(), 2);
        assert_eq!(mgr.list(), vec!["cli", "tcp"]);
    }

    #[tokio::test]
    async fn test_receive_any_round_robin() {
        let mut mgr = ChannelManager::new();
        mgr.add_channel(Box::new(MockChannel::new("ch1", vec!["msg1"])));
        mgr.add_channel(Box::new(MockChannel::new("ch2", vec!["msg2"])));

        let (idx, msg) = mgr.receive_any().await.unwrap().unwrap();
        assert_eq!(idx, 0);
        assert_eq!(msg.content, "msg1");

        let (idx, msg) = mgr.receive_any().await.unwrap().unwrap();
        assert_eq!(idx, 1);
        assert_eq!(msg.content, "msg2");

        assert!(mgr.receive_any().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_send_to_channel() {
        let mut mgr = ChannelManager::new();
        let ch = MockChannel::new("test", vec![]);
        mgr.add_channel(Box::new(ch));

        mgr.send_to(0, &OutgoingMessage {
            destination: "test".into(),
            content: "response".into(),
        }).await.unwrap();
    }

    #[tokio::test]
    async fn test_send_to_invalid_index() {
        let mgr = ChannelManager::new();
        let result = mgr.send_to(99, &OutgoingMessage {
            destination: "test".into(),
            content: "response".into(),
        }).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_send_by_name() {
        let mut mgr = ChannelManager::new();
        let ch = MockChannel::new("mytest", vec![]);
        mgr.add_channel(Box::new(ch));

        mgr.send_by_name("mytest", &OutgoingMessage {
            destination: "mytest".into(),
            content: "hello".into(),
        }).await.unwrap();

        let result = mgr.send_by_name("nonexistent", &OutgoingMessage {
            destination: "x".into(),
            content: "x".into(),
        }).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_empty_manager_returns_none() {
        let mgr = ChannelManager::new();
        assert_eq!(mgr.count(), 0);
        assert!(mgr.receive_any().await.unwrap().is_none());
    }
}
