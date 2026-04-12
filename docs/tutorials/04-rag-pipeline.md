# Tutorial 4: Building a RAG Pipeline

> Ingest your documents, embed them, and inject retrieved context into the agent's prompt — the "R" in RAG.

Large language models forget. A **Retrieval-Augmented Generation** (RAG) pipeline remembers for them: chunk your documents, store vector embeddings, semantic-search at query time, and pass the most relevant chunks as context to the LLM.

Argentor's `argentor-memory` crate gives you everything: `VectorStore`, `EmbeddingProvider`, chunking strategies, hybrid search (BM25 + embeddings), and a pre-wired `RagPipeline`.

---

## Prerequisites

- Completed [Tutorial 1](./01-first-agent.md)
- A working Cargo project with Argentor deps
- A folder of text/Markdown files you want to index (or we will create one)

---

## 1. Add the Memory Crate

```toml
[dependencies]
argentor-memory = { git = "https://github.com/fboiero/Agentor", branch = "master" }
```

The re-exports we need:

```rust
use argentor_memory::{
    ChunkingStrategy, Document, InMemoryVectorStore, LocalEmbedding,
    RagConfig, RagPipeline, VectorStore, EmbeddingProvider,
};
```

---

## 2. Pick a Vector Store

Argentor ships four implementations of the `VectorStore` trait:

| Store | Persistence | Use case |
|-------|-------------|----------|
| `InMemoryVectorStore` | none | tests, short-lived jobs |
| `FileVectorStore` | JSONL on disk | local dev, single-node prod |
| `PineconeStore` | managed | production, multi-region |
| Weaviate / Qdrant / pgvector | external | teams already running them |

> The Pinecone, Weaviate, Qdrant, and pgvector adapters currently default to a **stub mode** (in-memory, same trait surface). Real HTTP calls are gated behind the `http-vectorstore` feature flag. The adapters ship so you can point your code at them now and flip the flag later without refactoring.

```rust
use argentor_memory::PineconeStore;
use std::sync::Arc;

let store: Arc<dyn VectorStore> = Arc::new(
    PineconeStore::new("pcsk-...", "my-index", "us-east-1-aws")
);
```

For this tutorial we use `InMemoryVectorStore` so you do not need external services.

---

## 3. Pick an Embedding Provider

Argentor ships:

- `LocalEmbedding` — zero-cost TF-IDF-style embeddings (bag-of-words FNV, 256 dims). Great for tests.
- `OpenAiEmbeddingProvider` — `text-embedding-3-small` / `-large`.
- `CohereEmbeddingProvider` — Cohere's embed API.
- `VoyageEmbeddingProvider` — Voyage AI.
- `CachedEmbeddingProvider` — wraps any provider with an in-memory LRU cache.
- `BatchEmbeddingProvider` — coalesces many small requests into batches.

```rust
use argentor_memory::LocalEmbedding;
use std::sync::Arc;

let embedder: Arc<dyn EmbeddingProvider> = Arc::new(LocalEmbedding::default());
```

---

## 4. Build the RAG Pipeline

```rust
use argentor_memory::{RagConfig, RagPipeline, ChunkingStrategy};

let config = RagConfig {
    chunking: ChunkingStrategy::FixedSize { chunk_size: 512, overlap: 64 },
    top_k: 5,
    min_relevance_score: 0.3,
    include_metadata: true,
    max_context_tokens: 4096,
};

let rag = RagPipeline::new(store.clone(), embedder.clone(), config);
```

### Chunking strategies

- `FixedSize { chunk_size, overlap }` — character windows with overlap
- `Paragraph` — split on blank lines
- `Sentence` — split on `.`/`!`/`?`
- `Semantic { max_chunk_tokens }` — split on Markdown headings, merge small sections up to a token budget

---

## 5. Ingest Documents

