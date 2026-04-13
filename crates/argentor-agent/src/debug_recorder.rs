//! Debug recorder for step-by-step agent reasoning traces.
//!
//! Captures detailed execution traces for debugging agent behavior,
//! including decisions made, tools called, and reasoning steps.
//!
//! # Main types
//!
//! - [`DebugRecorder`] — Records execution steps.
//! - [`DebugStep`] — A single recorded step.
//! - [`DebugTrace`] — A complete execution trace.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

// ---------------------------------------------------------------------------
// StepType
// ---------------------------------------------------------------------------

/// Classification of a debug step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepType {
    /// Agent received input.
    Input,
    /// Agent is thinking/reasoning.
    Thinking,
    /// Agent decided on an action.
    Decision,
    /// Agent called a tool.
    ToolCall,
    /// Tool returned a result.
    ToolResult,
    /// LLM API call was made.
    LlmCall,
    /// LLM response received.
    LlmResponse,
    /// Cache hit (response served from cache).
    CacheHit,
    /// An error occurred.
    Error,
    /// Agent produced final output.
    Output,
    /// Custom step type.
    Custom(String),
}

// ---------------------------------------------------------------------------
// DebugStep
// ---------------------------------------------------------------------------

/// A single step in an execution trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugStep {
    /// Step sequence number.
    pub seq: u64,
    /// Type of step.
    pub step_type: StepType,
    /// Human-readable description of what happened.
    pub description: String,
    /// Detailed data (JSON).
    pub data: Option<serde_json::Value>,
    /// When this step occurred.
    pub timestamp: DateTime<Utc>,
    /// Duration of this step in milliseconds (if applicable).
    pub duration_ms: Option<u64>,
    /// Token usage for this step (if applicable).
    pub tokens: Option<TokenUsage>,
}

/// Token usage for a single step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Input/prompt tokens.
    pub input: u64,
    /// Output/completion tokens.
    pub output: u64,
}

// ---------------------------------------------------------------------------
// DebugTrace
// ---------------------------------------------------------------------------

/// A complete execution trace containing all recorded steps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugTrace {
    /// Trace identifier.
    pub trace_id: String,
    /// When the trace started.
    pub started_at: DateTime<Utc>,
    /// When the trace ended (if finished).
    pub ended_at: Option<DateTime<Utc>>,
    /// All recorded steps.
    pub steps: Vec<DebugStep>,
    /// Total duration in milliseconds.
    pub total_duration_ms: Option<u64>,
    /// Total tokens used.
    pub total_tokens: TokenUsage,
    /// Optional metadata.
    pub metadata: serde_json::Value,
}

impl DebugTrace {
    /// Get a summary of the trace.
    pub fn summary(&self) -> TraceSummary {
        let step_counts: std::collections::HashMap<String, usize> = {
            let mut map = std::collections::HashMap::new();
            for step in &self.steps {
                let key = serde_json::to_string(&step.step_type).unwrap_or_default();
                *map.entry(key).or_insert(0) += 1;
            }
            map
        };

        TraceSummary {
            trace_id: self.trace_id.clone(),
            total_steps: self.steps.len(),
            total_duration_ms: self.total_duration_ms.unwrap_or(0),
            total_tokens: self.total_tokens.input + self.total_tokens.output,
            step_counts,
            has_errors: self.steps.iter().any(|s| s.step_type == StepType::Error),
        }
    }
}

/// Summary of a debug trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSummary {
    /// Trace ID.
    pub trace_id: String,
    /// Total number of steps.
    pub total_steps: usize,
    /// Total duration in milliseconds.
    pub total_duration_ms: u64,
    /// Total tokens consumed.
    pub total_tokens: u64,
    /// Count of steps by type.
    pub step_counts: std::collections::HashMap<String, usize>,
    /// Whether any errors were recorded.
    pub has_errors: bool,
}

// ---------------------------------------------------------------------------
// DebugRecorder
// ---------------------------------------------------------------------------

/// Thread-safe recorder for execution debug traces.
///
/// Clone is cheap (inner state is behind `Arc<RwLock>`).
#[derive(Debug, Clone)]
pub struct DebugRecorder {
    trace_id: String,
    inner: Arc<RwLock<RecorderInner>>,
    enabled: bool,
    /// Maximum number of steps retained. 0 = unbounded (legacy behavior).
    /// When reached, oldest steps are dropped (ring buffer / FIFO).
    /// Closes #10 — prevents unbounded memory growth in long-running agents.
    max_steps: usize,
}

