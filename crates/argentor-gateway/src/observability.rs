//! End-to-end observability wiring for the Argentor gateway.
//!
//! Connects the core [`TelemetryConfig`] (OTLP tracing), the security
//! [`AgentMetricsCollector`] (Prometheus metrics), and the core
//! [`CorrelationContext`] (W3C trace propagation) into a single
//! [`ObservabilityStack`] that the gateway initializes at startup.
//!
//! Also provides request-tracing middleware that:
//! - Creates a correlation context per request
//! - Logs method, path, status code, and duration
//! - Adds an `X-Trace-Id` response header
//! - Increments request counters and duration histograms
//!
//! # Main types
//!
//! - [`ObservabilityConfig`] — Unified config for tracing + metrics.
//! - [`ObservabilityStack`] — Initializes and shuts down the full pipeline.
//! - [`RequestMetrics`] — In-memory request-level counters and histograms.

use argentor_core::correlation::CorrelationContext;
use argentor_core::telemetry::{self, TelemetryConfig};
use argentor_core::ArgentorResult;
use argentor_security::observability::AgentMetricsCollector;
use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tracing::{info, info_span, warn, Instrument};

// ---------------------------------------------------------------------------
// ObservabilityConfig
// ---------------------------------------------------------------------------

/// Unified configuration for all observability subsystems.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    /// Enable distributed tracing (OTLP export when the `telemetry` feature is on).
    pub enable_tracing: bool,
    /// Enable in-memory request metrics (counters, histograms).
    pub enable_metrics: bool,
    /// OTLP gRPC endpoint (e.g., `"http://localhost:4317"`).
    pub otlp_endpoint: Option<String>,
    /// Service name reported in traces and metrics.
    pub service_name: String,
    /// Service version reported in traces.
    pub service_version: String,
    /// Minimum log level filter (e.g., `"info"`, `"debug"`).
    pub log_level: String,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            enable_tracing: false,
            enable_metrics: true,
            otlp_endpoint: None,
            service_name: "argentor-gateway".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            log_level: "info".to_string(),
        }
    }
}

impl ObservabilityConfig {
    /// Build a config with tracing and metrics enabled, pointed at an OTLP endpoint.
    pub fn with_otlp(endpoint: impl Into<String>) -> Self {
        Self {
            enable_tracing: true,
            enable_metrics: true,
            otlp_endpoint: Some(endpoint.into()),
            ..Self::default()
        }
    }

    /// Build a config with only local metrics (no OTLP export).
    pub fn metrics_only() -> Self {
        Self {
            enable_tracing: false,
            enable_metrics: true,
            ..Self::default()
        }
    }

    /// Build a config with everything disabled (for testing).
    pub fn disabled() -> Self {
        Self {
            enable_tracing: false,
            enable_metrics: false,
            ..Self::default()
        }
    }
}

// ---------------------------------------------------------------------------
// ObservabilityStack
// ---------------------------------------------------------------------------

/// Manages the full observability lifecycle: init, runtime, and shutdown.
///
/// Create one at gateway startup and call [`init`] before serving requests.
/// Call [`shutdown`] during graceful shutdown to flush pending spans.
pub struct ObservabilityStack {
    config: ObservabilityConfig,
    /// Optional metrics collector (created if `enable_metrics` is true).
    metrics: Option<AgentMetricsCollector>,
    /// Request-level metrics (always created for middleware use).
    request_metrics: Arc<RequestMetrics>,
}

impl ObservabilityStack {
    /// Create a new stack from the given configuration.
    pub fn new(config: ObservabilityConfig) -> Self {
        let metrics = if config.enable_metrics {
            Some(AgentMetricsCollector::new())
        } else {
            None
        };
        Self {
            config,
            metrics,
            request_metrics: Arc::new(RequestMetrics::new()),
        }
    }