```rust
use argentor_memory::Document;
use std::collections::HashMap;

let mut docs = Vec::new();

// Manually construct a document
docs.push(Document {
    id: "doc-001".into(),
    title: "Argentor Architecture".into(),
    content: std::fs::read_to_string("./docs/ARCHITECTURE.md")?,
    source: "internal-docs".into(),
    metadata: HashMap::from([("version".into(), "1.0".into())]),
    category: Some("docs".into()),
});

// Or ingest a whole folder
for entry in std::fs::read_dir("./knowledge")? {
    let path = entry?.path();
    if path.extension().map(|e| e == "md").unwrap_or(false) {
        let content = std::fs::read_to_string(&path)?;
        let id = path.file_stem().unwrap().to_string_lossy().into_owned();
        docs.push(Document {
            id: id.clone(),
            title: id,
            content,
            source: path.display().to_string(),
            metadata: HashMap::new(),
            category: None,
        });
    }
}

// Ingest all at once
for doc in &docs {
    rag.ingest(doc).await?;
}

println!("Ingested {} documents", docs.len());
```

Under the hood, `ingest()` does:

1. `chunk_document(doc, &strategy)` → `Vec<DocumentChunk>`
2. For each chunk: `embedder.embed(&chunk.content)` → `Vec<f32>`
3. Store `MemoryEntry { id, content, embedding, metadata }` in the `VectorStore`

---

## 6. Query the Pipeline

```rust
let result = rag.query("How does Argentor's WASM sandbox work?").await?;

println!("Searched {} chunks in {} ms", result.total_chunks_searched, result.query_time_ms);
for chunk in &result.chunks {
    println!(
        "  [{:.3}] {} / {}: {}...",
        chunk.score,
        chunk.document_title,
        chunk.chunk.chunk_id,
        &chunk.chunk.content[..chunk.chunk.content.len().min(120)],
    );
}

println!("\n=== Context for LLM ===\n{}", result.context_text);
```

Output:

```
Searched 87 chunks in 12 ms
  [0.842] Argentor Architecture / doc-001_chunk_4: WASM plugins run inside wasmtime...
  [0.791] Security Model / doc-003_chunk_2: Each skill declares capabilities...
  [0.715] Skills / doc-005_chunk_1: The WasmSkillRuntime loads .wasm files...

=== Context for LLM ===
[doc-001 — Argentor Architecture] WASM plugins run inside wasmtime...
[doc-003 — Security Model] Each skill declares capabilities...
[doc-005 — Skills] The WasmSkillRuntime loads .wasm files...
```

---

## 7. Inject Context into the Agent

The agent does not know about your knowledge base unless you put the retrieved chunks into its system prompt or conversation. A standard pattern:

```rust
use argentor_agent::{AgentRunner, LlmProvider, ModelConfig};
use argentor_session::Session;

let user_question = "How does Argentor's WASM sandbox work?";

// 1. Retrieve context.
let rag_result = rag.query(user_question).await?;

// 2. Compose an augmented system prompt.
let augmented_prompt = format!(
    "You are a helpful assistant answering questions about Argentor.\n\
     Use ONLY the context below to answer. If the answer is not in context, say so.\n\n\
     === CONTEXT ===\n{}\n\n=== END CONTEXT ===",
    rag_result.context_text,
);

// 3. Build the agent with the augmented prompt.
let config = ModelConfig {
    provider: LlmProvider::Claude,
    model_id: "claude-sonnet-4-20250514".into(),
    api_key: std::env::var("ANTHROPIC_API_KEY")?,
    api_base_url: None,
    temperature: 0.3,
    max_tokens: 2048,
    max_turns: 3,
    fallback_models: vec![],
    retry_policy: None,
};

let runner = AgentRunner::new(
    config,
    Arc::new(argentor_skills::SkillRegistry::new()),
    argentor_security::PermissionSet::new(),
    Arc::new(argentor_security::AuditLog::new(std::path::PathBuf::from("./audit"))),
)
.with_system_prompt(augmented_prompt);

let mut session = Session::new();
let answer = runner.run(&mut session, user_question).await?;
println!("\n{answer}");
```

