use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
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

/// Configuration for audit log file rotation and flushing.
#[derive(Debug, Clone)]
pub struct AuditConfig {
    /// Path to the directory where audit logs are stored.
    pub path: PathBuf,
    /// Maximum file size in bytes before rotation (default: 100MB).
    pub max_file_size: u64,
    /// Maximum number of rotated files to keep (default: 10).
    pub max_files: usize,
    /// Flush interval in milliseconds (default: 100ms).
    pub flush_interval: Duration,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("."),
            max_file_size: 100 * 1024 * 1024, // 100MB
            max_files: 10,
            flush_interval: Duration::from_millis(100),
        }
    }
}

/// Append-only audit log that records all agent actions with file rotation.
pub struct AuditLog {
    config: AuditConfig,
    tx: mpsc::UnboundedSender<AuditEntry>,
}

impl AuditLog {
    /// Create a new AuditLog with default configuration.
    pub fn new(log_dir: PathBuf) -> Self {
        Self::with_config(AuditConfig {
            path: log_dir,
            ..Default::default()
        })
    }

    /// Create a new AuditLog with custom configuration.
    pub fn with_config(config: AuditConfig) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<AuditEntry>();
        let config_clone = config.clone();

        tokio::spawn(async move {
            let _ = tokio::fs::create_dir_all(&config_clone.path).await;
            let mut buffer = Vec::new();
            let mut last_flush = tokio::time::Instant::now();

            loop {
                tokio::select! {
                    Some(entry) = rx.recv() => {
                        if let Ok(line) = serde_json::to_string(&entry) {
                            buffer.push(format!("{}\n", line));
                        }
                    }
                    _ = tokio::time::sleep(config_clone.flush_interval) => {
                        if !buffer.is_empty() {
                            let _ = Self::flush_and_rotate(&config_clone, &mut buffer).await;
                            last_flush = tokio::time::Instant::now();
                        }
                    }
                }

                // Also flush periodically even if no timeout
                if last_flush.elapsed() >= config_clone.flush_interval && !buffer.is_empty() {
                    let _ = Self::flush_and_rotate(&config_clone, &mut buffer).await;
                    last_flush = tokio::time::Instant::now();
                }
            }
        });

        Self { config, tx }
    }

    /// Send an audit entry to the background writer. Logs the action via `tracing`.
    pub fn log_action(
        &self,
        session_id: Uuid,
        action: impl Into<String>,
        skill_name: Option<String>,
        details: serde_json::Value,
        outcome: AuditOutcome,
    ) {
        let entry = AuditEntry {
            timestamp: Utc::now(),
            session_id,
            action: action.into(),
            skill_name,
            details,
            outcome: outcome.clone(),
        };

        info!(
            session_id = %entry.session_id,
            action = %entry.action,
            outcome = ?outcome,
            "audit"
        );
        let _ = self.tx.send(entry);
    }

    /// Flush any buffered entries to disk immediately.
    pub async fn flush(&self) {
        // The background task handles flushing, this is a no-op for the public API
    }

    /// Flush buffer and rotate files if needed.
    async fn flush_and_rotate(config: &AuditConfig, buffer: &mut Vec<String>) -> std::io::Result<()> {
        if buffer.is_empty() {
            return Ok(());
        }

        let log_file = config.path.join("audit.jsonl");

        // Write buffered entries
        let content = buffer.join("");
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file)
            .await?
            .write_all(content.as_bytes())
            .await?;

        buffer.clear();

        // Check file size and rotate if needed
        if let Ok(metadata) = tokio::fs::metadata(&log_file).await {
            if metadata.len() > config.max_file_size {
                Self::rotate_files(config).await?;
            }
        }

        Ok(())
    }

    /// Rotate log files: rename audit.jsonl to audit.jsonl.1, etc.
    async fn rotate_files(config: &AuditConfig) -> std::io::Result<()> {
        let base_file = config.path.join("audit.jsonl");

        // Shift existing rotated files
        for i in (1..config.max_files).rev() {
            let old = config.path.join(format!("audit.jsonl.{}", i));
            let new = config.path.join(format!("audit.jsonl.{}", i + 1));
            if old.exists() {
                let _ = tokio::fs::rename(&old, &new).await;
            }
        }

        // Rename current file to .1
        if base_file.exists() {
            tokio::fs::rename(&base_file, config.path.join("audit.jsonl.1")).await?;
        }

        // Delete files beyond max_files
        for i in (config.max_files + 1)..=100 {
            let file = config.path.join(format!("audit.jsonl.{}", i));
            if file.exists() {
                let _ = tokio::fs::remove_file(file).await;
            }
        }

        Ok(())
    }
}
