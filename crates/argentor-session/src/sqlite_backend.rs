//! Real SQLite-backed persistence for sessions, messages, usage, and personas.
//!
//! Gated behind the `sqlite` feature flag. Provides [`SqliteBackend`] — a
//! single connection wrapper that manages four tables:
//!
//! | Table      | Purpose                                          |
//! |------------|--------------------------------------------------|
//! | `sessions` | Session metadata (id, timestamps, JSON metadata)  |
//! | `messages` | Per-session messages (role, content, timestamps)  |
//! | `usage`    | Per-tenant token/cost accounting                 |
//! | `personas` | Per-tenant agent persona configurations           |
//!
//! The connection uses WAL mode for better concurrent-read performance and
//! is protected by a [`std::sync::Mutex`] since SQLite is single-writer.

use argentor_core::{ArgentorError, ArgentorResult, Message, Role};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use uuid::Uuid;

use crate::session::Session;
use crate::sqlite_store::{PersonaConfig, UsageRecord};

// ---------------------------------------------------------------------------
// SqliteBackend
// ---------------------------------------------------------------------------

/// A real SQLite-backed store for sessions, messages, usage records, and
/// persona configurations.
///
/// All writes go through a [`Mutex<Connection>`]. SQLite WAL mode is enabled
/// at construction time for better read concurrency.
pub struct SqliteBackend {
    conn: Mutex<Connection>,
}

impl SqliteBackend {
    /// Open (or create) a SQLite database at `path` and run migrations.
    pub fn new(path: &Path) -> ArgentorResult<Self> {
        let conn = Connection::open(path)
            .map_err(|e| ArgentorError::Session(format!("Failed to open SQLite DB: {e}")))?;
        let backend = Self {
            conn: Mutex::new(conn),
        };
        backend.run_migrations()?;
        Ok(backend)
    }

