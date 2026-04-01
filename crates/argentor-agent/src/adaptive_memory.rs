//! Adaptive memory integration for the agent loop.
//!
//! Provides automatic storage and retrieval of relevant context from past
//! interactions. The agent learns facts, user preferences, tool usage patterns,
//! conversation summaries, and error resolutions — then recalls them when relevant.
//!
//! # Architecture
//!
//! ```text
//! ┌────────────────┐     ┌──────────────────┐     ┌────────────────┐
//! │  Agent Loop    │ --> │ AdaptiveMemory   │ --> │  Recall Result │
//! │  (user query)  │     │ (store + recall)  │     │  (context)     │
//! └────────────────┘     └──────────────────┘     └────────────────┘
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Types of memory entries.
///
/// Classifies memories so the system can prioritize and filter by category.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemoryKind {
    /// A fact learned during conversation.
    Fact,
    /// A user preference or pattern.
    Preference,
    /// A tool usage pattern (which tools work for which tasks).
    ToolPattern,
    /// A conversation summary.
    Summary,
    /// An error and how it was resolved.
    ErrorResolution,
}

/// A single memory entry.
///
/// Captures a piece of knowledge with metadata for relevance scoring:
/// keywords for lookup, importance for prioritization, and access tracking
/// for recency-based decay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Unique identifier for this memory.
    pub id: String,
    /// Category of this memory.
    pub kind: MemoryKind,
    /// The textual content of the memory.
    pub content: String,
    /// Keywords extracted from the content for fast lookup.
    pub keywords: Vec<String>,
    /// Base importance score (0.0 to 1.0).
    pub importance: f32,
    /// Number of times this memory has been recalled.
    pub access_count: u32,
    /// When this memory was first created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When this memory was last accessed/recalled.
    pub last_accessed: chrono::DateTime<chrono::Utc>,
    /// Arbitrary metadata (e.g., tool_name, success flag).
    pub metadata: HashMap<String, String>,
}

impl MemoryEntry {
    /// Create a new memory entry with auto-generated ID and extracted keywords.
    pub fn new(kind: MemoryKind, content: &str, importance: f32) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            kind,
            content: content.to_string(),
            keywords: Self::extract_keywords(content),
            importance: importance.clamp(0.0, 1.0),
            access_count: 0,
            created_at: now,
            last_accessed: now,
            metadata: HashMap::new(),
        }
    }

    /// Extract keywords from content (simple tokenization).
    ///
    /// Splits on non-alphanumeric characters, lowercases, filters words
    /// shorter than 4 characters, and deduplicates.
    pub fn extract_keywords(content: &str) -> Vec<String> {
        content
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 3)
            .map(String::from)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect()
    }

    /// Calculate a relevance-weighted importance score.
    ///
    /// Memories that are accessed more often and more recently score higher.
    /// The recency boost decays over days, and access frequency provides a
    /// logarithmic boost.
    pub fn effective_importance(&self) -> f32 {
        let recency_boost = {
            let age_hours = (chrono::Utc::now() - self.last_accessed).num_hours() as f32;
            1.0 / (1.0 + age_hours / 24.0)
        };
        let access_boost = (self.access_count as f32).ln_1p() / 10.0;
        (self.importance + recency_boost * 0.3 + access_boost).min(1.0)
    }
}

/// Configuration for the adaptive memory system.
///
/// Controls recall limits, relevance thresholds, auto-extraction behavior,
/// capacity limits, and tool-pattern tracking.
#[derive(Debug, Clone)]
pub struct AdaptiveMemoryConfig {
    /// Maximum number of memories to retrieve per query.
    pub max_recall: usize,
    /// Minimum relevance score to include a memory in recall results.
    pub min_relevance: f32,
    /// Whether to auto-extract facts from conversations.
    pub auto_extract: bool,
    /// Maximum total memories to store (oldest/least important are pruned).
    pub max_memories: usize,
    /// Whether to track tool usage patterns.
    pub track_tool_patterns: bool,
}

