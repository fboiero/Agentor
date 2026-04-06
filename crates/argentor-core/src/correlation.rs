//! Distributed correlation context for request tracing across agent boundaries.
//!
//! Provides lightweight trace context propagation so that every operation
//! within a multi-agent pipeline can be correlated back to the originating
//! request. Compatible with W3C Trace Context headers.
//!
//! # Main types
//!
//! - [`CorrelationContext`] — Carries trace/span IDs and baggage across calls.
//! - [`CorrelationId`] — A unique request identifier.
//! - [`SpanContext`] — Represents a single span within a trace.
//! - [`ContextPropagator`] — Injects/extracts context from HTTP headers.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// CorrelationId
// ---------------------------------------------------------------------------

/// A unique identifier for correlating requests across agent boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CorrelationId(String);

impl CorrelationId {
    /// Create a new random correlation ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Create a correlation ID from an existing string.
    pub fn from_string(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Return the string representation.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for CorrelationId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ---------------------------------------------------------------------------
// SpanContext
// ---------------------------------------------------------------------------

/// Represents a single span in a distributed trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanContext {
    /// Unique identifier for this span.
    pub span_id: String,
    /// The parent span ID, if any.
    pub parent_span_id: Option<String>,
    /// Name describing what this span represents.
    pub operation: String,
    /// When the span started (Unix millis).
    pub start_time_ms: u64,
    /// Duration in milliseconds (set when span ends).
    pub duration_ms: Option<u64>,
    /// Key-value attributes attached to this span.
    pub attributes: HashMap<String, String>,
    /// Status of the span.
    pub status: SpanStatus,
}

/// Status of a completed span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanStatus {
    /// Span is still in progress.
    InProgress,
    /// Span completed successfully.
    Ok,
    /// Span completed with an error.
    Error,
}

impl SpanContext {
    /// Create a new span with the given operation name.
    pub fn new(operation: impl Into<String>) -> Self {
        Self {
            span_id: Uuid::new_v4().simple().to_string()[..16].to_string(),
            parent_span_id: None,
            operation: operation.into(),
            start_time_ms: current_time_ms(),
            duration_ms: None,
            attributes: HashMap::new(),
            status: SpanStatus::InProgress,
        }
    }

    /// Create a child span under this parent.
    pub fn child(&self, operation: impl Into<String>) -> Self {
        Self {
            span_id: Uuid::new_v4().simple().to_string()[..16].to_string(),
            parent_span_id: Some(self.span_id.clone()),
            operation: operation.into(),
            start_time_ms: current_time_ms(),
            duration_ms: None,
            attributes: HashMap::new(),
            status: SpanStatus::InProgress,
        }
    }

    /// Add an attribute to this span.
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    /// Mark the span as completed successfully.
    pub fn finish(&mut self) {
        let elapsed = current_time_ms().saturating_sub(self.start_time_ms);
        self.duration_ms = Some(elapsed);
        self.status = SpanStatus::Ok;
    }

    /// Mark the span as completed with an error.
    pub fn finish_with_error(&mut self, error: impl Into<String>) {
        let elapsed = current_time_ms().saturating_sub(self.start_time_ms);
        self.duration_ms = Some(elapsed);
        self.status = SpanStatus::Error;
        self.attributes.insert("error".to_string(), error.into());
    }
}

// ---------------------------------------------------------------------------
// CorrelationContext
// ---------------------------------------------------------------------------

/// Carries trace context and baggage items across agent boundaries.
///
/// This is the primary type for distributed tracing in Argentor. Pass it
/// through function calls and across agent messages to maintain trace
/// continuity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationContext {
    /// The trace ID shared by all spans in this trace.
    pub trace_id: String,
    /// The correlation ID for this request.
    pub correlation_id: CorrelationId,
    /// The current span context.
    pub current_span: SpanContext,
    /// Baggage items propagated across boundaries.
    pub baggage: HashMap<String, String>,
    /// Depth in the call chain (incremented on each child context).
    pub depth: u32,
}

