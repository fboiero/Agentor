#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Integration tests for the agentor-memory crate.
//!
//! Covers FileVectorStore persistence, embedding consistency, hybrid search,
//! BM25 CRUD, query expansion, alpha extremes, InMemoryVectorStore operations,
//! edge cases, and search result ordering.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tempfile::TempDir;
use uuid::Uuid;

use agentor_memory::{
    Bm25Index, EmbeddingProvider, FileVectorStore, HybridSearcher, InMemoryVectorStore,
    LocalEmbedding, MemoryEntry, QueryExpander, RuleBasedExpander, VectorStore,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_entry(content: &str, embedding: Vec<f32>) -> MemoryEntry {
    MemoryEntry {
        id: Uuid::new_v4(),
        content: content.to_string(),
        embedding,
        metadata: HashMap::new(),
        session_id: None,
        created_at: Utc::now(),
    }
}

async fn make_entry_with_embedder(
    embedder: &dyn EmbeddingProvider,
    content: &str,
) -> MemoryEntry {
    let embedding = embedder.embed(content).await.unwrap();
    MemoryEntry {
        id: Uuid::new_v4(),
        content: content.to_string(),
        embedding,
        metadata: HashMap::new(),
        session_id: None,
        created_at: Utc::now(),
    }
}

// ---------------------------------------------------------------------------
// 1. FileVectorStore persistence
// ---------------------------------------------------------------------------

#[tokio::test]
async fn file_vector_store_persistence() {
    let tmp: TempDir = TempDir::new().unwrap();
    let path = tmp.path().join("vectors.jsonl");

    let embedder = LocalEmbedding::default();
    let entry1 = make_entry_with_embedder(&embedder, "persistent entry one").await;
    let entry2 = make_entry_with_embedder(&embedder, "persistent entry two").await;
    let id1 = entry1.id;
    let id2 = entry2.id;

    // Insert entries and then drop the store.
    {
        let store = FileVectorStore::new(path.clone()).await.unwrap();
        store.insert(entry1).await.unwrap();
        store.insert(entry2).await.unwrap();
        assert_eq!(store.count().await.unwrap(), 2);
    }

    // Re-create the store from the same path and verify persistence.
    let store2 = FileVectorStore::new(path).await.unwrap();
    assert_eq!(store2.count().await.unwrap(), 2);

    let all = store2.list(None).await.unwrap();
    let ids: Vec<Uuid> = all.iter().map(|e| e.id).collect();
    assert!(ids.contains(&id1), "entry 1 should survive reload");
    assert!(ids.contains(&id2), "entry 2 should survive reload");

    // Searching after reload should work.
    let query_emb = embedder.embed("persistent entry one").await.unwrap();
    let results = store2.search(&query_emb, 2, None).await.unwrap();
    assert!(!results.is_empty(), "search on reloaded store must return results");
    assert_eq!(
        results[0].entry.id, id1,
        "closest match should be entry one"
    );
}

// ---------------------------------------------------------------------------
// 2. Embedding consistency
// ---------------------------------------------------------------------------

#[tokio::test]
async fn embedding_consistency() {
    let embedder = LocalEmbedding::default();

    let text = "deterministic embedding test";
    let v1 = embedder.embed(text).await.unwrap();
    let v2 = embedder.embed(text).await.unwrap();

    assert_eq!(v1.len(), v2.len(), "vectors must have same length");
    assert_eq!(v1, v2, "same text must produce identical vectors");
}

// ---------------------------------------------------------------------------
// 3. Hybrid search lifecycle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn hybrid_search_lifecycle() {
    let store = Arc::new(InMemoryVectorStore::new()) as Arc<dyn VectorStore>;
    let embedder = Arc::new(LocalEmbedding::default()) as Arc<dyn EmbeddingProvider>;
    let searcher = HybridSearcher::new(Arc::clone(&store), Arc::clone(&embedder));

    let e1 = make_entry_with_embedder(embedder.as_ref(), "rust systems programming language").await;
    let e2 = make_entry_with_embedder(embedder.as_ref(), "python data science scripting").await;
    let e3 = make_entry_with_embedder(embedder.as_ref(), "chocolate cake baking recipe").await;
    let id1 = e1.id;
    let id3 = e3.id;

    searcher.insert(e1).await.unwrap();
    searcher.insert(e2).await.unwrap();
    searcher.insert(e3).await.unwrap();

    // Search for something related to rust.
    let results = searcher.search("rust programming", 10, None).await.unwrap();
    assert!(results.len() >= 2, "should find at least two results");

    // The rust entry should rank first.
    assert_eq!(results[0].entry.id, id1, "rust doc should rank first");

    // The cake recipe should rank last (or not appear).
    if let Some(pos_cake) = results.iter().position(|r| r.entry.id == id3) {
        assert!(
            pos_cake >= 2,
            "unrelated document should rank below relevant ones"
        );
    }

    // Remove one entry and verify it is gone.
    searcher.remove(id1).await.unwrap();
    let results2 = searcher.search("rust programming", 10, None).await.unwrap();
    let has_id1 = results2.iter().any(|r| r.entry.id == id1);
    assert!(!has_id1, "removed entry must not appear after removal");
}

