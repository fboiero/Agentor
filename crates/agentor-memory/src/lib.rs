//! Vector-based semantic memory with hybrid search and query expansion.
//!
//! Provides persistent vector storage, local embedding generation,
//! BM25 keyword scoring, hybrid (embedding + BM25) search, and
//! rule-based query expansion for improved recall.
//!
//! # Main types
//!
//! - [`VectorStore`] — Trait for storing and querying embedding vectors.
//! - [`FileVectorStore`] — File-backed persistent vector store.
//! - [`LocalEmbedding`] — Local TF-IDF-based embedding provider.
//! - [`HybridSearcher`] — Combines embedding similarity with BM25 keyword scoring.
//! - [`Bm25Index`] — BM25 inverted index for keyword-based retrieval.
//! - [`QueryExpander`] — Trait for expanding queries to improve search recall.

/// BM25 inverted index for keyword-based retrieval.
pub mod bm25;
/// Embedding provider trait and local implementation.
pub mod embedding;
/// Hybrid search combining embeddings and BM25.
pub mod hybrid;
/// Query expansion for improved recall.
pub mod query_expansion;
/// Vector store trait and file-backed implementation.
pub mod store;

pub use bm25::Bm25Index;
pub use embedding::{EmbeddingProvider, LocalEmbedding};
pub use hybrid::HybridSearcher;
pub use query_expansion::{QueryExpander, RuleBasedExpander};
pub use store::{FileVectorStore, InMemoryVectorStore, MemoryEntry, SearchResult, VectorStore};
