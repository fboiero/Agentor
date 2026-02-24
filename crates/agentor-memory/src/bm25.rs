use std::collections::HashMap;
use uuid::Uuid;

/// BM25 parameters.
const K1: f32 = 1.2;
const B: f32 = 0.75;

/// Tokenize text into lowercase words, filtering tokens with length <= 1.
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|w| w.len() > 1)
        .collect()
}

/// A BM25 inverted index for keyword-based document retrieval.
///
/// Maintains an inverted index mapping terms to documents and their
/// term frequencies, along with document length statistics needed
/// for the BM25 scoring formula.
#[derive(Debug, Clone)]
pub struct Bm25Index {
    /// term -> (doc_id -> term_frequency)
    inverted_index: HashMap<String, HashMap<Uuid, f32>>,
    /// doc_id -> document length (word count)
    doc_lengths: HashMap<Uuid, f32>,
    /// Total number of documents in the index.
    doc_count: usize,
    /// Average document length across all indexed documents.
    avg_doc_length: f32,
}

impl Bm25Index {
    /// Create a new, empty BM25 index.
    pub fn new() -> Self {
        Self {
            inverted_index: HashMap::new(),
            doc_lengths: HashMap::new(),
            doc_count: 0,
            avg_doc_length: 0.0,
        }
    }

    /// Add a document to the index.
    ///
    /// Tokenizes the text, updates the inverted index with term frequencies,
    /// and recomputes average document length.
    pub fn add_document(&mut self, id: Uuid, text: &str) {
        let tokens = tokenize(text);
        let doc_len = tokens.len() as f32;

        // Count term frequencies for this document
        let mut term_freq: HashMap<String, f32> = HashMap::new();
        for token in &tokens {
            *term_freq.entry(token.clone()).or_insert(0.0) += 1.0;
        }

        // Update inverted index
        for (term, freq) in term_freq {
            self.inverted_index
                .entry(term)
                .or_default()
                .insert(id, freq);
        }

        // Update document length
        self.doc_lengths.insert(id, doc_len);
        self.doc_count += 1;

        // Recompute average document length
        self.recompute_avg_doc_length();
    }

    /// Remove a document from the index.
    ///
    /// Removes the document from all inverted index entries and from
    /// the document length table, then recomputes average document length.
    pub fn remove_document(&mut self, id: Uuid) {
        if self.doc_lengths.remove(&id).is_none() {
            return; // Document not in index
        }

        self.doc_count = self.doc_count.saturating_sub(1);

        // Remove from inverted index
        let mut empty_terms = Vec::new();
        for (term, postings) in &mut self.inverted_index {
            postings.remove(&id);
            if postings.is_empty() {
                empty_terms.push(term.clone());
            }
        }

        // Clean up empty term entries
        for term in empty_terms {
            self.inverted_index.remove(&term);
        }

        // Recompute average document length
        self.recompute_avg_doc_length();
    }

    /// Search the index for documents matching the query, returning up to
    /// `top_k` results sorted by descending BM25 score.
    ///
    /// Uses the standard BM25 scoring formula:
    /// ```text
    /// score = sum over query terms of:
    ///   IDF(t) * (tf * (k1 + 1)) / (tf + k1 * (1 - b + b * dl / avgdl))
    /// ```
    /// where:
    /// - `IDF(t) = ln((N - df + 0.5) / (df + 0.5) + 1.0)`
    /// - `tf` = term frequency of term t in the document
    /// - `dl` = document length
    /// - `avgdl` = average document length
    /// - `N` = total number of documents
    /// - `df` = number of documents containing term t
    pub fn search(&self, query: &str, top_k: usize) -> Vec<(Uuid, f32)> {
        if self.doc_count == 0 {
            return Vec::new();
        }

        let query_tokens = tokenize(query);
        if query_tokens.is_empty() {
            return Vec::new();
        }

        let n = self.doc_count as f32;
        let mut scores: HashMap<Uuid, f32> = HashMap::new();

        for token in &query_tokens {
            if let Some(postings) = self.inverted_index.get(token) {
                let df = postings.len() as f32;
                // IDF with Robertson's formula (always non-negative)
                let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();

                for (&doc_id, &tf) in postings {
                    let dl = self.doc_lengths.get(&doc_id).copied().unwrap_or(0.0);
                    let avgdl = if self.avg_doc_length > 0.0 {
                        self.avg_doc_length
                    } else {
                        1.0
                    };

                    let numerator = tf * (K1 + 1.0);
                    let denominator = tf + K1 * (1.0 - B + B * dl / avgdl);
                    let term_score = idf * numerator / denominator;

                    *scores.entry(doc_id).or_insert(0.0) += term_score;
                }
            }
        }

        // Sort by score descending and take top_k
        let mut results: Vec<(Uuid, f32)> = scores.into_iter().collect();
        results.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(top_k);

        results
    }