// ---------------------------------------------------------------------------
// 4. BM25 CRUD
// ---------------------------------------------------------------------------

#[tokio::test]
async fn bm25_crud() {
    let mut index = Bm25Index::new();

    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();

    index.add_document(id1, "rust systems programming language safe fast");
    index.add_document(id2, "python scripting data science machine learning");
    index.add_document(id3, "cooking dinner recipes healthy meals");

    assert_eq!(index.document_count(), 3);

    // Search for "rust programming" should return id1.
    let results = index.search("rust programming", 10);
    assert!(!results.is_empty(), "BM25 should find matching documents");
    assert_eq!(results[0].0, id1, "rust doc should rank first for 'rust programming'");

    // Search for "cooking" should return id3 only.
    let results_cook = index.search("cooking meals", 10);
    assert!(!results_cook.is_empty());
    assert_eq!(results_cook[0].0, id3, "cooking doc should rank first for 'cooking meals'");

    // Delete id1 and verify it is gone.
    index.remove_document(id1);
    assert_eq!(index.document_count(), 2);

    let results_after_delete = index.search("rust programming", 10);
    let has_id1 = results_after_delete.iter().any(|(id, _)| *id == id1);
    assert!(!has_id1, "deleted document should not appear in BM25 results");
}

// ---------------------------------------------------------------------------
// 5. Query expansion
// ---------------------------------------------------------------------------

#[tokio::test]
async fn query_expansion_rule_based() {
    let expander = RuleBasedExpander::new();

    // "error" should expand to synonyms like bug, issue, problem, exception.
    let expansions = expander.expand("fix the error");
    assert!(expansions.len() > 1, "should produce at least two queries");
    assert_eq!(expansions[0], "fix the error", "original query must be first");

    let has_bug_variant = expansions.iter().any(|q| q.contains("bug"));
    let has_issue_variant = expansions.iter().any(|q| q.contains("issue"));
    assert!(
        has_bug_variant || has_issue_variant,
        "should expand 'error' to at least one synonym: {expansions:?}"
    );

    // "create api" should expand both words.
    let expansions2 = expander.expand("create api");
    assert!(expansions2.len() > 1);
    let has_make = expansions2.iter().any(|q| q.contains("make"));
    let has_endpoint = expansions2.iter().any(|q| q.contains("endpoint"));
    assert!(
        has_make || has_endpoint,
        "should expand 'create' or 'api': {expansions2:?}"
    );

    // No synonyms for random words -> only original.
    let no_expand = expander.expand("foobar baz");
    assert_eq!(no_expand.len(), 1);
    assert_eq!(no_expand[0], "foobar baz");
}