impl CorrelationContext {
    /// Create a new root correlation context.
    pub fn new(operation: impl Into<String>) -> Self {
        let trace_id = Uuid::new_v4().simple().to_string();
        Self {
            trace_id,
            correlation_id: CorrelationId::new(),
            current_span: SpanContext::new(operation),
            baggage: HashMap::new(),
            depth: 0,
        }
    }

    /// Create a new context with an explicit correlation ID.
    pub fn with_correlation_id(mut self, id: CorrelationId) -> Self {
        self.correlation_id = id;
        self
    }

    /// Add a baggage item.
    pub fn with_baggage(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.baggage.insert(key.into(), value.into());
        self
    }

    /// Create a child context for a sub-operation, inheriting trace ID and baggage.
    pub fn child(&self, operation: impl Into<String>) -> Self {
        Self {
            trace_id: self.trace_id.clone(),
            correlation_id: self.correlation_id.clone(),
            current_span: self.current_span.child(operation),
            baggage: self.baggage.clone(),
            depth: self.depth + 1,
        }
    }

    /// Finish the current span as successful.
    pub fn finish(&mut self) {
        self.current_span.finish();
    }

    /// Finish the current span with an error.
    pub fn finish_with_error(&mut self, error: impl Into<String>) {
        self.current_span.finish_with_error(error);
    }

    /// Serialize to W3C traceparent header value.
    ///
    /// Format: `00-{trace_id}-{span_id}-01`
    pub fn to_traceparent(&self) -> String {
        // Pad trace_id to 32 hex chars, span_id to 16 hex chars
        let trace = &self.trace_id;
        let span = &self.current_span.span_id;
        format!("00-{trace}-{span}-01")
    }

    /// Parse from a W3C traceparent header value.
    pub fn from_traceparent(header: &str, operation: impl Into<String>) -> Option<Self> {
        let parts: Vec<&str> = header.split('-').collect();
        if parts.len() < 4 {
            return None;
        }
        let trace_id = parts[1].to_string();
        let parent_span_id = parts[2].to_string();
        let mut span = SpanContext::new(operation);
        span.parent_span_id = Some(parent_span_id);

        Some(Self {
            trace_id,
            correlation_id: CorrelationId::new(),
            current_span: span,
            baggage: HashMap::new(),
            depth: 0,
        })
    }
}

// ---------------------------------------------------------------------------
// ContextPropagator
// ---------------------------------------------------------------------------

/// Header names used for context propagation.
pub const HEADER_TRACEPARENT: &str = "traceparent";
/// Header for baggage propagation.
pub const HEADER_BAGGAGE: &str = "baggage";
/// Header for Argentor correlation ID.
pub const HEADER_CORRELATION_ID: &str = "x-correlation-id";

/// Injects and extracts correlation context from HTTP headers.
#[derive(Debug, Default)]
pub struct ContextPropagator;

impl ContextPropagator {
    /// Create a new propagator.
    pub fn new() -> Self {
        Self
    }

    /// Inject a correlation context into HTTP headers.
    pub fn inject(&self, ctx: &CorrelationContext) -> HashMap<String, String> {
        let mut headers = HashMap::new();
        headers.insert(HEADER_TRACEPARENT.to_string(), ctx.to_traceparent());
        headers.insert(
            HEADER_CORRELATION_ID.to_string(),
            ctx.correlation_id.to_string(),
        );

        if !ctx.baggage.is_empty() {
            let baggage_str: Vec<String> = ctx
                .baggage
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            headers.insert(HEADER_BAGGAGE.to_string(), baggage_str.join(","));
        }

        headers
    }

    /// Extract a correlation context from HTTP headers.
    pub fn extract(
        &self,
        headers: &HashMap<String, String>,
        operation: impl Into<String>,
    ) -> CorrelationContext {
        let op = operation.into();

        // Try traceparent first
        let mut ctx = if let Some(traceparent) = headers.get(HEADER_TRACEPARENT) {
            CorrelationContext::from_traceparent(traceparent, &op)
                .unwrap_or_else(|| CorrelationContext::new(&op))
        } else {
            CorrelationContext::new(&op)
        };

        // Restore correlation ID if present
        if let Some(corr_id) = headers.get(HEADER_CORRELATION_ID) {
            ctx.correlation_id = CorrelationId::from_string(corr_id);
        }

        // Parse baggage
        if let Some(baggage_str) = headers.get(HEADER_BAGGAGE) {
            for item in baggage_str.split(',') {
                let item = item.trim();
                if let Some((k, v)) = item.split_once('=') {
                    ctx.baggage
                        .insert(k.trim().to_string(), v.trim().to_string());
                }
            }
        }

        ctx
    }
}