impl Default for AdaptiveMemoryConfig {
    fn default() -> Self {
        Self {
            max_recall: 5,
            min_relevance: 0.3,
            auto_extract: true,
            max_memories: 1000,
            track_tool_patterns: true,
        }
    }
}

/// Result of a memory recall operation.
///
/// Contains the matching memories with their relevance scores and a
/// pre-formatted context string suitable for prompt injection.
#[derive(Debug, Clone)]
pub struct RecallResult {
    /// Matching memories paired with their relevance scores.
    pub memories: Vec<(MemoryEntry, f32)>,
    /// Formatted text ready to inject into the agent prompt.
    pub context_text: String,
}

/// The adaptive memory manager.
///
/// Stores, indexes, recalls, and prunes memories using keyword-based
/// matching combined with importance scoring. Designed to integrate
/// directly into the agent loop so that relevant past context is
/// automatically available for each new query.
pub struct AdaptiveMemory {
    /// Configuration parameters.
    config: AdaptiveMemoryConfig,
    /// All stored memory entries.
    entries: Vec<MemoryEntry>,
    /// Index from keyword to memory indices for fast lookup.
    keyword_index: HashMap<String, Vec<usize>>,
}

impl AdaptiveMemory {
    /// Create a new adaptive memory manager with the given configuration.
    pub fn new(config: AdaptiveMemoryConfig) -> Self {
        Self {
            config,
            entries: Vec::new(),
            keyword_index: HashMap::new(),
        }
    }

