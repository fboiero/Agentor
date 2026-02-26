use std::collections::HashMap;
use std::sync::Arc;

use agentor_core::AgentorResult;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::bm25::Bm25Index;
use crate::embedding::EmbeddingProvider;
use crate::store::{MemoryEntry, SearchResult, VectorStore};

/// The rank assigned to a document that only appears in one of the two
/// result lists (vector or BM25) when computing Reciprocal Rank Fusion.
const MISSING_RANK: f32 = 1000.0;

/// Hybrid searcher that combines dense vector search with BM25 keyword
/// search using Reciprocal Rank Fusion (RRF).
///
/// This provides better recall than either method alone:
/// - Vector search captures semantic similarity (meaning)
/// - BM25 captures exact keyword matches (lexical)
///
/// The `alpha` parameter controls the balance:
/// - `alpha = 1.0` — pure vector search
/// - `alpha = 0.0` — pure BM25 search
/// - `alpha = 0.5` — equal blend (default)
pub struct HybridSearcher {
    vector_store: Arc<dyn VectorStore>,
    embedder: Arc<dyn EmbeddingProvider>,
    bm25: RwLock<Bm25Index>,
    /// Balance factor: 0.0 = pure BM25, 1.0 = pure vector, default 0.5.
    alpha: f32,
    /// RRF constant (default 60.0). Higher values smooth out rank differences.
    rrf_k: f32,
}

impl HybridSearcher {
    /// Create a new hybrid searcher with default alpha=0.5 and rrf_k=60.0.
    pub fn new(
        vector_store: Arc<dyn VectorStore>,
        embedder: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        Self {
            vector_store,
            embedder,
            bm25: RwLock::new(Bm25Index::new()),
            alpha: 0.5,
            rrf_k: 60.0,
        }
    }

    /// Set the alpha balance factor. Chainable builder method.
    ///
    /// - `alpha = 0.0` — pure BM25
    /// - `alpha = 1.0` — pure vector
    /// - `alpha = 0.5` — equal blend (default)
    pub fn with_alpha(mut self, alpha: f32) -> Self {
        self.alpha = alpha.clamp(0.0, 1.0);
        self
    }

    /// Insert a memory entry into both the vector store and the BM25 index.
    pub async fn insert(&self, entry: MemoryEntry) -> AgentorResult<()> {
        // Add to BM25 index
        {
            let mut bm25 = self.bm25.write().await;
            bm25.add_document(entry.id, &entry.content);
        }

        // Add to vector store
        self.vector_store.insert(entry).await?;

        Ok(())
    }

    /// Search using both vector similarity and BM25, fusing results with
    /// Reciprocal Rank Fusion (RRF).
    ///
    /// The RRF score for each document is:
    /// ```text
    /// score = alpha * (1 / (rrf_k + vector_rank))
    ///       + (1 - alpha) * (1 / (rrf_k + bm25_rank))
    /// ```
    ///
    /// Documents that only appear in one list receive `MISSING_RANK` (1000)
    /// for the other, effectively penalizing single-source matches.
    pub async fn search(
        &self,
        query: &str,
        top_k: usize,
        session_filter: Option<Uuid>,
    ) -> AgentorResult<Vec<SearchResult>> {
        // Retrieve more candidates than top_k from each source for better fusion
        let fetch_k = top_k * 3;

        // Embed the query for vector search
        let query_embedding = self.embedder.embed(query).await?;

        // Run vector search
        let vector_results = self
            .vector_store
            .search(&query_embedding, fetch_k, session_filter)
            .await?;

        // Run BM25 search
        let bm25_results = {
            let bm25 = self.bm25.read().await;
            bm25.search(query, fetch_k)
        };

        // Build rank maps (doc_id -> 1-based rank)
        let mut vector_ranks: HashMap<Uuid, f32> = HashMap::new();
        for (rank, result) in vector_results.iter().enumerate() {
            vector_ranks.insert(result.entry.id, (rank + 1) as f32);
        }

        let mut bm25_ranks: HashMap<Uuid, f32> = HashMap::new();
        for (rank, (doc_id, _score)) in bm25_results.iter().enumerate() {
            bm25_ranks.insert(*doc_id, (rank + 1) as f32);
        }

        // Collect all unique document IDs and their entries
        let mut entries_map: HashMap<Uuid, MemoryEntry> = HashMap::new();
        for result in &vector_results {
            entries_map.insert(result.entry.id, result.entry.clone());
        }

        // For BM25 results not already in the map, we need to find them
        // from the vector results or the vector store. Since BM25 only
        // returns IDs, we need to check if we already have the entry.
        // Documents from BM25 that are NOT in the vector results are rare
        // in the fused top-k; we fetch them from the vector store's list
        // if needed.
        let bm25_missing_ids: Vec<Uuid> = bm25_results
            .iter()
            .filter(|(id, _)| !entries_map.contains_key(id))
            .map(|(id, _)| *id)
            .collect();

        if !bm25_missing_ids.is_empty() {
            // Fetch all entries to find the missing ones
            let all_entries = self.vector_store.list(session_filter).await?;
            for entry in all_entries {
                if bm25_missing_ids.contains(&entry.id) {
                    entries_map.insert(entry.id, entry);
                }
            }
        }

        // Compute RRF scores
        let all_ids: Vec<Uuid> = entries_map.keys().copied().collect();
        let mut fused_scores: Vec<(Uuid, f32)> = Vec::new();

        for doc_id in all_ids {
            let v_rank = vector_ranks.get(&doc_id).copied().unwrap_or(MISSING_RANK);
            let b_rank = bm25_ranks.get(&doc_id).copied().unwrap_or(MISSING_RANK);

            let rrf_score = self.alpha * (1.0 / (self.rrf_k + v_rank))
                + (1.0 - self.alpha) * (1.0 / (self.rrf_k + b_rank));

            fused_scores.push((doc_id, rrf_score));
        }

        // Sort by RRF score descending
        fused_scores.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        fused_scores.truncate(top_k);

        // Build final SearchResult list
        let results: Vec<SearchResult> = fused_scores
            .into_iter()
            .filter_map(|(doc_id, score)| {
                entries_map.remove(&doc_id).map(|entry| SearchResult {
                    entry,
                    score,
                })
            })
            .collect();

        Ok(results)
    }

