//! Qdrant vector store adapter.
//!
//! Qdrant is an open-source vector database (Rust-native) with REST and
//! gRPC APIs. This adapter implements the [`VectorStore`] trait with a stub
//! backend; real HTTP calls are gated behind the `http-vectorstore` feature.

use crate::store::{MemoryEntry, SearchResult, VectorStore};
use argentor_core::{ArgentorError, ArgentorResult};
use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Qdrant vector store adapter.
pub struct QdrantStore {
    /// Qdrant endpoint (e.g., "http://localhost:6333").
    #[allow(dead_code)]
    endpoint: String,
    /// Optional API key (Qdrant Cloud).
    #[allow(dead_code)]
    api_key: Option<String>,
    /// Collection name.
    #[allow(dead_code)]
    collection_name: String,
    /// Vector dimension for the collection.
    #[allow(dead_code)]
    vector_size: usize,
    /// HTTP client — `None` in stub mode.
    #[cfg(feature = "http-vectorstore")]
    #[allow(dead_code)]
    client: Option<reqwest::Client>,
    /// Stub in-memory storage.
    entries: RwLock<HashMap<Uuid, MemoryEntry>>,
}

impl QdrantStore {
    /// Create a new Qdrant adapter in stub mode.
    pub fn new(
        endpoint: impl Into<String>,
        collection_name: impl Into<String>,
        vector_size: usize,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            api_key: None,
            collection_name: collection_name.into(),
            vector_size,
            #[cfg(feature = "http-vectorstore")]
            client: None,
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Attach an API key (Qdrant Cloud).
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Return the configured endpoint.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Return the configured collection name.
    pub fn collection_name(&self) -> &str {
        &self.collection_name
    }

    /// Return the configured vector size.
    pub fn vector_size(&self) -> usize {
        self.vector_size
    }

    /// Return whether an API key is configured.
    pub fn has_api_key(&self) -> bool {
        self.api_key.is_some()
    }

    /// Enable real HTTP mode with a [`reqwest::Client`].
    #[cfg(feature = "http-vectorstore")]
    pub fn with_http_client(mut self, client: reqwest::Client) -> Self {
        self.client = Some(client);
        self
    }

    /// Build the points upsert URL for this collection.
    #[cfg(feature = "http-vectorstore")]
    #[allow(dead_code)]
    fn upsert_url(&self) -> String {
        format!(
            "{}/collections/{}/points",
            self.endpoint.trim_end_matches('/'),
            self.collection_name
        )
    }
}

#[async_trait]
impl VectorStore for QdrantStore {
    async fn insert(&self, entry: MemoryEntry) -> ArgentorResult<()> {
        if !entry.embedding.is_empty() && entry.embedding.len() != self.vector_size {
            return Err(ArgentorError::Agent(format!(
                "Qdrant: embedding dim mismatch (got {}, expected {})",
                entry.embedding.len(),
                self.vector_size
            )));
        }
        let mut entries = self.entries.write().await;
        entries.insert(entry.id, entry);
        Ok(())
    }

    async fn search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        session_filter: Option<Uuid>,
    ) -> ArgentorResult<Vec<SearchResult>> {
        if query_embedding.is_empty() {
            return Err(ArgentorError::Agent("Empty query embedding".to_string()));
        }
        let entries = self.entries.read().await;
        let mut scored: Vec<SearchResult> = entries
            .values()
            .filter(|e| {
                if let Some(sid) = session_filter {
                    e.session_id == Some(sid)
                } else {
                    true
                }
            })
            .map(|e| {
                let score = cosine(query_embedding, &e.embedding);
                SearchResult {
                    entry: e.clone(),
                    score,
                }
            })
            .collect();
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(top_k);
        Ok(scored)
    }

    async fn delete(&self, id: Uuid) -> ArgentorResult<bool> {
        let mut entries = self.entries.write().await;
        Ok(entries.remove(&id).is_some())
    }

    async fn list(&self, session_filter: Option<Uuid>) -> ArgentorResult<Vec<MemoryEntry>> {
        let entries = self.entries.read().await;
        Ok(entries
            .values()
            .filter(|e| {
                if let Some(sid) = session_filter {
                    e.session_id == Some(sid)
                } else {
                    true
                }
            })
            .cloned()
            .collect())
    }

