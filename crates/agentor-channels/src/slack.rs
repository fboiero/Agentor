use crate::channel::{Channel, ChannelEvent, ChannelMessage};
use agentor_core::{AgentorError, AgentorResult};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Slack channel adapter.
///
/// Uses the Slack Web API (`chat.postMessage`) for outbound messages.
/// Inbound messages can be received either through an incoming-webhook
/// handler ([`handle_webhook_event`]) or through Slack Socket Mode
/// ([`start_socket_mode`]).
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

/// Socket Mode connection response.
#[derive(Debug, Deserialize)]
struct ConnectionsOpenResponse {
    ok: bool,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

/// Socket Mode envelope received over WebSocket.
#[derive(Debug, Deserialize)]
struct SocketModeEnvelope {
    envelope_id: String,
    #[serde(rename = "type")]
    envelope_type: String,
    payload: Option<serde_json::Value>,
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
    pub fn take_event_receiver(&mut self) -> Option<mpsc::Receiver<ChannelEvent>> {
        self.event_rx.take()
    }

    /// Handle an incoming webhook event from the Slack Events API.
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

    /// Start Slack Socket Mode for real-time event delivery.
    ///
    /// Socket Mode uses an app-level token (`xapp-...`) to open a WebSocket
    /// connection. Events are received as envelopes that must be acknowledged.
    ///
    /// This method runs until the WebSocket connection closes or an error occurs.
    /// For reconnection, wrap this in a loop with backoff.
    pub async fn start_socket_mode(&self, app_token: &str) -> AgentorResult<()> {
        // Step 1: Get WebSocket URL from connections.open
        let ws_url = self.get_socket_url(app_token).await?;
        info!(url = %ws_url, "Slack Socket Mode: connecting");

        // Step 2: Connect to WebSocket
        let (ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .map_err(|e| AgentorError::Channel(format!("Slack Socket Mode connect error: {e}")))?;

        let (mut write, mut read) = ws_stream.split();
        info!("Slack Socket Mode: connected");

        // Step 3: Read events, acknowledge, and forward
        while let Some(msg) = read.next().await {
            match msg {
                Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                    match serde_json::from_str::<SocketModeEnvelope>(&text) {
                        Ok(envelope) => {
                            // Acknowledge the envelope
                            let ack = serde_json::json!({
                                "envelope_id": envelope.envelope_id
                            });
                            if let Err(e) = write
                                .send(tokio_tungstenite::tungstenite::Message::Text(
                                    ack.to_string(),
                                ))
                                .await
                            {
                                warn!(error = %e, "Failed to ACK Slack envelope");
                            }

                            // Process events_api type envelopes
                            if envelope.envelope_type == "events_api" {
                                if let Some(payload) = envelope.payload {
                                    self.process_socket_event(payload).await;
                                }
                            }
                            debug!(
                                envelope_type = %envelope.envelope_type,
                                "Slack Socket Mode envelope processed"
                            );
                        }
                        Err(e) => {
                            debug!(error = %e, "Non-envelope message from Slack Socket Mode");
                        }
                    }
                }
                Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                    info!("Slack Socket Mode: server closed connection");
                    break;
                }
                Ok(_) => {} // Ignore ping/pong/binary
                Err(e) => {
                    error!(error = %e, "Slack Socket Mode read error");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Request a WebSocket URL from Slack's connections.open API.
    async fn get_socket_url(&self, app_token: &str) -> AgentorResult<String> {
        let resp = self
            .client
            .post("https://slack.com/api/apps.connections.open")
            .bearer_auth(app_token)
            .send()
            .await
            .map_err(|e| AgentorError::Channel(format!("Slack connections.open error: {e}")))?;

        let body: ConnectionsOpenResponse = resp
            .json()
            .await
            .map_err(|e| AgentorError::Channel(format!("Slack connections.open parse error: {e}")))?;

        if !body.ok {
            return Err(AgentorError::Channel(format!(
                "Slack connections.open failed: {}",
                body.error.unwrap_or_default()
            )));
        }

        body.url.ok_or_else(|| {
            AgentorError::Channel("Slack connections.open returned no URL".into())
        })
    }

    /// Process a Socket Mode event payload and forward as ChannelEvent.
    async fn process_socket_event(&self, payload: serde_json::Value) {
        if let Some(event) = payload.get("event") {
            let event_type = event["type"].as_str().unwrap_or("");
            if event_type == "message" {
                let channel = event["channel"].as_str().unwrap_or("").to_string();
                let user = event["user"].as_str().unwrap_or("").to_string();
                let text = event["text"].as_str().unwrap_or("").to_string();

                if !channel.is_empty() && !text.is_empty() {
                    let msg = ChannelMessage {
                        channel_id: channel,
                        sender_id: user,
                        content: text,
                        session_id: None,
                    };
                    let _ = self.event_tx.send(ChannelEvent::MessageReceived(msg)).await;
                }
            }
        }
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
