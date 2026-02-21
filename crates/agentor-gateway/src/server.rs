use crate::connection::{Connection, ConnectionManager};
use crate::router::{InboundMessage, MessageRouter};
use agentor_agent::AgentRunner;
use agentor_session::SessionStore;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info};
use uuid::Uuid;

/// Shared application state.
pub struct AppState {
    pub router: Arc<MessageRouter>,
    pub connections: Arc<ConnectionManager>,
}

/// The main gateway server.
pub struct GatewayServer;

impl GatewayServer {
    pub fn build(
        agent: Arc<AgentRunner>,
        sessions: Arc<dyn SessionStore>,
    ) -> Router {
        let connections = ConnectionManager::new();
        let router = Arc::new(MessageRouter::new(
            agent,
            sessions,
            connections.clone(),
        ));

        let state = Arc::new(AppState {
            router,
            connections,
        });

        Router::new()
            .route("/ws", get(ws_handler))
            .route("/health", get(health_handler))
            .with_state(state)
    }
}

async fn health_handler() -> impl IntoResponse {
    serde_json::json!({"status": "ok", "service": "agentor"}).to_string()
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let connection_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Channel for sending messages back to the WebSocket
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    let conn = Connection {
        id: connection_id,
        session_id,
        tx,
    };
    state.connections.add(conn).await;

    info!(
        connection_id = %connection_id,
        session_id = %session_id,
        "WebSocket connected"
    );

    // Send initial session info
    let welcome = serde_json::json!({
        "type": "connected",
        "session_id": session_id,
        "connection_id": connection_id,
    });
    let _ = state
        .connections
        .send_to_session(session_id, &welcome.to_string())
        .await;

    // Task: forward messages from channel to WebSocket
    use axum::extract::ws::Message as WsMessage;
    use futures_util::SinkExt;
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sender.send(WsMessage::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Task: receive messages from WebSocket and route them
    use futures_util::StreamExt;
    let router = state.router.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            match msg {
                Message::Text(text) => {
                    let inbound: InboundMessage = match serde_json::from_str(&text) {
                        Ok(m) => m,
                        Err(_) => InboundMessage {
                            session_id: Some(session_id),
                            content: text.to_string(),
                        },
                    };

                    if let Err(e) = router.handle_message(inbound, connection_id).await {
                        error!(error = %e, "Failed to handle message");
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    state.connections.remove(connection_id).await;
    info!(connection_id = %connection_id, "WebSocket disconnected");
}
