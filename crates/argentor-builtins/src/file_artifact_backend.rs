//! File-system-based artifact backend for persistent storage.
//!
//! Stores artifacts as files on disk with sidecar JSON metadata.
//! Unlike [`InMemoryArtifactBackend`](super::InMemoryArtifactBackend), artifacts
//! survive process restarts and can be shared across runs.
//!
//! # Storage layout
//!
//! ```text
//! base_dir/
//!   artifacts/
//!     {key}/
//!       content.dat    -- the artifact content
//!       metadata.json  -- kind, stored_at, size
//!   index.json         -- list of all keys with metadata
//! ```

use crate::artifact_store::{ArtifactBackend, ArtifactEntry};
use argentor_core::ArgentorResult;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::RwLock;

/// Metadata for a single stored artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactMeta {
    /// Artifact key (same as the directory name).
    pub key: String,
    /// Kind of artifact (e.g. "code", "spec", "test").
    pub kind: String,
    /// Timestamp when the artifact was stored.
    pub stored_at: DateTime<Utc>,
    /// Size of the content in bytes.
    pub size_bytes: usize,
}

/// Serializable index that tracks all stored artifacts.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ArtifactIndex {
    entries: Vec<ArtifactMeta>,
}

/// File-system-based artifact backend for persistent storage.
///
/// Stores each artifact in its own directory under `base_dir/artifacts/{key}/`,
/// with `content.dat` for the raw content and `metadata.json` for sidecar metadata.
/// A top-level `index.json` tracks all stored artifacts.
///
/// Concurrent access is protected by an async `RwLock` to prevent torn reads/writes.
pub struct FileArtifactBackend {
    base_dir: PathBuf,
    /// Lock to serialize mutations and prevent concurrent index corruption.
    lock: RwLock<()>,
}

impl FileArtifactBackend {
    /// Create a new `FileArtifactBackend` rooted at the given directory.
    ///
    /// The directory structure is created lazily when [`init`](Self::init) is called
    /// or on the first operation.
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            base_dir,
            lock: RwLock::new(()),
        }
    }

    /// Ensure the required directory structure exists.
    pub async fn init(&self) -> ArgentorResult<()> {
        let artifacts_dir = self.base_dir.join("artifacts");
        tokio::fs::create_dir_all(&artifacts_dir)
            .await
            .map_err(|e| {
                argentor_core::ArgentorError::Skill(format!(
                    "Failed to create artifacts directory: {e}"
                ))
            })?;

        let index_path = self.base_dir.join("index.json");
        if !index_path.exists() {
            let empty_index = ArtifactIndex::default();
            let json = serde_json::to_string_pretty(&empty_index).map_err(|e| {
                argentor_core::ArgentorError::Skill(format!("Failed to serialize index: {e}"))
            })?;
            tokio::fs::write(&index_path, json).await.map_err(|e| {
                argentor_core::ArgentorError::Skill(format!("Failed to write index.json: {e}"))
            })?;
        }

        Ok(())
    }

    /// Return the path to the artifacts sub-directory for a given key.
    fn artifact_dir(&self, key: &str) -> PathBuf {
        self.base_dir.join("artifacts").join(key)
    }

    /// Return the path to `index.json`.
    fn index_path(&self) -> PathBuf {
        self.base_dir.join("index.json")
    }

    /// Validate that a key is safe and does not attempt path traversal.
    fn validate_key(key: &str) -> ArgentorResult<()> {
        if key.is_empty() {
            return Err(argentor_core::ArgentorError::Skill(
                "Artifact key must not be empty".to_string(),
            ));
        }
        if key.contains("..") || key.contains('/') || key.contains('\\') {
            return Err(argentor_core::ArgentorError::Skill(format!(
                "Artifact key contains invalid characters (path traversal attempt): {key}"
            )));
        }
        Ok(())
    }

    /// Read the index from disk.
    async fn read_index(&self) -> ArgentorResult<ArtifactIndex> {
        let index_path = self.index_path();
        if !index_path.exists() {
            return Ok(ArtifactIndex::default());
        }
        let data = tokio::fs::read_to_string(&index_path).await.map_err(|e| {
            argentor_core::ArgentorError::Skill(format!("Failed to read index.json: {e}"))
        })?;
        let index: ArtifactIndex = serde_json::from_str(&data).map_err(|e| {
            argentor_core::ArgentorError::Skill(format!("Failed to parse index.json: {e}"))
        })?;
        Ok(index)
    }

    /// Write the index to disk.
    async fn write_index(&self, index: &ArtifactIndex) -> ArgentorResult<()> {
        let json = serde_json::to_string_pretty(index).map_err(|e| {
            argentor_core::ArgentorError::Skill(format!("Failed to serialize index: {e}"))
        })?;
        tokio::fs::write(self.index_path(), json)
            .await
            .map_err(|e| {
                argentor_core::ArgentorError::Skill(format!("Failed to write index.json: {e}"))
            })?;
        Ok(())
    }
}