    /// Create an in-memory SQLite database (useful for testing).
    pub fn new_in_memory() -> ArgentorResult<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| ArgentorError::Session(format!("Failed to open in-memory SQLite: {e}")))?;
        let backend = Self {
            conn: Mutex::new(conn),
        };
        backend.run_migrations()?;
        Ok(backend)
    }

    // -----------------------------------------------------------------------
    // Migrations
    // -----------------------------------------------------------------------

    fn run_migrations(&self) -> ArgentorResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ArgentorError::Session(format!("Mutex poisoned: {e}")))?;

        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| ArgentorError::Session(format!("Failed to set WAL mode: {e}")))?;

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS sessions (
                id         TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                metadata   TEXT NOT NULL DEFAULT '{}'
            );

            CREATE TABLE IF NOT EXISTS messages (
                id         TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                role       TEXT NOT NULL,
                content    TEXT NOT NULL,
                timestamp  TEXT NOT NULL,
                metadata   TEXT NOT NULL DEFAULT '{}',
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_messages_session_id
                ON messages(session_id);

            CREATE TABLE IF NOT EXISTS usage (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                tenant_id  TEXT NOT NULL,
                timestamp  TEXT NOT NULL,
                tokens_in  INTEGER NOT NULL,
                tokens_out INTEGER NOT NULL,
                model      TEXT NOT NULL,
                cost       REAL NOT NULL,
                agent_role TEXT NOT NULL DEFAULT ''
            );

            CREATE INDEX IF NOT EXISTS idx_usage_tenant_id
                ON usage(tenant_id);

            CREATE TABLE IF NOT EXISTS personas (
                tenant_id TEXT NOT NULL,
                name      TEXT NOT NULL,
                config    TEXT NOT NULL DEFAULT '{}',
                updated_at TEXT NOT NULL,
                PRIMARY KEY (tenant_id, name)
            );
            ",
        )
        .map_err(|e| ArgentorError::Session(format!("Migration failed: {e}")))?;

        // Enable foreign keys.
        conn.execute_batch("PRAGMA foreign_keys=ON;")
            .map_err(|e| ArgentorError::Session(format!("Failed to enable foreign keys: {e}")))?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Session CRUD
    // -----------------------------------------------------------------------

    /// Save a session (insert or replace). Messages within `session.messages`
    /// are **not** touched — use [`add_message`](Self::add_message) separately.
    pub fn save_session(&self, session: &Session) -> ArgentorResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ArgentorError::Session(format!("Mutex poisoned: {e}")))?;

        let metadata_json = serde_json::to_string(&session.metadata)?;

        conn.execute(
            "INSERT OR REPLACE INTO sessions (id, created_at, updated_at, metadata)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                session.id.to_string(),
                session.created_at.to_rfc3339(),
                session.updated_at.to_rfc3339(),
                metadata_json,
            ],
        )
        .map_err(|e| ArgentorError::Session(format!("Failed to save session: {e}")))?;

        Ok(())
    }

    /// Load a session by ID. Returns `None` if the session does not exist.
    /// The returned session's `messages` field will be empty — call
    /// [`get_messages`](Self::get_messages) to populate it.
    pub fn load_session(&self, id: Uuid) -> ArgentorResult<Option<Session>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ArgentorError::Session(format!("Mutex poisoned: {e}")))?;

        let mut stmt = conn
            .prepare("SELECT id, created_at, updated_at, metadata FROM sessions WHERE id = ?1")
            .map_err(|e| ArgentorError::Session(format!("Prepare failed: {e}")))?;

        let result = stmt
            .query_row(params![id.to_string()], |row| {
                let id_str: String = row.get(0)?;
                let created_str: String = row.get(1)?;
                let updated_str: String = row.get(2)?;
                let metadata_str: String = row.get(3)?;
                Ok((id_str, created_str, updated_str, metadata_str))
            })
            .optional()
            .map_err(|e| ArgentorError::Session(format!("Query failed: {e}")))?;

        match result {
            None => Ok(None),
            Some((id_str, created_str, updated_str, metadata_str)) => {
                let id = Uuid::parse_str(&id_str)
                    .map_err(|e| ArgentorError::Session(format!("Invalid UUID: {e}")))?;
                let created_at = DateTime::parse_from_rfc3339(&created_str)
                    .map_err(|e| ArgentorError::Session(format!("Invalid date: {e}")))?
                    .with_timezone(&Utc);
                let updated_at = DateTime::parse_from_rfc3339(&updated_str)
                    .map_err(|e| ArgentorError::Session(format!("Invalid date: {e}")))?
                    .with_timezone(&Utc);
                let metadata: HashMap<String, serde_json::Value> =
                    serde_json::from_str(&metadata_str)?;

                Ok(Some(Session {
                    id,
                    messages: Vec::new(),
                    active_skills: Vec::new(),
                    created_at,
                    updated_at,
                    metadata,
                }))
            }
        }
    }

    /// Delete a session and all its messages (via CASCADE).
    pub fn delete_session(&self, id: Uuid) -> ArgentorResult<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ArgentorError::Session(format!("Mutex poisoned: {e}")))?;

        // Manually delete messages first since SQLite FK cascade requires
        // PRAGMA foreign_keys to be ON per-connection (which we set), but
        // being explicit is safer.
        conn.execute(
            "DELETE FROM messages WHERE session_id = ?1",
            params![id.to_string()],
        )
        .map_err(|e| ArgentorError::Session(format!("Failed to delete messages: {e}")))?;

        let rows = conn
            .execute(
                "DELETE FROM sessions WHERE id = ?1",
                params![id.to_string()],
            )
            .map_err(|e| ArgentorError::Session(format!("Failed to delete session: {e}")))?;

        Ok(rows > 0)
    }

    /// List all session IDs.
    pub fn list_sessions(&self) -> ArgentorResult<Vec<Uuid>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ArgentorError::Session(format!("Mutex poisoned: {e}")))?;

        let mut stmt = conn
            .prepare("SELECT id FROM sessions ORDER BY created_at")
            .map_err(|e| ArgentorError::Session(format!("Prepare failed: {e}")))?;

        let ids = stmt
            .query_map([], |row| {
                let id_str: String = row.get(0)?;
                Ok(id_str)
            })
            .map_err(|e| ArgentorError::Session(format!("Query failed: {e}")))?
            .filter_map(|r| r.ok())
            .filter_map(|s| Uuid::parse_str(&s).ok())
            .collect();

        Ok(ids)
    }

    // -----------------------------------------------------------------------
    // Message operations
    // -----------------------------------------------------------------------

    /// Append a message to a session.
    pub fn add_message(&self, msg: &Message) -> ArgentorResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ArgentorError::Session(format!("Mutex poisoned: {e}")))?;

        let role_str = role_to_str(&msg.role);
        let metadata_json = serde_json::to_string(&msg.metadata)?;

        conn.execute(
            "INSERT INTO messages (id, session_id, role, content, timestamp, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                msg.id.to_string(),
                msg.session_id.to_string(),
                role_str,
                msg.content,
                msg.timestamp.to_rfc3339(),
                metadata_json,
            ],
        )
        .map_err(|e| ArgentorError::Session(format!("Failed to add message: {e}")))?;

        // Update the session's updated_at timestamp.
        conn.execute(
            "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), msg.session_id.to_string()],
        )
        .map_err(|e| ArgentorError::Session(format!("Failed to update session timestamp: {e}")))?;

        Ok(())
    }

    /// Get all messages for a session, ordered by timestamp.
    pub fn get_messages(&self, session_id: Uuid) -> ArgentorResult<Vec<Message>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ArgentorError::Session(format!("Mutex poisoned: {e}")))?;

        Self::query_messages(&conn, session_id, None, None)
    }

    /// Get messages with pagination (limit + offset), ordered by timestamp.
    pub fn get_messages_paginated(
        &self,
        session_id: Uuid,
        limit: u32,
        offset: u32,
    ) -> ArgentorResult<Vec<Message>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ArgentorError::Session(format!("Mutex poisoned: {e}")))?;

        Self::query_messages(&conn, session_id, Some(limit), Some(offset))
    }

    fn query_messages(
        conn: &Connection,
        session_id: Uuid,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> ArgentorResult<Vec<Message>> {
        let sql = match (limit, offset) {
            (Some(l), Some(o)) => format!(
                "SELECT id, session_id, role, content, timestamp, metadata
                 FROM messages WHERE session_id = ?1
                 ORDER BY timestamp ASC
                 LIMIT {l} OFFSET {o}"
            ),
            _ => "SELECT id, session_id, role, content, timestamp, metadata
                  FROM messages WHERE session_id = ?1
                  ORDER BY timestamp ASC"
                .to_string(),
        };

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| ArgentorError::Session(format!("Prepare failed: {e}")))?;

        let rows = stmt
            .query_map(params![session_id.to_string()], |row| {
                let id_str: String = row.get(0)?;
                let session_id_str: String = row.get(1)?;
                let role_str: String = row.get(2)?;
                let content: String = row.get(3)?;
                let ts_str: String = row.get(4)?;
                let meta_str: String = row.get(5)?;
                Ok((id_str, session_id_str, role_str, content, ts_str, meta_str))
            })
            .map_err(|e| ArgentorError::Session(format!("Query failed: {e}")))?;

        let mut messages = Vec::new();
        for row_result in rows {
            let (id_str, session_id_str, role_str, content, ts_str, meta_str) =
                row_result.map_err(|e| ArgentorError::Session(format!("Row read failed: {e}")))?;

            let id = Uuid::parse_str(&id_str)
                .map_err(|e| ArgentorError::Session(format!("Invalid UUID: {e}")))?;
            let session_id = Uuid::parse_str(&session_id_str)
                .map_err(|e| ArgentorError::Session(format!("Invalid UUID: {e}")))?;
            let role = str_to_role(&role_str)?;
            let timestamp = DateTime::parse_from_rfc3339(&ts_str)
                .map_err(|e| ArgentorError::Session(format!("Invalid date: {e}")))?
                .with_timezone(&Utc);
            let metadata: HashMap<String, serde_json::Value> = serde_json::from_str(&meta_str)?;

            messages.push(Message {
                id,
                role,
                content,
                session_id,
                timestamp,
                metadata,
            });
        }

        Ok(messages)
    }

    // -----------------------------------------------------------------------
    // Usage operations
    // -----------------------------------------------------------------------

    /// Record a usage entry.
    pub fn record_usage(&self, record: &UsageRecord) -> ArgentorResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ArgentorError::Session(format!("Mutex poisoned: {e}")))?;

        conn.execute(
            "INSERT INTO usage (tenant_id, timestamp, tokens_in, tokens_out, model, cost, agent_role)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                record.tenant_id,
                record.timestamp.to_rfc3339(),
                record.tokens_in as i64,
                record.tokens_out as i64,
                record.model,
                record.cost_usd,
                record.agent_role,
            ],
        )
        .map_err(|e| ArgentorError::Session(format!("Failed to record usage: {e}")))?;

        Ok(())
    }

    /// Get usage records for a tenant within a time range (inclusive).
    pub fn get_usage(
        &self,
        tenant_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> ArgentorResult<Vec<UsageRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ArgentorError::Session(format!("Mutex poisoned: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT tenant_id, timestamp, tokens_in, tokens_out, model, cost, agent_role
                 FROM usage
                 WHERE tenant_id = ?1 AND timestamp >= ?2 AND timestamp <= ?3
                 ORDER BY timestamp ASC",
            )
            .map_err(|e| ArgentorError::Session(format!("Prepare failed: {e}")))?;

        let rows = stmt
            .query_map(
                params![tenant_id, from.to_rfc3339(), to.to_rfc3339()],
                |row| {
                    let tid: String = row.get(0)?;
                    let ts_str: String = row.get(1)?;
                    let tokens_in: i64 = row.get(2)?;
                    let tokens_out: i64 = row.get(3)?;
                    let model: String = row.get(4)?;
                    let cost: f64 = row.get(5)?;
                    let agent_role: String = row.get(6)?;
                    Ok((tid, ts_str, tokens_in, tokens_out, model, cost, agent_role))
                },
            )
            .map_err(|e| ArgentorError::Session(format!("Query failed: {e}")))?;

        let mut records = Vec::new();
        for row_result in rows {
            let (tid, ts_str, tokens_in, tokens_out, model, cost, agent_role) =
                row_result.map_err(|e| ArgentorError::Session(format!("Row read failed: {e}")))?;
            let timestamp = DateTime::parse_from_rfc3339(&ts_str)
                .map_err(|e| ArgentorError::Session(format!("Invalid date: {e}")))?
                .with_timezone(&Utc);

            records.push(UsageRecord {
                tenant_id: tid,
                agent_role,
                model,
                tokens_in: tokens_in as u64,
                tokens_out: tokens_out as u64,
                cost_usd: cost,
                timestamp,
            });
        }

        Ok(records)
    }

    /// Get the total cost for a tenant across all usage records.
    pub fn get_total_cost(&self, tenant_id: &str) -> ArgentorResult<f64> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ArgentorError::Session(format!("Mutex poisoned: {e}")))?;

        let total: f64 = conn
            .query_row(
                "SELECT COALESCE(SUM(cost), 0.0) FROM usage WHERE tenant_id = ?1",
                params![tenant_id],
                |row| row.get(0),
            )
            .map_err(|e| ArgentorError::Session(format!("Query failed: {e}")))?;

        Ok(total)
    }

    // -----------------------------------------------------------------------
    // Persona operations
    // -----------------------------------------------------------------------

    /// Save (insert or replace) a persona configuration.
    pub fn save_persona(&self, persona: &PersonaConfig) -> ArgentorResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ArgentorError::Session(format!("Mutex poisoned: {e}")))?;

        let config_json = serde_json::to_string(&persona.config)?;

        conn.execute(
            "INSERT OR REPLACE INTO personas (tenant_id, name, config, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                persona.tenant_id,
                persona.agent_role,
                config_json,
                persona.updated_at.to_rfc3339(),
            ],
        )
        .map_err(|e| ArgentorError::Session(format!("Failed to save persona: {e}")))?;

        Ok(())
    }

    /// Load a specific persona by tenant and role name.
    pub fn load_persona(
        &self,
        tenant_id: &str,
        name: &str,
    ) -> ArgentorResult<Option<PersonaConfig>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ArgentorError::Session(format!("Mutex poisoned: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT tenant_id, name, config, updated_at
                 FROM personas WHERE tenant_id = ?1 AND name = ?2",
            )
            .map_err(|e| ArgentorError::Session(format!("Prepare failed: {e}")))?;

        let result = stmt
            .query_row(params![tenant_id, name], |row| {
                let tid: String = row.get(0)?;
                let role: String = row.get(1)?;
                let config_str: String = row.get(2)?;
                let updated_str: String = row.get(3)?;
                Ok((tid, role, config_str, updated_str))
            })
            .optional()
            .map_err(|e| ArgentorError::Session(format!("Query failed: {e}")))?;

        match result {
            None => Ok(None),
            Some((tid, role, config_str, updated_str)) => {
                let config: serde_json::Value = serde_json::from_str(&config_str)?;
                let updated_at = DateTime::parse_from_rfc3339(&updated_str)
                    .map_err(|e| ArgentorError::Session(format!("Invalid date: {e}")))?
                    .with_timezone(&Utc);

                Ok(Some(PersonaConfig {
                    tenant_id: tid,
                    agent_role: role,
                    config,
                    updated_at,
                }))
            }
        }
    }

    /// List all personas for a given tenant.
    pub fn list_personas(&self, tenant_id: &str) -> ArgentorResult<Vec<PersonaConfig>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ArgentorError::Session(format!("Mutex poisoned: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT tenant_id, name, config, updated_at
                 FROM personas WHERE tenant_id = ?1
                 ORDER BY name",
            )
            .map_err(|e| ArgentorError::Session(format!("Prepare failed: {e}")))?;

        let rows = stmt
            .query_map(params![tenant_id], |row| {
                let tid: String = row.get(0)?;
                let role: String = row.get(1)?;
                let config_str: String = row.get(2)?;
                let updated_str: String = row.get(3)?;
                Ok((tid, role, config_str, updated_str))
            })
            .map_err(|e| ArgentorError::Session(format!("Query failed: {e}")))?;

        let mut personas = Vec::new();
        for row_result in rows {
            let (tid, role, config_str, updated_str) =
                row_result.map_err(|e| ArgentorError::Session(format!("Row read failed: {e}")))?;
            let config: serde_json::Value = serde_json::from_str(&config_str)?;
            let updated_at = DateTime::parse_from_rfc3339(&updated_str)
                .map_err(|e| ArgentorError::Session(format!("Invalid date: {e}")))?
                .with_timezone(&Utc);

            personas.push(PersonaConfig {
                tenant_id: tid,
                agent_role: role,
                config,
                updated_at,
            });
        }

        Ok(personas)
    }

    /// Delete a specific persona. Returns `true` if a row was deleted.
    pub fn delete_persona(&self, tenant_id: &str, name: &str) -> ArgentorResult<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| ArgentorError::Session(format!("Mutex poisoned: {e}")))?;

        let rows = conn
            .execute(
                "DELETE FROM personas WHERE tenant_id = ?1 AND name = ?2",
                params![tenant_id, name],
            )
            .map_err(|e| ArgentorError::Session(format!("Failed to delete persona: {e}")))?;

        Ok(rows > 0)
    }
}

