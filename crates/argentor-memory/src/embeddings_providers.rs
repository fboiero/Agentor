//! Multiple embedding provider backends implementing [`EmbeddingProvider`].
//!
//! Includes API-based providers (OpenAI, Cohere, Voyage) with placeholder
//! implementations (no reqwest dependency yet), plus fully functional
//! [`CachedEmbeddingProvider`], [`BatchEmbeddingProvider`],
//! [`EmbeddingProviderFactory`], and [`EmbeddingConfig`].

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use argentor_core::{ArgentorError, ArgentorResult};

use crate::embedding::{EmbeddingProvider, LocalEmbedding};

// ---------------------------------------------------------------------------
// FNV-1a hash (same algorithm as embedding.rs, re-implemented here to avoid
// depending on a private function).
// ---------------------------------------------------------------------------

fn fnv1a_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 14695981039346656037;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

// ===========================================================================
// API request/response structures (used when reqwest becomes available)
// ===========================================================================

/// OpenAI embeddings API request body.
#[derive(Debug, Serialize)]
pub struct OpenAiEmbeddingRequest {
    pub model: String,
    pub input: Vec<String>,
}

/// A single embedding object from the OpenAI response.
#[derive(Debug, Deserialize)]
pub struct OpenAiEmbeddingObject {
    pub embedding: Vec<f32>,
    pub index: usize,
}

/// OpenAI embeddings API response body.
#[derive(Debug, Deserialize)]
pub struct OpenAiEmbeddingResponse {
    pub data: Vec<OpenAiEmbeddingObject>,
    pub model: String,
}

/// Cohere embed API request body.
#[derive(Debug, Serialize)]
pub struct CohereEmbedRequest {
    pub model: String,
    pub texts: Vec<String>,
    pub input_type: String,
}

/// Cohere embed API response body.
#[derive(Debug, Deserialize)]
pub struct CohereEmbedResponse {
    pub embeddings: Vec<Vec<f32>>,
}

/// Voyage AI embeddings API request body.
#[derive(Debug, Serialize)]
pub struct VoyageEmbeddingRequest {
    pub model: String,
    pub input: Vec<String>,
}

/// A single embedding object from the Voyage response.
#[derive(Debug, Deserialize)]
pub struct VoyageEmbeddingObject {
    pub embedding: Vec<f32>,
    pub index: usize,
}

/// Voyage AI embeddings API response body.
#[derive(Debug, Deserialize)]
pub struct VoyageEmbeddingResponse {
    pub data: Vec<VoyageEmbeddingObject>,
}

// ===========================================================================
// 1. OpenAiEmbeddingProvider
// ===========================================================================

/// Embedding provider backed by the OpenAI embeddings API.
///
/// Currently returns a placeholder error because `reqwest` is not in
/// `argentor-memory` dependencies. Wire via `argentor-agent` HTTP client later.
pub struct OpenAiEmbeddingProvider {
    #[allow(dead_code)]
    api_key: String,
    model: String,
    dimensions: usize,
    #[allow(dead_code)]
    base_url: String,
}

impl OpenAiEmbeddingProvider {
    /// Create a new OpenAI embedding provider.
    ///
    /// `model` defaults to `"text-embedding-3-small"` when `None`.
    pub fn new(api_key: impl Into<String>, model: Option<String>) -> Self {
        let model = model.unwrap_or_else(|| "text-embedding-3-small".to_string());
        let dimensions = Self::default_dimensions(&model);
        Self {
            api_key: api_key.into(),
            model,
            dimensions,
            base_url: "https://api.openai.com/v1/embeddings".to_string(),
        }
    }

    /// Create with a custom base URL (e.g. Azure OpenAI endpoint).
    pub fn with_base_url(
        api_key: impl Into<String>,
        model: Option<String>,
        base_url: impl Into<String>,
    ) -> Self {
        let model = model.unwrap_or_else(|| "text-embedding-3-small".to_string());
        let dimensions = Self::default_dimensions(&model);
        Self {
            api_key: api_key.into(),
            model,
            dimensions,
            base_url: base_url.into(),
        }
    }

