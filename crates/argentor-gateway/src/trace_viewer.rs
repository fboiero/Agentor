//! Trace visualization system for debugging agent execution.
//!
//! Provides a [`TraceStore`] that stores, queries, and analyzes
//! [`DebugTrace`](argentor_agent::debug_recorder::DebugTrace) objects, plus
//! REST endpoints mounted under `/api/v1/traces/`.
//!
//! # Endpoints
//!
//! - `GET  /api/v1/traces`                  — List traces with filter/pagination
//! - `GET  /api/v1/traces/{trace_id}`       — Full trace detail
//! - `GET  /api/v1/traces/{trace_id}/cost`  — Per-step cost breakdown
//! - `GET  /api/v1/traces/{trace_id}/timeline` — Visual timeline data
//! - `DELETE /api/v1/traces/{trace_id}`      — Delete a trace

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use argentor_agent::debug_recorder::{DebugTrace, StepType};

// ---------------------------------------------------------------------------
// Cost model
// ---------------------------------------------------------------------------

/// Cost per 1 000 input tokens (USD).
const INPUT_COST_PER_1K: f64 = 0.003;
/// Cost per 1 000 output tokens (USD).
const OUTPUT_COST_PER_1K: f64 = 0.015;

// ---------------------------------------------------------------------------
// Types — summaries and filters
// ---------------------------------------------------------------------------

/// Lightweight summary returned when listing traces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSummary {
    /// Trace identifier.
    pub trace_id: String,
    /// Agent role extracted from trace metadata.
    pub agent_role: Option<String>,
    /// Session identifier extracted from trace metadata.
    pub session_id: Option<String>,
    /// When the trace started.
    pub started_at: DateTime<Utc>,
    /// Total duration in milliseconds.
    pub duration_ms: u64,
    /// Total tokens (input + output).
    pub total_tokens: u64,
    /// Estimated total cost in USD.
    pub total_cost_usd: f64,
    /// Number of steps in the trace.
    pub step_count: usize,
    /// Whether any step recorded an error.
    pub has_errors: bool,
    /// Overall status: "completed", "failed", or "in_progress".
    pub status: String,
}

/// Query parameters for filtering traces.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TraceFilter {
    /// Filter by agent role.
    pub agent_role: Option<String>,
    /// Filter by session identifier.
    pub session_id: Option<String>,
    /// Filter by tenant identifier.
    pub tenant_id: Option<String>,
    /// Only include traces started at or after this time.
    pub from: Option<DateTime<Utc>>,
    /// Only include traces started at or before this time.
    pub to: Option<DateTime<Utc>>,
    /// If `Some(true)`, only traces with errors; `Some(false)`, only without.
    pub has_errors: Option<bool>,
    /// Minimum duration in milliseconds.
    pub min_duration_ms: Option<u64>,
    /// Maximum number of results to return (default 50).
    pub limit: Option<usize>,
    /// Number of results to skip (default 0).
    pub offset: Option<usize>,
}

impl TraceFilter {
    fn effective_limit(&self) -> usize {
        self.limit.unwrap_or(50)
    }

    fn effective_offset(&self) -> usize {
        self.offset.unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Types — cost analysis
// ---------------------------------------------------------------------------

/// Per-step cost breakdown for a single trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostBreakdown {
    /// Trace identifier.
    pub trace_id: String,
    /// Total estimated cost in USD.
    pub total_cost_usd: f64,
    /// Total tokens consumed.
    pub total_tokens: u64,
    /// Per-step cost detail.
    pub steps: Vec<StepCost>,
}

/// Cost information for a single step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepCost {
    /// Human-readable step name.
    pub step_name: String,
    /// Step type (serialised).
    pub step_type: String,
    /// Input tokens consumed.
    pub tokens_in: u64,
    /// Output tokens produced.
    pub tokens_out: u64,
    /// Estimated cost in USD.
    pub cost_usd: f64,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// Model used (from step data if available).
    pub model: Option<String>,
}