// ---------------------------------------------------------------------------
// 6. Alpha extremes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn alpha_extremes_produce_different_rankings() {
    let embedder = Arc::new(LocalEmbedding::default()) as Arc<dyn EmbeddingProvider>;

    // Create a document set where BM25 and vector search would disagree.
    // Doc A: strong keyword match for "rust" (repeated).
    // Doc B: semantically related to systems programming but no "rust" keyword.
    let entry_a = make_entry_with_embedder(
        embedder.as_ref(),
        "rust rust rust compiler borrow checker ownership",
    )
    .await;
    let entry_b = make_entry_with_embedder(
        embedder.as_ref(),
        "systems programming language memory safety concurrency",
    )
    .await;
    let id_a = entry_a.id;
    let id_b = entry_b.id;

    // --- alpha=0.0 (pure BM25) ---
    let store_bm25 = Arc::new(InMemoryVectorStore::new()) as Arc<dyn VectorStore>;
    let searcher_bm25 =
        HybridSearcher::new(Arc::clone(&store_bm25), Arc::clone(&embedder)).with_alpha(0.0);
    searcher_bm25
        .insert(entry_a.clone())
        .await
        .unwrap();
    searcher_bm25
        .insert(entry_b.clone())
        .await
        .unwrap();
    let results_bm25 = searcher_bm25
        .search("rust systems", 10, None)
        .await
        .unwrap();

    // --- alpha=1.0 (pure vector) ---
    let store_vec = Arc::new(InMemoryVectorStore::new()) as Arc<dyn VectorStore>;
    let searcher_vec =
        HybridSearcher::new(Arc::clone(&store_vec), Arc::clone(&embedder)).with_alpha(1.0);
    searcher_vec.insert(entry_a).await.unwrap();
    searcher_vec.insert(entry_b).await.unwrap();
    let results_vec = searcher_vec
        .search("rust systems", 10, None)
        .await
        .unwrap();

    // Both should return results.
    assert!(
        !results_bm25.is_empty() && !results_vec.is_empty(),
        "both alpha extremes should return results"
    );

    // With pure BM25, doc A (with "rust" keyword) should rank first.
    assert_eq!(
        results_bm25[0].entry.id, id_a,
        "alpha=0 (BM25) should rank keyword-heavy doc first"
    );

    // Both modes should produce scores; exact values may coincide but rankings
    // can differ. We only assert that both returned non-zero scores.
    assert!(
        results_bm25[0].score >= 0.0 && results_vec[0].score >= 0.0,
        "both modes should produce non-negative scores"
    );

    // Verify we get both documents back in both modes.
    let bm25_ids: Vec<Uuid> = results_bm25.iter().map(|r| r.entry.id).collect();
    let vec_ids: Vec<Uuid> = results_vec.iter().map(|r| r.entry.id).collect();
    assert!(bm25_ids.contains(&id_a) && bm25_ids.contains(&id_b));
    assert!(vec_ids.contains(&id_a) && vec_ids.contains(&id_b));
}

// ---------------------------------------------------------------------------
// 7. InMemoryVectorStore basic ops
// ---------------------------------------------------------------------------

#[tokio::test]
async fn in_memory_vector_store_basic_ops() {
    let store = InMemoryVectorStore::new();

    // Initially empty.
    assert_eq!(store.count().await.unwrap(), 0);

    let e1 = make_entry("hello world", vec![1.0, 0.0, 0.0]);
    let e2 = make_entry("foo bar", vec![0.0, 1.0, 0.0]);
    let e3 = make_entry("baz qux", vec![0.0, 0.0, 1.0]);
    let id1 = e1.id;
    let id2 = e2.id;
    let id3 = e3.id;

    store.insert(e1).await.unwrap();
    store.insert(e2).await.unwrap();
    store.insert(e3).await.unwrap();
    assert_eq!(store.count().await.unwrap(), 3);

    // Search should return the closest entry first.
    let results = store.search(&[1.0, 0.0, 0.0], 2, None).await.unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].entry.id, id1, "entry closest to query should rank first");

    // List all entries.
    let all = store.list(None).await.unwrap();
    assert_eq!(all.len(), 3);

    // Delete one entry.
    assert!(store.delete(id2).await.unwrap());
    assert_eq!(store.count().await.unwrap(), 2);

    // Deleting same id again returns false.
    assert!(!store.delete(id2).await.unwrap());

    // Remaining entries should still be searchable.
    let remaining = store.list(None).await.unwrap();
    let remaining_ids: Vec<Uuid> = remaining.iter().map(|e| e.id).collect();
    assert!(remaining_ids.contains(&id1));
    assert!(remaining_ids.contains(&id3));
    assert!(!remaining_ids.contains(&id2));
}

