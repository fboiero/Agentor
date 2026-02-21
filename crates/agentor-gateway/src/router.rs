use agentor_agent::AgentRunner;
use agentor_core::AgentorResult;
use agentor_security::Sanitizer;
use agentor_session::{Session, SessionStore};
use crate::connection::ConnectionManager;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct InboundMessage {
    pub session_id: Option<Uuid>,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct OutboundMessage {
    pub session_id: Uuid,
    pub content: String,
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

    pub async fn handle_message(
        &self,
        msg: InboundMessage,
        connection_id: Uuid,
    ) -> AgentorResult<()> {
        // Sanitize input
        let content = match self.sanitizer.sanitize(&msg.content).into_string() {
            Some(clean) => clean,
            None => {
                warn!("Rejected message from connection {}: failed sanitization", connection_id);
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

        // Get or create session
        let mut session = if let Some(sid) = msg.session_id {
            self.sessions
                .get(sid)
                .await?
                .unwrap_or_else(Session::new)
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
                    content: format!("Error: {}", e),
                    msg_type: "error".to_string(),
                };
                let json = serde_json::to_string(&outbound)?;
                self.connections.send_to_session(session_id, &json).await;
            }
        }

        Ok(())
    }
}