#[async_trait]
impl ArtifactBackend for FileArtifactBackend {
    async fn store(&self, key: &str, content: &str, kind: &str) -> ArgentorResult<String> {
        Self::validate_key(key)?;

        let _guard = self.lock.write().await;

        // Ensure directories exist.
        self.init().await?;

        // Create artifact directory.
        let dir = self.artifact_dir(key);
        tokio::fs::create_dir_all(&dir).await.map_err(|e| {
            argentor_core::ArgentorError::Skill(format!(
                "Failed to create artifact directory for '{key}': {e}"
            ))
        })?;

        // Write content.
        tokio::fs::write(dir.join("content.dat"), content)
            .await
            .map_err(|e| {
                argentor_core::ArgentorError::Skill(format!(
                    "Failed to write content for '{key}': {e}"
                ))
            })?;

        // Write metadata.
        let meta = ArtifactMeta {
            key: key.to_string(),
            kind: kind.to_string(),
            stored_at: Utc::now(),
            size_bytes: content.len(),
        };
        let meta_json = serde_json::to_string_pretty(&meta).map_err(|e| {
            argentor_core::ArgentorError::Skill(format!("Failed to serialize metadata: {e}"))
        })?;
        tokio::fs::write(dir.join("metadata.json"), meta_json)
            .await
            .map_err(|e| {
                argentor_core::ArgentorError::Skill(format!(
                    "Failed to write metadata for '{key}': {e}"
                ))
            })?;

        // Update index.
        let mut index = self.read_index().await?;
        index.entries.retain(|e| e.key != key);
        index.entries.push(meta);
        self.write_index(&index).await?;

        Ok(key.to_string())
    }

    async fn retrieve(&self, key: &str) -> ArgentorResult<Option<String>> {
        Self::validate_key(key)?;

        let _guard = self.lock.read().await;

        let content_path = self.artifact_dir(key).join("content.dat");
        if !content_path.exists() {
            return Ok(None);
        }

        let content = tokio::fs::read_to_string(&content_path)
            .await
            .map_err(|e| {
                argentor_core::ArgentorError::Skill(format!(
                    "Failed to read content for '{key}': {e}"
                ))
            })?;

        Ok(Some(content))
    }