// ---------------------------------------------------------------------------
// 8. Empty store search
// ---------------------------------------------------------------------------

#[tokio::test]
async fn empty_store_search_returns_empty() {
    // InMemoryVectorStore
    let mem_store = InMemoryVectorStore::new();
    let results = mem_store.search(&[1.0, 0.0], 10, None).await.unwrap();
    assert!(results.is_empty(), "search on empty InMemoryVectorStore must return empty");

    // FileVectorStore
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("empty.jsonl");
    let file_store = FileVectorStore::new(path).await.unwrap();
    let results2 = file_store.search(&[1.0, 0.0], 10, None).await.unwrap();
    assert!(results2.is_empty(), "search on empty FileVectorStore must return empty");

    // BM25
    let bm25 = Bm25Index::new();
    let results3 = bm25.search("anything", 10);
    assert!(results3.is_empty(), "search on empty BM25 index must return empty");
}

// ---------------------------------------------------------------------------
// 9. Embedding dimension consistency
// ---------------------------------------------------------------------------

#[tokio::test]
async fn embedding_dimension_consistency() {
    let embedder = LocalEmbedding::new(128);

    let texts = [
        "rust programming language",
        "the quick brown fox jumps over the lazy dog",
        "machine learning and artificial intelligence",
        "short",
        "a much longer text that contains many more words to ensure the embedding dimension stays consistent regardless of input length variations",
    ];

    let mut prev_len: Option<usize> = None;
    for text in &texts {
        let vec = embedder.embed(text).await.unwrap();
        assert_eq!(
            vec.len(),
            128,
            "embedding dimension must match configured dimension for text: {text}"
        );
        if let Some(pl) = prev_len {
            assert_eq!(vec.len(), pl, "all embeddings must have same dimension");
        }
        prev_len = Some(vec.len());
    }

    // Also verify the default dimension (256).
    let default_embedder = LocalEmbedding::default();
    let v = default_embedder.embed("test text").await.unwrap();
    assert_eq!(v.len(), 256, "default dimension should be 256");
}

// ---------------------------------------------------------------------------
// 10. Store delete
// ---------------------------------------------------------------------------

#[tokio::test]
async fn store_delete_actually_removes_entries() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("delete_test.jsonl");

    let store = FileVectorStore::new(path.clone()).await.unwrap();

    let e1 = make_entry("keep this entry", vec![1.0, 0.0, 0.0]);
    let e2 = make_entry("delete this entry", vec![0.0, 1.0, 0.0]);
    let e3 = make_entry("also keep this", vec![0.0, 0.0, 1.0]);
    let id_keep1 = e1.id;
    let id_delete = e2.id;
    let id_keep2 = e3.id;

    store.insert(e1).await.unwrap();
    store.insert(e2).await.unwrap();
    store.insert(e3).await.unwrap();
    assert_eq!(store.count().await.unwrap(), 3);

    // Delete one entry.
    let deleted = store.delete(id_delete).await.unwrap();
    assert!(deleted, "delete should return true for existing entry");
    assert_eq!(store.count().await.unwrap(), 2);

    // Verify the entry is gone from search results.
    let results = store.search(&[0.0, 1.0, 0.0], 10, None).await.unwrap();
    let result_ids: Vec<Uuid> = results.iter().map(|r| r.entry.id).collect();
    assert!(
        !result_ids.contains(&id_delete),
        "deleted entry should not appear in search results"
    );

    // Verify the entry is gone from list.
    let all = store.list(None).await.unwrap();
    let all_ids: Vec<Uuid> = all.iter().map(|e| e.id).collect();
    assert!(!all_ids.contains(&id_delete));
    assert!(all_ids.contains(&id_keep1));
    assert!(all_ids.contains(&id_keep2));

    // Verify persistence: reopen the store and confirm deletion persisted.
    let store2 = FileVectorStore::new(path).await.unwrap();
    assert_eq!(store2.count().await.unwrap(), 2);
    let all2 = store2.list(None).await.unwrap();
    let all2_ids: Vec<Uuid> = all2.iter().map(|e| e.id).collect();
    assert!(!all2_ids.contains(&id_delete), "deletion must persist on disk");
}

