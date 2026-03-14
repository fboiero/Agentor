//! Communication channel abstraction for multi-platform messaging.
//!
//! Provides a unified [`Channel`] trait and concrete implementations for
//! various messaging platforms, along with a manager for routing messages
//! across multiple channels.
//!
//! # Main types
//!
//! - [`Channel`] — Trait for sending and receiving messages on a platform.
//! - [`ChannelManager`] — Routes messages to the appropriate channel.
//! - [`WebChatChannel`] — Built-in web chat channel implementation.

/// Core channel trait and message types.
pub mod channel;
/// Discord channel integration.
pub mod discord;
/// Channel manager for multi-channel routing.
pub mod manager;
/// Slack channel integration.
pub mod slack;
/// Telegram channel integration.
pub mod telegram;
/// Web chat channel implementation.
pub mod webchat;

pub use channel::{Channel, ChannelEvent, ChannelMessage};
pub use discord::DiscordChannel;
pub use manager::ChannelManager;
pub use slack::SlackChannel;
pub use telegram::TelegramChannel;
pub use webchat::WebChatChannel;
