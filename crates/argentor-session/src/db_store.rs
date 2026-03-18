//! Database-backed session store abstraction.
//!
//! Provides [`DatabaseSessionStore`] — a session persistence backend designed
//! around a relational database model. The current implementation uses JSON
//! files organised in a database-like directory layout (`sessions/`, `index.json`)
//! so there are **no extra dependencies**. When the `sqlite` or `postgres`
//! feature flags are enabled in a future release the internals will swap to
//! [`sqlx`](https://crates.io/crates/sqlx) while the public API stays the same.
//!
//! # Directory layout (file-based fallback)
//!
//! ```text
//! <base_dir>/
//!   sessions/
//!     <uuid>.json          — serialised [`Session`]
//!   index.json             — `{ "<uuid>": { …metadata… }, … }`
//! ```
//!
//! # Feature flags
//!
//! | Flag       | Effect                                           |
//! |------------|--------------------------------------------------|
//! | `sqlite`   | (future) Use SQLite via `sqlx`                   |
//! | `postgres` | (future) Use PostgreSQL via `sqlx`                |

use crate::session::Session;
use crate::store::SessionStore;
use argentor_core::{ArgentorError, ArgentorResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// DatabaseConfig
// ---------------------------------------------------------------------------

/// Configuration describing how to connect to the backing store.
///
/// For now only the directory layout is used regardless of variant, but the
/// enum captures the connection parameters that `sqlx` will need later.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DatabaseConfig {
    /// Use a local SQLite database file.
    Sqlite {
        /// Path to the SQLite file (e.g. `"./data/argentor.db"`).
        path: String,
    },
    /// Use a PostgreSQL server.
    Postgres {
        /// Full connection string (`postgres://user:pass@host/db`).
        connection_string: String,
        /// Maximum number of connections in the pool.
        max_connections: u32,
    },
}

// ---------------------------------------------------------------------------
// Index entry
// ---------------------------------------------------------------------------

/// Metadata cached in the on-disk index for fast lookups without
/// deserialising every session file.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexEntry {
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    metadata: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// DatabaseSessionStore
// ---------------------------------------------------------------------------

/// A session store that mirrors a relational database layout using the
/// local filesystem. Thread-safe via an internal [`RwLock`] that guards
/// the in-memory index (simulating a connection pool).
///
/// Implements the [`SessionStore`] trait so it can be used as a drop-in
/// replacement for [`FileSessionStore`](crate::store::FileSessionStore).
pub struct DatabaseSessionStore {
    /// Root directory that contains `sessions/` and `index.json`.
    base_dir: PathBuf,
    /// Configuration that was used to create this store.
    #[allow(dead_code)]
    config: DatabaseConfig,
    /// In-memory cache of the session index, protected by a read-write lock
    /// to allow concurrent reads.
    index: Arc<RwLock<HashMap<Uuid, IndexEntry>>>,
}

impl DatabaseSessionStore {
    /// Create a new [`DatabaseSessionStore`].
    ///
    /// The required directory structure is created automatically if it does
    /// not exist yet. The on-disk index is loaded into memory at startup.
    pub async fn new(config: DatabaseConfig, base_dir: PathBuf) -> ArgentorResult<Self> {
        let sessions_dir = base_dir.join("sessions");
        tokio::fs::create_dir_all(&sessions_dir).await?;

        let index = Self::load_index(&base_dir).await?;

        Ok(Self {
            base_dir,
            config,
            index: Arc::new(RwLock::new(index)),
        })
    }

    // -- path helpers -------------------------------------------------------

    fn sessions_dir(&self) -> PathBuf {
        self.base_dir.join("sessions")
    }

    fn session_path(&self, id: Uuid) -> PathBuf {
        self.sessions_dir().join(format!("{id}.json"))
    }

    fn index_path(&self) -> PathBuf {
        self.base_dir.join("index.json")
    }

    // -- index persistence --------------------------------------------------

    async fn load_index(base_dir: &std::path::Path) -> ArgentorResult<HashMap<Uuid, IndexEntry>> {
        let path = base_dir.join("index.json");
        if !path.exists() {
            return Ok(HashMap::new());
        }
        let data = tokio::fs::read_to_string(&path).await?;
        let index: HashMap<Uuid, IndexEntry> = serde_json::from_str(&data)
            .map_err(|e| ArgentorError::Session(format!("Failed to parse index: {e}")))?;
        Ok(index)
    }