// ---------------------------------------------------------------------------
// Role helpers
// ---------------------------------------------------------------------------

fn role_to_str(role: &Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "system",
        Role::Tool => "tool",
    }
}

fn str_to_role(s: &str) -> ArgentorResult<Role> {
    match s {
        "user" => Ok(Role::User),
        "assistant" => Ok(Role::Assistant),
        "system" => Ok(Role::System),
        "tool" => Ok(Role::Tool),
        other => Err(ArgentorError::Session(format!("Unknown role: {other}"))),
    }
}

/// Helper trait to add `.optional()` to rusqlite query results, mirroring
/// the pattern from `rusqlite::OptionalExtension`.
trait OptionalExt<T> {
    /// Convert a `QueryReturnedNoRows` error into `Ok(None)`.
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use chrono::Duration;
    use std::sync::Arc;
    use tempfile::TempDir;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_backend() -> SqliteBackend {
        SqliteBackend::new_in_memory().unwrap()
    }

    fn make_session() -> Session {
        Session::new()
    }

    fn make_message(session_id: Uuid, role: Role, content: &str) -> Message {
        Message {
            id: Uuid::new_v4(),
            role,
            content: content.to_string(),
            session_id,
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    fn make_usage_record(tenant: &str, model: &str, tokens_in: u64) -> UsageRecord {
        UsageRecord {
            tenant_id: tenant.to_string(),
            agent_role: "coder".to_string(),
            model: model.to_string(),
            tokens_in,
            tokens_out: tokens_in / 2,
            cost_usd: tokens_in as f64 * 0.001,
            timestamp: Utc::now(),
        }
    }

    // -----------------------------------------------------------------------
    // 1. Session CRUD lifecycle
    // -----------------------------------------------------------------------

    #[test]
    fn session_create_and_load() {
        let db = make_backend();
        let session = make_session();

        db.save_session(&session).unwrap();
        let loaded = db.load_session(session.id).unwrap();

        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.id, session.id);
    }

    #[test]
    fn session_load_nonexistent_returns_none() {
        let db = make_backend();
        let loaded = db.load_session(Uuid::new_v4()).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn session_update_metadata() {
        let db = make_backend();
        let mut session = make_session();
        db.save_session(&session).unwrap();

        session
            .metadata
            .insert("env".into(), serde_json::json!("prod"));
        session.updated_at = Utc::now();
        db.save_session(&session).unwrap();

        let loaded = db.load_session(session.id).unwrap().unwrap();
        assert_eq!(
            loaded.metadata.get("env").unwrap(),
            &serde_json::json!("prod")
        );
    }

    #[test]
    fn session_delete() {
        let db = make_backend();
        let session = make_session();
        db.save_session(&session).unwrap();

        let deleted = db.delete_session(session.id).unwrap();
        assert!(deleted);

        let loaded = db.load_session(session.id).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn session_delete_nonexistent_returns_false() {
        let db = make_backend();
        let deleted = db.delete_session(Uuid::new_v4()).unwrap();
        assert!(!deleted);
    }

    #[test]
    fn session_list() {
        let db = make_backend();
        let s1 = make_session();
        let s2 = make_session();
        let s3 = make_session();

        db.save_session(&s1).unwrap();
        db.save_session(&s2).unwrap();
        db.save_session(&s3).unwrap();

        let ids = db.list_sessions().unwrap();
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&s1.id));
        assert!(ids.contains(&s2.id));
        assert!(ids.contains(&s3.id));
    }

    // -----------------------------------------------------------------------
    // 2. Message append and retrieval
    // -----------------------------------------------------------------------

    #[test]
    fn message_add_and_get() {
        let db = make_backend();
        let session = make_session();
        db.save_session(&session).unwrap();

        let m1 = make_message(session.id, Role::User, "Hello");
        let m2 = make_message(session.id, Role::Assistant, "Hi there!");

        db.add_message(&m1).unwrap();
        db.add_message(&m2).unwrap();

        let messages = db.get_messages(session.id).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "Hello");
        assert_eq!(messages[1].content, "Hi there!");
    }

