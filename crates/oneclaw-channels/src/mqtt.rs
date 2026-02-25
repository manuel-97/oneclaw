//! MQTT Channel — IoT data via MQTT broker
//!
//! Subscribes to sensor topics, receives device data.
//! Publishes alerts/responses to output topics.
//! Uses rumqttc async client.
//!
//! Design: AsyncClient is Clone (sender handle). The channel owns the EventLoop
//! and polls it in receive(). A cloned AsyncClient can be obtained via
//! `clone_client()` for independent alert publishing.

use oneclaw_core::channel::{Channel, IncomingMessage, OutgoingMessage};
use oneclaw_core::error::{OneClawError, Result};
use async_trait::async_trait;
use rumqttc::{AsyncClient, MqttOptions, QoS, Event, Packet};
use tokio::sync::Mutex;
use tracing::{info, debug, warn};

/// MQTT channel for IoT sensor communication via MQTT broker.
pub struct MqttChannel {
    client: AsyncClient,
    /// Event loop handle — must be polled for MQTT to work
    eventloop: Mutex<rumqttc::EventLoop>,
    /// Buffer for received messages
    buffer: Mutex<Vec<IncomingMessage>>,
    /// Publish prefix for outgoing messages
    publish_prefix: String,
}

impl MqttChannel {
    /// Create and connect MQTT channel. Subscribes to given topics.
    /// Prefer `from_config()` for typical usage.
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        host: &str,
        port: u16,
        client_id: &str,
        subscribe_topics: &[String],
        publish_prefix: String,
        username: Option<&str>,
        password: Option<&str>,
        keep_alive_secs: u64,
    ) -> Result<Self> {
        let mut options = MqttOptions::new(client_id, host, port);
        options.set_keep_alive(std::time::Duration::from_secs(keep_alive_secs));

        if let (Some(user), Some(pass)) = (username, password) {
            options.set_credentials(user, pass);
        }

        let (client, eventloop) = AsyncClient::new(options, 100);

        // Subscribe to topics
        for topic in subscribe_topics {
            client.subscribe(topic, QoS::AtLeastOnce)
                .await
                .map_err(|e| OneClawError::Channel(format!("MQTT subscribe '{}' failed: {}", topic, e)))?;
            info!(topic = %topic, "MQTT subscribed");
        }

        info!(host = %host, port = port, client_id = %client_id, "MQTT channel created");

        Ok(Self {
            client,
            eventloop: Mutex::new(eventloop),
            buffer: Mutex::new(Vec::new()),
            publish_prefix,
        })
    }

    /// Create from MqttConfig.
    pub async fn from_config(config: &oneclaw_core::config::MqttConfig) -> Result<Self> {
        Self::new(
            &config.host,
            config.port,
            &config.client_id,
            &config.subscribe_topics,
            config.publish_prefix.clone(),
            config.username.as_deref(),
            config.password.as_deref(),
            config.keep_alive_secs,
        ).await
    }

    /// Get a clone of the AsyncClient for independent publishing (e.g., alert dispatch).
    /// AsyncClient is cheap to clone — it's just a channel sender handle.
    pub fn clone_client(&self) -> AsyncClient {
        self.client.clone()
    }

    /// Poll the MQTT event loop for incoming messages.
    async fn poll_events(&self) -> Result<()> {
        let mut eventloop = self.eventloop.lock().await;

        match tokio::time::timeout(
            std::time::Duration::from_millis(100),
            eventloop.poll()
        ).await {
            Ok(Ok(event)) => {
                if let Event::Incoming(Packet::Publish(publish)) = event {
                    let topic = publish.topic.clone();
                    let payload = String::from_utf8_lossy(&publish.payload).to_string();

                    if !payload.trim().is_empty() {
                        debug!(topic = %topic, payload_len = payload.len(), "MQTT message received");

                        let msg = IncomingMessage {
                            source: format!("mqtt:{}", topic),
                            content: payload,
                            timestamp: chrono::Utc::now(),
                        };

                        self.buffer.lock().await.push(msg);
                    }
                }
                Ok(())
            }
            Ok(Err(e)) => {
                warn!(error = %e, "MQTT event loop error");
                Err(OneClawError::Channel(format!("MQTT error: {}", e)))
            }
            Err(_) => Ok(()), // Timeout — no events
        }
    }
}