    async fn persist_index(&self, index: &HashMap<Uuid, IndexEntry>) -> ArgentorResult<()> {
        let json = serde_json::to_string_pretty(index)?;
        tokio::fs::write(self.index_path(), json).await?;
        Ok(())
    }

    // -- public helpers (beyond SessionStore) --------------------------------

    /// Return session IDs whose metadata contains a key with the given
    /// string value.
    ///
    /// Comparison is exact (case-sensitive). Only the string representation
    /// of the JSON value is compared, so `serde_json::Value::String("foo")`
    /// matches the query value `"foo"`.
    pub async fn query_by_metadata(&self, key: &str, value: &str) -> ArgentorResult<Vec<Uuid>> {
        let index = self.index.read().await;
        let ids = index
            .iter()
            .filter(|(_, entry)| {
                entry
                    .metadata
                    .get(key)
                    .and_then(|v| v.as_str())
                    .is_some_and(|v| v == value)
            })
            .map(|(id, _)| *id)
            .collect();
        Ok(ids)
    }

    /// Remove sessions older than `max_age` (measured from `updated_at`).
    ///
    /// Returns the number of sessions that were deleted.
    pub async fn cleanup_expired(&self, max_age: Duration) -> ArgentorResult<usize> {
        let now = chrono::Utc::now();
        let cutoff = now
            - chrono::Duration::from_std(max_age).map_err(|e| {
                ArgentorError::Session(format!("Invalid duration for cleanup: {e}"))
            })?;

        let mut index = self.index.write().await;
        let expired: Vec<Uuid> = index
            .iter()
            .filter(|(_, entry)| entry.updated_at < cutoff)
            .map(|(id, _)| *id)
            .collect();

        let count = expired.len();
        for id in &expired {
            let path = self.session_path(*id);
            if path.exists() {
                tokio::fs::remove_file(&path).await?;
            }
            index.remove(id);
        }

        self.persist_index(&index).await?;
        Ok(count)
    }

    /// Return the total number of sessions tracked in the index.
    pub async fn count(&self) -> ArgentorResult<usize> {
        let index = self.index.read().await;
        Ok(index.len())
    }
}

