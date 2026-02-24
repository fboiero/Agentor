use chrono::{DateTime, Utc};
use serde::Serialize;
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct AuditEntry {
    pub timestamp: DateTime<Utc>,
    pub session_id: Uuid,
    pub action: String,
    pub skill_name: Option<String>,
    pub details: serde_json::Value,
    pub outcome: AuditOutcome,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditOutcome {
    Success,
    Denied,
    Error,
}

/// Append-only audit log that records all agent actions.
pub struct AuditLog {
    tx: mpsc::UnboundedSender<AuditEntry>,
}

impl AuditLog {
    /// Create a new AuditLog. Spawns a background task that writes entries to disk.
    pub fn new(log_dir: PathBuf) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<AuditEntry>();

        tokio::spawn(async move {
            let _ = tokio::fs::create_dir_all(&log_dir).await;
            let log_file = log_dir.join("audit.jsonl");

            while let Some(entry) = rx.recv().await {
                if let Ok(line) = serde_json::to_string(&entry) {
                    let _ = tokio::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&log_file)
                        .await
                        .map(|file| {
                            use tokio::io::AsyncWriteExt;
                            let line = format!("{line}\n");
                            tokio::spawn(async move {
                                let mut f = file;
                                let _ = f.write_all(line.as_bytes()).await;
                            });
                        });
                }
            }
        });

        Self { tx }
    }

    pub fn log(&self, entry: AuditEntry) {
        info!(
            session_id = %entry.session_id,
            action = %entry.action,
            outcome = ?entry.outcome,
            "audit"
        );
        let _ = self.tx.send(entry);
    }

    pub fn log_action(
        &self,
        session_id: Uuid,
        action: impl Into<String>,
        skill_name: Option<String>,
        details: serde_json::Value,
        outcome: AuditOutcome,
    ) {
        self.log(AuditEntry {
            timestamp: Utc::now(),
            session_id,
            action: action.into(),
            skill_name,
            details,
            outcome,
        });
    }
}