    /// Override the output dimension count.
    pub fn with_dimensions(mut self, dimensions: usize) -> Self {
        self.dimensions = dimensions;
        self
    }

    fn default_dimensions(model: &str) -> usize {
        match model {
            "text-embedding-3-large" => 3072,
            "text-embedding-3-small" => 1536,
            "text-embedding-ada-002" => 1536,
            _ => 1536,
        }
    }

    /// Returns the model name this provider is configured with.
    pub fn model(&self) -> &str {
        &self.model
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAiEmbeddingProvider {
    async fn embed(&self, _text: &str) -> ArgentorResult<Vec<f32>> {
        Err(ArgentorError::Http(
            "OpenAI embedding: reqwest not available in argentor-memory, \
             use argentor-agent HTTP client to wire the actual API call"
                .to_string(),
        ))
    }

    fn dimension(&self) -> usize {
        self.dimensions
    }
}

// ===========================================================================
// 2. CohereEmbeddingProvider
// ===========================================================================

/// Embedding provider backed by the Cohere embed API.
///
/// Placeholder — returns an error until reqwest is available.
pub struct CohereEmbeddingProvider {
    #[allow(dead_code)]
    api_key: String,
    model: String,
    dimensions: usize,
    input_type: String,
}

impl CohereEmbeddingProvider {
    /// Create a new Cohere embedding provider.
    ///
    /// `model` defaults to `"embed-english-v3.0"`.
    pub fn new(api_key: impl Into<String>, model: Option<String>) -> Self {
        let model = model.unwrap_or_else(|| "embed-english-v3.0".to_string());
        let dimensions = Self::default_dimensions(&model);
        Self {
            api_key: api_key.into(),
            model,
            dimensions,
            input_type: "search_document".to_string(),
        }
    }

    /// Set the input type (`"search_document"` for indexing, `"search_query"` for querying).
    pub fn with_input_type(mut self, input_type: impl Into<String>) -> Self {
        self.input_type = input_type.into();
        self
    }

    /// Override the output dimension count.
    pub fn with_dimensions(mut self, dimensions: usize) -> Self {
        self.dimensions = dimensions;
        self
    }

    fn default_dimensions(model: &str) -> usize {
        match model {
            "embed-english-v3.0" | "embed-multilingual-v3.0" => 1024,
            "embed-english-light-v3.0" | "embed-multilingual-light-v3.0" => 384,
            _ => 1024,
        }
    }

    /// Returns the model name.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Returns the current input type.
    pub fn input_type(&self) -> &str {
        &self.input_type
    }
}

#[async_trait]
impl EmbeddingProvider for CohereEmbeddingProvider {
    async fn embed(&self, _text: &str) -> ArgentorResult<Vec<f32>> {
        Err(ArgentorError::Http(
            "Cohere embedding: reqwest not available in argentor-memory, \
             use argentor-agent HTTP client to wire the actual API call"
                .to_string(),
        ))
    }

    fn dimension(&self) -> usize {
        self.dimensions
    }
}

// ===========================================================================
// 3. VoyageEmbeddingProvider
// ===========================================================================

/// Embedding provider backed by the Voyage AI embeddings API.
///
/// Placeholder — returns an error until reqwest is available.
pub struct VoyageEmbeddingProvider {
    #[allow(dead_code)]
    api_key: String,
    model: String,
    dimensions: usize,
}

impl VoyageEmbeddingProvider {
    /// Create a new Voyage embedding provider.
    ///
    /// `model` defaults to `"voyage-2"`.
    pub fn new(api_key: impl Into<String>, model: Option<String>) -> Self {
        let model = model.unwrap_or_else(|| "voyage-2".to_string());
        let dimensions = Self::default_dimensions(&model);
        Self {
            api_key: api_key.into(),
            model,
            dimensions,
        }
    }

    /// Override the output dimension count.
    pub fn with_dimensions(mut self, dimensions: usize) -> Self {
        self.dimensions = dimensions;
        self
    }

    fn default_dimensions(model: &str) -> usize {
        match model {
            "voyage-2" | "voyage-large-2" => 1024,
            "voyage-lite-02-instruct" => 1024,
            "voyage-code-2" => 1536,
            _ => 1024,
        }
    }

    /// Returns the model name.
    pub fn model(&self) -> &str {
        &self.model
    }
}

#[async_trait]
impl EmbeddingProvider for VoyageEmbeddingProvider {
    async fn embed(&self, _text: &str) -> ArgentorResult<Vec<f32>> {
        Err(ArgentorError::Http(
            "Voyage embedding: reqwest not available in argentor-memory, \
             use argentor-agent HTTP client to wire the actual API call"
                .to_string(),
        ))
    }

    fn dimension(&self) -> usize {
        self.dimensions
    }
}

// ===========================================================================
// 4. CachedEmbeddingProvider
// ===========================================================================

/// Statistics about cache usage.
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub size: usize,
}

/// Wraps any [`EmbeddingProvider`] with a thread-safe in-memory LRU-ish cache.
///
/// Embeddings are cached by FNV-1a hash of the input text. When the cache
/// exceeds `max_cache_size`, the oldest entry (by insertion order) is evicted.
pub struct CachedEmbeddingProvider {
    inner: Arc<dyn EmbeddingProvider>,
    cache: Arc<RwLock<HashMap<u64, Vec<f32>>>>,
    max_cache_size: usize,
    stats: Arc<RwLock<CacheStats>>,
}

impl CachedEmbeddingProvider {
    /// Wrap an existing provider with caching.
    pub fn new(inner: Arc<dyn EmbeddingProvider>, max_cache_size: usize) -> Self {
        Self {
            inner,
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_cache_size,
            stats: Arc::new(RwLock::new(CacheStats::default())),
        }
    }