    /// Create a new adaptive memory manager with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(AdaptiveMemoryConfig::default())
    }

    /// Store a new memory entry.
    ///
    /// Adds the entry to the internal store, updates the keyword index,
    /// and prunes if capacity is exceeded.
    pub fn store(&mut self, entry: MemoryEntry) {
        let idx = self.entries.len();
        for kw in &entry.keywords {
            self.keyword_index.entry(kw.clone()).or_default().push(idx);
        }
        self.entries.push(entry);
        self.prune();
    }

    /// Store a fact extracted from conversation.
    pub fn store_fact(&mut self, content: &str, importance: f32) {
        let entry = MemoryEntry::new(MemoryKind::Fact, content, importance);
        self.store(entry);
    }

    /// Store a tool usage pattern.
    ///
    /// Records whether a tool succeeded or failed for a given task.
    /// Failures are stored with higher importance since they are more
    /// valuable to remember.
    pub fn store_tool_pattern(&mut self, task_description: &str, tool_name: &str, success: bool) {
        let content = if success {
            format!("Tool '{tool_name}' worked well for: {task_description}")
        } else {
            format!("Tool '{tool_name}' failed for: {task_description}")
        };
        let importance = if success { 0.6 } else { 0.8 };
        let mut entry = MemoryEntry::new(MemoryKind::ToolPattern, &content, importance);
        entry
            .metadata
            .insert("tool_name".to_string(), tool_name.to_string());
        entry
            .metadata
            .insert("success".to_string(), success.to_string());
        self.store(entry);
    }

    /// Store a conversation summary.
    pub fn store_summary(&mut self, summary: &str) {
        self.store(MemoryEntry::new(MemoryKind::Summary, summary, 0.5));
    }

    /// Recall relevant memories for a given query.
    ///
    /// Scores each memory by keyword overlap with the query (70% weight)
    /// combined with effective importance (30% weight). Returns the top
    /// matches above the minimum relevance threshold, and updates their
    /// access counts.
    pub fn recall(&mut self, query: &str) -> RecallResult {
        let query_keywords = MemoryEntry::extract_keywords(query);

        let mut scored: Vec<(usize, f32)> = self
            .entries
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                let keyword_score = keyword_overlap(&query_keywords, &entry.keywords);
                let importance_score = entry.effective_importance();
                let score = keyword_score * 0.7 + importance_score * 0.3;
                (idx, score)
            })
            .filter(|(_, score)| *score >= self.config.min_relevance)
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(self.config.max_recall);

        // Update access counts and last-accessed timestamps
        for (idx, _) in &scored {
            self.entries[*idx].access_count += 1;
            self.entries[*idx].last_accessed = chrono::Utc::now();
        }

        let memories: Vec<(MemoryEntry, f32)> = scored
            .iter()
            .map(|(idx, score)| (self.entries[*idx].clone(), *score))
            .collect();

        let context_text = format_recall_context(&memories);

        RecallResult {
            memories,
            context_text,
        }
    }

    /// Extract potential facts from a conversation turn.
    ///
    /// Uses heuristics to identify factual statements in the agent response:
    /// sentences of moderate length containing factual indicator verbs.
    pub fn extract_facts(&self, _user_message: &str, agent_response: &str) -> Vec<MemoryEntry> {
        let mut facts = Vec::new();

        let sentences: Vec<&str> = agent_response.split('.').collect();
        for sentence in sentences {
            let trimmed = sentence.trim();
            if trimmed.len() > 20 && trimmed.len() < 200 && contains_factual_indicator(trimmed) {
                facts.push(MemoryEntry::new(MemoryKind::Fact, trimmed, 0.5));
            }
        }

        facts
    }

    /// Prune old/unimportant memories when over capacity.
    ///
    /// Sorts by effective importance ascending and removes the least
    /// important entries until the count is within `max_memories`.
    /// Rebuilds the keyword index after pruning.
    pub fn prune(&mut self) {
        if self.entries.len() <= self.config.max_memories {
            return;
        }

        self.entries.sort_by(|a, b| {
            a.effective_importance()
                .partial_cmp(&b.effective_importance())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let to_remove = self.entries.len() - self.config.max_memories;
        self.entries.drain(..to_remove);
        self.rebuild_keyword_index();
    }

    /// Get all stored memories.
    pub fn all_memories(&self) -> &[MemoryEntry] {
        &self.entries
    }

    /// Get the number of stored memories.
    pub fn memory_count(&self) -> usize {
        self.entries.len()
    }

    /// Get memories filtered by kind.
    pub fn memories_by_kind(&self, kind: &MemoryKind) -> Vec<&MemoryEntry> {
        self.entries.iter().filter(|e| &e.kind == kind).collect()
    }

    /// Rebuild the keyword index from scratch.
    fn rebuild_keyword_index(&mut self) {
        self.keyword_index.clear();
        for (idx, entry) in self.entries.iter().enumerate() {
            for kw in &entry.keywords {
                self.keyword_index.entry(kw.clone()).or_default().push(idx);
            }
        }
    }
}

/// Compute keyword overlap between a query's keywords and an entry's keywords.
///
/// Returns the fraction of query keywords that appear in the entry keywords.
fn keyword_overlap(query_kw: &[String], entry_kw: &[String]) -> f32 {
    if query_kw.is_empty() || entry_kw.is_empty() {
        return 0.0;
    }
    let matches = query_kw
        .iter()
        .filter(|q| entry_kw.iter().any(|e| e == *q))
        .count();
    matches as f32 / query_kw.len().max(1) as f32
}

/// Format recalled memories into a context string for prompt injection.
fn format_recall_context(memories: &[(MemoryEntry, f32)]) -> String {
    if memories.is_empty() {
        return String::new();
    }
    let mut ctx = String::from("[Relevant context from memory]\n");
    for (entry, score) in memories {
        ctx.push_str(&format!(
            "- ({:.0}% relevant) {}\n",
            score * 100.0,
            entry.content
        ));
    }
    ctx
}

/// Check if a sentence contains factual indicators.
///
/// Looks for common verbs that signal a factual statement.
fn contains_factual_indicator(text: &str) -> bool {
    let lower = text.to_lowercase();
    let indicators = [
        "is ",
        "are ",
        "was ",
        "were ",
        "has ",
        "have ",
        "contains ",
        "returns ",
        "produces ",
        "costs ",
        "takes ",
        "requires ",
    ];
    indicators.iter().any(|ind| lower.contains(ind))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_entry_new() {
        let entry = MemoryEntry::new(MemoryKind::Fact, "Rust is a systems language", 0.7);
        assert_eq!(entry.kind, MemoryKind::Fact);
        assert_eq!(entry.content, "Rust is a systems language");
        assert!((entry.importance - 0.7).abs() < f32::EPSILON);
        assert_eq!(entry.access_count, 0);
        assert!(!entry.id.is_empty());
        assert!(!entry.keywords.is_empty());
        assert!(entry.metadata.is_empty());
    }

    #[test]
    fn test_memory_entry_new_clamps_importance() {
        let high = MemoryEntry::new(MemoryKind::Fact, "test content here", 1.5);
        assert!((high.importance - 1.0).abs() < f32::EPSILON);

        let low = MemoryEntry::new(MemoryKind::Fact, "test content here", -0.5);
        assert!(low.importance.abs() < f32::EPSILON);
    }

    #[test]
    fn test_memory_entry_extract_keywords() {
        let keywords = MemoryEntry::extract_keywords("Rust is a fast systems language!");
        // Words with len > 3: "rust", "fast", "systems", "language"
        assert!(keywords.contains(&"rust".to_string()));
        assert!(keywords.contains(&"fast".to_string()));
        assert!(keywords.contains(&"systems".to_string()));
        assert!(keywords.contains(&"language".to_string()));
        // Short words filtered out
        assert!(!keywords.contains(&"is".to_string()));
        assert!(!keywords.contains(&"a".to_string()));
    }

    #[test]
    fn test_memory_entry_extract_keywords_empty() {
        let keywords = MemoryEntry::extract_keywords("");
        assert!(keywords.is_empty());
    }

    #[test]
    fn test_memory_entry_extract_keywords_deduplicates() {
        let keywords = MemoryEntry::extract_keywords("rust rust rust language language");
        let rust_count = keywords.iter().filter(|k| *k == "rust").count();
        assert_eq!(rust_count, 1);
    }

    #[test]
    fn test_memory_entry_effective_importance() {
        let entry = MemoryEntry::new(MemoryKind::Fact, "something important here", 0.5);
        let score = entry.effective_importance();
        // Should be >= base importance (recency boost is positive for recent entries)
        assert!(score >= 0.5);
        assert!(score <= 1.0);
    }

    #[test]
    fn test_memory_entry_effective_importance_with_access() {
        let mut entry = MemoryEntry::new(MemoryKind::Fact, "accessed many times here", 0.3);
        entry.access_count = 100;
        let score = entry.effective_importance();
        // Access boost should increase the score
        assert!(score > 0.3);
    }

    #[test]
    fn test_memory_kind_equality() {
        assert_eq!(MemoryKind::Fact, MemoryKind::Fact);
        assert_eq!(MemoryKind::Preference, MemoryKind::Preference);
        assert_eq!(MemoryKind::ToolPattern, MemoryKind::ToolPattern);
        assert_eq!(MemoryKind::Summary, MemoryKind::Summary);
        assert_eq!(MemoryKind::ErrorResolution, MemoryKind::ErrorResolution);
        assert_ne!(MemoryKind::Fact, MemoryKind::Preference);
        assert_ne!(MemoryKind::ToolPattern, MemoryKind::Summary);
    }

    #[test]
    fn test_memory_kind_serde_roundtrip() {
        let kind = MemoryKind::ToolPattern;
        let json = serde_json::to_string(&kind).unwrap();
        let parsed: MemoryKind = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, kind);
    }

    #[test]
    fn test_adaptive_memory_config_defaults() {
        let config = AdaptiveMemoryConfig::default();
        assert_eq!(config.max_recall, 5);
        assert!((config.min_relevance - 0.3).abs() < f32::EPSILON);
        assert!(config.auto_extract);
        assert_eq!(config.max_memories, 1000);
        assert!(config.track_tool_patterns);
    }

    #[test]
    fn test_store_fact() {
        let mut mem = AdaptiveMemory::with_defaults();
        mem.store_fact("The database runs on port 5432", 0.8);
        assert_eq!(mem.memory_count(), 1);
        let entries = mem.all_memories();
        assert_eq!(entries[0].kind, MemoryKind::Fact);
        assert!(entries[0].content.contains("5432"));
    }

    #[test]
    fn test_store_tool_pattern_success() {
        let mut mem = AdaptiveMemory::with_defaults();
        mem.store_tool_pattern("reading files", "file_read", true);
        assert_eq!(mem.memory_count(), 1);
        let entry = &mem.all_memories()[0];
        assert_eq!(entry.kind, MemoryKind::ToolPattern);
        assert!(entry.content.contains("worked well"));
        assert!(entry.content.contains("file_read"));
        assert_eq!(entry.metadata.get("tool_name").unwrap(), "file_read");
        assert_eq!(entry.metadata.get("success").unwrap(), "true");
        assert!((entry.importance - 0.6).abs() < f32::EPSILON);
    }

    #[test]
    fn test_store_tool_pattern_failure() {
        let mut mem = AdaptiveMemory::with_defaults();
        mem.store_tool_pattern("parsing JSON", "json_parse", false);
        assert_eq!(mem.memory_count(), 1);
        let entry = &mem.all_memories()[0];
        assert!(entry.content.contains("failed"));
        assert!(entry.content.contains("json_parse"));
        assert_eq!(entry.metadata.get("success").unwrap(), "false");
        // Failures have higher importance
        assert!((entry.importance - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn test_store_summary() {
        let mut mem = AdaptiveMemory::with_defaults();
        mem.store_summary("User asked about Rust error handling patterns");
        assert_eq!(mem.memory_count(), 1);
        let entry = &mem.all_memories()[0];
        assert_eq!(entry.kind, MemoryKind::Summary);
        assert!((entry.importance - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_recall_by_keywords() {
        let mut mem = AdaptiveMemory::new(AdaptiveMemoryConfig {
            min_relevance: 0.0,
            ..Default::default()
        });
        mem.store_fact("Rust compiler produces fast binaries", 0.7);
        mem.store_fact("Python is great for scripting tasks", 0.5);

        let result = mem.recall("Rust compiler optimization");
        // The Rust entry should match better
        assert!(!result.memories.is_empty());
        let top = &result.memories[0];
        assert!(top.0.content.contains("Rust"));
    }

    #[test]
    fn test_recall_updates_access_count() {
        let mut mem = AdaptiveMemory::new(AdaptiveMemoryConfig {
            min_relevance: 0.0,
            ..Default::default()
        });
        mem.store_fact("Database connection pooling is important", 0.7);

        // Before recall
        assert_eq!(mem.all_memories()[0].access_count, 0);

        let result = mem.recall("database connection pooling configuration");
        assert!(!result.memories.is_empty());

        // After recall, the access count should be incremented
        assert_eq!(mem.all_memories()[0].access_count, 1);
    }

    #[test]
    fn test_recall_respects_max_recall() {
        let mut mem = AdaptiveMemory::new(AdaptiveMemoryConfig {
            max_recall: 2,
            min_relevance: 0.0,
            ..Default::default()
        });
        for i in 0..10 {
            mem.store_fact(&format!("Fact number {i} about memory systems"), 0.5);
        }

        let result = mem.recall("fact about memory systems");
        assert!(result.memories.len() <= 2);
    }

    #[test]
    fn test_recall_respects_min_relevance() {
        let mut mem = AdaptiveMemory::new(AdaptiveMemoryConfig {
            min_relevance: 0.99,
            ..Default::default()
        });
        mem.store_fact("Completely unrelated topic about cooking", 0.1);

        let result = mem.recall("Rust programming language");
        assert!(result.memories.is_empty());
    }

    #[test]
    fn test_recall_empty_memory() {
        let mut mem = AdaptiveMemory::with_defaults();
        let result = mem.recall("anything at all");
        assert!(result.memories.is_empty());
        assert!(result.context_text.is_empty());
    }

    #[test]
    fn test_prune_removes_least_important() {
        let mut mem = AdaptiveMemory::new(AdaptiveMemoryConfig {
            max_memories: 3,
            ..Default::default()
        });

        // Store 5 entries; only 3 should survive
        mem.store(MemoryEntry::new(
            MemoryKind::Fact,
            "low importance entry one",
            0.1,
        ));
        mem.store(MemoryEntry::new(
            MemoryKind::Fact,
            "low importance entry two",
            0.1,
        ));
        mem.store(MemoryEntry::new(
            MemoryKind::Fact,
            "high importance entry here",
            0.9,
        ));
        mem.store(MemoryEntry::new(
            MemoryKind::Fact,
            "medium importance entry data",
            0.5,
        ));
        mem.store(MemoryEntry::new(
            MemoryKind::Fact,
            "very high importance entry value",
            0.95,
        ));

        assert!(mem.memory_count() <= 3);
        // The high-importance entries should survive
        let contents: Vec<&str> = mem
            .all_memories()
            .iter()
            .map(|e| e.content.as_str())
            .collect();
        assert!(contents.iter().any(|c| c.contains("very high")));
    }

    #[test]
    fn test_prune_under_capacity_noop() {
        let mut mem = AdaptiveMemory::new(AdaptiveMemoryConfig {
            max_memories: 100,
            ..Default::default()
        });
        mem.store_fact("Just one fact here today", 0.5);
        mem.store_fact("And another fact about something", 0.6);

        assert_eq!(mem.memory_count(), 2);
        mem.prune();
        assert_eq!(mem.memory_count(), 2);
    }

    #[test]
    fn test_memories_by_kind() {
        let mut mem = AdaptiveMemory::with_defaults();
        mem.store_fact("A fact about something useful", 0.5);
        mem.store_summary("A summary of the conversation today");
        mem.store_fact("Another fact about something else", 0.6);
        mem.store_tool_pattern("searching logs", "grep_tool", true);

        let facts = mem.memories_by_kind(&MemoryKind::Fact);
        assert_eq!(facts.len(), 2);

        let summaries = mem.memories_by_kind(&MemoryKind::Summary);
        assert_eq!(summaries.len(), 1);

        let patterns = mem.memories_by_kind(&MemoryKind::ToolPattern);
        assert_eq!(patterns.len(), 1);

        let prefs = mem.memories_by_kind(&MemoryKind::Preference);
        assert!(prefs.is_empty());
    }

    #[test]
    fn test_memory_count() {
        let mut mem = AdaptiveMemory::with_defaults();
        assert_eq!(mem.memory_count(), 0);
        mem.store_fact("First fact about memory testing", 0.5);
        assert_eq!(mem.memory_count(), 1);
        mem.store_fact("Second fact about memory testing", 0.6);
        assert_eq!(mem.memory_count(), 2);
    }

    #[test]
    fn test_keyword_overlap() {
        let query = vec![
            "rust".to_string(),
            "compiler".to_string(),
            "fast".to_string(),
        ];
        let entry = vec![
            "rust".to_string(),
            "fast".to_string(),
            "language".to_string(),
        ];
        let score = keyword_overlap(&query, &entry);
        // 2 out of 3 query keywords match
        assert!((score - 2.0 / 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_keyword_overlap_no_match() {
        let query = vec!["rust".to_string()];
        let entry = vec!["python".to_string()];
        let score = keyword_overlap(&query, &entry);
        assert!(score.abs() < f32::EPSILON);
    }

    #[test]
    fn test_keyword_overlap_empty() {
        let empty: Vec<String> = vec![];
        let non_empty = vec!["rust".to_string()];
        assert!(keyword_overlap(&empty, &non_empty).abs() < f32::EPSILON);
        assert!(keyword_overlap(&non_empty, &empty).abs() < f32::EPSILON);
        assert!(keyword_overlap(&empty, &empty).abs() < f32::EPSILON);
    }

    #[test]
    fn test_format_recall_context() {
        let entry = MemoryEntry::new(MemoryKind::Fact, "Rust is fast and safe", 0.7);
        let memories = vec![(entry, 0.85)];
        let text = format_recall_context(&memories);
        assert!(text.contains("[Relevant context from memory]"));
        assert!(text.contains("85% relevant"));
        assert!(text.contains("Rust is fast and safe"));
    }

    #[test]
    fn test_format_recall_context_empty() {
        let text = format_recall_context(&[]);
        assert!(text.is_empty());
    }

    #[test]
    fn test_contains_factual_indicator() {
        assert!(contains_factual_indicator("Rust is a systems language"));
        assert!(contains_factual_indicator("The function returns a value"));
        assert!(contains_factual_indicator("This requires admin access"));
        assert!(contains_factual_indicator("The list contains three items"));
        assert!(!contains_factual_indicator("Hello world"));
        assert!(!contains_factual_indicator("Just a random phrase"));
    }

    #[test]
    fn test_extract_facts() {
        let mem = AdaptiveMemory::with_defaults();
        let response = "The Rust compiler is very fast. It produces optimized binaries. Ok.";
        let facts = mem.extract_facts("tell me about Rust", response);
        // "The Rust compiler is very fast" has "is " and len > 20 ... actually len=30, good
        // "It produces optimized binaries" has "produces " and len > 20
        // "Ok" is too short
        assert!(facts.len() >= 1);
        for fact in &facts {
            assert_eq!(fact.kind, MemoryKind::Fact);
        }
    }

    #[test]
    fn test_extract_facts_no_facts() {
        let mem = AdaptiveMemory::with_defaults();
        let response = "Hi. Ok. Sure.";
        let facts = mem.extract_facts("hello", response);
        assert!(facts.is_empty());
    }

    #[test]
    fn test_recall_context_text_populated() {
        let mut mem = AdaptiveMemory::new(AdaptiveMemoryConfig {
            min_relevance: 0.0,
            ..Default::default()
        });
        mem.store_fact("The server runs on port 8080 normally", 0.8);

        let result = mem.recall("server port configuration details");
        if !result.memories.is_empty() {
            assert!(!result.context_text.is_empty());
            assert!(result.context_text.contains("Relevant context from memory"));
        }
    }

    #[test]
    fn test_store_multiple_and_recall_best() {
        let mut mem = AdaptiveMemory::new(AdaptiveMemoryConfig {
            max_recall: 1,
            min_relevance: 0.0,
            ..Default::default()
        });
        mem.store_fact("Cooking recipes need fresh ingredients always", 0.3);
        mem.store_fact("Rust async runtime uses tokio library", 0.9);

        let result = mem.recall("Rust tokio async runtime performance");
        assert!(!result.memories.is_empty());
        assert!(result.memories[0].0.content.contains("tokio"));
    }

    #[test]
    fn test_memory_entry_serde_roundtrip() {
        let entry = MemoryEntry::new(MemoryKind::Preference, "User prefers dark mode theme", 0.6);
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: MemoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, entry.id);
        assert_eq!(parsed.kind, entry.kind);
        assert_eq!(parsed.content, entry.content);
        assert!((parsed.importance - entry.importance).abs() < f32::EPSILON);
    }
}