    /// Return the number of documents currently in the index.
    pub fn document_count(&self) -> usize {
        self.doc_count
    }

    /// Recompute the average document length from current doc_lengths.
    fn recompute_avg_doc_length(&mut self) {
        if self.doc_count == 0 {
            self.avg_doc_length = 0.0;
        } else {
            let total: f32 = self.doc_lengths.values().sum();
            self.avg_doc_length = total / self.doc_count as f32;
        }
    }
}

impl Default for Bm25Index {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_add_document_and_search_finds_it() {
        let mut index = Bm25Index::new();
        let id = Uuid::new_v4();
        index.add_document(id, "the quick brown fox jumps over the lazy dog");

        let results = index.search("quick brown fox", 10);
        assert!(!results.is_empty(), "search should return at least one result");
        assert_eq!(results[0].0, id, "the matching document should be returned");
        assert!(results[0].1 > 0.0, "score should be positive");
    }

    #[test]
    fn test_remove_document_removes_from_results() {
        let mut index = Bm25Index::new();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        index.add_document(id1, "rust programming language systems");
        index.add_document(id2, "python programming language scripting");

        // Both should appear for "programming"
        let results = index.search("programming", 10);
        assert_eq!(results.len(), 2);

        // Remove id1 and verify it no longer appears
        index.remove_document(id1);
        assert_eq!(index.document_count(), 1);

        let results = index.search("rust programming", 10);
        // Only id2 should remain (matching "programming")
        for (doc_id, _) in &results {
            assert_ne!(*doc_id, id1, "removed document should not appear in results");
        }
    }

    #[test]
    fn test_search_no_matches_returns_empty() {
        let mut index = Bm25Index::new();
        let id = Uuid::new_v4();
        index.add_document(id, "rust programming language");

        let results = index.search("cooking recipes dinner", 10);
        assert!(
            results.is_empty(),
            "search for non-matching terms should return empty"
        );
    }

    #[test]
    fn test_multiple_documents_ranked_correctly() {
        let mut index = Bm25Index::new();

        let id_rust = Uuid::new_v4();
        let id_python = Uuid::new_v4();
        let id_cooking = Uuid::new_v4();

        // id_rust has "rust" mentioned multiple times, should rank highest for "rust"
        index.add_document(
            id_rust,
            "rust is a systems programming language rust is fast rust is safe",
        );
        index.add_document(
            id_python,
            "python is a scripting programming language used for data science",
        );
        index.add_document(id_cooking, "cooking recipes for a delicious dinner meal");

        let results = index.search("rust programming", 10);

        // id_rust should rank first (has both "rust" and "programming")
        assert!(results.len() >= 2, "should return at least two results");
        assert_eq!(
            results[0].0, id_rust,
            "document with most relevant content should rank first"
        );

        // id_python should rank second (has "programming" but not "rust")
        assert_eq!(
            results[1].0, id_python,
            "document with partial match should rank second"
        );

        // Scores should be descending
        assert!(
            results[0].1 > results[1].1,
            "first result score {} should be > second result score {}",
            results[0].1,
            results[1].1
        );

        // cooking should not appear since it has neither term
        let cooking_present = results.iter().any(|(id, _)| *id == id_cooking);
        assert!(
            !cooking_present,
            "unrelated document should not appear in results"
        );
    }

    #[test]
    fn test_empty_index_search() {
        let index = Bm25Index::new();
        let results = index.search("anything", 10);
        assert!(results.is_empty());
        assert_eq!(index.document_count(), 0);
    }

    #[test]
    fn test_tokenize_basic() {
        let tokens = tokenize("Hello, World! This is a TEST.");
        // "a" is filtered out (len <= 1)
        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
        assert!(tokens.contains(&"this".to_string()));
        assert!(tokens.contains(&"is".to_string()));
        assert!(tokens.contains(&"test".to_string()));
        assert!(!tokens.contains(&"a".to_string()));
    }
}