// ---------------------------------------------------------------------------
// TraceCollector — gathers finished spans
// ---------------------------------------------------------------------------

/// Collects finished spans from a distributed trace for export or debugging.
#[derive(Debug, Clone)]
pub struct TraceCollector {
    spans: Arc<std::sync::RwLock<Vec<FinishedSpan>>>,
    span_count: Arc<AtomicU64>,
    max_spans: usize,
}

/// A span that has been finished and recorded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinishedSpan {
    /// The trace this span belongs to.
    pub trace_id: String,
    /// Correlation ID from the originating request.
    pub correlation_id: String,
    /// The span details.
    pub span: SpanContext,
    /// Depth in the call chain.
    pub depth: u32,
}

impl Default for TraceCollector {
    fn default() -> Self {
        Self::new(10_000)
    }
}

impl TraceCollector {
    /// Create a new collector with the given maximum span capacity.
    pub fn new(max_spans: usize) -> Self {
        Self {
            spans: Arc::new(std::sync::RwLock::new(Vec::new())),
            span_count: Arc::new(AtomicU64::new(0)),
            max_spans,
        }
    }

    /// Record a finished correlation context's span.
    pub fn record(&self, ctx: &CorrelationContext) {
        let count = self.span_count.load(Ordering::Relaxed) as usize;
        if count >= self.max_spans {
            return; // Drop spans when at capacity
        }

        let finished = FinishedSpan {
            trace_id: ctx.trace_id.clone(),
            correlation_id: ctx.correlation_id.to_string(),
            span: ctx.current_span.clone(),
            depth: ctx.depth,
        };

        if let Ok(mut spans) = self.spans.write() {
            spans.push(finished);
            self.span_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Get all recorded spans.
    pub fn spans(&self) -> Vec<FinishedSpan> {
        self.spans
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Get spans for a specific trace ID.
    pub fn spans_for_trace(&self, trace_id: &str) -> Vec<FinishedSpan> {
        self.spans
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|s| s.trace_id == trace_id)
            .cloned()
            .collect()
    }

    /// Get the total number of recorded spans.
    pub fn count(&self) -> u64 {
        self.span_count.load(Ordering::Relaxed)
    }

    /// Clear all recorded spans.
    pub fn clear(&self) {
        if let Ok(mut spans) = self.spans.write() {
            spans.clear();
            self.span_count.store(0, Ordering::Relaxed);
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // 1. CorrelationId is unique
    #[test]
    fn test_correlation_id_unique() {
        let a = CorrelationId::new();
        let b = CorrelationId::new();
        assert_ne!(a, b);
    }

    // 2. CorrelationId from string
    #[test]
    fn test_correlation_id_from_string() {
        let id = CorrelationId::from_string("req-123");
        assert_eq!(id.as_str(), "req-123");
        assert_eq!(id.to_string(), "req-123");
    }

    // 3. SpanContext creation
    #[test]
    fn test_span_context_new() {
        let span = SpanContext::new("test-op");
        assert_eq!(span.operation, "test-op");
        assert_eq!(span.status, SpanStatus::InProgress);
        assert!(span.parent_span_id.is_none());
        assert!(span.duration_ms.is_none());
        assert_eq!(span.span_id.len(), 16);
    }

    // 4. Span child inherits parent
    #[test]
    fn test_span_child() {
        let parent = SpanContext::new("parent");
        let child = parent.child("child");
        assert_eq!(
            child.parent_span_id.as_deref(),
            Some(parent.span_id.as_str())
        );
        assert_eq!(child.operation, "child");
        assert_ne!(child.span_id, parent.span_id);
    }

    // 5. Span finish sets duration and status
    #[test]
    fn test_span_finish() {
        let mut span = SpanContext::new("op");
        span.finish();
        assert_eq!(span.status, SpanStatus::Ok);
        assert!(span.duration_ms.is_some());
    }

    // 6. Span finish with error
    #[test]
    fn test_span_finish_with_error() {
        let mut span = SpanContext::new("op");
        span.finish_with_error("something failed");
        assert_eq!(span.status, SpanStatus::Error);
        assert!(span.duration_ms.is_some());
        assert_eq!(span.attributes.get("error").unwrap(), "something failed");
    }

    // 7. Span with attribute
    #[test]
    fn test_span_with_attribute() {
        let span = SpanContext::new("op")
            .with_attribute("agent", "coder")
            .with_attribute("tool", "file_write");
        assert_eq!(span.attributes.get("agent").unwrap(), "coder");
        assert_eq!(span.attributes.get("tool").unwrap(), "file_write");
    }

    // 8. CorrelationContext creation
    #[test]
    fn test_context_new() {
        let ctx = CorrelationContext::new("root");
        assert_eq!(ctx.current_span.operation, "root");
        assert_eq!(ctx.depth, 0);
        assert!(!ctx.trace_id.is_empty());
    }

    // 9. Context child inherits trace ID
    #[test]
    fn test_context_child() {
        let parent = CorrelationContext::new("root").with_baggage("user", "alice");
        let child = parent.child("sub-op");
        assert_eq!(child.trace_id, parent.trace_id);
        assert_eq!(child.correlation_id, parent.correlation_id);
        assert_eq!(child.depth, 1);
        assert_eq!(child.baggage.get("user").unwrap(), "alice");
    }

    // 10. Nested children increment depth
    #[test]
    fn test_nested_depth() {
        let root = CorrelationContext::new("root");
        let l1 = root.child("level-1");
        let l2 = l1.child("level-2");
        let l3 = l2.child("level-3");
        assert_eq!(l3.depth, 3);
        assert_eq!(l3.trace_id, root.trace_id);
    }

    // 11. W3C traceparent round-trip
    #[test]
    fn test_traceparent_roundtrip() {
        let ctx = CorrelationContext::new("op");
        let header = ctx.to_traceparent();
        assert!(header.starts_with("00-"));
        assert!(header.ends_with("-01"));

        let restored = CorrelationContext::from_traceparent(&header, "restored").unwrap();
        assert_eq!(restored.trace_id, ctx.trace_id);
        assert_eq!(
            restored.current_span.parent_span_id.as_deref(),
            Some(ctx.current_span.span_id.as_str())
        );
    }

    // 12. Invalid traceparent returns None
    #[test]
    fn test_invalid_traceparent() {
        assert!(CorrelationContext::from_traceparent("invalid", "op").is_none());
        assert!(CorrelationContext::from_traceparent("", "op").is_none());
    }

    // 13. Propagator inject/extract
    #[test]
    fn test_propagator_inject_extract() {
        let propagator = ContextPropagator::new();
        let ctx = CorrelationContext::new("original")
            .with_correlation_id(CorrelationId::from_string("req-456"))
            .with_baggage("tenant", "acme")
            .with_baggage("env", "prod");

        let headers = propagator.inject(&ctx);
        assert!(headers.contains_key(HEADER_TRACEPARENT));
        assert_eq!(headers.get(HEADER_CORRELATION_ID).unwrap(), "req-456");
        assert!(headers.contains_key(HEADER_BAGGAGE));

        let extracted = propagator.extract(&headers, "extracted-op");
        assert_eq!(extracted.trace_id, ctx.trace_id);
        assert_eq!(extracted.correlation_id.as_str(), "req-456");
        assert_eq!(extracted.baggage.get("tenant").unwrap(), "acme");
        assert_eq!(extracted.baggage.get("env").unwrap(), "prod");
    }

    // 14. Propagator with empty headers creates new context
    #[test]
    fn test_propagator_empty_headers() {
        let propagator = ContextPropagator::new();
        let headers = HashMap::new();
        let ctx = propagator.extract(&headers, "fresh");
        assert_eq!(ctx.current_span.operation, "fresh");
        assert!(ctx.current_span.parent_span_id.is_none());
    }

    // 15. TraceCollector records spans
    #[test]
    fn test_trace_collector_record() {
        let collector = TraceCollector::new(100);
        let mut ctx = CorrelationContext::new("op");
        ctx.finish();
        collector.record(&ctx);

        assert_eq!(collector.count(), 1);
        let spans = collector.spans();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].trace_id, ctx.trace_id);
    }

    // 16. TraceCollector filters by trace ID
    #[test]
    fn test_trace_collector_filter_by_trace() {
        let collector = TraceCollector::new(100);

        let mut ctx1 = CorrelationContext::new("op1");
        ctx1.finish();
        let trace1 = ctx1.trace_id.clone();

        let mut ctx2 = CorrelationContext::new("op2");
        ctx2.finish();

        let mut child1 = ctx1.child("child");
        child1.finish();

        collector.record(&ctx1);
        collector.record(&ctx2);
        collector.record(&child1);

        let filtered = collector.spans_for_trace(&trace1);
        assert_eq!(filtered.len(), 2);
    }

    // 17. TraceCollector respects max capacity
    #[test]
    fn test_trace_collector_max_capacity() {
        let collector = TraceCollector::new(3);
        for i in 0..10 {
            let mut ctx = CorrelationContext::new(format!("op-{i}"));
            ctx.finish();
            collector.record(&ctx);
        }
        assert_eq!(collector.count(), 3);
    }

    // 18. TraceCollector clear
    #[test]
    fn test_trace_collector_clear() {
        let collector = TraceCollector::new(100);
        let mut ctx = CorrelationContext::new("op");
        ctx.finish();
        collector.record(&ctx);
        assert_eq!(collector.count(), 1);

        collector.clear();
        assert_eq!(collector.count(), 0);
        assert!(collector.spans().is_empty());
    }

    // 19. Context finish propagates to span
    #[test]
    fn test_context_finish() {
        let mut ctx = CorrelationContext::new("op");
        ctx.finish();
        assert_eq!(ctx.current_span.status, SpanStatus::Ok);
    }

    // 20. Context finish with error
    #[test]
    fn test_context_finish_error() {
        let mut ctx = CorrelationContext::new("op");
        ctx.finish_with_error("boom");
        assert_eq!(ctx.current_span.status, SpanStatus::Error);
        assert_eq!(ctx.current_span.attributes.get("error").unwrap(), "boom");
    }

    // 21. CorrelationContext serializable
    #[test]
    fn test_context_serializable() {
        let ctx = CorrelationContext::new("op").with_baggage("key", "val");
        let json = serde_json::to_string(&ctx).unwrap();
        let restored: CorrelationContext = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.trace_id, ctx.trace_id);
        assert_eq!(restored.baggage.get("key").unwrap(), "val");
    }

    // 22. FinishedSpan serializable
    #[test]
    fn test_finished_span_serializable() {
        let mut ctx = CorrelationContext::new("test");
        ctx.finish();
        let collector = TraceCollector::new(10);
        collector.record(&ctx);

        let spans = collector.spans();
        let json = serde_json::to_string(&spans[0]).unwrap();
        let restored: FinishedSpan = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.trace_id, ctx.trace_id);
    }

    // 23. Baggage propagation through multiple levels
    #[test]
    fn test_baggage_multi_level() {
        let root = CorrelationContext::new("root").with_baggage("tenant", "acme");
        let l1 = root.child("l1").with_baggage("region", "us-east");
        let l2 = l1.child("l2");
        assert_eq!(l2.baggage.get("tenant").unwrap(), "acme");
        assert_eq!(l2.baggage.get("region").unwrap(), "us-east");
    }

    // 24. Default implementations
    #[test]
    fn test_defaults() {
        let id = CorrelationId::default();
        assert!(!id.as_str().is_empty());

        let collector = TraceCollector::default();
        assert_eq!(collector.count(), 0);

        let propagator = ContextPropagator;
        let headers = propagator.inject(&CorrelationContext::new("test"));
        assert!(!headers.is_empty());
    }
}
