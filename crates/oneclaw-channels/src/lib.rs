#![warn(missing_docs)]
//! OneClaw Channels — I/O interface implementations

/// Interactive command-line channel.
pub mod cli;
/// Line-based TCP socket channel for IoT devices.
pub mod tcp;
/// MQTT channel for pub/sub messaging.
pub mod mqtt;
/// Telegram bot channel.
pub mod telegram;

pub use cli::CliChannel;
pub use tcp::TcpChannel;
pub use telegram::{TelegramChannel, send_telegram_alert};
pub use mqtt::MqttChannel;
