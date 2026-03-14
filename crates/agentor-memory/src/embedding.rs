use agentor_core::{AgentorError, AgentorResult};
use async_trait::async_trait;
use std::collections::HashMap;

/// Trait for computing text embeddings (vector representations).
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Compute embedding vector for a single text.
    async fn embed(&self, text: &str) -> AgentorResult<Vec<f32>>;

    /// Compute embeddings for a batch of texts.
    async fn embed_batch(&self, texts: &[&str]) -> AgentorResult<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
    }

    /// Dimension of the embedding vectors produced by this provider.
    fn dimension(&self) -> usize;
}

/// Local bag-of-words embedding for MVP (no external API needed).
/// Uses TF-based sparse-to-dense mapping with a fixed dimension.
/// Good enough for basic semantic search; replace with OpenAI/Cohere embeddings in production.
pub struct LocalEmbedding {
    dimension: usize,
}

impl LocalEmbedding {
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
    }
}

impl Default for LocalEmbedding {
    fn default() -> Self {
        Self::new(256)
    }
}

#[async_trait]
impl EmbeddingProvider for LocalEmbedding {
    async fn embed(&self, text: &str) -> AgentorResult<Vec<f32>> {
        if text.is_empty() {
            return Err(AgentorError::Agent("Cannot embed empty text".to_string()));
        }

        // Simple bag-of-words hashing to a fixed-size vector
        let mut vector = vec![0.0f32; self.dimension];

        let lowered = text.to_lowercase();
        let words: Vec<&str> = lowered
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty() && w.len() > 1)
            .collect();

        // Count word frequencies
        let mut freq: HashMap<&str, f32> = HashMap::new();
        for word in &words {
            *freq.entry(word).or_insert(0.0) += 1.0;
        }

        let total = words.len() as f32;
        if total == 0.0 {
            return Ok(vector);
        }

        // Hash each word to vector dimensions and add TF weight
        for (word, count) in &freq {
            let tf = count / total;
            // Use multiple hash positions per word for better distribution
            let hash1 = simple_hash(word.as_bytes()) as usize;
            let hash2 = simple_hash(&[word.as_bytes(), &[1u8]].concat()) as usize;
            let hash3 = simple_hash(&[word.as_bytes(), &[2u8]].concat()) as usize;

            vector[hash1 % self.dimension] += tf;
            vector[hash2 % self.dimension] += tf * 0.7;
            vector[hash3 % self.dimension] += tf * 0.5;
        }

        // L2 normalize
        let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut vector {
                *v /= norm;
            }
        }

        Ok(vector)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

/// Simple deterministic hash function (FNV-1a).
fn simple_hash(data: &[u8]) -> u32 {
    let mut hash: u32 = 2166136261;
    for &byte in data {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    hash
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_local_embedding_dimension() {
        let emb = LocalEmbedding::new(128);
        assert_eq!(emb.dimension(), 128);
        let vec = emb.embed("hello world").await.unwrap();
        assert_eq!(vec.len(), 128);
    }

    #[tokio::test]
    async fn test_local_embedding_normalized() {
        let emb = LocalEmbedding::default();
        let vec = emb.embed("the quick brown fox jumps").await.unwrap();
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_local_embedding_similar_texts() {
        let emb = LocalEmbedding::default();
        let v1 = emb.embed("rust programming language").await.unwrap();
        let v2 = emb.embed("rust programming systems").await.unwrap();
        let v3 = emb.embed("cooking recipes for dinner").await.unwrap();

        let sim_12 = cosine_similarity(&v1, &v2);
        let sim_13 = cosine_similarity(&v1, &v3);

        // Similar texts should have higher similarity
        assert!(
            sim_12 > sim_13,
            "sim(rust-rust)={sim_12} should be > sim(rust-cooking)={sim_13}"
        );
    }

    #[tokio::test]
    async fn test_local_embedding_empty() {
        let emb = LocalEmbedding::default();
        assert!(emb.embed("").await.is_err());
    }

    #[tokio::test]
    async fn test_local_embedding_deterministic() {
        let emb = LocalEmbedding::default();
        let v1 = emb.embed("test input").await.unwrap();
        let v2 = emb.embed("test input").await.unwrap();
        assert_eq!(v1, v2);
    }

    #[tokio::test]
    async fn test_embed_batch() {
        let emb = LocalEmbedding::default();
        let vecs = emb.embed_batch(&["hello", "world"]).await.unwrap();
        assert_eq!(vecs.len(), 2);
        assert_eq!(vecs[0].len(), 256);
    }

    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if na == 0.0 || nb == 0.0 {
            0.0
        } else {
            dot / (na * nb)
        }
    }
}