    /// Initialize all observability: tracing subscriber, OTLP exporter, metrics.
    ///
    /// This should be called once at gateway startup before the HTTP server
    /// begins accepting connections.
    pub fn init(&self) -> ArgentorResult<()> {
        let telemetry_config = TelemetryConfig {
            enabled: self.config.enable_tracing && self.config.otlp_endpoint.is_some(),
            otlp_endpoint: self
                .config
                .otlp_endpoint
                .clone()
                .or_else(|| std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok())
                .unwrap_or_else(|| "http://localhost:4317".to_string()),
            service_name: self.config.service_name.clone(),
            sample_rate: 1.0,
        };

        telemetry::init_telemetry(&telemetry_config).map_err(|e| {
            argentor_core::ArgentorError::Config(format!("Failed to initialize telemetry: {e}"))
        })?;

        info!(
            service = %self.config.service_name,
            version = %self.config.service_version,
            tracing = self.config.enable_tracing,
            metrics = self.config.enable_metrics,
            "Observability stack initialized"
        );

        Ok(())
    }

    /// Gracefully shut down: flush pending spans and metrics.
    pub async fn shutdown(&self) {
        info!("Shutting down observability stack");
        telemetry::shutdown_telemetry();
        if let Some(ref collector) = self.metrics {
            // Export final snapshot before shutdown
            let summary = collector.summary();
            info!(
                total_tool_calls = summary.total_tool_calls,
                total_tokens = summary.total_tokens,
                "Final metrics snapshot before shutdown"
            );
        }
    }

    /// Get the agent metrics collector (if metrics are enabled).
    pub fn metrics_collector(&self) -> Option<&AgentMetricsCollector> {
        self.metrics.as_ref()
    }

    /// Get the request metrics for wiring into middleware.
    pub fn request_metrics(&self) -> Arc<RequestMetrics> {
        Arc::clone(&self.request_metrics)
    }

    /// Get the configuration.
    pub fn config(&self) -> &ObservabilityConfig {
        &self.config
    }
}

// ---------------------------------------------------------------------------
// RequestMetrics — in-memory counters for HTTP request observability
// ---------------------------------------------------------------------------

/// In-memory request-level metrics: counters and duration tracking.
///
/// Thread-safe (interior `RwLock`). Designed to be shared across middleware
/// instances via `Arc<RequestMetrics>`.
#[derive(Debug)]
pub struct RequestMetrics {
    inner: RwLock<RequestMetricsInner>,
}

#[derive(Debug, Default)]
struct RequestMetricsInner {
    /// Request counter by (method, path_pattern, status_code).
    request_counts: HashMap<RequestKey, u64>,
    /// Cumulative request duration in microseconds by (method, path_pattern, status_code).
    request_duration_us: HashMap<RequestKey, u64>,
    /// Duration buckets (histogram) by (method, path_pattern).
    /// Bucket boundaries in milliseconds: [5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000, 10000].
    request_duration_buckets: HashMap<(String, String), [u64; 11]>,
    /// Currently active connections gauge.
    active_connections: u64,
    /// Total LLM calls by provider.
    llm_call_counts: HashMap<String, u64>,
    /// Total tool executions by skill name.
    tool_exec_counts: HashMap<String, u64>,
    /// Total token usage by (provider, direction).
    token_counts: HashMap<(String, String), u64>,
}

/// Key for per-request counters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RequestKey {
    method: String,
    path: String,
    status: u16,
}

/// Duration histogram bucket boundaries in milliseconds.
const DURATION_BUCKETS_MS: [u64; 11] = [5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000, 10000];

