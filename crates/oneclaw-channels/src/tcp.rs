//! TCP Socket Channel — Lightweight IoT-friendly channel
//!
//! Listens on a TCP port. Each line received = one message.
//! Responses sent back to connected client.
//!
//! Protocol: Line-based text (UTF-8). Each line = one message.
//! Newline (\n) terminates each message.
//!
//! Design: Non-blocking accept + read via tokio. Returns None if no data ready.

use oneclaw_core::channel::{Channel, IncomingMessage, OutgoingMessage};
use oneclaw_core::error::{OneClawError, Result};
use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tracing::{info, debug, warn};

/// Line-based TCP channel for lightweight IoT sensor communication.
pub struct TcpChannel {
    listener: TcpListener,
    /// Current connected client (if any)
    client: Mutex<Option<TcpStream>>,
    /// Buffered messages from current client
    buffer: Mutex<Vec<String>>,
    port: u16,
}

impl TcpChannel {
    /// Create a new TCP channel listening on the given address.
    pub async fn new(bind_addr: &str) -> Result<Self> {
        let listener = TcpListener::bind(bind_addr).await
            .map_err(|e| OneClawError::Channel(format!("TCP bind failed on {}: {}", bind_addr, e)))?;

        let port = listener.local_addr()
            .map(|a| a.port())
            .unwrap_or(0);

        info!(addr = %bind_addr, port = port, "TCP Channel listening");

        Ok(Self {
            listener,
            client: Mutex::new(None),
            buffer: Mutex::new(Vec::new()),
            port,
        })
    }

    /// Return the port this channel is listening on.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Try to accept a new connection (non-blocking via short timeout)
    async fn try_accept(&self) -> Option<TcpStream> {
        match tokio::time::timeout(
            std::time::Duration::from_millis(1),
            self.listener.accept(),
        ).await {
            Ok(Ok((stream, addr))) => {
                info!(addr = %addr, "TCP client connected");
                Some(stream)
            }
            Ok(Err(e)) => {
                debug!("TCP accept error: {}", e);
                None
            }
            Err(_) => None, // Timeout — no connection pending
        }
    }

    /// Read available lines from current client (non-blocking)
    async fn read_lines(stream: &mut TcpStream) -> Vec<String> {
        let mut lines = Vec::new();
        // Split the stream to get a read half for buffered reading
        let (reader, _writer) = stream.split();
        let mut buf_reader = BufReader::new(reader);

        loop {
            let mut line = String::new();
            // Use try_read via poll-like approach: read_line with a short timeout
            match tokio::time::timeout(
                std::time::Duration::from_millis(10),
                buf_reader.read_line(&mut line),
            ).await {
                Ok(Ok(0)) => break, // EOF
                Ok(Ok(_)) => {
                    let trimmed = line.trim().to_string();
                    if !trimmed.is_empty() {
                        lines.push(trimmed);
                    }
                }
                Ok(Err(_)) => break,
                Err(_) => break, // Timeout — no more data ready
            }
        }
        lines
    }
}

#[async_trait]
impl Channel for TcpChannel {
    fn name(&self) -> &str { "tcp" }

    async fn receive(&self) -> Result<Option<IncomingMessage>> {
        // Check buffer first
        {
            let mut buffer = self.buffer.lock().await;
            if let Some(line) = buffer.pop() {
                return Ok(Some(IncomingMessage {
                    source: format!("tcp:{}", self.port),
                    content: line,
                    timestamp: chrono::Utc::now(),
                }));
            }
        }

        // Try accept new client
        if let Some(stream) = self.try_accept().await {
            let mut client = self.client.lock().await;
            *client = Some(stream);
        }

        // Read from current client
        let mut client = self.client.lock().await;

        if let Some(stream) = client.as_mut() {
            let lines = Self::read_lines(stream).await;
            if !lines.is_empty() {
                let mut buffer = self.buffer.lock().await;
                // Return first line, push rest into buffer (reversed for pop order)
                let first = lines[0].clone();
                for line in lines.into_iter().skip(1).rev() {
                    buffer.push(line);
                }
                return Ok(Some(IncomingMessage {
                    source: format!("tcp:{}", self.port),
                    content: first,
                    timestamp: chrono::Utc::now(),
                }));
            }
        }

        Ok(None)
    }

    async fn send(&self, msg: &OutgoingMessage) -> Result<()> {
        let mut client = self.client.lock().await;

        if let Some(stream) = client.as_mut() {
            let data = format!("{}\n", msg.content);
            match stream.write_all(data.as_bytes()).await {
                Ok(_) => {
                    let _ = stream.flush().await;
                    Ok(())
                }
                Err(e) => {
                    warn!("TCP send failed (client disconnected?): {}", e);
                    *client = None;
                    Ok(())
                }
            }
        } else {
            debug!("No TCP client connected, message dropped");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream as TokioTcpStream;

    #[tokio::test]
    async fn test_tcp_channel_creation() {
        let ch = TcpChannel::new("127.0.0.1:0").await.unwrap();
        assert!(ch.port() > 0);
        assert_eq!(ch.name(), "tcp");
    }

    #[tokio::test]
    async fn test_tcp_no_client_returns_none() {
        let ch = TcpChannel::new("127.0.0.1:0").await.unwrap();
        let msg = ch.receive().await.unwrap();
        assert!(msg.is_none());
    }

    #[tokio::test]
    async fn test_tcp_send_receive() {
        let ch = TcpChannel::new("127.0.0.1:0").await.unwrap();
        let port = ch.port();

        // Connect a client
        let mut client = TokioTcpStream::connect(format!("127.0.0.1:{}", port)).await.unwrap();

        // Send a message from client
        client.write_all(b"hello from sensor\n").await.unwrap();
        client.flush().await.unwrap();

        // Brief pause for data to arrive
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Receive on channel
        let msg = ch.receive().await.unwrap();
        assert!(msg.is_some(), "Should receive message from TCP client");
        let msg = msg.unwrap();
        assert_eq!(msg.content, "hello from sensor");
        assert!(msg.source.contains("tcp:"));

        // Send response back
        ch.send(&OutgoingMessage {
            destination: msg.source,
            content: "acknowledged".into(),
        }).await.unwrap();

        // Read response on client
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let mut buf = [0u8; 256];
        match tokio::time::timeout(
            std::time::Duration::from_millis(100),
            client.read(&mut buf),
        ).await {
            Ok(Ok(n)) => {
                let response = String::from_utf8_lossy(&buf[..n]);
                assert!(response.contains("acknowledged"), "Client should receive response: '{}'", response);
            }
            _ => panic!("Timed out waiting for response"),
        }
    }

    #[tokio::test]
    async fn test_tcp_client_disconnect_graceful() {
        let ch = TcpChannel::new("127.0.0.1:0").await.unwrap();
        let port = ch.port();

        // Connect and disconnect
        {
            let mut client = TokioTcpStream::connect(format!("127.0.0.1:{}", port)).await.unwrap();
            client.write_all(b"bye\n").await.unwrap();
        } // client drops here

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Should handle gracefully
        let _ = ch.receive().await;

        // Send should not crash even with no client
        ch.send(&OutgoingMessage {
            destination: "tcp".into(),
            content: "orphaned".into(),
        }).await.unwrap();
    }
}