/// Default cap on retained steps — enough for a typical multi-turn conversation
/// with tool calls, small enough to bound memory on runaway agents.
pub const DEFAULT_MAX_STEPS: usize = 1000;

#[derive(Debug)]
struct RecorderInner {
    /// Deque for O(1) push_back and pop_front when cap reached.
    steps: std::collections::VecDeque<DebugStep>,
    /// Total steps ever recorded (including evicted). Used to report accurate counts.
    total_recorded: u64,
    next_seq: u64,
    started_at: DateTime<Utc>,
    total_input_tokens: u64,
    total_output_tokens: u64,
    metadata: serde_json::Value,
}

impl DebugRecorder {
    /// Create a new recorder with the default step cap ([`DEFAULT_MAX_STEPS`]).
    pub fn new(trace_id: impl Into<String>) -> Self {
        Self::with_capacity(trace_id, DEFAULT_MAX_STEPS)
    }

    /// Create a new recorder with a custom step cap.
    /// Pass `0` for unbounded (not recommended in production).
    pub fn with_capacity(trace_id: impl Into<String>, max_steps: usize) -> Self {
        Self {
            trace_id: trace_id.into(),
            inner: Arc::new(RwLock::new(RecorderInner {
                steps: std::collections::VecDeque::with_capacity(max_steps.min(1024)),
                total_recorded: 0,
                next_seq: 1,
                started_at: Utc::now(),
                total_input_tokens: 0,
                total_output_tokens: 0,
                metadata: serde_json::Value::Object(serde_json::Map::new()),
            })),
            enabled: true,
            max_steps,
        }
    }

    /// Create a disabled recorder (no-op, for production use).
    pub fn disabled() -> Self {
        Self {
            trace_id: String::new(),
            inner: Arc::new(RwLock::new(RecorderInner {
                steps: std::collections::VecDeque::new(),
                total_recorded: 0,
                next_seq: 1,
                started_at: Utc::now(),
                total_input_tokens: 0,
                total_output_tokens: 0,
                metadata: serde_json::Value::Null,
            })),
            enabled: false,
            max_steps: 0,
        }
    }

    /// Get the configured step cap (0 = unbounded).
    pub fn max_steps(&self) -> usize {
        self.max_steps
    }

    /// Get the total number of steps ever recorded (including evicted).
    pub fn total_recorded(&self) -> u64 {
        self.inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .total_recorded
    }

    /// Check how many steps have been evicted due to the cap.
    pub fn evicted_count(&self) -> u64 {
        let inner = self
            .inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner
            .total_recorded
            .saturating_sub(inner.steps.len() as u64)
    }

    /// Check if the recorder is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Record a step.
    pub fn record(
        &self,
        step_type: StepType,
        description: impl Into<String>,
        data: Option<serde_json::Value>,
    ) {
        if !self.enabled {
            return;
        }

        let mut inner = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let seq = inner.next_seq;
        inner.next_seq += 1;
        inner.total_recorded += 1;

        // Evict oldest if at capacity (ring buffer semantics)
        if self.max_steps > 0 && inner.steps.len() >= self.max_steps {
            inner.steps.pop_front();
        }

        inner.steps.push_back(DebugStep {
            seq,
            step_type,
            description: description.into(),
            data,
            timestamp: Utc::now(),
            duration_ms: None,
            tokens: None,
        });
    }

    /// Record a step with timing and token usage.
    pub fn record_with_metrics(
        &self,
        step_type: StepType,
        description: impl Into<String>,
        duration_ms: u64,
        input_tokens: u64,
        output_tokens: u64,
    ) {
        if !self.enabled {
            return;
        }

        let mut inner = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let seq = inner.next_seq;
        inner.next_seq += 1;
        inner.total_recorded += 1;
        inner.total_input_tokens += input_tokens;
        inner.total_output_tokens += output_tokens;

        // Evict oldest if at capacity (ring buffer semantics)
        if self.max_steps > 0 && inner.steps.len() >= self.max_steps {
            inner.steps.pop_front();
        }

        inner.steps.push_back(DebugStep {
            seq,
            step_type,
            description: description.into(),
            data: None,
            timestamp: Utc::now(),
            duration_ms: Some(duration_ms),
            tokens: Some(TokenUsage {
                input: input_tokens,
                output: output_tokens,
            }),
        });
    }