### Example answer

```
Based on the provided context, Argentor's WASM sandbox uses wasmtime (the
reference WebAssembly runtime) to load .wasm plugin files compiled from
skill source. Each plugin runs with WASI enabled but no ambient file or
network access — capabilities must be explicitly granted to the host
before the plugin can perform any I/O.
```

---

## 8. Hybrid Search (BM25 + Embeddings)

Pure vector similarity misses exact keywords. The `HybridSearcher` combines BM25 scoring with embedding similarity using Reciprocal Rank Fusion:

```rust
use argentor_memory::{Bm25Index, HybridSearcher};

let mut bm25 = Bm25Index::new();
for doc in &docs {
    bm25.add(&doc.id, &doc.content);
}

let hybrid = HybridSearcher::new(store.clone(), Arc::new(bm25));
let results = hybrid.search("WASM sandbox capabilities", 5).await?;
```

Hybrid search typically boosts recall by 15-30% on real corpora.

---

## 9. Query Expansion

Users rarely type the best query. `RuleBasedExpander` rewrites and expands queries using a synonym map:

```rust
use argentor_memory::{QueryExpander, RuleBasedExpander};

let expander = RuleBasedExpander::default();
let expanded = expander.expand("how to deploy");
// -> ["how to deploy", "how to ship", "how to release", "how to publish"]

// Use with RAG:
for q in expanded {
    let result = rag.query(&q).await?;
    // merge / dedupe chunks
}
```

---

## 10. Persistence

`InMemoryVectorStore` loses everything on shutdown. Two options:

**FileVectorStore** — JSONL-backed, commit to git, survives restarts:

```rust
use argentor_memory::FileVectorStore;

let store: Arc<dyn VectorStore> = Arc::new(
    FileVectorStore::new("./vector-data.jsonl").await?
);
```

**Managed backend** — `PineconeStore`, Weaviate, Qdrant, pgvector. Enable the `http-vectorstore` feature in your `Cargo.toml`:

```toml
argentor-memory = { git = "...", branch = "master", features = ["http-vectorstore"] }
```

The trait surface is the same — only the constructor changes.

---

## Common Issues

**`score` values are very low (< 0.3)**
`LocalEmbedding` uses 256 bag-of-words dimensions — it is great for smoke tests but mediocre for semantic recall. Switch to `OpenAiEmbeddingProvider` (`text-embedding-3-small` is fast and cheap).

**Memory usage grows unbounded**
You are ingesting duplicates. `Document.id` is the dedup key. Reuse the same ID when re-ingesting a document.

**Retrieved chunks are irrelevant**
Tune `top_k`, lower `min_relevance_score`, or switch chunking from `FixedSize` to `Semantic { max_chunk_tokens: 512 }` to preserve headings.

**LLM ignores the context**
Strengthen the system prompt: `"Answer ONLY from the context below. If the answer is not present, reply 'I don't know based on the provided documents.'"` Low temperature (0.0–0.3) also reduces hallucination.

**"File too large" during ingestion**
Use smaller chunks, or pre-chunk the file manually and call `rag.ingest()` per chunk.

---

## What You Built

- A document-ingestion pipeline with pluggable chunking strategies
- A vector store holding embeddings (swap in Pinecone / Weaviate / pgvector with one line)
- A hybrid retriever combining BM25 and vector similarity
- An agent whose system prompt is dynamically augmented with retrieved context

---

## Next Steps

- **[Tutorial 3: Multi-Agent Orchestration](./03-multi-agent-orchestration.md)** — share a RAG store across a whole team of workers.
- **[Tutorial 7: Agent Intelligence](./07-agent-intelligence.md)** — combine RAG with extended thinking and self-critique.
- **[Tutorial 10: Observability](./10-observability.md)** — trace every retrieval in OTLP so you can debug latency.
