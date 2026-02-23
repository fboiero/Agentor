use crate::channel::{Channel, ChannelEvent, ChannelMessage};
use agentor_core::{AgentorError, AgentorResult};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

const DISCORD_API_BASE: &str = "https://discord.com/api/v10";

/// Discord channel adapter.
///
/// Uses the Discord REST API for sending messages. Inbound messages can
/// be received through the Gateway WebSocket connection via
/// [`start_gateway`], or by feeding events manually via [`handle_event`].
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

/// Discord Gateway event payload.
#[derive(Debug, Deserialize)]
struct GatewayPayload {
    op: u8,
    d: Option<serde_json::Value>,
    s: Option<u64>,
    t: Option<String>,
}

/// Discord Hello payload (op 10).
#[derive(Debug, Deserialize)]
struct HelloPayload {
    heartbeat_interval: u64,
}

/// Discord Gateway URL response.
#[derive(Debug, Deserialize)]
struct GatewayUrlResponse {
    url: String,
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
    pub fn take_event_receiver(&mut self) -> Option<mpsc::Receiver<ChannelEvent>> {
        self.event_rx.take()
    }

    /// Feed an externally-received event into the channel.
    pub async fn handle_event(&self, message: ChannelMessage) -> AgentorResult<()> {
        self.event_tx
            .send(ChannelEvent::MessageReceived(message))
            .await
            .map_err(|e| AgentorError::Channel(format!("Discord event forward error: {e}")))?;
        Ok(())
    }

    /// Start the Discord Gateway WebSocket connection.
    ///
    /// Connects to the Discord Gateway, performs the IDENTIFY handshake,
    /// starts the heartbeat loop, and listens for MESSAGE_CREATE events.
    ///
    /// This method runs until the connection is closed or an error occurs.
    /// For reconnection, wrap this in a loop with backoff.
    pub async fn start_gateway(&self) -> AgentorResult<()> {
        // Step 1: Get gateway URL
        let gateway_url = self.get_gateway_url().await?;
        let ws_url = format!("{}/?v=10&encoding=json", gateway_url);
        info!(url = %ws_url, "Discord Gateway: connecting");

        // Step 2: Connect WebSocket
        let (ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .map_err(|e| AgentorError::Channel(format!("Discord Gateway connect error: {e}")))?;

        let (mut write, mut read) = ws_stream.split();
        info!("Discord Gateway: connected");

        // Step 3: Wait for Hello (op 10)
        let heartbeat_interval = match read.next().await {
            Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                let payload: GatewayPayload = serde_json::from_str(&text)
                    .map_err(|e| AgentorError::Channel(format!("Discord Gateway parse error: {e}")))?;
                if payload.op != 10 {
                    return Err(AgentorError::Channel(
                        "Expected op 10 (Hello) from Discord Gateway".into(),
                    ));
                }
                let hello: HelloPayload = serde_json::from_value(
                    payload.d.ok_or_else(|| AgentorError::Channel("Missing Hello data".into()))?,
                )
                .map_err(|e| AgentorError::Channel(format!("Hello parse error: {e}")))?;
                info!(interval_ms = hello.heartbeat_interval, "Discord Gateway: received Hello");
                hello.heartbeat_interval
            }
            _ => {
                return Err(AgentorError::Channel(
                    "Failed to receive Hello from Discord Gateway".into(),
                ))
            }
        };

        // Step 4: Send Identify (op 2)
        let identify = serde_json::json!({
            "op": 2,
            "d": {
                "token": self.bot_token,
                "intents": 512 | 32768, // GUILD_MESSAGES | MESSAGE_CONTENT
                "properties": {
                    "os": "linux",
                    "browser": "agentor",
                    "device": "agentor"
                }
            }
        });
        write
            .send(tokio_tungstenite::tungstenite::Message::Text(
                identify.to_string(),
            ))
            .await
            .map_err(|e| AgentorError::Channel(format!("Discord Identify error: {e}")))?;
        info!("Discord Gateway: sent Identify");

        // Step 5: Start heartbeat loop in background
        let heartbeat_write = std::sync::Arc::new(tokio::sync::Mutex::new(write));
        let heartbeat_write_clone = heartbeat_write.clone();
        let seq = std::sync::Arc::new(tokio::sync::Mutex::new(None::<u64>));
        let seq_clone = seq.clone();

        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_millis(heartbeat_interval));
            loop {
                interval.tick().await;
                let s = *seq_clone.lock().await;
                let hb = serde_json::json!({ "op": 1, "d": s });
                let mut w = heartbeat_write_clone.lock().await;
                if w.send(tokio_tungstenite::tungstenite::Message::Text(
                    hb.to_string(),
                ))
                .await
                .is_err()
                {
                    debug!("Discord heartbeat: connection closed");
                    break;
                }
            }
        });

        // Step 6: Event loop — listen for Dispatch (op 0) events
        while let Some(msg) = read.next().await {
            match msg {
                Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                    if let Ok(payload) = serde_json::from_str::<GatewayPayload>(&text) {
                        // Update sequence number
                        if let Some(s) = payload.s {
                            *seq.lock().await = Some(s);
                        }

                        match payload.op {
                            0 => {
                                // Dispatch event
                                if let Some(event_name) = &payload.t {
                                    if event_name == "MESSAGE_CREATE" {
                                        if let Some(data) = &payload.d {
                                            self.process_message_create(data).await;
                                        }
                                    }
                                    debug!(event = %event_name, "Discord Gateway dispatch");
                                }
                            }
                            11 => {
                                // Heartbeat ACK — all good
                                debug!("Discord Gateway: heartbeat ACK");
                            }
                            _ => {
                                debug!(op = payload.op, "Discord Gateway: unhandled opcode");
                            }
                        }
                    }
                }
                Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                    info!("Discord Gateway: server closed connection");
                    break;
                }
                Ok(_) => {} // Ignore ping/pong/binary
                Err(e) => {
                    error!(error = %e, "Discord Gateway read error");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Get the Discord Gateway URL.
    async fn get_gateway_url(&self) -> AgentorResult<String> {
        let resp = self
            .client
            .get(format!("{}/gateway", DISCORD_API_BASE))
            .send()
            .await
            .map_err(|e| AgentorError::Channel(format!("Discord gateway URL error: {e}")))?;

        let body: GatewayUrlResponse = resp
            .json()
            .await
            .map_err(|e| AgentorError::Channel(format!("Discord gateway URL parse error: {e}")))?;

        Ok(body.url)
    }

    /// Process a MESSAGE_CREATE event from the Gateway.
    async fn process_message_create(&self, data: &serde_json::Value) {
        let channel_id = data["channel_id"].as_str().unwrap_or("").to_string();
        let content = data["content"].as_str().unwrap_or("").to_string();
        let author_id = data["author"]["id"].as_str().unwrap_or("").to_string();
        let is_bot = data["author"]["bot"].as_bool().unwrap_or(false);

        // Skip bot messages to avoid loops
        if is_bot || content.is_empty() {
            return;
        }

        let msg = ChannelMessage {
            channel_id,
            sender_id: author_id,
            content,
            session_id: None,
        };
        let _ = self.event_tx.send(ChannelEvent::MessageReceived(msg)).await;
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
