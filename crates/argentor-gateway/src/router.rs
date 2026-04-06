use crate::connection::ConnectionManager;
use argentor_agent::{AgentRunner, StreamEvent};
use argentor_core::ArgentorResult;
use argentor_security::Sanitizer;
use argentor_session::{Session, SessionStore};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn};
use uuid::Uuid;

/// An incoming message from a client.
#[derive(Debug, Deserialize)]
pub struct InboundMessage {
    /// Optional existing session to attach to.
    pub session_id: Option<Uuid>,
    /// Message text.
    pub content: String,
}

/// A message sent back to the client.
#[derive(Debug, Serialize)]
pub struct OutboundMessage {
    /// Session this message belongs to.
    pub session_id: Uuid,
    /// Response text.
    pub content: String,
    /// Message type discriminator.
    #[serde(rename = "type")]
    pub msg_type: String,
}

/// Routes inbound messages to the appropriate session and agent.
pub struct MessageRouter {
    agent: Arc<AgentRunner>,
    sessions: Arc<dyn SessionStore>,
    connections: Arc<ConnectionManager>,
    sanitizer: Sanitizer,
}

impl MessageRouter {
    /// Create a new message router.
    pub fn new(
        agent: Arc<AgentRunner>,
        sessions: Arc<dyn SessionStore>,
        connections: Arc<ConnectionManager>,
    ) -> Self {
        Self {
            agent,
            sessions,
            connections,
            sanitizer: Sanitizer::default(),
        }
    }

    /// Get a reference to the underlying agent runner.
    pub fn agent(&self) -> &AgentRunner {
        &self.agent
    }

    /// Route an inbound message to the appropriate session and agent.
    pub async fn handle_message(
        &self,
        msg: InboundMessage,
        connection_id: Uuid,
    ) -> ArgentorResult<()> {
        // Sanitize input
        let content = match self.sanitizer.sanitize(&msg.content).into_string() {
            Some(clean) => clean,
            None => {
                warn!(
                    "Rejected message from connection {}: failed sanitization",
                    connection_id
                );
                let error = OutboundMessage {
                    session_id: msg.session_id.unwrap_or_default(),
                    content: "Message rejected: invalid content".to_string(),
                    msg_type: "error".to_string(),
                };
                let json = serde_json::to_string(&error)?;
                self.connections
                    .send_to_session(msg.session_id.unwrap_or_default(), &json)
                    .await;
                return Ok(());
            }
        };

        // Get or create session, preserving the provided session_id
        let mut session = if let Some(sid) = msg.session_id {
            match self.sessions.get(sid).await? {
                Some(s) => s,
                None => {
                    let mut s = Session::new();
                    s.id = sid;
                    s
                }
            }
        } else {
            Session::new()
        };

        let session_id = session.id;
        info!(session_id = %session_id, "Routing message to agent");

        // Run the agent
        match self.agent.run(&mut session, &content).await {
            Ok(response) => {
                // Save session
                self.sessions.update(&session).await?;

                // Send response back
                let outbound = OutboundMessage {
                    session_id,
                    content: response,
                    msg_type: "response".to_string(),
                };
                let json = serde_json::to_string(&outbound)?;
                self.connections.send_to_session(session_id, &json).await;
            }
            Err(e) => {
                warn!(error = %e, "Agent error");
                let outbound = OutboundMessage {
                    session_id,
                    content: format!("Error: {e}"),
                    msg_type: "error".to_string(),
                };
                let json = serde_json::to_string(&outbound)?;
                self.connections.send_to_session(session_id, &json).await;
            }
        }

        Ok(())
    }