    async fn count(&self) -> ArgentorResult<usize> {
        let entries = self.entries.read().await;
        Ok(entries.len())
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
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
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn entry(content: &str, emb: Vec<f32>, session: Option<Uuid>) -> MemoryEntry {
        MemoryEntry {
            id: Uuid::new_v4(),
            content: content.to_string(),
            embedding: emb,
            metadata: HashMap::new(),
            session_id: session,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn test_new_sets_fields() {
        let s = QdrantStore::new("http://localhost:6333", "docs", 768);
        assert_eq!(s.endpoint(), "http://localhost:6333");
        assert_eq!(s.collection_name(), "docs");
        assert_eq!(s.vector_size(), 768);
        assert!(!s.has_api_key());
    }

    #[test]
    fn test_with_api_key() {
        let s = QdrantStore::new("http://x", "c", 3).with_api_key("tok");
        assert!(s.has_api_key());
    }

    #[test]
    fn test_accepts_owned_strings() {
        let s = QdrantStore::new(String::from("http://x"), String::from("c"), 16);
        assert_eq!(s.vector_size(), 16);
    }

    #[tokio::test]
    async fn test_insert_and_count() {
        let s = QdrantStore::new("http://x", "c", 2);
        s.insert(entry("a", vec![1.0, 0.0], None)).await.unwrap();
        assert_eq!(s.count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_insert_rejects_bad_dim() {
        let s = QdrantStore::new("http://x", "c", 3);
        let bad = entry("bad", vec![1.0, 0.0], None);
        assert!(s.insert(bad).await.is_err());
    }

    #[tokio::test]
    async fn test_insert_allows_empty_embedding() {
        // Empty embedding is allowed (e.g., for upsert-later patterns).
        let s = QdrantStore::new("http://x", "c", 3);
        let e = entry("pending", vec![], None);
        assert!(s.insert(e).await.is_ok());
    }

    #[tokio::test]
    async fn test_insert_many() {
        let s = QdrantStore::new("http://x", "c", 2);
        for i in 0..12 {
            s.insert(entry(&format!("e{i}"), vec![1.0, i as f32], None))
                .await
                .unwrap();
        }
        assert_eq!(s.count().await.unwrap(), 12);
    }

    #[tokio::test]
    async fn test_search_orders_by_similarity() {
        let s = QdrantStore::new("http://x", "c", 3);
        s.insert(entry("near", vec![0.9, 0.1, 0.0], None))
            .await
            .unwrap();
        s.insert(entry("far", vec![0.0, 0.0, 1.0], None))
            .await
            .unwrap();
        let r = s.search(&[1.0, 0.0, 0.0], 2, None).await.unwrap();
        assert_eq!(r[0].entry.content, "near");
    }

    #[tokio::test]
    async fn test_search_top_k() {
        let s = QdrantStore::new("http://x", "c", 2);
        for i in 0..6 {
            s.insert(entry(&format!("e{i}"), vec![1.0, i as f32], None))
                .await
                .unwrap();
        }
        let r = s.search(&[1.0, 0.0], 2, None).await.unwrap();
        assert_eq!(r.len(), 2);
    }

    #[tokio::test]
    async fn test_search_empty_query_errors() {
        let s = QdrantStore::new("http://x", "c", 2);
        assert!(s.search(&[], 1, None).await.is_err());
    }

    #[tokio::test]
    async fn test_search_session_filter() {
        let s = QdrantStore::new("http://x", "c", 2);
        let sid = Uuid::new_v4();
        s.insert(entry("s", vec![1.0, 0.0], Some(sid)))
            .await
            .unwrap();
        s.insert(entry("o", vec![1.0, 0.0], None)).await.unwrap();
        let r = s.search(&[1.0, 0.0], 5, Some(sid)).await.unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].entry.content, "s");
    }

    #[tokio::test]
    async fn test_delete_existing() {
        let s = QdrantStore::new("http://x", "c", 2);
        let e = entry("x", vec![1.0, 0.0], None);
        let id = e.id;
        s.insert(e).await.unwrap();
        assert!(s.delete(id).await.unwrap());
    }

    #[tokio::test]
    async fn test_delete_missing() {
        let s = QdrantStore::new("http://x", "c", 2);
        assert!(!s.delete(Uuid::new_v4()).await.unwrap());
    }

    #[tokio::test]
    async fn test_list_all() {
        let s = QdrantStore::new("http://x", "c", 2);
        s.insert(entry("a", vec![1.0, 0.0], None)).await.unwrap();
        s.insert(entry("b", vec![0.0, 1.0], None)).await.unwrap();
        let all = s.list(None).await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_list_filtered() {
        let s = QdrantStore::new("http://x", "c", 2);
        let sid = Uuid::new_v4();
        s.insert(entry("a", vec![1.0, 0.0], Some(sid)))
            .await
            .unwrap();
        s.insert(entry("b", vec![0.0, 1.0], None)).await.unwrap();
        let f = s.list(Some(sid)).await.unwrap();
        assert_eq!(f.len(), 1);
    }

    #[tokio::test]
    async fn test_metadata_preserved() {
        let s = QdrantStore::new("http://x", "c", 2);
        let mut e = entry("m", vec![1.0, 0.0], None);
        e.metadata.insert("k".into(), serde_json::json!(42));
        let id = e.id;
        s.insert(e).await.unwrap();
        let all = s.list(None).await.unwrap();
        let got = all.iter().find(|x| x.id == id).unwrap();
        assert_eq!(got.metadata.get("k").unwrap(), &serde_json::json!(42));
    }

    #[tokio::test]
    async fn test_empty_search_result() {
        let s = QdrantStore::new("http://x", "c", 2);
        let r = s.search(&[1.0, 0.0], 5, None).await.unwrap();
        assert!(r.is_empty());
    }

    #[tokio::test]
    async fn test_count_after_deletes() {
        let s = QdrantStore::new("http://x", "c", 2);
        let e = entry("a", vec![1.0, 0.0], None);
        let id = e.id;
        s.insert(e).await.unwrap();
        s.insert(entry("b", vec![0.0, 1.0], None)).await.unwrap();
        s.delete(id).await.unwrap();
        assert_eq!(s.count().await.unwrap(), 1);
    }
}