    /// Set metadata on the trace.
    pub fn set_metadata(&self, key: impl Into<String>, value: serde_json::Value) {
        if !self.enabled {
            return;
        }
        let mut inner = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(obj) = inner.metadata.as_object_mut() {
            obj.insert(key.into(), value);
        }
    }

    /// Get the number of recorded steps.
    pub fn step_count(&self) -> usize {
        self.inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .steps
            .len()
    }

    /// Finalize and return the complete debug trace.
    pub fn finalize(&self) -> DebugTrace {
        let inner = self
            .inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let now = Utc::now();
        let duration = (now - inner.started_at).num_milliseconds().unsigned_abs();

        DebugTrace {
            trace_id: self.trace_id.clone(),
            started_at: inner.started_at,
            ended_at: Some(now),
            steps: inner.steps.iter().cloned().collect(),
            total_duration_ms: Some(duration),
            total_tokens: TokenUsage {
                input: inner.total_input_tokens,
                output: inner.total_output_tokens,
            },
            metadata: inner.metadata.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // 1. New recorder
    #[test]
    fn test_new_recorder() {
        let r = DebugRecorder::new("trace-1");
        assert!(r.is_enabled());
        assert_eq!(r.step_count(), 0);
    }

    // 2. Disabled recorder
    #[test]
    fn test_disabled_recorder() {
        let r = DebugRecorder::disabled();
        assert!(!r.is_enabled());
        r.record(StepType::Input, "test", None);
        assert_eq!(r.step_count(), 0);
    }

    // 3. Record steps
    #[test]
    fn test_record_steps() {
        let r = DebugRecorder::new("t1");
        r.record(StepType::Input, "user message", None);
        r.record(StepType::Thinking, "analyzing...", None);
        r.record(StepType::Output, "response", None);
        assert_eq!(r.step_count(), 3);
    }

    // 4. Step sequence numbers
    #[test]
    fn test_step_sequence() {
        let r = DebugRecorder::new("t1");
        r.record(StepType::Input, "a", None);
        r.record(StepType::Output, "b", None);

        let trace = r.finalize();
        assert_eq!(trace.steps[0].seq, 1);
        assert_eq!(trace.steps[1].seq, 2);
    }

    // 5. Record with metrics
    #[test]
    fn test_record_with_metrics() {
        let r = DebugRecorder::new("t1");
        r.record_with_metrics(StepType::LlmCall, "claude call", 500, 1000, 200);

        let trace = r.finalize();
        assert_eq!(trace.steps[0].duration_ms, Some(500));
        assert_eq!(trace.steps[0].tokens.as_ref().unwrap().input, 1000);
        assert_eq!(trace.total_tokens.input, 1000);
        assert_eq!(trace.total_tokens.output, 200);
    }

    // 6. Multiple LLM calls accumulate tokens
    #[test]
    fn test_token_accumulation() {
        let r = DebugRecorder::new("t1");
        r.record_with_metrics(StepType::LlmCall, "call 1", 100, 500, 100);
        r.record_with_metrics(StepType::LlmCall, "call 2", 200, 300, 50);

        let trace = r.finalize();
        assert_eq!(trace.total_tokens.input, 800);
        assert_eq!(trace.total_tokens.output, 150);
    }

    // 7. Finalize produces trace
    #[test]
    fn test_finalize() {
        let r = DebugRecorder::new("trace-42");
        r.record(StepType::Input, "hello", None);

        let trace = r.finalize();
        assert_eq!(trace.trace_id, "trace-42");
        assert!(trace.ended_at.is_some());
        assert!(trace.total_duration_ms.is_some());
        assert_eq!(trace.steps.len(), 1);
    }

    // 8. Step with data
    #[test]
    fn test_step_with_data() {
        let r = DebugRecorder::new("t1");
        r.record(
            StepType::ToolCall,
            "calling file_write",
            Some(serde_json::json!({"path": "/tmp/test.txt"})),
        );

        let trace = r.finalize();
        assert_eq!(
            trace.steps[0].data.as_ref().unwrap()["path"],
            "/tmp/test.txt"
        );
    }

    // 9. Set metadata
    #[test]
    fn test_set_metadata() {
        let r = DebugRecorder::new("t1");
        r.set_metadata("model", serde_json::json!("claude-3"));
        r.set_metadata("session", serde_json::json!("abc-123"));

        let trace = r.finalize();
        assert_eq!(trace.metadata["model"], "claude-3");
        assert_eq!(trace.metadata["session"], "abc-123");
    }

    // 10. Trace summary
    #[test]
    fn test_trace_summary() {
        let r = DebugRecorder::new("t1");
        r.record(StepType::Input, "in", None);
        r.record(StepType::Thinking, "think", None);
        r.record(StepType::ToolCall, "tool", None);
        r.record(StepType::Error, "oops", None);
        r.record(StepType::Output, "out", None);

        let trace = r.finalize();
        let summary = trace.summary();
        assert_eq!(summary.total_steps, 5);
        assert!(summary.has_errors);
    }

    // 11. Summary without errors
    #[test]
    fn test_summary_no_errors() {
        let r = DebugRecorder::new("t1");
        r.record(StepType::Input, "in", None);
        r.record(StepType::Output, "out", None);

        let summary = r.finalize().summary();
        assert!(!summary.has_errors);
    }

    // 12. Step types
    #[test]
    fn test_step_types() {
        let r = DebugRecorder::new("t1");
        r.record(StepType::CacheHit, "cache", None);
        r.record(StepType::LlmResponse, "response", None);
        r.record(StepType::Decision, "decided", None);
        r.record(StepType::ToolResult, "result", None);
        r.record(StepType::Custom("custom".to_string()), "custom step", None);
        assert_eq!(r.step_count(), 5);
    }

    // 13. Clone shares state
    #[test]
    fn test_clone_shares_state() {
        let r1 = DebugRecorder::new("t1");
        let r2 = r1.clone();
        r1.record(StepType::Input, "from r1", None);
        assert_eq!(r2.step_count(), 1);
    }

    // 14. DebugTrace serializable
    #[test]
    fn test_trace_serializable() {
        let r = DebugRecorder::new("t1");
        r.record(StepType::Input, "hello", None);
        let trace = r.finalize();

        let json = serde_json::to_string(&trace).unwrap();
        assert!(json.contains("\"trace_id\":\"t1\""));

        let restored: DebugTrace = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.trace_id, "t1");
    }

    // 15. TraceSummary serializable
    #[test]
    fn test_summary_serializable() {
        let r = DebugRecorder::new("t1");
        r.record(StepType::Input, "x", None);
        let summary = r.finalize().summary();

        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("\"total_steps\":1"));
    }