impl RequestMetrics {
    /// Create a new, empty metrics store.
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(RequestMetricsInner::default()),
        }
    }

    /// Record a completed HTTP request.
    pub fn record_request(&self, method: &str, path: &str, status: u16, duration_us: u64) {
        let mut inner = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let key = RequestKey {
            method: method.to_string(),
            path: normalize_path(path),
            status,
        };

        *inner.request_counts.entry(key.clone()).or_insert(0) += 1;
        *inner.request_duration_us.entry(key).or_insert(0) += duration_us;

        // Update histogram buckets
        let duration_ms = duration_us / 1000;
        let bucket_key = (method.to_string(), normalize_path(path));
        let buckets = inner
            .request_duration_buckets
            .entry(bucket_key)
            .or_insert([0; 11]);
        for (i, &boundary) in DURATION_BUCKETS_MS.iter().enumerate() {
            if duration_ms <= boundary {
                buckets[i] += 1;
            }
        }
    }

    /// Increment the active connections gauge.
    pub fn connection_opened(&self) {
        let mut inner = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.active_connections += 1;
    }

    /// Decrement the active connections gauge.
    pub fn connection_closed(&self) {
        let mut inner = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.active_connections = inner.active_connections.saturating_sub(1);
    }

    /// Record an LLM call by provider.
    pub fn record_llm_call(&self, provider: &str) {
        let mut inner = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *inner
            .llm_call_counts
            .entry(provider.to_string())
            .or_insert(0) += 1;
    }

    /// Record a tool execution by skill name.
    pub fn record_tool_execution(&self, skill_name: &str) {
        let mut inner = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *inner
            .tool_exec_counts
            .entry(skill_name.to_string())
            .or_insert(0) += 1;
    }

    /// Record token usage by provider and direction ("input" or "output").
    pub fn record_tokens(&self, provider: &str, direction: &str, count: u64) {
        let mut inner = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *inner
            .token_counts
            .entry((provider.to_string(), direction.to_string()))
            .or_insert(0) += count;
    }

    /// Get the current request count for a specific (method, path, status) combination.
    pub fn request_count(&self, method: &str, path: &str, status: u16) -> u64 {
        let inner = self
            .inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let key = RequestKey {
            method: method.to_string(),
            path: normalize_path(path),
            status,
        };
        inner.request_counts.get(&key).copied().unwrap_or(0)
    }

    /// Get the current active connections count.
    pub fn active_connections(&self) -> u64 {
        let inner = self
            .inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.active_connections
    }

    /// Get the LLM call count for a specific provider.
    pub fn llm_call_count(&self, provider: &str) -> u64 {
        let inner = self
            .inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.llm_call_counts.get(provider).copied().unwrap_or(0)
    }

    /// Get the tool execution count for a specific skill.
    pub fn tool_exec_count(&self, skill_name: &str) -> u64 {
        let inner = self
            .inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.tool_exec_counts.get(skill_name).copied().unwrap_or(0)
    }

    /// Get token count for a (provider, direction) pair.
    pub fn token_count(&self, provider: &str, direction: &str) -> u64 {
        let inner = self
            .inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner
            .token_counts
            .get(&(provider.to_string(), direction.to_string()))
            .copied()
            .unwrap_or(0)
    }

    /// Export all request metrics in Prometheus text exposition format.
    pub fn prometheus_export(&self) -> String {
        let inner = self
            .inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let mut out = String::with_capacity(4096);

        // -- request counter ---------------------------------------------------
        out.push_str(
            "# HELP argentor_http_requests_total Total HTTP requests by method, path, status\n",
        );
        out.push_str("# TYPE argentor_http_requests_total counter\n");
        let mut sorted_req: Vec<_> = inner.request_counts.iter().collect();
        sorted_req.sort_by(|a, b| {
            (&a.0.method, &a.0.path, a.0.status).cmp(&(&b.0.method, &b.0.path, b.0.status))
        });
        for (key, count) in &sorted_req {
            out.push_str(&format!(
                "argentor_http_requests_total{{method=\"{}\",path=\"{}\",status=\"{}\"}} {}\n",
                key.method, key.path, key.status, count
            ));
        }

        // -- request duration --------------------------------------------------
        out.push_str(
            "# HELP argentor_http_request_duration_us Cumulative request duration in microseconds\n",
        );
        out.push_str("# TYPE argentor_http_request_duration_us counter\n");
        let mut sorted_dur: Vec<_> = inner.request_duration_us.iter().collect();
        sorted_dur.sort_by(|a, b| {
            (&a.0.method, &a.0.path, a.0.status).cmp(&(&b.0.method, &b.0.path, b.0.status))
        });
        for (key, dur) in &sorted_dur {
            out.push_str(&format!(
                "argentor_http_request_duration_us{{method=\"{}\",path=\"{}\",status=\"{}\"}} {}\n",
                key.method, key.path, key.status, dur
            ));
        }

        // -- active connections gauge ------------------------------------------
        out.push_str(
            "# HELP argentor_active_connections Current number of active HTTP/WS connections\n",
        );
        out.push_str("# TYPE argentor_active_connections gauge\n");
        out.push_str(&format!(
            "argentor_active_connections {}\n",
            inner.active_connections
        ));

        // -- LLM call counter --------------------------------------------------
        out.push_str("# HELP argentor_llm_calls_total Total LLM API calls by provider\n");
        out.push_str("# TYPE argentor_llm_calls_total counter\n");
        let mut sorted_llm: Vec<_> = inner.llm_call_counts.iter().collect();
        sorted_llm.sort_by_key(|(k, _)| (*k).clone());
        for (provider, count) in &sorted_llm {
            out.push_str(&format!(
                "argentor_llm_calls_total{{provider=\"{provider}\"}} {count}\n"
            ));
        }

        // -- tool execution counter --------------------------------------------
        out.push_str("# HELP argentor_tool_executions_total Total tool executions by skill name\n");
        out.push_str("# TYPE argentor_tool_executions_total counter\n");
        let mut sorted_tool: Vec<_> = inner.tool_exec_counts.iter().collect();
        sorted_tool.sort_by_key(|(k, _)| (*k).clone());
        for (skill, count) in &sorted_tool {
            out.push_str(&format!(
                "argentor_tool_executions_total{{skill=\"{skill}\"}} {count}\n"
            ));
        }

        // -- token usage counter -----------------------------------------------
        out.push_str(
            "# HELP argentor_tokens_used_total Total token usage by provider and direction\n",
        );
        out.push_str("# TYPE argentor_tokens_used_total counter\n");
        let mut sorted_tok: Vec<_> = inner.token_counts.iter().collect();
        sorted_tok.sort_by(|a, b| a.0.cmp(b.0));
        for ((provider, direction), count) in &sorted_tok {
            out.push_str(&format!(
                "argentor_tokens_used_total{{provider=\"{provider}\",direction=\"{direction}\"}} {count}\n"
            ));
        }

        out
    }
}

