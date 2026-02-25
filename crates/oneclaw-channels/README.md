# oneclaw-channels

Channel implementations for OneClaw I/O.

Implements the `Channel` trait from `oneclaw-core` for various communication interfaces.

## Channels

- **CliChannel** — Interactive command-line interface with configurable prompt. Default channel for human interaction.
- **TcpChannel** — Line-based TCP socket server for IoT sensor data. Accepts one client at a time. Default port: 9100.

## Planned

- **MqttChannel** — MQTT pub/sub for sensor networks (placeholder)
- **TelegramChannel** — Telegram bot interface (placeholder)

## Usage

```rust
use oneclaw_channels::{CliChannel, TcpChannel};

let cli = CliChannel::with_prompt("agent> ");
let tcp = TcpChannel::new("0.0.0.0:9100")?;
```