    #[test]
    fn message_get_empty_session() {
        let db = make_backend();
        let session = make_session();
        db.save_session(&session).unwrap();

        let messages = db.get_messages(session.id).unwrap();
        assert!(messages.is_empty());
    }

    #[test]
    fn message_roles_round_trip() {
        let db = make_backend();
        let session = make_session();
        db.save_session(&session).unwrap();

        let roles = vec![Role::User, Role::Assistant, Role::System, Role::Tool];
        for role in &roles {
            let msg = make_message(session.id, role.clone(), &format!("{role:?} message"));
            db.add_message(&msg).unwrap();
        }

        let messages = db.get_messages(session.id).unwrap();
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].role, Role::User);
        assert_eq!(messages[1].role, Role::Assistant);
        assert_eq!(messages[2].role, Role::System);
        assert_eq!(messages[3].role, Role::Tool);
    }

    #[test]
    fn message_metadata_preserved() {
        let db = make_backend();
        let session = make_session();
        db.save_session(&session).unwrap();

        let mut msg = make_message(session.id, Role::User, "with meta");
        msg.metadata
            .insert("source".into(), serde_json::json!("web"));

        db.add_message(&msg).unwrap();

        let messages = db.get_messages(session.id).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0].metadata.get("source").unwrap(),
            &serde_json::json!("web")
        );
    }

    // -----------------------------------------------------------------------
    // 3. Pagination
    // -----------------------------------------------------------------------

    #[test]
    fn message_pagination() {
        let db = make_backend();
        let session = make_session();
        db.save_session(&session).unwrap();

        // Insert 10 messages with slight timestamp offsets.
        for i in 0..10 {
            let mut msg = make_message(session.id, Role::User, &format!("msg-{i}"));
            msg.timestamp = Utc::now() + Duration::milliseconds(i * 10);
            db.add_message(&msg).unwrap();
        }

        // Page 1: first 3.
        let page1 = db.get_messages_paginated(session.id, 3, 0).unwrap();
        assert_eq!(page1.len(), 3);
        assert_eq!(page1[0].content, "msg-0");
        assert_eq!(page1[2].content, "msg-2");

        // Page 2: next 3.
        let page2 = db.get_messages_paginated(session.id, 3, 3).unwrap();
        assert_eq!(page2.len(), 3);
        assert_eq!(page2[0].content, "msg-3");

        // Page 4: last 1.
        let page4 = db.get_messages_paginated(session.id, 3, 9).unwrap();
        assert_eq!(page4.len(), 1);
        assert_eq!(page4[0].content, "msg-9");

        // Beyond range: empty.
        let beyond = db.get_messages_paginated(session.id, 3, 20).unwrap();
        assert!(beyond.is_empty());
    }

    // -----------------------------------------------------------------------
    // 4. Usage recording and querying
    // -----------------------------------------------------------------------

    #[test]
    fn usage_record_and_query() {
        let db = make_backend();

        let r1 = make_usage_record("tenant-a", "gpt-4o", 100);
        let r2 = make_usage_record("tenant-a", "claude-opus-4-20250514", 200);

        db.record_usage(&r1).unwrap();
        db.record_usage(&r2).unwrap();

        let from = Utc::now() - Duration::hours(1);
        let to = Utc::now() + Duration::hours(1);
        let records = db.get_usage("tenant-a", from, to).unwrap();

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].tokens_in, 100);
        assert_eq!(records[1].tokens_in, 200);
    }

    #[test]
    fn usage_total_cost() {
        let db = make_backend();

        let r1 = make_usage_record("t1", "gpt-4o", 100); // cost = 0.1
        let r2 = make_usage_record("t1", "gpt-4o", 200); // cost = 0.2

        db.record_usage(&r1).unwrap();
        db.record_usage(&r2).unwrap();

        let total = db.get_total_cost("t1").unwrap();
        assert!((total - 0.3).abs() < 0.0001);
    }

    #[test]
    fn usage_total_cost_empty_tenant() {
        let db = make_backend();
        let total = db.get_total_cost("nonexistent").unwrap();
        assert!((total - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn usage_time_range_filter() {
        let db = make_backend();

        let mut old = make_usage_record("t1", "gpt-4o", 50);
        old.timestamp = Utc::now() - Duration::days(10);
        db.record_usage(&old).unwrap();

        let recent = make_usage_record("t1", "gpt-4o", 75);
        db.record_usage(&recent).unwrap();

        // Query only the last day.
        let from = Utc::now() - Duration::days(1);
        let to = Utc::now() + Duration::hours(1);
        let records = db.get_usage("t1", from, to).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].tokens_in, 75);
    }

    #[test]
    fn usage_different_tenants_isolated() {
        let db = make_backend();

        db.record_usage(&make_usage_record("alpha", "gpt-4o", 100))
            .unwrap();
        db.record_usage(&make_usage_record("beta", "gpt-4o", 200))
            .unwrap();

        let alpha_cost = db.get_total_cost("alpha").unwrap();
        let beta_cost = db.get_total_cost("beta").unwrap();

        assert!((alpha_cost - 0.1).abs() < 0.0001);
        assert!((beta_cost - 0.2).abs() < 0.0001);
    }

    // -----------------------------------------------------------------------
    // 5. Persona CRUD
    // -----------------------------------------------------------------------

    #[test]
    fn persona_save_and_load() {
        let db = make_backend();

        let persona = PersonaConfig {
            tenant_id: "t1".to_string(),
            agent_role: "coder".to_string(),
            config: serde_json::json!({"model": "claude-opus-4-20250514", "temperature": 0.3}),
            updated_at: Utc::now(),
        };

        db.save_persona(&persona).unwrap();
        let loaded = db.load_persona("t1", "coder").unwrap();

        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.tenant_id, "t1");
        assert_eq!(loaded.agent_role, "coder");
        assert_eq!(loaded.config, persona.config);
    }

    #[test]
    fn persona_load_nonexistent() {
        let db = make_backend();
        let loaded = db.load_persona("ghost", "phantom").unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn persona_overwrite() {
        let db = make_backend();

        let p1 = PersonaConfig {
            tenant_id: "t1".to_string(),
            agent_role: "coder".to_string(),
            config: serde_json::json!({"v": 1}),
            updated_at: Utc::now(),
        };
        db.save_persona(&p1).unwrap();

        let p2 = PersonaConfig {
            tenant_id: "t1".to_string(),
            agent_role: "coder".to_string(),
            config: serde_json::json!({"v": 2}),
            updated_at: Utc::now(),
        };
        db.save_persona(&p2).unwrap();

        let loaded = db.load_persona("t1", "coder").unwrap().unwrap();
        assert_eq!(loaded.config, serde_json::json!({"v": 2}));
    }

    #[test]
    fn persona_list_for_tenant() {
        let db = make_backend();

        for role in &["coder", "reviewer", "planner"] {
            let p = PersonaConfig {
                tenant_id: "t1".to_string(),
                agent_role: role.to_string(),
                config: serde_json::json!({}),
                updated_at: Utc::now(),
            };
            db.save_persona(&p).unwrap();
        }

        let p_other = PersonaConfig {
            tenant_id: "t2".to_string(),
            agent_role: "coder".to_string(),
            config: serde_json::json!({}),
            updated_at: Utc::now(),
        };
        db.save_persona(&p_other).unwrap();

        let t1_personas = db.list_personas("t1").unwrap();
        assert_eq!(t1_personas.len(), 3);

        let t2_personas = db.list_personas("t2").unwrap();
        assert_eq!(t2_personas.len(), 1);
    }

    #[test]
    fn persona_delete() {
        let db = make_backend();

        let p = PersonaConfig {
            tenant_id: "t1".to_string(),
            agent_role: "coder".to_string(),
            config: serde_json::json!({}),
            updated_at: Utc::now(),
        };
        db.save_persona(&p).unwrap();

        let deleted = db.delete_persona("t1", "coder").unwrap();
        assert!(deleted);

        let loaded = db.load_persona("t1", "coder").unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn persona_delete_nonexistent_returns_false() {
        let db = make_backend();
        let deleted = db.delete_persona("ghost", "phantom").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn persona_list_empty_tenant() {
        let db = make_backend();
        let personas = db.list_personas("nonexistent").unwrap();
        assert!(personas.is_empty());
    }

    // -----------------------------------------------------------------------
    // 6. Multiple sessions
    // -----------------------------------------------------------------------

    #[test]
    fn multiple_sessions_with_messages() {
        let db = make_backend();

        let s1 = make_session();
        let s2 = make_session();
        db.save_session(&s1).unwrap();
        db.save_session(&s2).unwrap();

        db.add_message(&make_message(s1.id, Role::User, "s1-msg1"))
            .unwrap();
        db.add_message(&make_message(s1.id, Role::Assistant, "s1-msg2"))
            .unwrap();
        db.add_message(&make_message(s2.id, Role::User, "s2-msg1"))
            .unwrap();

        let s1_msgs = db.get_messages(s1.id).unwrap();
        assert_eq!(s1_msgs.len(), 2);

        let s2_msgs = db.get_messages(s2.id).unwrap();
        assert_eq!(s2_msgs.len(), 1);
    }

    #[test]
    fn delete_session_cascades_messages() {
        let db = make_backend();
        let session = make_session();
        db.save_session(&session).unwrap();

        db.add_message(&make_message(session.id, Role::User, "hello"))
            .unwrap();
        db.add_message(&make_message(session.id, Role::Assistant, "hi"))
            .unwrap();

        db.delete_session(session.id).unwrap();

        // Messages should also be gone.
        let messages = db.get_messages(session.id).unwrap();
        assert!(messages.is_empty());
    }

    // -----------------------------------------------------------------------
    // 7. In-memory mode
    // -----------------------------------------------------------------------

    #[test]
    fn in_memory_backend_works() {
        let db = SqliteBackend::new_in_memory().unwrap();
        let session = make_session();
        db.save_session(&session).unwrap();

        let loaded = db.load_session(session.id).unwrap();
        assert!(loaded.is_some());
    }

    // -----------------------------------------------------------------------
    // 8. File-based persistence
    // -----------------------------------------------------------------------

    #[test]
    fn file_based_persists_across_instances() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");

        let session = make_session();

        // First instance: create session.
        {
            let db = SqliteBackend::new(&db_path).unwrap();
            db.save_session(&session).unwrap();
            db.add_message(&make_message(session.id, Role::User, "persist me"))
                .unwrap();
        }

        // Second instance: should see data.
        {
            let db2 = SqliteBackend::new(&db_path).unwrap();
            let loaded = db2.load_session(session.id).unwrap();
            assert!(loaded.is_some());

            let messages = db2.get_messages(session.id).unwrap();
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0].content, "persist me");
        }
    }

    // -----------------------------------------------------------------------
    // 9. Error on invalid path
    // -----------------------------------------------------------------------

    #[test]
    fn error_on_invalid_path() {
        // Attempt to open a DB in a non-writable / nonexistent deep path.
        let result = SqliteBackend::new(std::path::Path::new(
            "/nonexistent_root_dir/deep/path/test.db",
        ));
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // 10. Concurrent reads (via Arc)
    // -----------------------------------------------------------------------

    #[test]
    fn concurrent_reads() {
        let db = Arc::new(make_backend());

        let session = make_session();
        db.save_session(&session).unwrap();

        for i in 0..20 {
            let mut msg = make_message(session.id, Role::User, &format!("msg-{i}"));
            msg.timestamp = Utc::now() + Duration::milliseconds(i * 10);
            db.add_message(&msg).unwrap();
        }

        let mut handles = Vec::new();
        for _ in 0..10 {
            let db_clone = Arc::clone(&db);
            let sid = session.id;
            handles.push(std::thread::spawn(move || {
                let messages = db_clone.get_messages(sid).unwrap();
                assert_eq!(messages.len(), 20);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }
    }

    // -----------------------------------------------------------------------
    // 11. Session timestamps preserved
    // -----------------------------------------------------------------------

    #[test]
    fn session_timestamps_preserved() {
        let db = make_backend();
        let session = make_session();
        let created = session.created_at;
        let updated = session.updated_at;

        db.save_session(&session).unwrap();
        let loaded = db.load_session(session.id).unwrap().unwrap();

        // Allow 1-second tolerance for RFC3339 rounding.
        assert!(
            (loaded.created_at - created)
                .num_milliseconds()
                .unsigned_abs()
                < 1000
        );
        assert!(
            (loaded.updated_at - updated)
                .num_milliseconds()
                .unsigned_abs()
                < 1000
        );
    }

    // -----------------------------------------------------------------------
    // 12. Usage agent_role round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn usage_agent_role_preserved() {
        let db = make_backend();
        let mut record = make_usage_record("t1", "gpt-4o", 100);
        record.agent_role = "reviewer".to_string();
        db.record_usage(&record).unwrap();

        let from = Utc::now() - Duration::hours(1);
        let to = Utc::now() + Duration::hours(1);
        let records = db.get_usage("t1", from, to).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].agent_role, "reviewer");
    }

    // -----------------------------------------------------------------------
    // 13. Large batch insert
    // -----------------------------------------------------------------------

    #[test]
    fn large_batch_messages() {
        let db = make_backend();
        let session = make_session();
        db.save_session(&session).unwrap();

        for i in 0..100 {
            let mut msg = make_message(session.id, Role::User, &format!("msg-{i}"));
            msg.timestamp = Utc::now() + Duration::milliseconds(i * 5);
            db.add_message(&msg).unwrap();
        }

        let messages = db.get_messages(session.id).unwrap();
        assert_eq!(messages.len(), 100);
    }

    // -----------------------------------------------------------------------
    // 14. Persona config with nested JSON
    // -----------------------------------------------------------------------

    #[test]
    fn persona_nested_json_config() {
        let db = make_backend();

        let config = serde_json::json!({
            "system_prompt": "You are a Rust expert.",
            "temperature": 0.3,
            "tools": ["echo", "time", "memory_search"],
            "nested": {
                "a": 1,
                "b": [true, false]
            }
        });

        let p = PersonaConfig {
            tenant_id: "t1".to_string(),
            agent_role: "coder".to_string(),
            config: config.clone(),
            updated_at: Utc::now(),
        };
        db.save_persona(&p).unwrap();

        let loaded = db.load_persona("t1", "coder").unwrap().unwrap();
        assert_eq!(loaded.config, config);
    }

    // -----------------------------------------------------------------------
    // 15. Session with empty metadata
    // -----------------------------------------------------------------------

    #[test]
    fn session_empty_metadata() {
        let db = make_backend();
        let session = make_session();
        db.save_session(&session).unwrap();

        let loaded = db.load_session(session.id).unwrap().unwrap();
        assert!(loaded.metadata.is_empty());
    }

    // -----------------------------------------------------------------------
    // 16. Multiple usage records per tenant
    // -----------------------------------------------------------------------

    #[test]
    fn many_usage_records_per_tenant() {
        let db = make_backend();

        for i in 0..50 {
            let mut r = make_usage_record("heavy-user", "gpt-4o", 10 + i);
            r.timestamp = Utc::now() + Duration::milliseconds(i as i64 * 10);
            db.record_usage(&r).unwrap();
        }

        let total = db.get_total_cost("heavy-user").unwrap();
        // Each record: (10 + i) * 0.001, sum for i=0..49
        let expected: f64 = (0..50).map(|i| (10 + i) as f64 * 0.001).sum();
        assert!((total - expected).abs() < 0.0001);
    }

    // -----------------------------------------------------------------------
    // 17. Pagination edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn pagination_limit_zero() {
        let db = make_backend();
        let session = make_session();
        db.save_session(&session).unwrap();

        db.add_message(&make_message(session.id, Role::User, "hello"))
            .unwrap();

        let page = db.get_messages_paginated(session.id, 0, 0).unwrap();
        assert!(page.is_empty());
    }

    #[test]
    fn pagination_offset_beyond_count() {
        let db = make_backend();
        let session = make_session();
        db.save_session(&session).unwrap();

        db.add_message(&make_message(session.id, Role::User, "hello"))
            .unwrap();

        let page = db.get_messages_paginated(session.id, 10, 100).unwrap();
        assert!(page.is_empty());
    }

    // -----------------------------------------------------------------------
    // 18. Delete session then re-create with same ID
    // -----------------------------------------------------------------------

    #[test]
    fn recreate_session_after_delete() {
        let db = make_backend();
        let session = make_session();

        db.save_session(&session).unwrap();
        db.add_message(&make_message(session.id, Role::User, "old"))
            .unwrap();

        db.delete_session(session.id).unwrap();

        // Re-create with same ID.
        db.save_session(&session).unwrap();
        let messages = db.get_messages(session.id).unwrap();
        assert!(messages.is_empty()); // old messages should be gone

        let loaded = db.load_session(session.id).unwrap();
        assert!(loaded.is_some());
    }

    // -----------------------------------------------------------------------
    // 19. Concurrent writes + reads mixed
    // -----------------------------------------------------------------------

    #[test]
    fn concurrent_writes_and_reads() {
        let db = Arc::new(make_backend());
        let session = make_session();
        db.save_session(&session).unwrap();

        let mut handles = Vec::new();

        // Writers.
        for i in 0..10 {
            let db_clone = Arc::clone(&db);
            let sid = session.id;
            handles.push(std::thread::spawn(move || {
                let mut msg = make_message(sid, Role::User, &format!("concurrent-{i}"));
                msg.timestamp = Utc::now() + Duration::milliseconds(i * 10);
                db_clone.add_message(&msg).unwrap();
            }));
        }

        // Readers.
        for _ in 0..10 {
            let db_clone = Arc::clone(&db);
            let sid = session.id;
            handles.push(std::thread::spawn(move || {
                let _ = db_clone.get_messages(sid).unwrap();
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let final_msgs = db.get_messages(session.id).unwrap();
        assert_eq!(final_msgs.len(), 10);
    }

    // -----------------------------------------------------------------------
    // 20. List sessions returns empty on fresh DB
    // -----------------------------------------------------------------------

    #[test]
    fn list_sessions_empty() {
        let db = make_backend();
        let ids = db.list_sessions().unwrap();
        assert!(ids.is_empty());
    }

    // -----------------------------------------------------------------------
    // 21. Usage empty time range
    // -----------------------------------------------------------------------

    #[test]
    fn usage_empty_time_range() {
        let db = make_backend();
        db.record_usage(&make_usage_record("t1", "gpt-4o", 100))
            .unwrap();

        // Query a range in the past that excludes the record.
        let from = Utc::now() - Duration::days(10);
        let to = Utc::now() - Duration::days(5);
        let records = db.get_usage("t1", from, to).unwrap();
        assert!(records.is_empty());
    }

    // -----------------------------------------------------------------------
    // 22. Persona updated_at preserved
    // -----------------------------------------------------------------------

    #[test]
    fn persona_updated_at_preserved() {
        let db = make_backend();

        let now = Utc::now();
        let p = PersonaConfig {
            tenant_id: "t1".to_string(),
            agent_role: "coder".to_string(),
            config: serde_json::json!({}),
            updated_at: now,
        };
        db.save_persona(&p).unwrap();

        let loaded = db.load_persona("t1", "coder").unwrap().unwrap();
        assert!((loaded.updated_at - now).num_milliseconds().unsigned_abs() < 1000);
    }
}
