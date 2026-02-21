use crate::channel::{Channel, ChannelEvent, ChannelMessage};
use agentor_core::{AgentorError, AgentorResult};
use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::mpsc;

const DISCORD_API_BASE: &str = "https://discord.com/api/v10";

/// Discord channel adapter.
///
/// Uses the Discord REST API for sending messages. Inbound messages can
/// be received through the Gateway WebSocket connection once
/// [`start_gateway`] is implemented, or by feeding events manually via
/// [`handle_event`].
pub struct DiscordChannel {
    bot_token: String,
    client: reqwest::Client,
    event_tx: mpsc::Sender<ChannelEvent>,
    event_rx: Option<mpsc::Receiver<ChannelEvent>>,
}

// ── Discord API types ───────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct CreateMessageRequest<'a> {
    content: &'a str,
}

// ── Implementation ──────────────────────────────────────────────────────────

impl DiscordChannel {
    /// Create a new `DiscordChannel`.
    ///
    /// * `bot_token` – The Discord bot token from the Developer Portal.
    /// * `event_buffer` – Capacity of the internal mpsc event buffer.
    pub fn new(bot_token: impl Into<String>, event_buffer: usize) -> Self {
        let (event_tx, event_rx) = mpsc::channel(event_buffer);
        Self {
            bot_token: bot_token.into(),
            client: reqwest::Client::new(),
            event_tx,
            event_rx: Some(event_rx),
        }
    }

    /// Take the receiving half of the event channel.
    ///
    /// This can only be called once; subsequent calls return `None`.
    pub fn take_event_receiver(&mut self) -> Option<mpsc::Receiver<ChannelEvent>> {
        self.event_rx.take()
    }

    /// Feed an externally-received event into the channel.
    ///
    /// Use this when you have your own webhook or gateway handler that
    /// produces [`ChannelMessage`]s. The message is forwarded as a
    /// [`ChannelEvent::MessageReceived`] through the internal mpsc
    /// channel.
    pub async fn handle_event(&self, message: ChannelMessage) -> AgentorResult<()> {
        self.event_tx
            .send(ChannelEvent::MessageReceived(message))
            .await
            .map_err(|e| AgentorError::Channel(format!("Discord event forward error: {e}")))?;
        Ok(())
    }

    /// Placeholder for the Discord Gateway WebSocket connection.
    ///
    /// The Gateway provides real-time events (messages, reactions, etc.)
    /// over a WebSocket. This method is not yet implemented; once it is,
    /// it should:
    ///
    /// 1. Connect to `wss://gateway.discord.gg/?v=10&encoding=json`.
    /// 2. Handle the HELLO / IDENTIFY / HEARTBEAT handshake.
    /// 3. Listen for `MESSAGE_CREATE` events and forward them through
    ///    the mpsc channel.
    pub async fn start_gateway(&self) -> AgentorResult<()> {
        // TODO: Implement Discord Gateway WebSocket connection with
        // heartbeat, identify, and event dispatch.
        Err(AgentorError::Channel(
            "Discord Gateway is not yet implemented".to_string(),
        ))
    }
}

#[async_trait]
impl Channel for DiscordChannel {
    fn name(&self) -> &str {
        "discord"
    }

    async fn send(&self, message: ChannelMessage) -> AgentorResult<()> {
        let url = format!(
            "{}/channels/{}/messages",
            DISCORD_API_BASE, message.channel_id
        );

        let payload = CreateMessageRequest {
            content: &message.content,
        };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bot {}", self.bot_token))
            .json(&payload)
            .send()
            .await
            .map_err(|e| AgentorError::Channel(format!("Discord send error: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            return Err(AgentorError::Channel(format!(
                "Discord create message failed ({status}): {body}"
            )));
        }

        Ok(())
    }
}