impl Default for RequestMetrics {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Request tracing middleware
// ---------------------------------------------------------------------------

/// Shared state for the observability middleware.
#[derive(Clone)]
pub struct ObservabilityMiddlewareState {
    /// Request-level metrics store.
    pub request_metrics: Arc<RequestMetrics>,
}

/// Axum middleware that traces every request with correlation context.
///
/// For each request it:
/// 1. Extracts or creates a `CorrelationContext` (W3C traceparent).
/// 2. Wraps the handler in a tracing span with method, path, trace_id.
/// 3. Measures duration and records status code.
/// 4. Adds `X-Trace-Id` and `X-Span-Id` response headers.
/// 5. Increments request counters.
pub async fn request_tracing_middleware(
    axum::extract::State(state): axum::extract::State<Arc<ObservabilityMiddlewareState>>,
    request: Request,
    next: Next,
) -> Response {
    let method = request.method().to_string();
    let path = request.uri().path().to_string();

    // Extract or create correlation context from incoming traceparent header
    let traceparent = request
        .headers()
        .get("traceparent")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let ctx = if let Some(ref tp) = traceparent {
        CorrelationContext::from_traceparent(tp, format!("{method} {path}"))
            .unwrap_or_else(|| CorrelationContext::new(format!("{method} {path}")))
    } else {
        CorrelationContext::new(format!("{method} {path}"))
    };

    let trace_id = ctx.trace_id.clone();
    let span_id = ctx.current_span.span_id.clone();

    let span = info_span!(
        "http_request",
        method = %method,
        path = %path,
        trace_id = %trace_id,
        span_id = %span_id,
    );

    let start = Instant::now();

    // Execute the next handler inside the span
    let mut response = next.run(request).instrument(span).await;

    let duration = start.elapsed();
    let status = response.status().as_u16();

    // Record metrics
    state
        .request_metrics
        .record_request(&method, &path, status, duration.as_micros() as u64);

    // Add trace headers to response
    if let Ok(val) = HeaderValue::from_str(&trace_id) {
        response.headers_mut().insert("X-Trace-Id", val);
    }
    if let Ok(val) = HeaderValue::from_str(&span_id) {
        response.headers_mut().insert("X-Span-Id", val);
    }

    // Log the completed request
    if status >= 500 {
        warn!(
            method = %method,
            path = %path,
            status = status,
            duration_ms = duration.as_millis() as u64,
            trace_id = %trace_id,
            "Request completed with server error"
        );
    } else {
        info!(
            method = %method,
            path = %path,
            status = status,
            duration_ms = duration.as_millis() as u64,
            trace_id = %trace_id,
            "Request completed"
        );
    }

    response
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Normalize a request path for metric aggregation.
///
/// Replaces UUID-like segments and numeric IDs with placeholders to avoid
/// cardinality explosion in metrics.
fn normalize_path(path: &str) -> String {
    let segments: Vec<&str> = path.split('/').collect();
    let normalized: Vec<String> = segments
        .into_iter()
        .map(|seg| {
            // Replace UUIDs (8-4-4-4-12 hex pattern) or pure numeric segments
            if (seg.len() == 36 && seg.chars().filter(|c| *c == '-').count() == 4)
                || (!seg.is_empty() && seg.chars().all(|c| c.is_ascii_digit()))
            {
                ":id".to_string()
            } else {
                seg.to_string()
            }
        })
        .collect();
    normalized.join("/")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // 1. ObservabilityConfig defaults
    #[test]
    fn test_config_defaults() {
        let config = ObservabilityConfig::default();
        assert!(!config.enable_tracing);
        assert!(config.enable_metrics);
        assert!(config.otlp_endpoint.is_none());
        assert_eq!(config.service_name, "argentor-gateway");
        assert_eq!(config.log_level, "info");
    }

    // 2. ObservabilityConfig with_otlp
    #[test]
    fn test_config_with_otlp() {
        let config = ObservabilityConfig::with_otlp("http://otel:4317");
        assert!(config.enable_tracing);
        assert!(config.enable_metrics);
        assert_eq!(config.otlp_endpoint.as_deref(), Some("http://otel:4317"));
    }

    // 3. ObservabilityConfig disabled
    #[test]
    fn test_config_disabled() {
        let config = ObservabilityConfig::disabled();
        assert!(!config.enable_tracing);
        assert!(!config.enable_metrics);
    }

    // 4. ObservabilityConfig metrics_only
    #[test]
    fn test_config_metrics_only() {
        let config = ObservabilityConfig::metrics_only();
        assert!(!config.enable_tracing);
        assert!(config.enable_metrics);
    }

    // 5. ObservabilityStack creation with metrics enabled
    #[test]
    fn test_stack_with_metrics() {
        let stack = ObservabilityStack::new(ObservabilityConfig::default());
        assert!(stack.metrics_collector().is_some());
    }

    // 6. ObservabilityStack creation with metrics disabled
    #[test]
    fn test_stack_without_metrics() {
        let stack = ObservabilityStack::new(ObservabilityConfig::disabled());
        assert!(stack.metrics_collector().is_none());
    }

    // 7. ObservabilityStack init with tracing disabled does not panic
    #[test]
    fn test_stack_init_disabled_tracing() {
        let stack = ObservabilityStack::new(ObservabilityConfig::metrics_only());
        // init_telemetry with disabled config should not panic
        // (it may fail silently if a global subscriber is already set)
        let _ = stack.init();
    }

    // 8. RequestMetrics creation
    #[test]
    fn test_request_metrics_new() {
        let m = RequestMetrics::new();
        assert_eq!(m.active_connections(), 0);
        assert_eq!(m.request_count("GET", "/health", 200), 0);
    }

    // 9. RequestMetrics record_request increments counter
    #[test]
    fn test_request_metrics_record() {
        let m = RequestMetrics::new();
        m.record_request("GET", "/health", 200, 5000);
        m.record_request("GET", "/health", 200, 3000);
        m.record_request("POST", "/ws", 101, 100);

        assert_eq!(m.request_count("GET", "/health", 200), 2);
        assert_eq!(m.request_count("POST", "/ws", 101), 1);
        assert_eq!(m.request_count("GET", "/missing", 404), 0);
    }

    // 10. RequestMetrics active connections gauge
    #[test]
    fn test_active_connections_gauge() {
        let m = RequestMetrics::new();
        m.connection_opened();
        m.connection_opened();
        assert_eq!(m.active_connections(), 2);

        m.connection_closed();
        assert_eq!(m.active_connections(), 1);

        // Saturating: closing when 0 stays at 0
        m.connection_closed();
        m.connection_closed();
        assert_eq!(m.active_connections(), 0);
    }

    // 11. RequestMetrics LLM call counter
    #[test]
    fn test_llm_call_counter() {
        let m = RequestMetrics::new();
        m.record_llm_call("openai");
        m.record_llm_call("openai");
        m.record_llm_call("anthropic");

        assert_eq!(m.llm_call_count("openai"), 2);
        assert_eq!(m.llm_call_count("anthropic"), 1);
        assert_eq!(m.llm_call_count("unknown"), 0);
    }

    // 12. RequestMetrics tool execution counter
    #[test]
    fn test_tool_exec_counter() {
        let m = RequestMetrics::new();
        m.record_tool_execution("file_write");
        m.record_tool_execution("file_write");
        m.record_tool_execution("shell_exec");

        assert_eq!(m.tool_exec_count("file_write"), 2);
        assert_eq!(m.tool_exec_count("shell_exec"), 1);
        assert_eq!(m.tool_exec_count("missing"), 0);
    }

    // 13. RequestMetrics token usage counter
    #[test]
    fn test_token_counter() {
        let m = RequestMetrics::new();
        m.record_tokens("openai", "input", 500);
        m.record_tokens("openai", "output", 200);
        m.record_tokens("openai", "input", 300);

        assert_eq!(m.token_count("openai", "input"), 800);
        assert_eq!(m.token_count("openai", "output"), 200);
        assert_eq!(m.token_count("anthropic", "input"), 0);
    }

    // 14. Prometheus export contains expected metric lines
    #[test]
    fn test_prometheus_export() {
        let m = RequestMetrics::new();
        m.record_request("GET", "/health", 200, 5000);
        m.record_llm_call("openai");
        m.record_tool_execution("echo");
        m.record_tokens("openai", "input", 100);
        m.connection_opened();

        let prom = m.prometheus_export();
        assert!(prom.contains("# HELP argentor_http_requests_total"));
        assert!(prom.contains("# TYPE argentor_http_requests_total counter"));
        assert!(prom.contains(
            "argentor_http_requests_total{method=\"GET\",path=\"/health\",status=\"200\"} 1"
        ));
        assert!(prom.contains("argentor_active_connections 1"));
        assert!(prom.contains("argentor_llm_calls_total{provider=\"openai\"} 1"));
        assert!(prom.contains("argentor_tool_executions_total{skill=\"echo\"} 1"));
        assert!(prom
            .contains("argentor_tokens_used_total{provider=\"openai\",direction=\"input\"} 100"));
    }

    // 15. Path normalization replaces UUIDs and numeric IDs
    #[test]
    fn test_normalize_path() {
        assert_eq!(
            normalize_path("/api/v1/sessions/550e8400-e29b-41d4-a716-446655440000/messages"),
            "/api/v1/sessions/:id/messages"
        );
        assert_eq!(normalize_path("/api/v1/users/12345"), "/api/v1/users/:id");
        assert_eq!(normalize_path("/health"), "/health");
        assert_eq!(normalize_path("/"), "/");
    }

    // 16. ObservabilityConfig serialization roundtrip
    #[test]
    fn test_config_serialization() {
        let config = ObservabilityConfig::with_otlp("http://otel:4317");
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"enable_tracing\":true"));
        assert!(json.contains("http://otel:4317"));

        let restored: ObservabilityConfig = serde_json::from_str(&json).unwrap();
        assert!(restored.enable_tracing);
        assert_eq!(restored.otlp_endpoint.as_deref(), Some("http://otel:4317"));
    }