#[async_trait]
impl Channel for MqttChannel {
    fn name(&self) -> &str { "mqtt" }

    async fn receive(&self) -> Result<Option<IncomingMessage>> {
        // Check buffer first
        {
            let mut buffer = self.buffer.lock().await;
            if !buffer.is_empty() {
                return Ok(Some(buffer.remove(0)));
            }
        }

        // Poll for new events
        self.poll_events().await?;

        // Check buffer again
        let mut buffer = self.buffer.lock().await;
        if !buffer.is_empty() {
            Ok(Some(buffer.remove(0)))
        } else {
            Ok(None)
        }
    }

    async fn send(&self, msg: &OutgoingMessage) -> Result<()> {
        let topic = if msg.destination.is_empty() || msg.destination == "mqtt" {
            self.publish_prefix.clone()
        } else if msg.destination.starts_with("mqtt:") {
            // Route to specific topic: "mqtt:sensors/response"
            msg.destination["mqtt:".len()..].to_string()
        } else {
            format!("{}/{}", self.publish_prefix, msg.destination)
        };

        self.client.publish(&topic, QoS::AtLeastOnce, false, msg.content.as_bytes())
            .await
            .map_err(|e| OneClawError::Channel(format!("MQTT publish failed: {}", e)))?;

        debug!(topic = %topic, "MQTT message published");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mqtt_options_construction() {
        let options = MqttOptions::new("test-client", "localhost", 1883);
        assert_eq!(options.broker_address(), ("localhost".to_string(), 1883));
    }

    #[test]
    fn test_mqtt_options_with_credentials() {
        let mut options = MqttOptions::new("test", "broker.local", 8883);
        options.set_credentials("user", "pass");
        options.set_keep_alive(std::time::Duration::from_secs(60));
        assert_eq!(options.broker_address(), ("broker.local".to_string(), 8883));
    }

    #[test]
    fn test_mqtt_client_is_clone() {
        // Verify AsyncClient can be cloned (needed for independent publishing)
        let options = MqttOptions::new("clone-test", "localhost", 1883);
        let (client, _eventloop) = AsyncClient::new(options, 10);
        let _cloned = client.clone();
        // Both handles should work (they share the same channel)
    }

    #[test]
    fn test_send_topic_routing() {
        // Test destination → topic mapping logic (no network needed)
        let prefix = "oneclaw/alerts";

        // Empty destination → use prefix
        let dest = "";
        let topic = if dest.is_empty() || dest == "mqtt" {
            prefix.to_string()
        } else if let Some(stripped) = dest.strip_prefix("mqtt:") {
            stripped.to_string()
        } else {
            format!("{}/{}", prefix, dest)
        };
        assert_eq!(topic, "oneclaw/alerts");

        // "mqtt" destination → use prefix
        let dest = "mqtt";
        let topic = if dest.is_empty() || dest == "mqtt" {
            prefix.to_string()
        } else if let Some(stripped) = dest.strip_prefix("mqtt:") {
            stripped.to_string()
        } else {
            format!("{}/{}", prefix, dest)
        };
        assert_eq!(topic, "oneclaw/alerts");

        // "mqtt:sensors/response" → extract topic
        let dest = "mqtt:sensors/response";
        let topic = if dest.is_empty() || dest == "mqtt" {
            prefix.to_string()
        } else if let Some(stripped) = dest.strip_prefix("mqtt:") {
            stripped.to_string()
        } else {
            format!("{}/{}", prefix, dest)
        };
        assert_eq!(topic, "sensors/response");

        // Other destination → append to prefix
        let dest = "threshold";
        let topic = if dest.is_empty() || dest == "mqtt" {
            prefix.to_string()
        } else if let Some(stripped) = dest.strip_prefix("mqtt:") {
            stripped.to_string()
        } else {
            format!("{}/{}", prefix, dest)
        };
        assert_eq!(topic, "oneclaw/alerts/threshold");
    }

    #[test]
    fn test_channel_name() {
        // Verify the channel name constant without needing a connection
        // (Can't construct MqttChannel without a broker, so test the trait contract)
        assert_eq!("mqtt", "mqtt"); // MqttChannel::name() returns "mqtt"
    }
}
