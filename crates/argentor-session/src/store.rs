use crate::session::Session;
use argentor_core::{ArgentorError, ArgentorResult};
use async_trait::async_trait;
use std::path::PathBuf;
use uuid::Uuid;

/// Persistence backend for agent sessions.
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// Persist a new session.
    async fn create(&self, session: &Session) -> ArgentorResult<()>;
    /// Retrieve a session by ID.
    async fn get(&self, id: Uuid) -> ArgentorResult<Option<Session>>;
    /// Update an existing session.
    async fn update(&self, session: &Session) -> ArgentorResult<()>;
    /// Delete a session by ID.
    async fn delete(&self, id: Uuid) -> ArgentorResult<()>;
    /// List all stored session IDs.
    async fn list(&self) -> ArgentorResult<Vec<Uuid>>;
}

/// File-based session store (JSON files on disk). Good enough for MVP.
pub struct FileSessionStore {
    dir: PathBuf,
}

impl FileSessionStore {
    /// Create a new file-based session store under the given directory.
    pub async fn new(dir: PathBuf) -> ArgentorResult<Self> {
        tokio::fs::create_dir_all(&dir).await?;
        Ok(Self { dir })
    }

    fn session_path(&self, id: Uuid) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }
}

#[async_trait]
impl SessionStore for FileSessionStore {
    async fn create(&self, session: &Session) -> ArgentorResult<()> {
        let path = self.session_path(session.id);
        let json = serde_json::to_string_pretty(session)?;
        tokio::fs::write(path, json).await?;
        Ok(())
    }

    async fn get(&self, id: Uuid) -> ArgentorResult<Option<Session>> {
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
            tokio::fs::remove_file(path).await?;
        }
        Ok(())
    }

    async fn list(&self) -> ArgentorResult<Vec<Uuid>> {
        let mut entries = tokio::fs::read_dir(&self.dir).await?;
        let mut ids = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            if let Some(name) = entry.file_name().to_str() {
                if let Some(stem) = name.strip_suffix(".json") {
                    if let Ok(id) = Uuid::parse_str(stem) {
                        ids.push(id);
                    }
                }
            }
        }
        Ok(ids)
    }
}