// ---------------------------------------------------------------------------
// SessionStore implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl SessionStore for DatabaseSessionStore {
    async fn create(&self, session: &Session) -> ArgentorResult<()> {
        let path = self.session_path(session.id);
        let json = serde_json::to_string_pretty(session)?;
        tokio::fs::write(path, json).await?;

        let entry = IndexEntry {
            created_at: session.created_at,
            updated_at: session.updated_at,
            metadata: session.metadata.clone(),
        };

        let mut index = self.index.write().await;
        index.insert(session.id, entry);
        self.persist_index(&index).await?;

        Ok(())
    }

    async fn get(&self, id: Uuid) -> ArgentorResult<Option<Session>> {
        // Fast-path: check the index first so we avoid a filesystem hit for
        // IDs that were never stored.
        {
            let index = self.index.read().await;
            if !index.contains_key(&id) {
                return Ok(None);
            }
        }

        let path = self.session_path(id);
        if !path.exists() {
            return Ok(None);
        }

        let data = tokio::fs::read_to_string(path).await?;
        let session: Session = serde_json::from_str(&data)
            .map_err(|e| ArgentorError::Session(format!("Failed to parse session: {e}")))?;
        Ok(Some(session))
    }

    async fn update(&self, session: &Session) -> ArgentorResult<()> {
        self.create(session).await
    }

    async fn delete(&self, id: Uuid) -> ArgentorResult<()> {
        let path = self.session_path(id);
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
        }

        let mut index = self.index.write().await;
        index.remove(&id);
        self.persist_index(&index).await?;

        Ok(())
    }

    async fn list(&self) -> ArgentorResult<Vec<Uuid>> {
        let index = self.index.read().await;
        Ok(index.keys().copied().collect())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper: create a store backed by a temporary directory.
    async fn temp_store(config: DatabaseConfig) -> (TempDir, DatabaseSessionStore) {
        let tmp = TempDir::new().unwrap();
        let store = DatabaseSessionStore::new(config, tmp.path().to_path_buf())
            .await
            .unwrap();
        (tmp, store)
    }

    fn sqlite_config() -> DatabaseConfig {
        DatabaseConfig::Sqlite {
            path: ":memory:".into(),
        }
    }

    fn postgres_config() -> DatabaseConfig {
        DatabaseConfig::Postgres {
            connection_string: "postgres://localhost/test".into(),
            max_connections: 5,
        }
    }

    // -- DatabaseConfig creation --------------------------------------------

    #[test]
    fn database_config_sqlite_variant() {
        let cfg = sqlite_config();
        assert!(matches!(cfg, DatabaseConfig::Sqlite { .. }));
        if let DatabaseConfig::Sqlite { path } = &cfg {
            assert_eq!(path, ":memory:");
        }
    }

    #[test]
    fn database_config_postgres_variant() {
        let cfg = postgres_config();
        assert!(matches!(cfg, DatabaseConfig::Postgres { .. }));
        if let DatabaseConfig::Postgres {
            connection_string,
            max_connections,
        } = &cfg
        {
            assert_eq!(connection_string, "postgres://localhost/test");
            assert_eq!(*max_connections, 5);
        }
    }

    // -- CRUD ---------------------------------------------------------------

    #[tokio::test]
    async fn create_and_get_session() {
        let (_tmp, store) = temp_store(sqlite_config()).await;
        let session = Session::new();

        store.create(&session).await.unwrap();
        let loaded = store.get(session.id).await.unwrap();

        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.id, session.id);
    }

    #[tokio::test]
    async fn get_nonexistent_returns_none() {
        let (_tmp, store) = temp_store(sqlite_config()).await;
        let result = store.get(Uuid::new_v4()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn update_overwrites_session() {
        let (_tmp, store) = temp_store(sqlite_config()).await;
        let mut session = Session::new();
        store.create(&session).await.unwrap();

        session
            .metadata
            .insert("key".into(), serde_json::json!("v2"));
        store.update(&session).await.unwrap();

        let loaded = store.get(session.id).await.unwrap().unwrap();
        assert_eq!(
            loaded.metadata.get("key").unwrap(),
            &serde_json::json!("v2")
        );
    }

    #[tokio::test]
    async fn delete_removes_session() {
        let (_tmp, store) = temp_store(sqlite_config()).await;
        let session = Session::new();
        store.create(&session).await.unwrap();

        store.delete(session.id).await.unwrap();

        assert!(store.get(session.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_nonexistent_is_ok() {
        let (_tmp, store) = temp_store(sqlite_config()).await;
        // Should not error.
        store.delete(Uuid::new_v4()).await.unwrap();
    }

    #[tokio::test]
    async fn list_returns_all_ids() {
        let (_tmp, store) = temp_store(sqlite_config()).await;
        let s1 = Session::new();
        let s2 = Session::new();
        let s3 = Session::new();

        store.create(&s1).await.unwrap();
        store.create(&s2).await.unwrap();
        store.create(&s3).await.unwrap();

        let mut ids = store.list().await.unwrap();
        ids.sort();

        let mut expected = vec![s1.id, s2.id, s3.id];
        expected.sort();

        assert_eq!(ids, expected);
    }

    // -- query_by_metadata --------------------------------------------------

    #[tokio::test]
    async fn query_by_metadata_finds_matching() {
        let (_tmp, store) = temp_store(sqlite_config()).await;

        let mut s1 = Session::new();
        s1.metadata
            .insert("env".into(), serde_json::json!("production"));
        store.create(&s1).await.unwrap();

        let mut s2 = Session::new();
        s2.metadata
            .insert("env".into(), serde_json::json!("staging"));
        store.create(&s2).await.unwrap();

        let mut s3 = Session::new();
        s3.metadata
            .insert("env".into(), serde_json::json!("production"));
        store.create(&s3).await.unwrap();

        let results = store.query_by_metadata("env", "production").await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.contains(&s1.id));
        assert!(results.contains(&s3.id));
    }

    #[tokio::test]
    async fn query_by_metadata_no_match() {
        let (_tmp, store) = temp_store(sqlite_config()).await;
        let session = Session::new();
        store.create(&session).await.unwrap();

        let results = store
            .query_by_metadata("nonexistent", "value")
            .await
            .unwrap();
        assert!(results.is_empty());
    }

    // -- cleanup_expired ----------------------------------------------------

    #[tokio::test]
    async fn cleanup_expired_removes_old_sessions() {
        let (_tmp, store) = temp_store(sqlite_config()).await;

        // Create a session and manually backdate its index entry.
        let session = Session::new();
        store.create(&session).await.unwrap();

        {
            let mut index = store.index.write().await;
            if let Some(entry) = index.get_mut(&session.id) {
                entry.updated_at = chrono::Utc::now() - chrono::Duration::hours(25);
            }
            store.persist_index(&index).await.unwrap();
        }

        // Create a fresh session that should NOT be cleaned up.
        let fresh = Session::new();
        store.create(&fresh).await.unwrap();

        let removed = store
            .cleanup_expired(Duration::from_secs(24 * 3600))
            .await
            .unwrap();

        assert_eq!(removed, 1);
        assert!(store.get(session.id).await.unwrap().is_none());
        assert!(store.get(fresh.id).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn cleanup_expired_nothing_to_remove() {
        let (_tmp, store) = temp_store(sqlite_config()).await;
        let session = Session::new();
        store.create(&session).await.unwrap();

        let removed = store
            .cleanup_expired(Duration::from_secs(3600))
            .await
            .unwrap();

        assert_eq!(removed, 0);
    }

    // -- count --------------------------------------------------------------

    #[tokio::test]
    async fn count_reflects_store_size() {
        let (_tmp, store) = temp_store(sqlite_config()).await;
        assert_eq!(store.count().await.unwrap(), 0);

        let s1 = Session::new();
        store.create(&s1).await.unwrap();
        assert_eq!(store.count().await.unwrap(), 1);

        let s2 = Session::new();
        store.create(&s2).await.unwrap();
        assert_eq!(store.count().await.unwrap(), 2);

        store.delete(s1.id).await.unwrap();
        assert_eq!(store.count().await.unwrap(), 1);
    }

    // -- concurrent access --------------------------------------------------

    #[tokio::test]
    async fn concurrent_creates_do_not_lose_data() {
        let (_tmp, store) = temp_store(sqlite_config()).await;
        let store = Arc::new(store);

        let mut handles = Vec::new();
        let mut expected_ids = Vec::new();

        for _ in 0..20 {
            let s = Session::new();
            expected_ids.push(s.id);
            let store_clone = Arc::clone(&store);
            handles.push(tokio::spawn(async move {
                store_clone.create(&s).await.unwrap();
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        let mut ids = store.list().await.unwrap();
        ids.sort();
        expected_ids.sort();
        assert_eq!(ids, expected_ids);
        assert_eq!(store.count().await.unwrap(), 20);
    }

    #[tokio::test]
    async fn concurrent_read_write() {
        let (_tmp, store) = temp_store(sqlite_config()).await;
        let store = Arc::new(store);

        // Pre-populate some sessions.
        let mut ids = Vec::new();
        for _ in 0..5 {
            let s = Session::new();
            ids.push(s.id);
            store.create(&s).await.unwrap();
        }

        let mut handles = Vec::new();

        // Readers.
        for id in &ids {
            let store_clone = Arc::clone(&store);
            let id = *id;
            handles.push(tokio::spawn(async move {
                let result = store_clone.get(id).await.unwrap();
                assert!(result.is_some());
            }));
        }

        // Writer — add more sessions concurrently with readers.
        for _ in 0..5 {
            let store_clone = Arc::clone(&store);
            handles.push(tokio::spawn(async move {
                let s = Session::new();
                store_clone.create(&s).await.unwrap();
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        assert_eq!(store.count().await.unwrap(), 10);
    }

    // -- persistence across instances ---------------------------------------

    #[tokio::test]
    async fn index_persists_across_instances() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().to_path_buf();
        let session = Session::new();

        // First instance: create a session.
        {
            let store = DatabaseSessionStore::new(sqlite_config(), dir.clone())
                .await
                .unwrap();
            store.create(&session).await.unwrap();
        }

        // Second instance: should see the session via the persisted index.
        {
            let store2 = DatabaseSessionStore::new(sqlite_config(), dir)
                .await
                .unwrap();
            let loaded = store2.get(session.id).await.unwrap();
            assert!(loaded.is_some());
            assert_eq!(loaded.unwrap().id, session.id);
            assert_eq!(store2.count().await.unwrap(), 1);
        }
    }
}
