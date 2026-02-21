use crate::session::Session;
use agentor_core::{AgentorError, AgentorResult};
use async_trait::async_trait;
use std::path::PathBuf;
use uuid::Uuid;

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn create(&self, session: &Session) -> AgentorResult<()>;
    async fn get(&self, id: Uuid) -> AgentorResult<Option<Session>>;
    async fn update(&self, session: &Session) -> AgentorResult<()>;
    async fn delete(&self, id: Uuid) -> AgentorResult<()>;
    async fn list(&self) -> AgentorResult<Vec<Uuid>>;
}

/// File-based session store (JSON files on disk). Good enough for MVP.
pub struct FileSessionStore {
    dir: PathBuf,
}

impl FileSessionStore {
    pub async fn new(dir: PathBuf) -> AgentorResult<Self> {
        tokio::fs::create_dir_all(&dir).await?;
        Ok(Self { dir })
    }

    fn session_path(&self, id: Uuid) -> PathBuf {
        self.dir.join(format!("{}.json", id))
    }
}

#[async_trait]
impl SessionStore for FileSessionStore {
    async fn create(&self, session: &Session) -> AgentorResult<()> {
        let path = self.session_path(session.id);
        let json = serde_json::to_string_pretty(session)?;
        tokio::fs::write(path, json).await?;
        Ok(())
    }

    async fn get(&self, id: Uuid) -> AgentorResult<Option<Session>> {
        let path = self.session_path(id);
        if !path.exists() {
            return Ok(None);
        }
        let data = tokio::fs::read_to_string(path).await?;
        let session: Session = serde_json::from_str(&data)
            .map_err(|e| AgentorError::Session(format!("Failed to parse session: {}", e)))?;
        Ok(Some(session))
    }

    async fn update(&self, session: &Session) -> AgentorResult<()> {
        self.create(session).await
    }

    async fn delete(&self, id: Uuid) -> AgentorResult<()> {
        let path = self.session_path(id);
        if path.exists() {
            tokio::fs::remove_file(path).await?;
        }
        Ok(())
    }

    async fn list(&self) -> AgentorResult<Vec<Uuid>> {
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