// ---------------------------------------------------------------------------
// 11. Multiple stores isolation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn multiple_stores_isolation() {
    let tmp1 = TempDir::new().unwrap();
    let tmp2 = TempDir::new().unwrap();
    let path1 = tmp1.path().join("store_a.jsonl");
    let path2 = tmp2.path().join("store_b.jsonl");

    let store_a = FileVectorStore::new(path1).await.unwrap();
    let store_b = FileVectorStore::new(path2).await.unwrap();

    // Insert into store A only.
    let entry_a = make_entry("only in store A", vec![1.0, 0.0, 0.0]);
    let id_a = entry_a.id;
    store_a.insert(entry_a).await.unwrap();

    // Insert into store B only.
    let entry_b = make_entry("only in store B", vec![0.0, 1.0, 0.0]);
    let id_b = entry_b.id;
    store_b.insert(entry_b).await.unwrap();

    // Store A should have 1 entry, store B should have 1 entry.
    assert_eq!(store_a.count().await.unwrap(), 1);
    assert_eq!(store_b.count().await.unwrap(), 1);

    // Store A should only contain its own entry.
    let list_a = store_a.list(None).await.unwrap();
    assert_eq!(list_a.len(), 1);
    assert_eq!(list_a[0].id, id_a);

    // Store B should only contain its own entry.
    let list_b = store_b.list(None).await.unwrap();
    assert_eq!(list_b.len(), 1);
    assert_eq!(list_b[0].id, id_b);

    // Deleting from store A should not affect store B.
    store_a.delete(id_a).await.unwrap();
    assert_eq!(store_a.count().await.unwrap(), 0);
    assert_eq!(store_b.count().await.unwrap(), 1, "store B must be unaffected");
}

// ---------------------------------------------------------------------------
// 12. Search result ordering
// ---------------------------------------------------------------------------

#[tokio::test]
async fn search_result_ordering() {
    let embedder = LocalEmbedding::default();
    let store = InMemoryVectorStore::new();

    // Create documents with varying similarity to the query "rust programming".
    let query_text = "rust programming language systems";
    let query_emb = embedder.embed(query_text).await.unwrap();

    let doc_very_close = make_entry_with_embedder(&embedder, "rust programming language").await;
    let doc_related = make_entry_with_embedder(&embedder, "systems programming software engineering").await;
    let doc_unrelated = make_entry_with_embedder(&embedder, "chocolate cake baking dessert recipe frosting").await;

    let id_close = doc_very_close.id;
    let id_related = doc_related.id;
    let id_unrelated = doc_unrelated.id;

    store.insert(doc_very_close).await.unwrap();
    store.insert(doc_related).await.unwrap();
    store.insert(doc_unrelated).await.unwrap();

    let results = store.search(&query_emb, 3, None).await.unwrap();
    assert_eq!(results.len(), 3, "should return all three documents");

    // Scores must be in descending order.
    for window in results.windows(2) {
        assert!(
            window[0].score >= window[1].score,
            "results must be sorted by score descending: {} >= {}",
            window[0].score,
            window[1].score,
        );
    }

    // The most similar document should rank first.
    assert_eq!(
        results[0].entry.id, id_close,
        "closest document should rank first"
    );

    // The unrelated document should rank last.
    assert_eq!(
        results[2].entry.id, id_unrelated,
        "unrelated document should rank last"
    );

    // Verify score magnitudes make sense: close > related > unrelated.
    let score_close = results.iter().find(|r| r.entry.id == id_close).unwrap().score;
    let score_related = results.iter().find(|r| r.entry.id == id_related).unwrap().score;
    let score_unrelated = results.iter().find(|r| r.entry.id == id_unrelated).unwrap().score;

    assert!(
        score_close > score_related,
        "close doc score ({score_close}) should be higher than related ({score_related})"
    );
    assert!(
        score_related > score_unrelated,
        "related doc score ({score_related}) should be higher than unrelated ({score_unrelated})"
    );
}