    // 17. ObservabilityStack request_metrics returns shared instance
    #[test]
    fn test_stack_request_metrics_shared() {
        let stack = ObservabilityStack::new(ObservabilityConfig::default());
        let m1 = stack.request_metrics();
        let m2 = stack.request_metrics();

        m1.record_request("GET", "/test", 200, 1000);
        assert_eq!(m2.request_count("GET", "/test", 200), 1);
    }

    // 18. RequestMetrics default trait
    #[test]
    fn test_request_metrics_default() {
        let m = RequestMetrics::default();
        assert_eq!(m.active_connections(), 0);
    }

    // 19. Prometheus export with empty metrics produces valid headers
    #[test]
    fn test_prometheus_export_empty() {
        let m = RequestMetrics::new();
        let prom = m.prometheus_export();
        assert!(prom.contains("# HELP argentor_http_requests_total"));
        assert!(prom.contains("argentor_active_connections 0"));
    }

    // 20. ObservabilityMiddlewareState can be cloned
    #[test]
    fn test_middleware_state_clone() {
        let state = ObservabilityMiddlewareState {
            request_metrics: Arc::new(RequestMetrics::new()),
        };
        let cloned = state.clone();
        cloned.request_metrics.record_request("GET", "/", 200, 100);
        assert_eq!(state.request_metrics.request_count("GET", "/", 200), 1);
    }
}