    /// Handle an inbound message from a webhook (HTTP request/response).
    ///
    /// Unlike `handle_message` which sends the response over a WebSocket connection,
    /// this method returns the agent response directly so the webhook handler can
    /// include it in the HTTP response body.
    pub async fn handle_webhook_message(&self, msg: InboundMessage) -> ArgentorResult<String> {
        // Sanitize input
        let content = match self.sanitizer.sanitize(&msg.content).into_string() {
            Some(clean) => clean,
            None => {
                warn!("Rejected webhook message: failed sanitization");
                return Ok("Message rejected: invalid content".to_string());
            }
        };

        // Get or create session, preserving the provided session_id
        let mut session = if let Some(sid) = msg.session_id {
            match self.sessions.get(sid).await? {
                Some(s) => s,
                None => {
                    let mut s = Session::new();
                    s.id = sid;
                    s
                }
            }
        } else {
            Session::new()
        };

        let session_id = session.id;
        info!(session_id = %session_id, "Routing webhook message to agent");

        // Run the agent
        match self.agent.run(&mut session, &content).await {
            Ok(response) => {
                // Save session
                self.sessions.update(&session).await?;
                Ok(response)
            }
            Err(e) => {
                warn!(error = %e, "Agent error processing webhook");
                Err(e)
            }
        }
    }

    /// Handle an inbound message with streaming.
    ///
    /// Instead of waiting for the full agent response, this method streams
    /// `StreamEvent`s to the WebSocket connection as JSON messages in real time.
    /// The client receives incremental text deltas, tool call progress, and a
    /// final `Done` event.
    pub async fn handle_message_streaming(
        &self,
        msg: InboundMessage,
        connection_id: Uuid,
    ) -> ArgentorResult<()> {
        // Sanitize input
        let content = match self.sanitizer.sanitize(&msg.content).into_string() {
            Some(clean) => clean,
            None => {
                warn!(
                    "Rejected message from connection {}: failed sanitization",
                    connection_id
                );
                let error = OutboundMessage {
                    session_id: msg.session_id.unwrap_or_default(),
                    content: "Message rejected: invalid content".to_string(),
                    msg_type: "error".to_string(),
                };
                let json = serde_json::to_string(&error)?;
                self.connections
                    .send_to_session(msg.session_id.unwrap_or_default(), &json)
                    .await;
                return Ok(());
            }
        };

        // Get or create session, preserving the provided session_id
        let mut session = if let Some(sid) = msg.session_id {
            match self.sessions.get(sid).await? {
                Some(s) => s,
                None => {
                    let mut s = Session::new();
                    s.id = sid;
                    s
                }
            }
        } else {
            Session::new()
        };

        let session_id = session.id;
        info!(
            session_id = %session_id,
            "Routing message to agent (streaming)"
        );

        // Create a channel for stream events
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<StreamEvent>();

        // Spawn a task to forward stream events to the WebSocket connection
        let connections = self.connections.clone();
        let forward_handle = tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                // Wrap the StreamEvent with the session_id for the client
                let ws_msg = serde_json::json!({
                    "session_id": session_id,
                    "msg_type": "stream",
                    "event": event,
                });
                if let Ok(json) = serde_json::to_string(&ws_msg) {
                    connections.send_to_session(session_id, &json).await;
                }
            }
        });

        // Run the streaming agent
        match self
            .agent
            .run_streaming(&mut session, &content, event_tx)
            .await
        {
            Ok(final_response) => {
                // Save session
                self.sessions.update(&session).await?;

                // Send the final complete response as well
                let outbound = OutboundMessage {
                    session_id,
                    content: final_response,
                    msg_type: "response".to_string(),
                };
                let json = serde_json::to_string(&outbound)?;
                self.connections.send_to_session(session_id, &json).await;
            }
            Err(e) => {
                warn!(error = %e, "Agent streaming error");
                let outbound = OutboundMessage {
                    session_id,
                    content: format!("Error: {e}"),
                    msg_type: "error".to_string(),
                };
                let json = serde_json::to_string(&outbound)?;
                self.connections.send_to_session(session_id, &json).await;
            }
        }

        // Ensure the forwarding task completes
        let _ = forward_handle.await;

        Ok(())
    }
}
