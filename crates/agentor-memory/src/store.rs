use agentor_core::{AgentorError, AgentorResult};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

/// A single entry stored in vector memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: Uuid,
    pub content: String,
    pub embedding: Vec<f32>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub session_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Result of a semantic search query.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub entry: MemoryEntry,
    pub score: f32,
}

/// Trait for vector storage backends.
#[async_trait]
pub trait VectorStore: Send + Sync {
    /// Insert a memory entry.
    async fn insert(&self, entry: MemoryEntry) -> AgentorResult<()>;

    /// Search for the top-k most similar entries to a query embedding.
    async fn search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        session_filter: Option<Uuid>,
    ) -> AgentorResult<Vec<SearchResult>>;

    /// Delete a memory entry by ID.
    async fn delete(&self, id: Uuid) -> AgentorResult<bool>;

    /// List all entries (optionally filtered by session).
    async fn list(&self, session_filter: Option<Uuid>) -> AgentorResult<Vec<MemoryEntry>>;

    /// Count entries.
    async fn count(&self) -> AgentorResult<usize>;
}

/// In-memory vector store using brute-force cosine similarity.
/// Suitable for MVP and small datasets (<100k entries).
pub struct InMemoryVectorStore {
    entries: RwLock<Vec<MemoryEntry>>,
}

impl InMemoryVectorStore {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(Vec::new()),
        }
    }
}

impl Default for InMemoryVectorStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VectorStore for InMemoryVectorStore {
    async fn insert(&self, entry: MemoryEntry) -> AgentorResult<()> {
        let mut entries = self.entries.write().await;
        entries.push(entry);
        Ok(())
    }

    async fn search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        session_filter: Option<Uuid>,
    ) -> AgentorResult<Vec<SearchResult>> {
        if query_embedding.is_empty() {
            return Err(AgentorError::Agent("Empty query embedding".to_string()));
        }

        let entries = self.entries.read().await;

        let mut scored: Vec<SearchResult> = entries
            .iter()
            .filter(|e| {
                if let Some(sid) = session_filter {
                    e.session_id == Some(sid)
                } else {
                    true
                }
            })
            .map(|e| {
                let score = cosine_similarity(query_embedding, &e.embedding);
                SearchResult {
                    entry: e.clone(),
                    score,
                }
            })
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(top_k);

        Ok(scored)
    }

    async fn delete(&self, id: Uuid) -> AgentorResult<bool> {
        let mut entries = self.entries.write().await;
        let before = entries.len();
        entries.retain(|e| e.id != id);
        Ok(entries.len() < before)
    }

    async fn list(&self, session_filter: Option<Uuid>) -> AgentorResult<Vec<MemoryEntry>> {
        let entries = self.entries.read().await;
        let filtered: Vec<MemoryEntry> = entries
            .iter()
            .filter(|e| {
                if let Some(sid) = session_filter {
                    e.session_id == Some(sid)
                } else {
                    true
                }
            })
            .cloned()
            .collect();
        Ok(filtered)
    }

    async fn count(&self) -> AgentorResult<usize> {
        let entries = self.entries.read().await;
        Ok(entries.len())
    }
}

/// File-backed vector store that persists entries as JSONL on disk.
/// Loads all entries into memory on creation; appends on insert; rewrites on delete.
pub struct FileVectorStore {
    path: std::path::PathBuf,
    inner: InMemoryVectorStore,
}

