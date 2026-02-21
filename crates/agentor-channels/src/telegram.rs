use crate::channel::{Channel, ChannelEvent, ChannelMessage};
use agentor_core::{AgentorError, AgentorResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

/// Telegram Bot API channel adapter.
///
/// Uses the Telegram Bot HTTP API for sending messages and long-polling
/// (`getUpdates`) for receiving them. Incoming messages are forwarded
/// through a `tokio::sync::mpsc` channel as [`ChannelEvent`]s.
pub struct TelegramChannel {
    bot_token: String,
    client: reqwest::Client,
    event_tx: mpsc::Sender<ChannelEvent>,
    event_rx: Option<mpsc::Receiver<ChannelEvent>>,
}

// ── Telegram API response types ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TelegramResponse<T> {
    ok: bool,
    #[serde(default)]
    description: Option<String>,
    result: Option<T>,
}

#[derive(Debug, Deserialize)]
struct TelegramUpdate {
    update_id: i64,
    message: Option<TelegramMessagePayload>,
}

#[derive(Debug, Deserialize)]
struct TelegramMessagePayload {
    #[allow(dead_code)]
    message_id: i64,
    from: Option<TelegramUser>,
    chat: TelegramChat,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TelegramUser {
    id: i64,
    #[allow(dead_code)]
    first_name: String,
}

#[derive(Debug, Deserialize)]
struct TelegramChat {
    id: i64,
}

#[derive(Debug, Serialize)]
struct SendMessageRequest<'a> {
    chat_id: &'a str,
    text: &'a str,
}

// ── Implementation ──────────────────────────────────────────────────────────

impl TelegramChannel {
    /// Create a new `TelegramChannel`.
    ///
    /// * `bot_token` – The bot token obtained from @BotFather.
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

    /// Start long-polling the Telegram `getUpdates` endpoint.
    ///
    /// This method runs indefinitely, forwarding every incoming text
    /// message as a [`ChannelEvent::MessageReceived`] through the mpsc
    /// channel. It should be spawned onto a Tokio task.
    pub async fn poll_updates(&self) -> AgentorResult<()> {
        let mut offset: Option<i64> = None;

        loop {
            let url = self.api_url("getUpdates");

            let mut params: Vec<(&str, String)> = vec![
                ("timeout", "30".to_string()),
            ];
            if let Some(off) = offset {
                params.push(("offset", off.to_string()));
            }

            let response = self
                .client
                .get(&url)
                .query(&params)
                .send()
                .await
                .map_err(|e| AgentorError::Channel(format!("Telegram poll error: {e}")))?;

            let body: TelegramResponse<Vec<TelegramUpdate>> = response
                .json()
                .await
                .map_err(|e| AgentorError::Channel(format!("Telegram parse error: {e}")))?;

            if !body.ok {
                return Err(AgentorError::Channel(format!(
                    "Telegram API error: {}",
                    body.description.unwrap_or_default()
                )));
            }

            if let Some(updates) = body.result {
                for update in updates {
                    // Advance the offset so we do not receive this update again.
                    offset = Some(update.update_id + 1);

                    if let Some(msg) = update.message {
                        if let Some(text) = msg.text {
                            let channel_message = ChannelMessage {
                                channel_id: msg.chat.id.to_string(),
                                sender_id: msg
                                    .from
                                    .map(|u| u.id.to_string())
                                    .unwrap_or_default(),
                                content: text,
                                session_id: None,
                            };

                            // Best-effort send; if the receiver is dropped we stop.
                            if self
                                .event_tx
                                .send(ChannelEvent::MessageReceived(channel_message))
                                .await
                                .is_err()
                            {
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    fn api_url(&self, method: &str) -> String {
        format!("https://api.telegram.org/bot{}/{}", self.bot_token, method)
    }
}

#[async_trait]
impl Channel for TelegramChannel {
    fn name(&self) -> &str {
        "telegram"
    }

    async fn send(&self, message: ChannelMessage) -> AgentorResult<()> {
        let url = self.api_url("sendMessage");

        let payload = SendMessageRequest {
            chat_id: &message.channel_id,
            text: &message.content,
        };

        let response = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| AgentorError::Channel(format!("Telegram send error: {e}")))?;

        let body: TelegramResponse<serde_json::Value> = response
            .json()
            .await
            .map_err(|e| AgentorError::Channel(format!("Telegram parse error: {e}")))?;

        if !body.ok {
            return Err(AgentorError::Channel(format!(
                "Telegram sendMessage failed: {}",
                body.description.unwrap_or_default()
            )));
        }

        Ok(())
    }
}