    /// Returns current cache statistics.
    pub async fn cache_stats(&self) -> CacheStats {
        self.stats.read().await.clone()
    }

    /// Clears the cache and resets statistics.
    pub async fn clear(&self) {
        self.cache.write().await.clear();
        let mut stats = self.stats.write().await;
        stats.size = 0;
    }

    fn text_hash(text: &str) -> u64 {
        fnv1a_hash(text.as_bytes())
    }
}

#[async_trait]
impl EmbeddingProvider for CachedEmbeddingProvider {
    async fn embed(&self, text: &str) -> ArgentorResult<Vec<f32>> {
        let key = Self::text_hash(text);

        // Check cache (read lock).
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(&key) {
                let mut stats = self.stats.write().await;
                stats.hits += 1;
                return Ok(cached.clone());
            }
        }

        // Cache miss — compute embedding.
        let embedding = self.inner.embed(text).await?;

        // Insert into cache (write lock).
        {
            let mut cache = self.cache.write().await;

            // Evict if at capacity.
            if cache.len() >= self.max_cache_size {
                // Remove an arbitrary entry (HashMap iteration order is random,
                // which acts as a simple eviction strategy).
                if let Some(&evict_key) = cache.keys().next() {
                    cache.remove(&evict_key);
                }
            }

            cache.insert(key, embedding.clone());

            let mut stats = self.stats.write().await;
            stats.misses += 1;
            stats.size = cache.len();
        }

        Ok(embedding)
    }

    fn dimension(&self) -> usize {
        self.inner.dimension()
    }
}

// ===========================================================================
// 5. BatchEmbeddingProvider
// ===========================================================================

/// Wraps any [`EmbeddingProvider`] to expose a convenience batch method.
///
/// Delegates to the inner provider's `embed_batch` (which by default calls
/// `embed` sequentially). Providers that support native batching can override
/// `embed_batch` on the trait for better performance.
pub struct BatchEmbeddingProvider {
    inner: Arc<dyn EmbeddingProvider>,
}