impl FileVectorStore {
    /// Create a new FileVectorStore at the given path.
    /// If the file exists, loads all entries from it.
    pub async fn new(path: std::path::PathBuf) -> AgentorResult<Self> {
        let inner = InMemoryVectorStore::new();

        if path.exists() {
            let data = tokio::fs::read_to_string(&path).await.map_err(|e| {
                AgentorError::Session(format!("Failed to read vector store: {}", e))
            })?;
            for line in data.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                let entry: MemoryEntry = serde_json::from_str(line)
                    .map_err(|e| AgentorError::Session(format!("Invalid JSONL entry: {}", e)))?;
                inner.insert(entry).await?;
            }
        } else if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AgentorError::Session(format!("Failed to create dir: {}", e)))?;
        }

        Ok(Self { path, inner })
    }

    /// Append a single entry to the JSONL file.
    async fn append_to_file(&self, entry: &MemoryEntry) -> AgentorResult<()> {
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await
            .map_err(|e| AgentorError::Session(format!("Failed to open vector store: {}", e)))?;
        let mut line = serde_json::to_string(entry)
            .map_err(|e| AgentorError::Session(format!("Failed to serialize entry: {}", e)))?;
        line.push('\n');
        file.write_all(line.as_bytes())
            .await
            .map_err(|e| AgentorError::Session(format!("Failed to write entry: {}", e)))?;
        Ok(())
    }

    /// Rewrite the entire file from in-memory entries.
    async fn rewrite_file(&self) -> AgentorResult<()> {
        let entries = self.inner.list(None).await?;
        let mut data = String::new();
        for entry in &entries {
            let line = serde_json::to_string(entry)
                .map_err(|e| AgentorError::Session(format!("Failed to serialize entry: {}", e)))?;
            data.push_str(&line);
            data.push('\n');
        }
        tokio::fs::write(&self.path, data.as_bytes())
            .await
            .map_err(|e| AgentorError::Session(format!("Failed to write vector store: {}", e)))?;
        Ok(())
    }
}

#[async_trait]
impl VectorStore for FileVectorStore {
    async fn insert(&self, entry: MemoryEntry) -> AgentorResult<()> {
        self.append_to_file(&entry).await?;
        self.inner.insert(entry).await
    }

    async fn search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        session_filter: Option<Uuid>,
    ) -> AgentorResult<Vec<SearchResult>> {
        self.inner
            .search(query_embedding, top_k, session_filter)
            .await
    }

    async fn delete(&self, id: Uuid) -> AgentorResult<bool> {
        let deleted = self.inner.delete(id).await?;
        if deleted {
            self.rewrite_file().await?;
        }
        Ok(deleted)
    }

    async fn list(&self, session_filter: Option<Uuid>) -> AgentorResult<Vec<MemoryEntry>> {
        self.inner.list(session_filter).await
    }

    async fn count(&self) -> AgentorResult<usize> {
        self.inner.count().await
    }
}

/// Cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(content: &str, embedding: Vec<f32>, session: Option<Uuid>) -> MemoryEntry {
        MemoryEntry {
            id: Uuid::new_v4(),
            content: content.to_string(),
            embedding,
            metadata: HashMap::new(),
            session_id: session,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_insert_and_count() {
        let store = InMemoryVectorStore::new();
        assert_eq!(store.count().await.unwrap(), 0);

        store
            .insert(make_entry("hello", vec![1.0, 0.0, 0.0], None))
            .await
            .unwrap();
        assert_eq!(store.count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_search_returns_similar() {
        let store = InMemoryVectorStore::new();

        // Entry close to query
        store
            .insert(make_entry("rust lang", vec![0.9, 0.1, 0.0], None))
            .await
            .unwrap();
        // Entry far from query
        store
            .insert(make_entry("cooking", vec![0.0, 0.0, 1.0], None))
            .await
            .unwrap();

        let results = store.search(&[1.0, 0.0, 0.0], 2, None).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].entry.content, "rust lang");
        assert!(results[0].score > results[1].score);
    }

    #[tokio::test]
    async fn test_search_top_k() {
        let store = InMemoryVectorStore::new();
        for i in 0..10 {
            let mut emb = vec![0.0f32; 3];
            emb[i % 3] = 1.0;
            store
                .insert(make_entry(&format!("entry_{}", i), emb, None))
                .await
                .unwrap();
        }

        let results = store.search(&[1.0, 0.0, 0.0], 3, None).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_search_session_filter() {
        let store = InMemoryVectorStore::new();
        let sid1 = Uuid::new_v4();
        let sid2 = Uuid::new_v4();

        store
            .insert(make_entry("a", vec![1.0, 0.0], Some(sid1)))
            .await
            .unwrap();
        store
            .insert(make_entry("b", vec![0.9, 0.1], Some(sid2)))
            .await
            .unwrap();

        let results = store.search(&[1.0, 0.0], 10, Some(sid1)).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entry.content, "a");
    }

    #[tokio::test]
    async fn test_delete() {
        let store = InMemoryVectorStore::new();
        let entry = make_entry("to_delete", vec![1.0], None);
        let id = entry.id;

        store.insert(entry).await.unwrap();
        assert_eq!(store.count().await.unwrap(), 1);

        assert!(store.delete(id).await.unwrap());
        assert_eq!(store.count().await.unwrap(), 0);

        // Delete non-existent
        assert!(!store.delete(Uuid::new_v4()).await.unwrap());
    }

    #[tokio::test]
    async fn test_list_all() {
        let store = InMemoryVectorStore::new();
        store
            .insert(make_entry("a", vec![1.0], None))
            .await
            .unwrap();
        store
            .insert(make_entry("b", vec![0.5], None))
            .await
            .unwrap();

        let all = store.list(None).await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_list_filtered() {
        let store = InMemoryVectorStore::new();
        let sid = Uuid::new_v4();

        store
            .insert(make_entry("a", vec![1.0], Some(sid)))
            .await
            .unwrap();
        store
            .insert(make_entry("b", vec![0.5], None))
            .await
            .unwrap();

        let filtered = store.list(Some(sid)).await.unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].content, "a");
    }

    #[tokio::test]
    async fn test_search_empty_query() {
        let store = InMemoryVectorStore::new();
        assert!(store.search(&[], 5, None).await.is_err());
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let v = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        assert!((cosine_similarity(&a, &b) + 1.0).abs() < 0.001);
    }

    // --- FileVectorStore tests ---

    #[tokio::test]
    async fn test_file_store_insert_and_persist() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("vectors.jsonl");

        {
            let store = FileVectorStore::new(path.clone()).await.unwrap();
            store
                .insert(make_entry("hello", vec![1.0, 0.0], None))
                .await
                .unwrap();
            store
                .insert(make_entry("world", vec![0.0, 1.0], None))
                .await
                .unwrap();
            assert_eq!(store.count().await.unwrap(), 2);
        }

        // Reload from disk
        let store2 = FileVectorStore::new(path).await.unwrap();
        assert_eq!(store2.count().await.unwrap(), 2);
        let all = store2.list(None).await.unwrap();
        let contents: Vec<&str> = all.iter().map(|e| e.content.as_str()).collect();
        assert!(contents.contains(&"hello"));
        assert!(contents.contains(&"world"));
    }

    #[tokio::test]
    async fn test_file_store_delete_rewrites() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("vectors.jsonl");

        let store = FileVectorStore::new(path.clone()).await.unwrap();
        let entry = make_entry("to_delete", vec![1.0], None);
        let id = entry.id;
        store.insert(entry).await.unwrap();
        store
            .insert(make_entry("keep", vec![0.5], None))
            .await
            .unwrap();

        assert!(store.delete(id).await.unwrap());
        assert_eq!(store.count().await.unwrap(), 1);

        // Reload and verify
        let store2 = FileVectorStore::new(path).await.unwrap();
        assert_eq!(store2.count().await.unwrap(), 1);
        let all = store2.list(None).await.unwrap();
        assert_eq!(all[0].content, "keep");
    }

    #[tokio::test]
    async fn test_file_store_search() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("vectors.jsonl");

        let store = FileVectorStore::new(path).await.unwrap();
        store
            .insert(make_entry("close", vec![0.9, 0.1, 0.0], None))
            .await
            .unwrap();
        store
            .insert(make_entry("far", vec![0.0, 0.0, 1.0], None))
            .await
            .unwrap();

        let results = store.search(&[1.0, 0.0, 0.0], 2, None).await.unwrap();
        assert_eq!(results[0].entry.content, "close");
    }

    #[tokio::test]
    async fn test_file_store_empty_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("vectors.jsonl");

        let store = FileVectorStore::new(path).await.unwrap();
        assert_eq!(store.count().await.unwrap(), 0);
    }
}
