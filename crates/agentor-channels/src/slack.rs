use crate::channel::{Channel, ChannelEvent, ChannelMessage};
use agentor_core::{AgentorError, AgentorResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

/// Slack channel adapter.
///
/// Uses the Slack Web API (`chat.postMessage`) for outbound messages.
/// Inbound messages can be received either through an incoming-webhook
/// handler ([`handle_webhook_event`]) or (in the future) through Slack
/// Socket Mode ([`start_socket_mode`]).
pub struct SlackChannel {
    bot_token: String,
    client: reqwest::Client,
    event_tx: mpsc::Sender<ChannelEvent>,
    event_rx: Option<mpsc::Receiver<ChannelEvent>>,
}

// ── Slack API types ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct PostMessageRequest<'a> {
    channel: &'a str,
    text: &'a str,
}

#[derive(Debug, Deserialize)]
struct SlackApiResponse {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
}

/// Represents an incoming Slack event (simplified).
///
/// In production this would match the full Slack Events API payload; here
/// we keep only the fields required by the adapter.
#[derive(Debug, Deserialize)]
pub struct SlackEventPayload {
    pub event: Option<SlackEvent>,
}

#[derive(Debug, Deserialize)]
pub struct SlackEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub channel: Option<String>,
    pub user: Option<String>,
    pub text: Option<String>,
}

// ── Implementation ──────────────────────────────────────────────────────────

impl SlackChannel {
    /// Create a new `SlackChannel`.
    ///
    /// * `bot_token` – A Slack Bot User OAuth token (`xoxb-...`).
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

    /// Handle an incoming webhook event from the Slack Events API.
    ///
    /// Call this from your HTTP server's event endpoint. The method
    /// parses the payload and forwards `message`-type events through
    /// the internal mpsc channel.
    pub async fn handle_webhook_event(&self, payload: SlackEventPayload) -> AgentorResult<()> {
        if let Some(event) = payload.event {
            if event.event_type == "message" {
                if let (Some(channel), Some(text)) = (event.channel, event.text) {
                    let message = ChannelMessage {
                        channel_id: channel,
                        sender_id: event.user.unwrap_or_default(),
                        content: text,
                        session_id: None,
                    };

                    self.event_tx
                        .send(ChannelEvent::MessageReceived(message))
                        .await
                        .map_err(|e| {
                            AgentorError::Channel(format!("Slack event forward error: {e}"))
                        })?;
                }
            }
        }

        Ok(())
    }

    /// Placeholder for Slack Socket Mode support.
    ///
    /// Socket Mode allows your app to receive events over a WebSocket
    /// connection instead of requiring a publicly reachable HTTP
    /// endpoint. This method is not yet implemented.
    pub async fn start_socket_mode(&self, _app_token: &str) -> AgentorResult<()> {
        // TODO: Implement Slack Socket Mode using the `connections.open`
        // API and a WebSocket client.
        Err(AgentorError::Channel(
            "Slack Socket Mode is not yet implemented".to_string(),
        ))
    }
}

#[async_trait]
impl Channel for SlackChannel {
    fn name(&self) -> &str {
        "slack"
    }

    async fn send(&self, message: ChannelMessage) -> AgentorResult<()> {
        let payload = PostMessageRequest {
            channel: &message.channel_id,
            text: &message.content,
        };

        let response = self
            .client
            .post("https://slack.com/api/chat.postMessage")
            .bearer_auth(&self.bot_token)
            .json(&payload)
            .send()
            .await
            .map_err(|e| AgentorError::Channel(format!("Slack send error: {e}")))?;

        let body: SlackApiResponse = response
            .json()
            .await
            .map_err(|e| AgentorError::Channel(format!("Slack parse error: {e}")))?;

        if !body.ok {
            return Err(AgentorError::Channel(format!(
                "Slack chat.postMessage failed: {}",
                body.error.unwrap_or_default()
            )));
        }

        Ok(())
    }
}