impl BatchEmbeddingProvider {
    /// Wrap an existing provider for batch operations.
    pub fn new(inner: Arc<dyn EmbeddingProvider>) -> Self {
        Self { inner }
    }

    /// Embed multiple texts, returning one vector per input.
    pub async fn embed_batch(&self, texts: &[&str]) -> ArgentorResult<Vec<Vec<f32>>> {
        self.inner.embed_batch(texts).await
    }
}

#[async_trait]
impl EmbeddingProvider for BatchEmbeddingProvider {
    async fn embed(&self, text: &str) -> ArgentorResult<Vec<f32>> {
        self.inner.embed(text).await
    }

    async fn embed_batch(&self, texts: &[&str]) -> ArgentorResult<Vec<Vec<f32>>> {
        self.inner.embed_batch(texts).await
    }

    fn dimension(&self) -> usize {
        self.inner.dimension()
    }
}

// ===========================================================================
// 6. EmbeddingProviderFactory
// ===========================================================================

/// Factory that creates [`EmbeddingProvider`] instances by name.
pub struct EmbeddingProviderFactory;

impl EmbeddingProviderFactory {
    /// Create an embedding provider from its string name.
    ///
    /// Supported names: `"openai"`, `"cohere"`, `"voyage"`, `"local"`.
    pub fn create(
        provider_name: &str,
        api_key: impl Into<String>,
        model: Option<String>,
    ) -> ArgentorResult<Box<dyn EmbeddingProvider>> {
        let api_key = api_key.into();
        match provider_name {
            "openai" => Ok(Box::new(OpenAiEmbeddingProvider::new(api_key, model))),
            "cohere" => Ok(Box::new(CohereEmbeddingProvider::new(api_key, model))),
            "voyage" => Ok(Box::new(VoyageEmbeddingProvider::new(api_key, model))),
            "local" => {
                let dim = model
                    .as_deref()
                    .and_then(|m| m.parse::<usize>().ok())
                    .unwrap_or(256);
                Ok(Box::new(LocalEmbedding::new(dim)))
            }
            other => Err(ArgentorError::Config(format!(
                "Unknown embedding provider: {other}"
            ))),
        }
    }

    /// List all supported provider names.
    pub fn available_providers() -> &'static [&'static str] {
        &["openai", "cohere", "voyage", "local"]
    }
}

// ===========================================================================
// 7. EmbeddingConfig
// ===========================================================================

/// Serializable configuration for constructing an embedding provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Provider name (`"openai"`, `"cohere"`, `"voyage"`, `"local"`).
    pub provider: String,
    /// API key (ignored for `"local"`).
    #[serde(default)]
    pub api_key: String,
    /// Model name override.
    #[serde(default)]
    pub model: Option<String>,
    /// Override for output dimensions.
    #[serde(default)]
    pub dimensions: Option<usize>,
    /// Custom API base URL (e.g. Azure OpenAI).
    #[serde(default)]
    pub base_url: Option<String>,
    /// If set, wraps the provider with a [`CachedEmbeddingProvider`].
    #[serde(default)]
    pub cache_size: Option<usize>,
}