// ---------------------------------------------------------------------------
// Types — timeline
// ---------------------------------------------------------------------------

/// Timeline data for visualising trace execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceTimeline {
    /// Trace identifier.
    pub trace_id: String,
    /// Total duration in milliseconds.
    pub total_duration_ms: u64,
    /// Individual timeline lanes.
    pub lanes: Vec<TimelineLane>,
}

/// A single lane (bar) in the execution timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineLane {
    /// Step name / description.
    pub name: String,
    /// Offset from trace start in milliseconds.
    pub start_offset_ms: u64,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// Step type (serialised).
    pub step_type: String,
    /// Status of the lane: "ok" or "error".
    pub status: String,
}

// ---------------------------------------------------------------------------
// TraceStore
// ---------------------------------------------------------------------------

/// Thread-safe in-memory store for agent execution traces.
#[derive(Debug, Clone)]
pub struct TraceStore {
    inner: Arc<RwLock<TraceStoreInner>>,
}

#[derive(Debug)]
struct TraceStoreInner {
    traces: Vec<DebugTrace>,
    max_traces: usize,
}

impl TraceStore {
    /// Create a new store that retains at most `max_traces` entries.
    pub fn new(max_traces: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(TraceStoreInner {
                traces: Vec::new(),
                max_traces,
            })),
        }
    }

    /// Store a trace. If the store is full, the oldest trace is evicted.
    pub async fn store_trace(&self, trace: DebugTrace) {
        let mut inner = self.inner.write().await;
        if inner.traces.len() >= inner.max_traces {
            inner.traces.remove(0);
        }
        inner.traces.push(trace);
    }

    /// Retrieve a trace by its identifier.
    pub async fn get_trace(&self, trace_id: &str) -> Option<DebugTrace> {
        let inner = self.inner.read().await;
        inner
            .traces
            .iter()
            .find(|t| t.trace_id == trace_id)
            .cloned()
    }

    /// List traces matching a filter, with pagination.
    pub async fn list_traces(&self, filter: &TraceFilter) -> Vec<TraceSummary> {
        let inner = self.inner.read().await;
        inner
            .traces
            .iter()
            .filter(|t| matches_filter(t, filter))
            .map(|t| to_summary(t))
            .skip(filter.effective_offset())
            .take(filter.effective_limit())
            .collect()
    }

    /// Search traces by agent role, session, time range, or error status.
    pub async fn search_traces(&self, query: &TraceFilter) -> Vec<TraceSummary> {
        self.list_traces(query).await
    }

    /// Compute cost breakdown for a trace.
    pub async fn get_cost_breakdown(&self, trace_id: &str) -> Option<CostBreakdown> {
        let trace = self.get_trace(trace_id).await?;
        Some(compute_cost_breakdown(&trace))
    }

    /// Delete a trace by its identifier. Returns `true` if found and removed.
    pub async fn delete_trace(&self, trace_id: &str) -> bool {
        let mut inner = self.inner.write().await;
        let before = inner.traces.len();
        inner.traces.retain(|t| t.trace_id != trace_id);
        inner.traces.len() < before
    }

    /// Return the current number of stored traces.
    pub async fn len(&self) -> usize {
        self.inner.read().await.traces.len()
    }

    /// Whether the store is empty.
    pub async fn is_empty(&self) -> bool {
        self.inner.read().await.traces.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn extract_metadata_str(trace: &DebugTrace, key: &str) -> Option<String> {
    trace
        .metadata
        .get(key)
        .and_then(|v| v.as_str())
        .map(String::from)
}

fn compute_status(trace: &DebugTrace) -> String {
    if trace.steps.iter().any(|s| s.step_type == StepType::Error) {
        "failed".to_string()
    } else if trace.ended_at.is_some() {
        "completed".to_string()
    } else {
        "in_progress".to_string()
    }
}

fn compute_total_cost(trace: &DebugTrace) -> f64 {
    let input_cost = trace.total_tokens.input as f64 / 1000.0 * INPUT_COST_PER_1K;
    let output_cost = trace.total_tokens.output as f64 / 1000.0 * OUTPUT_COST_PER_1K;
    input_cost + output_cost
}

fn to_summary(trace: &DebugTrace) -> TraceSummary {
    TraceSummary {
        trace_id: trace.trace_id.clone(),
        agent_role: extract_metadata_str(trace, "agent_role"),
        session_id: extract_metadata_str(trace, "session_id"),
        started_at: trace.started_at,
        duration_ms: trace.total_duration_ms.unwrap_or(0),
        total_tokens: trace.total_tokens.input + trace.total_tokens.output,
        total_cost_usd: compute_total_cost(trace),
        step_count: trace.steps.len(),
        has_errors: trace.steps.iter().any(|s| s.step_type == StepType::Error),
        status: compute_status(trace),
    }
}

fn matches_filter(trace: &DebugTrace, filter: &TraceFilter) -> bool {
    if let Some(ref role) = filter.agent_role {
        let trace_role = extract_metadata_str(trace, "agent_role");
        if trace_role.as_deref() != Some(role.as_str()) {
            return false;
        }
    }
    if let Some(ref sid) = filter.session_id {
        let trace_sid = extract_metadata_str(trace, "session_id");
        if trace_sid.as_deref() != Some(sid.as_str()) {
            return false;
        }
    }
    if let Some(ref tid) = filter.tenant_id {
        let trace_tid = extract_metadata_str(trace, "tenant_id");
        if trace_tid.as_deref() != Some(tid.as_str()) {
            return false;
        }
    }
    if let Some(from) = filter.from {
        if trace.started_at < from {
            return false;
        }
    }
    if let Some(to) = filter.to {
        if trace.started_at > to {
            return false;
        }
    }
    if let Some(has_errors) = filter.has_errors {
        let trace_has = trace.steps.iter().any(|s| s.step_type == StepType::Error);
        if trace_has != has_errors {
            return false;
        }
    }
    if let Some(min_dur) = filter.min_duration_ms {
        let dur = trace.total_duration_ms.unwrap_or(0);
        if dur < min_dur {
            return false;
        }
    }
    true
}

fn step_type_str(st: &StepType) -> String {
    match st {
        StepType::Input => "input".to_string(),
        StepType::Thinking => "thinking".to_string(),
        StepType::Decision => "decision".to_string(),
        StepType::ToolCall => "tool_call".to_string(),
        StepType::ToolResult => "tool_result".to_string(),
        StepType::LlmCall => "llm_call".to_string(),
        StepType::LlmResponse => "llm_response".to_string(),
        StepType::CacheHit => "cache_hit".to_string(),
        StepType::Error => "error".to_string(),
        StepType::Output => "output".to_string(),
        StepType::Custom(s) => s.clone(),
    }
}

fn compute_cost_breakdown(trace: &DebugTrace) -> CostBreakdown {
    let mut steps = Vec::new();
    let mut total_cost = 0.0;
    let mut total_tokens = 0u64;

    for step in &trace.steps {
        let (tokens_in, tokens_out) = step
            .tokens
            .as_ref()
            .map(|t| (t.input, t.output))
            .unwrap_or((0, 0));

        let cost = tokens_in as f64 / 1000.0 * INPUT_COST_PER_1K
            + tokens_out as f64 / 1000.0 * OUTPUT_COST_PER_1K;

        total_cost += cost;
        total_tokens += tokens_in + tokens_out;

        let model = step
            .data
            .as_ref()
            .and_then(|d| d.get("model"))
            .and_then(|v| v.as_str())
            .map(String::from);

        steps.push(StepCost {
            step_name: step.description.clone(),
            step_type: step_type_str(&step.step_type),
            tokens_in,
            tokens_out,
            cost_usd: cost,
            duration_ms: step.duration_ms.unwrap_or(0),
            model,
        });
    }

    CostBreakdown {
        trace_id: trace.trace_id.clone(),
        total_cost_usd: total_cost,
        total_tokens,
        steps,
    }
}

fn compute_timeline(trace: &DebugTrace) -> TraceTimeline {
    let total_duration = trace.total_duration_ms.unwrap_or(0);
    let trace_start = trace.started_at;

    let lanes: Vec<TimelineLane> = trace
        .steps
        .iter()
        .map(|step| {
            let offset = (step.timestamp - trace_start)
                .num_milliseconds()
                .unsigned_abs();
            let duration = step.duration_ms.unwrap_or(0);
            let status = if step.step_type == StepType::Error {
                "error"
            } else {
                "ok"
            };
            TimelineLane {
                name: step.description.clone(),
                start_offset_ms: offset,
                duration_ms: duration,
                step_type: step_type_str(&step.step_type),
                status: status.to_string(),
            }
        })
        .collect();

    TraceTimeline {
        trace_id: trace.trace_id.clone(),
        total_duration_ms: total_duration,
        lanes,
    }
}

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

/// Shared state for the trace viewer endpoints.
#[derive(Debug, Clone)]
pub struct TraceViewerState {
    /// The underlying trace store.
    pub store: TraceStore,
}

// ---------------------------------------------------------------------------
// Axum REST handlers
// ---------------------------------------------------------------------------

/// Build the trace viewer router.
///
/// Mount under `/api/v1/traces` in your top-level router.
pub fn trace_viewer_router(state: TraceViewerState) -> Router {
    Router::new()
        .route("/api/v1/traces", get(list_traces_handler))
        .route(
            "/api/v1/traces/{trace_id}",
            get(get_trace_handler).delete(delete_trace_handler),
        )
        .route("/api/v1/traces/{trace_id}/cost", get(get_cost_handler))
        .route(
            "/api/v1/traces/{trace_id}/timeline",
            get(get_timeline_handler),
        )
        .with_state(state)
}

async fn list_traces_handler(
    State(state): State<TraceViewerState>,
    axum::extract::Query(params): axum::extract::Query<TraceFilter>,
) -> impl IntoResponse {
    let summaries = state.store.list_traces(&params).await;
    axum::Json(summaries).into_response()
}

async fn get_trace_handler(
    State(state): State<TraceViewerState>,
    Path(trace_id): Path<String>,
) -> impl IntoResponse {
    match state.store.get_trace(&trace_id).await {
        Some(trace) => axum::Json(trace).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn get_cost_handler(
    State(state): State<TraceViewerState>,
    Path(trace_id): Path<String>,
) -> impl IntoResponse {
    match state.store.get_cost_breakdown(&trace_id).await {
        Some(breakdown) => axum::Json(breakdown).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn get_timeline_handler(
    State(state): State<TraceViewerState>,
    Path(trace_id): Path<String>,
) -> impl IntoResponse {
    match state.store.get_trace(&trace_id).await {
        Some(trace) => {
            let timeline = compute_timeline(&trace);
            axum::Json(timeline).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn delete_trace_handler(
    State(state): State<TraceViewerState>,
    Path(trace_id): Path<String>,
) -> impl IntoResponse {
    if state.store.delete_trace(&trace_id).await {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use argentor_agent::debug_recorder::{DebugRecorder, StepType, TokenUsage};
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    /// Helper: create a minimal DebugTrace with optional metadata.
    fn make_trace(id: &str, agent_role: Option<&str>, session_id: Option<&str>) -> DebugTrace {
        let rec = DebugRecorder::new(id);
        if let Some(role) = agent_role {
            rec.set_metadata("agent_role", serde_json::json!(role));
        }
        if let Some(sid) = session_id {
            rec.set_metadata("session_id", serde_json::json!(sid));
        }
        rec.record(StepType::Input, "user input", None);
        rec.record_with_metrics(StepType::LlmCall, "call model", 100, 500, 100);
        rec.record(StepType::Output, "response", None);
        rec.finalize()
    }

    /// Helper: trace with errors.
    fn make_error_trace(id: &str) -> DebugTrace {
        let rec = DebugRecorder::new(id);
        rec.record(StepType::Input, "input", None);
        rec.record(StepType::Error, "something broke", None);
        rec.finalize()
    }

    /// Helper: empty trace (no steps).
    fn make_empty_trace(id: &str) -> DebugTrace {
        let rec = DebugRecorder::new(id);
        rec.finalize()
    }

    // --- TraceStore basic operations ---

    // 1. Store and retrieve
    #[tokio::test]
    async fn test_store_and_get() {
        let store = TraceStore::new(100);
        let trace = make_trace("t1", None, None);
        store.store_trace(trace).await;
        let got = store.get_trace("t1").await;
        assert!(got.is_some());
        assert_eq!(got.unwrap().trace_id, "t1");
    }

    // 2. Get non-existent trace
    #[tokio::test]
    async fn test_get_missing() {
        let store = TraceStore::new(100);
        assert!(store.get_trace("nope").await.is_none());
    }

    // 3. Delete trace
    #[tokio::test]
    async fn test_delete_trace() {
        let store = TraceStore::new(100);
        store.store_trace(make_trace("t1", None, None)).await;
        assert!(store.delete_trace("t1").await);
        assert!(store.get_trace("t1").await.is_none());
    }

    // 4. Delete non-existent
    #[tokio::test]
    async fn test_delete_missing() {
        let store = TraceStore::new(100);
        assert!(!store.delete_trace("nope").await);
    }

    // 5. Eviction when full
    #[tokio::test]
    async fn test_eviction() {
        let store = TraceStore::new(2);
        store.store_trace(make_trace("t1", None, None)).await;
        store.store_trace(make_trace("t2", None, None)).await;
        store.store_trace(make_trace("t3", None, None)).await;
        assert_eq!(store.len().await, 2);
        assert!(store.get_trace("t1").await.is_none());
        assert!(store.get_trace("t2").await.is_some());
        assert!(store.get_trace("t3").await.is_some());
    }

    // 6. List all traces
    #[tokio::test]
    async fn test_list_all() {
        let store = TraceStore::new(100);
        store.store_trace(make_trace("t1", None, None)).await;
        store.store_trace(make_trace("t2", None, None)).await;
        let list = store.list_traces(&TraceFilter::default()).await;
        assert_eq!(list.len(), 2);
    }

    // 7. List with agent_role filter
    #[tokio::test]
    async fn test_filter_by_agent_role() {
        let store = TraceStore::new(100);
        store
            .store_trace(make_trace("t1", Some("coder"), None))
            .await;
        store
            .store_trace(make_trace("t2", Some("reviewer"), None))
            .await;
        store
            .store_trace(make_trace("t3", Some("coder"), None))
            .await;
        let filter = TraceFilter {
            agent_role: Some("coder".to_string()),
            ..Default::default()
        };
        let list = store.list_traces(&filter).await;
        assert_eq!(list.len(), 2);
        assert!(list
            .iter()
            .all(|s| s.agent_role.as_deref() == Some("coder")));
    }

    // 8. List with session_id filter
    #[tokio::test]
    async fn test_filter_by_session() {
        let store = TraceStore::new(100);
        store.store_trace(make_trace("t1", None, Some("s1"))).await;
        store.store_trace(make_trace("t2", None, Some("s2"))).await;
        let filter = TraceFilter {
            session_id: Some("s1".to_string()),
            ..Default::default()
        };
        let list = store.list_traces(&filter).await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].trace_id, "t1");
    }

    // 9. Filter by has_errors = true
    #[tokio::test]
    async fn test_filter_has_errors_true() {
        let store = TraceStore::new(100);
        store.store_trace(make_trace("t1", None, None)).await;
        store.store_trace(make_error_trace("t2")).await;
        let filter = TraceFilter {
            has_errors: Some(true),
            ..Default::default()
        };
        let list = store.list_traces(&filter).await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].trace_id, "t2");
        assert!(list[0].has_errors);
    }

    // 10. Filter by has_errors = false
    #[tokio::test]
    async fn test_filter_has_errors_false() {
        let store = TraceStore::new(100);
        store.store_trace(make_trace("t1", None, None)).await;
        store.store_trace(make_error_trace("t2")).await;
        let filter = TraceFilter {
            has_errors: Some(false),
            ..Default::default()
        };
        let list = store.list_traces(&filter).await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].trace_id, "t1");
    }

    // 11. Pagination: limit
    #[tokio::test]
    async fn test_pagination_limit() {
        let store = TraceStore::new(100);
        for i in 0..10 {
            store
                .store_trace(make_trace(&format!("t{i}"), None, None))
                .await;
        }
        let filter = TraceFilter {
            limit: Some(3),
            ..Default::default()
        };
        let list = store.list_traces(&filter).await;
        assert_eq!(list.len(), 3);
    }

    // 12. Pagination: offset
    #[tokio::test]
    async fn test_pagination_offset() {
        let store = TraceStore::new(100);
        for i in 0..5 {
            store
                .store_trace(make_trace(&format!("t{i}"), None, None))
                .await;
        }
        let filter = TraceFilter {
            offset: Some(3),
            limit: Some(50),
            ..Default::default()
        };
        let list = store.list_traces(&filter).await;
        assert_eq!(list.len(), 2);
    }

    // 13. search_traces delegates to list_traces
    #[tokio::test]
    async fn test_search_traces() {
        let store = TraceStore::new(100);
        store
            .store_trace(make_trace("t1", Some("coder"), None))
            .await;
        store
            .store_trace(make_trace("t2", Some("reviewer"), None))
            .await;
        let filter = TraceFilter {
            agent_role: Some("reviewer".to_string()),
            ..Default::default()
        };
        let results = store.search_traces(&filter).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].trace_id, "t2");
    }

    // --- TraceSummary ---

    // 14. Summary fields
    #[tokio::test]
    async fn test_summary_fields() {
        let store = TraceStore::new(100);
        store
            .store_trace(make_trace("t1", Some("coder"), Some("s42")))
            .await;
        let list = store.list_traces(&TraceFilter::default()).await;
        let s = &list[0];
        assert_eq!(s.trace_id, "t1");
        assert_eq!(s.agent_role.as_deref(), Some("coder"));
        assert_eq!(s.session_id.as_deref(), Some("s42"));
        assert_eq!(s.step_count, 3);
        assert!(!s.has_errors);
        assert_eq!(s.status, "completed");
    }

    // 15. Error trace status
    #[tokio::test]
    async fn test_error_trace_status() {
        let store = TraceStore::new(100);
        store.store_trace(make_error_trace("err")).await;
        let list = store.list_traces(&TraceFilter::default()).await;
        assert_eq!(list[0].status, "failed");
    }

    // --- CostBreakdown ---

    // 16. Cost breakdown
    #[tokio::test]
    async fn test_cost_breakdown() {
        let store = TraceStore::new(100);
        store.store_trace(make_trace("t1", None, None)).await;
        let cb = store.get_cost_breakdown("t1").await.unwrap();
        assert_eq!(cb.trace_id, "t1");
        assert_eq!(cb.steps.len(), 3);
        // Only the LlmCall step has tokens
        let llm_step = cb.steps.iter().find(|s| s.step_type == "llm_call").unwrap();
        assert_eq!(llm_step.tokens_in, 500);
        assert_eq!(llm_step.tokens_out, 100);
        assert!(llm_step.cost_usd > 0.0);
        assert!(cb.total_cost_usd > 0.0);
    }

    // 17. Cost breakdown for missing trace
    #[tokio::test]
    async fn test_cost_breakdown_missing() {
        let store = TraceStore::new(100);
        assert!(store.get_cost_breakdown("nope").await.is_none());
    }

    // 18. Cost breakdown for empty trace
    #[tokio::test]
    async fn test_cost_breakdown_empty() {
        let store = TraceStore::new(100);
        store.store_trace(make_empty_trace("empty")).await;
        let cb = store.get_cost_breakdown("empty").await.unwrap();
        assert_eq!(cb.total_tokens, 0);
        assert!((cb.total_cost_usd - 0.0).abs() < f64::EPSILON);
    }

    // --- Timeline ---

    // 19. Timeline lanes
    #[tokio::test]
    async fn test_timeline() {
        let trace = make_trace("t1", None, None);
        let tl = compute_timeline(&trace);
        assert_eq!(tl.trace_id, "t1");
        assert_eq!(tl.lanes.len(), 3);
        assert_eq!(tl.lanes[0].step_type, "input");
        assert_eq!(tl.lanes[1].step_type, "llm_call");
        assert_eq!(tl.lanes[2].step_type, "output");
    }

    // 20. Timeline error lane
    #[tokio::test]
    async fn test_timeline_error_status() {
        let trace = make_error_trace("e1");
        let tl = compute_timeline(&trace);
        let error_lane = tl.lanes.iter().find(|l| l.step_type == "error").unwrap();
        assert_eq!(error_lane.status, "error");
    }

    // --- Store helpers ---

    // 21. len and is_empty
    #[tokio::test]
    async fn test_len_and_is_empty() {
        let store = TraceStore::new(100);
        assert!(store.is_empty().await);
        assert_eq!(store.len().await, 0);
        store.store_trace(make_trace("t1", None, None)).await;
        assert!(!store.is_empty().await);
        assert_eq!(store.len().await, 1);
    }

    // --- REST endpoint integration ---

    // 22. GET /api/v1/traces returns 200 with JSON array
    #[tokio::test]
    async fn test_rest_list_traces() {
        let state = TraceViewerState {
            store: TraceStore::new(100),
        };
        state.store.store_trace(make_trace("t1", None, None)).await;
        let app = trace_viewer_router(state);

        let req = Request::builder()
            .uri("/api/v1/traces")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1_000_000)
            .await
            .unwrap();
        let summaries: Vec<TraceSummary> = serde_json::from_slice(&body).unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].trace_id, "t1");
    }

    // 23. GET /api/v1/traces/{id} returns 200 for existing trace
    #[tokio::test]
    async fn test_rest_get_trace() {
        let state = TraceViewerState {
            store: TraceStore::new(100),
        };
        state.store.store_trace(make_trace("t1", None, None)).await;
        let app = trace_viewer_router(state);

        let req = Request::builder()
            .uri("/api/v1/traces/t1")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1_000_000)
            .await
            .unwrap();
        let trace: DebugTrace = serde_json::from_slice(&body).unwrap();
        assert_eq!(trace.trace_id, "t1");
    }

    // 24. GET /api/v1/traces/{id} returns 404 for missing
    #[tokio::test]
    async fn test_rest_get_trace_not_found() {
        let state = TraceViewerState {
            store: TraceStore::new(100),
        };
        let app = trace_viewer_router(state);

        let req = Request::builder()
            .uri("/api/v1/traces/nope")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // 25. GET /api/v1/traces/{id}/cost returns cost breakdown
    #[tokio::test]
    async fn test_rest_cost_endpoint() {
        let state = TraceViewerState {
            store: TraceStore::new(100),
        };
        state.store.store_trace(make_trace("t1", None, None)).await;
        let app = trace_viewer_router(state);

        let req = Request::builder()
            .uri("/api/v1/traces/t1/cost")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1_000_000)
            .await
            .unwrap();
        let cb: CostBreakdown = serde_json::from_slice(&body).unwrap();
        assert_eq!(cb.trace_id, "t1");
        assert!(!cb.steps.is_empty());
    }

    // 26. GET /api/v1/traces/{id}/timeline returns timeline
    #[tokio::test]
    async fn test_rest_timeline_endpoint() {
        let state = TraceViewerState {
            store: TraceStore::new(100),
        };
        state.store.store_trace(make_trace("t1", None, None)).await;
        let app = trace_viewer_router(state);

        let req = Request::builder()
            .uri("/api/v1/traces/t1/timeline")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1_000_000)
            .await
            .unwrap();
        let tl: TraceTimeline = serde_json::from_slice(&body).unwrap();
        assert_eq!(tl.trace_id, "t1");
        assert!(!tl.lanes.is_empty());
    }

    // 27. DELETE /api/v1/traces/{id} returns 204 for existing
    #[tokio::test]
    async fn test_rest_delete_trace() {
        let state = TraceViewerState {
            store: TraceStore::new(100),
        };
        state.store.store_trace(make_trace("t1", None, None)).await;
        let app = trace_viewer_router(state);

        let req = Request::builder()
            .method("DELETE")
            .uri("/api/v1/traces/t1")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    // 28. DELETE /api/v1/traces/{id} returns 404 for missing
    #[tokio::test]
    async fn test_rest_delete_not_found() {
        let state = TraceViewerState {
            store: TraceStore::new(100),
        };
        let app = trace_viewer_router(state);

        let req = Request::builder()
            .method("DELETE")
            .uri("/api/v1/traces/nope")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // --- Serialization ---

    // 29. TraceSummary serializable
    #[test]
    fn test_trace_summary_serializable() {
        let s = TraceSummary {
            trace_id: "t1".to_string(),
            agent_role: Some("coder".to_string()),
            session_id: None,
            started_at: Utc::now(),
            duration_ms: 42,
            total_tokens: 600,
            total_cost_usd: 0.003,
            step_count: 3,
            has_errors: false,
            status: "completed".to_string(),
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"trace_id\":\"t1\""));
        let back: TraceSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(back.trace_id, "t1");
    }

    // 30. CostBreakdown serializable
    #[test]
    fn test_cost_breakdown_serializable() {
        let cb = CostBreakdown {
            trace_id: "t1".to_string(),
            total_cost_usd: 0.01,
            total_tokens: 1000,
            steps: vec![StepCost {
                step_name: "call".to_string(),
                step_type: "llm_call".to_string(),
                tokens_in: 500,
                tokens_out: 500,
                cost_usd: 0.01,
                duration_ms: 200,
                model: Some("claude-3".to_string()),
            }],
        };
        let json = serde_json::to_string(&cb).unwrap();
        let back: CostBreakdown = serde_json::from_str(&json).unwrap();
        assert_eq!(back.steps.len(), 1);
    }

    // 31. TraceTimeline serializable
    #[test]
    fn test_timeline_serializable() {
        let tl = TraceTimeline {
            trace_id: "t1".to_string(),
            total_duration_ms: 500,
            lanes: vec![TimelineLane {
                name: "step1".to_string(),
                start_offset_ms: 0,
                duration_ms: 100,
                step_type: "input".to_string(),
                status: "ok".to_string(),
            }],
        };
        let json = serde_json::to_string(&tl).unwrap();
        let back: TraceTimeline = serde_json::from_str(&json).unwrap();
        assert_eq!(back.lanes.len(), 1);
    }

    // 32. TraceFilter default values
    #[test]
    fn test_filter_defaults() {
        let f = TraceFilter::default();
        assert_eq!(f.effective_limit(), 50);
        assert_eq!(f.effective_offset(), 0);
        assert!(f.agent_role.is_none());
        assert!(f.session_id.is_none());
        assert!(f.has_errors.is_none());
    }
}