    // 16. Disabled recorder finalize
    #[test]
    fn test_disabled_finalize() {
        let r = DebugRecorder::disabled();
        r.record(StepType::Input, "ignored", None);
        let trace = r.finalize();
        assert!(trace.steps.is_empty());
    }

    // 17. Disabled recorder metadata
    #[test]
    fn test_disabled_metadata() {
        let r = DebugRecorder::disabled();
        r.set_metadata("key", serde_json::json!("value"));
        // Should not crash, just no-op
    }

    // 18. DebugStep serializable
    #[test]
    fn test_step_serializable() {
        let step = DebugStep {
            seq: 1,
            step_type: StepType::Input,
            description: "test".to_string(),
            data: None,
            timestamp: Utc::now(),
            duration_ms: Some(42),
            tokens: None,
        };
        let json = serde_json::to_string(&step).unwrap();
        assert!(json.contains("\"step_type\":\"input\""));
    }

    // 19. TokenUsage serializable
    #[test]
    fn test_token_usage_serializable() {
        let usage = TokenUsage {
            input: 100,
            output: 50,
        };
        let json = serde_json::to_string(&usage).unwrap();
        assert!(json.contains("\"input\":100"));
    }

    // 20. Multiple finalizations produce same data
    #[test]
    fn test_multiple_finalize() {
        let r = DebugRecorder::new("t1");
        r.record(StepType::Input, "x", None);
        let t1 = r.finalize();
        let t2 = r.finalize();
        assert_eq!(t1.steps.len(), t2.steps.len());
    }
}