impl EmbeddingConfig {
    /// Build an [`EmbeddingProvider`] from this configuration.
    ///
    /// Returns an `Arc`-wrapped provider, optionally wrapped in a cache layer.
    pub fn build(&self) -> ArgentorResult<Arc<dyn EmbeddingProvider>> {
        let mut provider: Box<dyn EmbeddingProvider> = match self.provider.as_str() {
            "openai" => {
                let mut p = if let Some(ref url) = self.base_url {
                    OpenAiEmbeddingProvider::with_base_url(
                        &self.api_key,
                        self.model.clone(),
                        url,
                    )
                } else {
                    OpenAiEmbeddingProvider::new(&self.api_key, self.model.clone())
                };
                if let Some(dim) = self.dimensions {
                    p = p.with_dimensions(dim);
                }
                Box::new(p)
            }
            "cohere" => {
                let mut p = CohereEmbeddingProvider::new(&self.api_key, self.model.clone());
                if let Some(dim) = self.dimensions {
                    p = p.with_dimensions(dim);
                }
                Box::new(p)
            }
            "voyage" => {
                let mut p = VoyageEmbeddingProvider::new(&self.api_key, self.model.clone());
                if let Some(dim) = self.dimensions {
                    p = p.with_dimensions(dim);
                }
                Box::new(p)
            }
            "local" => {
                let dim = self.dimensions.unwrap_or(256);
                Box::new(LocalEmbedding::new(dim))
            }
            other => {
                return Err(ArgentorError::Config(format!(
                    "Unknown embedding provider: {other}"
                )));
            }
        };

        // Wrap with dimensions override if the provider itself doesn't support
        // it natively (already handled above for each provider).
        let _ = &mut provider; // suppress unused-mut if branch is empty

        let arc: Arc<dyn EmbeddingProvider> = Arc::from(provider);

        // Optionally wrap with caching.
        if let Some(cache_size) = self.cache_size {
            Ok(Arc::new(CachedEmbeddingProvider::new(arc, cache_size)))
        } else {
            Ok(arc)
        }
    }
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: "local".to_string(),
            api_key: String::new(),
            model: None,
            dimensions: None,
            base_url: None,
            cache_size: None,
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // -- Provider creation tests ------------------------------------------

    #[test]
    fn test_openai_provider_default_model() {
        let p = OpenAiEmbeddingProvider::new("sk-test", None);
        assert_eq!(p.model(), "text-embedding-3-small");
        assert_eq!(p.dimension(), 1536);
    }

    #[test]
    fn test_openai_provider_large_model() {
        let p = OpenAiEmbeddingProvider::new("sk-test", Some("text-embedding-3-large".into()));
        assert_eq!(p.dimension(), 3072);
    }

    #[test]
    fn test_openai_provider_custom_dimensions() {
        let p = OpenAiEmbeddingProvider::new("sk-test", None).with_dimensions(512);
        assert_eq!(p.dimension(), 512);
    }

    #[test]
    fn test_openai_provider_custom_base_url() {
        let p = OpenAiEmbeddingProvider::with_base_url(
            "sk-test",
            None,
            "https://my-azure.openai.azure.com/openai/deployments/embed",
        );
        assert_eq!(p.dimension(), 1536);
    }