    async fn list(&self) -> ArgentorResult<Vec<ArtifactEntry>> {
        let _guard = self.lock.read().await;

        let index = self.read_index().await?;

        Ok(index
            .entries
            .iter()
            .map(|meta| ArtifactEntry {
                key: meta.key.clone(),
                kind: meta.kind.clone(),
                size: meta.size_bytes,
            })
            .collect())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_backend() -> (TempDir, FileArtifactBackend) {
        let tmp = TempDir::new().unwrap();
        let backend = FileArtifactBackend::new(tmp.path().to_path_buf());
        (tmp, backend)
    }

    #[tokio::test]
    async fn test_init_creates_directory_structure() {
        let (tmp, backend) = make_backend();
        backend.init().await.unwrap();

        assert!(tmp.path().join("artifacts").is_dir());
        assert!(tmp.path().join("index.json").is_file());
    }

    #[tokio::test]
    async fn test_store_and_retrieve() {
        let (_tmp, backend) = make_backend();
        backend.init().await.unwrap();

        backend
            .store("main.rs", "fn main() {}", "code")
            .await
            .unwrap();

        let content = backend.retrieve("main.rs").await.unwrap();
        assert_eq!(content, Some("fn main() {}".to_string()));
    }

    #[tokio::test]
    async fn test_retrieve_nonexistent_returns_none() {
        let (_tmp, backend) = make_backend();
        backend.init().await.unwrap();

        let content = backend.retrieve("nonexistent").await.unwrap();
        assert!(content.is_none());
    }

    #[tokio::test]
    async fn test_list_returns_stored_artifacts() {
        let (_tmp, backend) = make_backend();
        backend.init().await.unwrap();

        backend.store("a.rs", "code_a", "code").await.unwrap();
        backend.store("b.md", "spec_b", "spec").await.unwrap();

        let entries = backend.list().await.unwrap();
        assert_eq!(entries.len(), 2);

        let keys: Vec<&str> = entries.iter().map(|e| e.key.as_str()).collect();
        assert!(keys.contains(&"a.rs"));
        assert!(keys.contains(&"b.md"));
    }

    #[tokio::test]
    async fn test_store_overwrites_existing() {
        let (_tmp, backend) = make_backend();
        backend.init().await.unwrap();

        backend.store("file", "v1", "code").await.unwrap();
        backend.store("file", "v2", "code").await.unwrap();

        let content = backend.retrieve("file").await.unwrap();
        assert_eq!(content, Some("v2".to_string()));

        // Index should contain only one entry for this key.
        let entries = backend.list().await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].size, 2); // "v2".len()
    }

    #[tokio::test]
    async fn test_path_traversal_rejected() {
        let (_tmp, backend) = make_backend();
        backend.init().await.unwrap();

        let result = backend.store("../etc/passwd", "bad", "exploit").await;
        assert!(result.is_err());

        let result = backend.store("foo/bar", "bad", "exploit").await;
        assert!(result.is_err());

        let result = backend.store("foo\\bar", "bad", "exploit").await;
        assert!(result.is_err());

        let result = backend.retrieve("../../secret").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_empty_key_rejected() {
        let (_tmp, backend) = make_backend();
        backend.init().await.unwrap();

        let result = backend.store("", "content", "code").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_metadata_file_written() {
        let (tmp, backend) = make_backend();
        backend.init().await.unwrap();

        backend
            .store("test-artifact", "hello world", "text")
            .await
            .unwrap();

        // Verify the sidecar metadata file exists and has correct content.
        let meta_path = tmp
            .path()
            .join("artifacts")
            .join("test-artifact")
            .join("metadata.json");
        assert!(meta_path.is_file());

        let meta_str = tokio::fs::read_to_string(&meta_path).await.unwrap();
        let meta: ArtifactMeta = serde_json::from_str(&meta_str).unwrap();

        assert_eq!(meta.key, "test-artifact");
        assert_eq!(meta.kind, "text");
        assert_eq!(meta.size_bytes, 11); // "hello world".len()
    }

    #[tokio::test]
    async fn test_list_empty_store() {
        let (_tmp, backend) = make_backend();
        backend.init().await.unwrap();

        let entries = backend.list().await.unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn test_store_without_explicit_init() {
        let (_tmp, backend) = make_backend();

        // store() should call init() internally, so this works without explicit init.
        backend.store("auto-init", "content", "code").await.unwrap();

        let content = backend.retrieve("auto-init").await.unwrap();
        assert_eq!(content, Some("content".to_string()));
    }

    #[tokio::test]
    async fn test_index_json_reflects_all_entries() {
        let (tmp, backend) = make_backend();
        backend.init().await.unwrap();

        backend.store("x", "data_x", "data").await.unwrap();
        backend.store("y", "data_yy", "data").await.unwrap();

        // Read index.json directly and verify.
        let index_str = tokio::fs::read_to_string(tmp.path().join("index.json"))
            .await
            .unwrap();
        let index: ArtifactIndex = serde_json::from_str(&index_str).unwrap();

        assert_eq!(index.entries.len(), 2);

        let x_entry = index.entries.iter().find(|e| e.key == "x").unwrap();
        assert_eq!(x_entry.size_bytes, 6); // "data_x".len()

        let y_entry = index.entries.iter().find(|e| e.key == "y").unwrap();
        assert_eq!(y_entry.size_bytes, 7); // "data_yy".len()
    }
}
