use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::info;
use uuid::Uuid;

/// A single entry in the audit log, recording one agent action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// UTC timestamp of when the action occurred.
    pub timestamp: DateTime<Utc>,
    /// Session in which the action was performed.
    pub session_id: Uuid,
    /// Human-readable description of the action (e.g., "tool_call", "login").
    pub action: String,
    /// Name of the skill involved, if any.
    pub skill_name: Option<String>,
    /// Structured details about the action (free-form JSON).
    pub details: serde_json::Value,
    /// Whether the action succeeded, was denied, or errored.
    pub outcome: AuditOutcome,
}

/// Outcome of an audited action.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditOutcome {
    /// The action completed successfully.
    Success,
    /// The action was denied by a security check.
    Denied,
    /// The action failed with an error.
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

    /// Send an audit entry to the background writer. Logs the action via `tracing`.
    pub fn log(&self, entry: AuditEntry) {
        info!(
            session_id = %entry.session_id,
            action = %entry.action,
            outcome = ?entry.outcome,
            "audit"
        );
        let _ = self.tx.send(entry);
    }

    /// Convenience method to construct and log an [`AuditEntry`] in one call.
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