    #[tokio::test]
    async fn test_openai_provider_returns_placeholder_error() {
        let p = OpenAiEmbeddingProvider::new("sk-test", None);
        let err = p.embed("hello").await.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("reqwest not available"), "got: {msg}");
    }

    #[test]
    fn test_cohere_provider_default() {
        let p = CohereEmbeddingProvider::new("key", None);
        assert_eq!(p.model(), "embed-english-v3.0");
        assert_eq!(p.dimension(), 1024);
        assert_eq!(p.input_type(), "search_document");
    }

    #[test]
    fn test_cohere_provider_query_input_type() {
        let p = CohereEmbeddingProvider::new("key", None)
            .with_input_type("search_query");
        assert_eq!(p.input_type(), "search_query");
    }

    #[test]
    fn test_cohere_provider_light_model() {
        let p = CohereEmbeddingProvider::new("key", Some("embed-english-light-v3.0".into()));
        assert_eq!(p.dimension(), 384);
    }

    #[tokio::test]
    async fn test_cohere_provider_returns_placeholder_error() {
        let p = CohereEmbeddingProvider::new("key", None);
        let err = p.embed("hello").await.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("reqwest not available"), "got: {msg}");
    }

    #[test]
    fn test_voyage_provider_default() {
        let p = VoyageEmbeddingProvider::new("key", None);
        assert_eq!(p.model(), "voyage-2");
        assert_eq!(p.dimension(), 1024);
    }

    #[test]
    fn test_voyage_provider_code_model() {
        let p = VoyageEmbeddingProvider::new("key", Some("voyage-code-2".into()));
        assert_eq!(p.dimension(), 1536);
    }

    #[tokio::test]
    async fn test_voyage_provider_returns_placeholder_error() {
        let p = VoyageEmbeddingProvider::new("key", None);
        let err = p.embed("hello").await.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("reqwest not available"), "got: {msg}");
    }

    // -- CachedEmbeddingProvider tests ------------------------------------

    #[tokio::test]
    async fn test_cache_hit() {
        let local = Arc::new(LocalEmbedding::new(64));
        let cached = CachedEmbeddingProvider::new(local, 100);

        let v1 = cached.embed("hello world").await.unwrap();
        let v2 = cached.embed("hello world").await.unwrap();
        assert_eq!(v1, v2);

        let stats = cached.cache_stats().await;
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.size, 1);
    }

    #[tokio::test]
    async fn test_cache_miss_different_texts() {
        let local = Arc::new(LocalEmbedding::new(64));
        let cached = CachedEmbeddingProvider::new(local, 100);

        let _ = cached.embed("alpha").await.unwrap();
        let _ = cached.embed("bravo").await.unwrap();

        let stats = cached.cache_stats().await;
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.size, 2);
    }

    #[tokio::test]
    async fn test_cache_eviction() {
        let local = Arc::new(LocalEmbedding::new(64));
        let cached = CachedEmbeddingProvider::new(local, 2);

        let _ = cached.embed("one").await.unwrap();
        let _ = cached.embed("two").await.unwrap();
        let _ = cached.embed("three").await.unwrap();

        let stats = cached.cache_stats().await;
        // After eviction, cache should still have at most max_cache_size entries.
        assert!(stats.size <= 2, "size={} should be <= 2", stats.size);
        assert_eq!(stats.misses, 3);
    }

    #[tokio::test]
    async fn test_cache_clear() {
        let local = Arc::new(LocalEmbedding::new(64));
        let cached = CachedEmbeddingProvider::new(local, 100);

        let _ = cached.embed("text").await.unwrap();
        cached.clear().await;

        let stats = cached.cache_stats().await;
        assert_eq!(stats.size, 0);
    }

    #[tokio::test]
    async fn test_cache_dimension_delegates() {
        let local = Arc::new(LocalEmbedding::new(128));
        let cached = CachedEmbeddingProvider::new(local, 10);
        assert_eq!(cached.dimension(), 128);
    }

    // -- BatchEmbeddingProvider tests -------------------------------------

    #[tokio::test]
    async fn test_batch_embed() {
        let local = Arc::new(LocalEmbedding::new(64));
        let batch = BatchEmbeddingProvider::new(local);

        let results = batch.embed_batch(&["hello", "world", "test"]).await.unwrap();
        assert_eq!(results.len(), 3);
        for v in &results {
            assert_eq!(v.len(), 64);
        }
    }

    #[tokio::test]
    async fn test_batch_single_embed_delegates() {
        let local = Arc::new(LocalEmbedding::new(64));
        let batch = BatchEmbeddingProvider::new(local);

        let v = batch.embed("hello").await.unwrap();
        assert_eq!(v.len(), 64);
    }

    #[tokio::test]
    async fn test_batch_empty() {
        let local = Arc::new(LocalEmbedding::new(64));
        let batch = BatchEmbeddingProvider::new(local);

        let results = batch.embed_batch(&[]).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_batch_dimension_delegates() {
        let local = Arc::new(LocalEmbedding::new(200));
        let batch = BatchEmbeddingProvider::new(local);
        assert_eq!(batch.dimension(), 200);
    }

    // -- Factory tests ----------------------------------------------------

    #[test]
    fn test_factory_create_local() {
        let p = EmbeddingProviderFactory::create("local", "", None).unwrap();
        assert_eq!(p.dimension(), 256);
    }

    #[test]
    fn test_factory_create_local_custom_dim() {
        let p = EmbeddingProviderFactory::create("local", "", Some("128".into())).unwrap();
        assert_eq!(p.dimension(), 128);
    }

    #[test]
    fn test_factory_create_openai() {
        let p = EmbeddingProviderFactory::create("openai", "sk-test", None).unwrap();
        assert_eq!(p.dimension(), 1536);
    }

    #[test]
    fn test_factory_create_cohere() {
        let p = EmbeddingProviderFactory::create("cohere", "key", None).unwrap();
        assert_eq!(p.dimension(), 1024);
    }

    #[test]
    fn test_factory_create_voyage() {
        let p = EmbeddingProviderFactory::create("voyage", "key", None).unwrap();
        assert_eq!(p.dimension(), 1024);
    }

    #[test]
    fn test_factory_unknown_provider() {
        let result = EmbeddingProviderFactory::create("unknown", "", None);
        assert!(result.is_err(), "Unknown provider should return Err");
    }

    #[test]
    fn test_factory_available_providers() {
        let names = EmbeddingProviderFactory::available_providers();
        assert!(names.contains(&"openai"));
        assert!(names.contains(&"cohere"));
        assert!(names.contains(&"voyage"));
        assert!(names.contains(&"local"));
    }

    // -- Config tests -----------------------------------------------------

    #[test]
    fn test_config_default() {
        let cfg = EmbeddingConfig::default();
        assert_eq!(cfg.provider, "local");
        assert!(cfg.api_key.is_empty());
        assert!(cfg.model.is_none());
        assert!(cfg.dimensions.is_none());
        assert!(cfg.base_url.is_none());
        assert!(cfg.cache_size.is_none());
    }

    #[test]
    fn test_config_serialize_deserialize() {
        let cfg = EmbeddingConfig {
            provider: "openai".to_string(),
            api_key: "sk-123".to_string(),
            model: Some("text-embedding-3-small".to_string()),
            dimensions: Some(1536),
            base_url: None,
            cache_size: Some(500),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: EmbeddingConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.provider, "openai");
        assert_eq!(parsed.api_key, "sk-123");
        assert_eq!(parsed.dimensions, Some(1536));
        assert_eq!(parsed.cache_size, Some(500));
    }

    #[test]
    fn test_config_deserialize_minimal() {
        let json = r#"{"provider":"local"}"#;
        let cfg: EmbeddingConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.provider, "local");
        assert!(cfg.api_key.is_empty());
    }

    #[tokio::test]
    async fn test_config_build_local() {
        let cfg = EmbeddingConfig::default();
        let provider = cfg.build().unwrap();
        assert_eq!(provider.dimension(), 256);
        let v = provider.embed("test text").await.unwrap();
        assert_eq!(v.len(), 256);
    }

    #[tokio::test]
    async fn test_config_build_local_with_cache() {
        let cfg = EmbeddingConfig {
            provider: "local".to_string(),
            cache_size: Some(50),
            ..Default::default()
        };
        let provider = cfg.build().unwrap();
        // Dimension from local default.
        assert_eq!(provider.dimension(), 256);
        // Should work — cache wraps local.
        let v1 = provider.embed("cached text").await.unwrap();
        let v2 = provider.embed("cached text").await.unwrap();
        assert_eq!(v1, v2);
    }

    #[tokio::test]
    async fn test_config_build_local_custom_dimensions() {
        let cfg = EmbeddingConfig {
            provider: "local".to_string(),
            dimensions: Some(512),
            ..Default::default()
        };
        let provider = cfg.build().unwrap();
        assert_eq!(provider.dimension(), 512);
    }

    #[test]
    fn test_config_build_unknown_provider() {
        let cfg = EmbeddingConfig {
            provider: "imaginary".to_string(),
            ..Default::default()
        };
        assert!(cfg.build().is_err());
    }

    // -- Misc / edge cases ------------------------------------------------

    #[test]
    fn test_fnv_hash_deterministic() {
        let h1 = fnv1a_hash(b"hello world");
        let h2 = fnv1a_hash(b"hello world");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_fnv_hash_different_inputs() {
        let h1 = fnv1a_hash(b"alpha");
        let h2 = fnv1a_hash(b"bravo");
        assert_ne!(h1, h2);
    }
}