    /// Remove a document from both the vector store and the BM25 index.
    pub async fn remove(&self, id: Uuid) -> AgentorResult<bool> {
        // Remove from BM25
        {
            let mut bm25 = self.bm25.write().await;
            bm25.remove_document(id);
        }

        // Remove from vector store
        self.vector_store.delete(id).await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::embedding::LocalEmbedding;
    use crate::store::InMemoryVectorStore;
    use chrono::Utc;

    /// Helper to create a MemoryEntry with an embedding from LocalEmbedding.
    async fn make_entry(
        embedder: &dyn EmbeddingProvider,
        content: &str,
        session_id: Option<Uuid>,
    ) -> MemoryEntry {
        let embedding = embedder.embed(content).await.unwrap();
        MemoryEntry {
            id: Uuid::new_v4(),
            content: content.to_string(),
            embedding,
            metadata: HashMap::new(),
            session_id,
            created_at: Utc::now(),
        }
    }

    fn make_searcher(alpha: f32) -> HybridSearcher {
        let store = Arc::new(InMemoryVectorStore::new()) as Arc<dyn VectorStore>;
        let embedder = Arc::new(LocalEmbedding::default()) as Arc<dyn EmbeddingProvider>;
        HybridSearcher::new(store, embedder).with_alpha(alpha)
    }

    #[tokio::test]
    async fn test_insert_and_search_finds_entry() {
        let embedder = Arc::new(LocalEmbedding::default());
        let searcher = make_searcher(0.5);

        let entry = make_entry(embedder.as_ref(), "rust programming language systems", None).await;
        let entry_id = entry.id;
        searcher.insert(entry).await.unwrap();

        let results = searcher.search("rust programming", 10, None).await.unwrap();
        assert!(!results.is_empty(), "should find at least one result");
        assert_eq!(
            results[0].entry.id, entry_id,
            "the inserted entry should be found"
        );
        assert!(results[0].score > 0.0, "score should be positive");
    }

    #[tokio::test]
    async fn test_alpha_zero_pure_bm25_side() {
        let embedder = Arc::new(LocalEmbedding::default());
        let searcher = make_searcher(0.0); // pure BM25

        let entry1 =
            make_entry(embedder.as_ref(), "rust rust rust systems programming", None).await;
        let id1 = entry1.id;
        searcher.insert(entry1).await.unwrap();

        let entry2 =
            make_entry(embedder.as_ref(), "python scripting language", None).await;
        let id2 = entry2.id;
        searcher.insert(entry2).await.unwrap();

        let results = searcher.search("rust systems", 10, None).await.unwrap();
        assert!(!results.is_empty(), "alpha=0 (BM25) should still return results");

        // With pure BM25 (alpha=0), the document containing "rust" and "systems" should rank first
        assert_eq!(
            results[0].entry.id, id1,
            "BM25 should rank the document with matching keywords first"
        );

        // With alpha=0 the BM25 side dominates scoring. The python entry has no
        // matching keywords, so its BM25 rank is MISSING_RANK (1000). Even though
        // it may still appear in results (via vector store candidates), it must
        // rank strictly below the relevant document.
        if let Some(pos_python) = results.iter().position(|r| r.entry.id == id2) {
            let pos_rust = results.iter().position(|r| r.entry.id == id1).unwrap();
            assert!(
                pos_rust < pos_python,
                "BM25-matching document should rank above non-matching document"
            );
            // The non-matching document should have a much lower score
            let rust_score = results[pos_rust].score;
            let python_score = results[pos_python].score;
            assert!(
                rust_score > python_score * 5.0,
                "matching doc score ({rust_score}) should be significantly higher than non-matching ({python_score})",
            );
        }
    }

    #[tokio::test]
    async fn test_alpha_one_pure_vector_side() {
        let embedder = Arc::new(LocalEmbedding::default());
        let searcher = make_searcher(1.0); // pure vector

        let entry1 = make_entry(
            embedder.as_ref(),
            "rust programming language for systems",
            None,
        )
        .await;
        let id1 = entry1.id;
        searcher.insert(entry1).await.unwrap();

        let entry2 =
            make_entry(embedder.as_ref(), "cooking delicious dinner recipes", None).await;
        searcher.insert(entry2).await.unwrap();

        let results = searcher
            .search("rust programming systems", 10, None)
            .await
            .unwrap();
        assert!(
            !results.is_empty(),
            "alpha=1 (vector) should still return results"
        );

        // With pure vector search, the semantically similar document should rank first
        assert_eq!(
            results[0].entry.id, id1,
            "vector search should rank semantically similar document first"
        );
    }

    #[tokio::test]
    async fn test_rrf_fusion_combines_results() {
        let embedder = Arc::new(LocalEmbedding::default());
        let searcher = make_searcher(0.5); // balanced

        // Document 1: strong keyword match for "rust"
        let entry1 = make_entry(
            embedder.as_ref(),
            "rust rust rust memory safety guaranteed by the compiler",
            None,
        )
        .await;
        let id1 = entry1.id;
        searcher.insert(entry1).await.unwrap();

        // Document 2: semantically related to programming but different keywords
        let entry2 = make_entry(
            embedder.as_ref(),
            "systems programming language with type safety",
            None,
        )
        .await;
        let id2 = entry2.id;
        searcher.insert(entry2).await.unwrap();

        // Document 3: completely unrelated
        let entry3 = make_entry(
            embedder.as_ref(),
            "chocolate cake recipe with frosting",
            None,
        )
        .await;
        let id3 = entry3.id;
        searcher.insert(entry3).await.unwrap();

        let results = searcher
            .search("rust programming", 10, None)
            .await
            .unwrap();

        // Both programming-related documents should appear before the cooking one
        let pos_1 = results.iter().position(|r| r.entry.id == id1);
        let pos_2 = results.iter().position(|r| r.entry.id == id2);
        let pos_3 = results.iter().position(|r| r.entry.id == id3);

        assert!(
            pos_1.is_some(),
            "document with strong keyword match should appear"
        );
        assert!(
            pos_2.is_some(),
            "document with semantic match should appear"
        );

        // The unrelated document should rank last (if it appears at all)
        if let Some(p3) = pos_3 {
            assert!(
                p3 > pos_1.unwrap() && p3 > pos_2.unwrap(),
                "unrelated document should rank below relevant documents"
            );
        }

        // Verify all scores are positive
        for result in &results {
            assert!(result.score > 0.0, "RRF scores should be positive");
        }
    }

    #[tokio::test]
    async fn test_remove_from_both_stores() {
        let embedder = Arc::new(LocalEmbedding::default());
        let searcher = make_searcher(0.5);

        let entry =
            make_entry(embedder.as_ref(), "rust programming language", None).await;
        let id = entry.id;
        searcher.insert(entry).await.unwrap();

        // Verify it can be found
        let results = searcher.search("rust", 10, None).await.unwrap();
        assert!(!results.is_empty());

        // Remove it
        let removed = searcher.remove(id).await.unwrap();
        assert!(removed, "remove should return true for existing document");

        // Verify it can no longer be found
        let results = searcher.search("rust", 10, None).await.unwrap();
        let has_removed = results.iter().any(|r| r.entry.id == id);
        assert!(
            !has_removed,
            "removed document should not appear in search results"
        );
    }
}
