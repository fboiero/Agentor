use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

/// Represents a connected WebSocket client.
#[derive(Debug)]
pub struct Connection {
    pub id: Uuid,
    pub session_id: Uuid,
    pub tx: mpsc::UnboundedSender<String>,
}

/// Manages active WebSocket connections.
pub struct ConnectionManager {
    connections: RwLock<HashMap<Uuid, Connection>>,
}

impl ConnectionManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            connections: RwLock::new(HashMap::new()),
        })
    }

    pub async fn add(&self, conn: Connection) {
        let id = conn.id;
        self.connections.write().await.insert(id, conn);
        tracing::info!(connection_id = %id, "Connection added");
    }

    pub async fn remove(&self, id: Uuid) {
        self.connections.write().await.remove(&id);
        tracing::info!(connection_id = %id, "Connection removed");
    }

    pub async fn send_to_session(&self, session_id: Uuid, message: &str) {
        let conns = self.connections.read().await;
        for conn in conns.values() {
            if conn.session_id == session_id {
                let _ = conn.tx.send(message.to_string());
            }
        }
    }

    pub async fn connection_count(&self) -> usize {
        self.connections.read().await.len()
    }

    /// Broadcast a message to all connected clients.
    pub async fn broadcast(&self, message: &str) {
        let conns = self.connections.read().await;
        for conn in conns.values() {
            let _ = conn.tx.send(message.to_string());
        }
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
        }
    }
}
