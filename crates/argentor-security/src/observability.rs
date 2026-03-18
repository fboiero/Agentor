//! Production observability for Argentor.
//!
//! Provides structured metrics collection and Prometheus-compatible export
//! without requiring any external OpenTelemetry crate. Metrics are kept
//! in-memory and can be scraped by any OTel-compatible backend (Prometheus,
//! Datadog, Grafana Agent, etc.) via the text exposition format.
//!
//! # Main types
//!
//! - [`AgentMetricsCollector`] — Thread-safe metrics sink.
//! - [`MetricEvent`] — Discrete event recorded by the collector.
//! - [`MetricsSummary`] — Point-in-time snapshot of all metrics.
//! - [`SecurityEventType`] — Classification of blocked security events.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tracing::debug;

// ---------------------------------------------------------------------------
// SecurityEventType
// ---------------------------------------------------------------------------

/// Classification of security events that are tracked by the metrics system.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurityEventType {
    /// A capability or permission check failed.
    PermissionDenied,
    /// A path traversal attempt (e.g. `../../etc/passwd`) was blocked.
    PathTraversalBlocked,
    /// A server-side request forgery attempt was blocked.
    SsrfBlocked,
    /// A shell injection attempt was blocked.
    ShellInjectionBlocked,
    /// A request exceeded the configured rate limit.
    RateLimitExceeded,
    /// An authentication attempt failed.
    AuthenticationFailed,
    /// Input was sanitized before processing.
    InputSanitized,
}

impl SecurityEventType {
    /// Stable label used in Prometheus export.
    fn prometheus_label(&self) -> &'static str {
        match self {
            Self::PermissionDenied => "permission_denied",
            Self::PathTraversalBlocked => "path_traversal_blocked",
            Self::SsrfBlocked => "ssrf_blocked",
            Self::ShellInjectionBlocked => "shell_injection_blocked",
            Self::RateLimitExceeded => "rate_limit_exceeded",
            Self::AuthenticationFailed => "authentication_failed",
            Self::InputSanitized => "input_sanitized",
        }
    }
}

// ---------------------------------------------------------------------------
// MetricEvent
// ---------------------------------------------------------------------------

/// A discrete event to be recorded by the [`AgentMetricsCollector`].
#[derive(Debug, Clone)]
pub enum MetricEvent {
    /// A tool invocation has started (used for in-flight tracking).
    ToolCallStarted {
        /// Role of the agent that initiated the call.
        agent_role: String,
        /// Name of the tool being invoked.
        tool_name: String,
    },
    /// A tool invocation has completed.
    ToolCallCompleted {
        /// Role of the agent that initiated the call.
        agent_role: String,
        /// Name of the tool that was invoked.
        tool_name: String,
        /// Wall-clock duration of the call in milliseconds.
        duration_ms: u64,
        /// Whether the call succeeded.
        success: bool,
    },
    /// Tokens were consumed by an LLM call.
    TokensUsed {
        /// Role of the agent that consumed tokens.
        agent_role: String,
        /// Number of input (prompt) tokens.
        input_tokens: u64,
        /// Number of output (completion) tokens.
        output_tokens: u64,
    },
    /// An agent has started processing a task.
    AgentStarted {
        /// Role of the agent.
        agent_role: String,
        /// Identifier of the task being processed.
        task_id: String,
    },
    /// An agent has finished processing a task.
    AgentCompleted {
        /// Role of the agent.
        agent_role: String,
        /// Identifier of the task that was processed.
        task_id: String,
        /// Total wall-clock duration in milliseconds.
        duration_ms: u64,
    },
    /// A security-relevant event occurred.
    SecurityEvent {
        /// Classification of the security event.
        event_type: SecurityEventType,
        /// Human-readable description.
        details: String,
    },
    /// A compliance framework check was executed.
    ComplianceCheck {
        /// Name of the compliance framework (e.g. "GDPR", "ISO27001").
        framework: String,
        /// Whether the check passed.
        passed: bool,
        /// Human-readable description of the result.
        details: String,
    },
}

// ---------------------------------------------------------------------------
// Internal per-agent / per-tool accumulators
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct AgentAccumulator {
    tool_calls: u64,
    tool_errors: u64,
    input_tokens: u64,
    output_tokens: u64,
    total_latency_ms: u64,
    tasks_started: u64,
    tasks_completed: u64,
}

#[derive(Debug, Clone, Default)]
struct ToolAccumulator {
    calls: u64,
    errors: u64,
    total_latency_ms: u64,
}

/// Key for the per-(agent, tool) counter stored inside the collector.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AgentToolKey {
    agent_role: String,
    tool_name: String,
}

#[derive(Debug, Clone, Default)]
struct AgentToolAccumulator {
    calls: u64,
    errors: u64,
}

// ---------------------------------------------------------------------------
// Inner (behind RwLock)
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct Inner {
    start_time: Instant,

    // Per-agent
    agents: HashMap<String, AgentAccumulator>,
    // Per-tool
    tools: HashMap<String, ToolAccumulator>,
    // Per-(agent, tool) — needed for Prometheus labels
    agent_tools: HashMap<AgentToolKey, AgentToolAccumulator>,

    // System-wide
    total_tool_calls: u64,
    total_errors: u64,
    total_input_tokens: u64,
    total_output_tokens: u64,
    active_agents: u64,
    queue_depth: u64,

    // Security
    security_events: HashMap<SecurityEventType, u64>,
    security_event_details: Vec<(SecurityEventType, String)>,

    // Compliance
    compliance_passed: u64,
    compliance_failed: u64,
}

impl Inner {
    fn new() -> Self {
        Self {
            start_time: Instant::now(),
            agents: HashMap::new(),
            tools: HashMap::new(),
            agent_tools: HashMap::new(),
            total_tool_calls: 0,
            total_errors: 0,
            total_input_tokens: 0,
            total_output_tokens: 0,
            active_agents: 0,
            queue_depth: 0,
            security_events: HashMap::new(),
            security_event_details: Vec::new(),
            compliance_passed: 0,
            compliance_failed: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// AgentMetricsCollector
// ---------------------------------------------------------------------------

/// Thread-safe, in-memory metrics collector for the Argentor framework.
///
/// Clone is cheap (inner data is behind `Arc<RwLock>`), so you can hand a
/// clone to every subsystem that needs to emit metrics.
#[derive(Debug, Clone)]
pub struct AgentMetricsCollector {
    inner: Arc<RwLock<Inner>>,
}

impl Default for AgentMetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentMetricsCollector {
    /// Create a new, empty collector. The uptime clock starts now.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner::new())),
        }
    }

    /// Record a [`MetricEvent`].
    pub fn record(&self, event: MetricEvent) {
        let mut inner = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        match event {
            MetricEvent::ToolCallStarted {
                agent_role,
                tool_name,
            } => {
                debug!(agent = %agent_role, tool = %tool_name, "tool_call_started");
                // Increment in-flight counter tracked via total_tool_calls at completion
                let agent_tool = inner
                    .agent_tools
                    .entry(AgentToolKey {
                        agent_role,
                        tool_name,
                    })
                    .or_default();
                // We count starts so that in-flight can be derived (starts - completions).
                agent_tool.calls += 0; // no-op here; counted at completion
            }
            MetricEvent::ToolCallCompleted {
                agent_role,
                tool_name,
                duration_ms,
                success,
            } => {
                debug!(
                    agent = %agent_role,
                    tool = %tool_name,
                    duration_ms,
                    success,
                    "tool_call_completed"
                );

                inner.total_tool_calls += 1;
                if !success {
                    inner.total_errors += 1;
                }

                // Per-agent
                let agent = inner.agents.entry(agent_role.clone()).or_default();
                agent.tool_calls += 1;
                agent.total_latency_ms += duration_ms;
                if !success {
                    agent.tool_errors += 1;
                }

                // Per-tool
                let tool = inner.tools.entry(tool_name.clone()).or_default();
                tool.calls += 1;
                tool.total_latency_ms += duration_ms;
                if !success {
                    tool.errors += 1;
                }

                // Per-(agent, tool)
                let at = inner
                    .agent_tools
                    .entry(AgentToolKey {
                        agent_role,
                        tool_name,
                    })
                    .or_default();
                at.calls += 1;
                if !success {
                    at.errors += 1;
                }
            }
            MetricEvent::TokensUsed {
                agent_role,
                input_tokens,
                output_tokens,
            } => {
                debug!(agent = %agent_role, input_tokens, output_tokens, "tokens_used");

                inner.total_input_tokens += input_tokens;
                inner.total_output_tokens += output_tokens;

                let agent = inner.agents.entry(agent_role).or_default();
                agent.input_tokens += input_tokens;
                agent.output_tokens += output_tokens;
            }
            MetricEvent::AgentStarted {
                agent_role,
                task_id,
            } => {
                debug!(agent = %agent_role, task_id = %task_id, "agent_started");
                inner.active_agents += 1;
                let agent = inner.agents.entry(agent_role).or_default();
                agent.tasks_started += 1;
            }
            MetricEvent::AgentCompleted {
                agent_role,
                task_id,
                duration_ms,
            } => {
                debug!(agent = %agent_role, task_id = %task_id, duration_ms, "agent_completed");
                inner.active_agents = inner.active_agents.saturating_sub(1);
                let agent = inner.agents.entry(agent_role).or_default();
                agent.tasks_completed += 1;
                agent.total_latency_ms += duration_ms;
            }
            MetricEvent::SecurityEvent {
                event_type,
                details,
            } => {
                debug!(event_type = ?event_type, details = %details, "security_event");
                *inner.security_events.entry(event_type.clone()).or_insert(0) += 1;
                inner.security_event_details.push((event_type, details));
            }
            MetricEvent::ComplianceCheck {
                framework,
                passed,
                details,
            } => {
                debug!(framework = %framework, passed, details = %details, "compliance_check");
                if passed {
                    inner.compliance_passed += 1;
                } else {
                    inner.compliance_failed += 1;
                }
            }
        }
    }

    /// Return a point-in-time snapshot of all metrics.
    pub fn summary(&self) -> MetricsSummary {
        let inner = self
            .inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let uptime_seconds = inner.start_time.elapsed().as_secs();

        let per_agent = inner
            .agents
            .iter()
            .map(|(role, acc)| {
                (
                    role.clone(),
                    AgentMetricsSummary {
                        tool_calls: acc.tool_calls,
                        tool_errors: acc.tool_errors,
                        input_tokens: acc.input_tokens,
                        output_tokens: acc.output_tokens,
                        total_latency_ms: acc.total_latency_ms,
                        tasks_started: acc.tasks_started,
                        tasks_completed: acc.tasks_completed,
                    },
                )
            })
            .collect();

        let per_tool = inner
            .tools
            .iter()
            .map(|(name, acc)| {
                (
                    name.clone(),
                    ToolMetricsSummary {
                        calls: acc.calls,
                        errors: acc.errors,
                        total_latency_ms: acc.total_latency_ms,
                    },
                )
            })
            .collect();

        let security_events_blocked: u64 = inner.security_events.values().sum();

        MetricsSummary {
            total_tool_calls: inner.total_tool_calls,
            total_errors: inner.total_errors,
            total_tokens: inner.total_input_tokens + inner.total_output_tokens,
            total_input_tokens: inner.total_input_tokens,
            total_output_tokens: inner.total_output_tokens,
            active_agents: inner.active_agents,
            queue_depth: inner.queue_depth,
            security_events_blocked,
            compliance_checks_passed: inner.compliance_passed,
            compliance_checks_failed: inner.compliance_failed,
            per_agent,
            per_tool,
            uptime_seconds,
        }
    }

    /// Export current metrics in the Prometheus text exposition format.
    ///
    /// The output is suitable for serving on an HTTP `/metrics` endpoint
    /// that a Prometheus server (or compatible agent) can scrape.
    pub fn prometheus_export(&self) -> String {
        let inner = self
            .inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let mut out = String::with_capacity(2048);

        // -- tool calls per (agent, tool) ------------------------------------
        out.push_str("# HELP argentor_tool_calls_total Total tool calls\n");
        out.push_str("# TYPE argentor_tool_calls_total counter\n");
        let mut sorted_at: Vec<_> = inner.agent_tools.iter().collect();
        sorted_at.sort_by(|a, b| {
            (&a.0.agent_role, &a.0.tool_name).cmp(&(&b.0.agent_role, &b.0.tool_name))
        });
        for (key, acc) in &sorted_at {
            out.push_str(&format!(
                "argentor_tool_calls_total{{agent=\"{}\",tool=\"{}\"}} {}\n",
                key.agent_role, key.tool_name, acc.calls
            ));
        }

        // -- tool errors per (agent, tool) -----------------------------------
        out.push_str("# HELP argentor_tool_errors_total Total tool call errors\n");
        out.push_str("# TYPE argentor_tool_errors_total counter\n");
        for (key, acc) in &sorted_at {
            if acc.errors > 0 {
                out.push_str(&format!(
                    "argentor_tool_errors_total{{agent=\"{}\",tool=\"{}\"}} {}\n",
                    key.agent_role, key.tool_name, acc.errors
                ));
            }
        }

        // -- tokens ----------------------------------------------------------
        out.push_str("# HELP argentor_tokens_total Total tokens consumed\n");
        out.push_str("# TYPE argentor_tokens_total counter\n");
        let mut sorted_agents: Vec<_> = inner.agents.iter().collect();
        sorted_agents.sort_by_key(|(k, _)| (*k).clone());
        for (role, acc) in &sorted_agents {
            let total = acc.input_tokens + acc.output_tokens;
            if total > 0 {
                out.push_str(&format!(
                    "argentor_tokens_total{{agent=\"{}\",direction=\"input\"}} {}\n",
                    role, acc.input_tokens
                ));
                out.push_str(&format!(
                    "argentor_tokens_total{{agent=\"{}\",direction=\"output\"}} {}\n",
                    role, acc.output_tokens
                ));
            }
        }

        // -- latency ---------------------------------------------------------
        out.push_str("# HELP argentor_tool_latency_ms_total Cumulative tool call latency in ms\n");
        out.push_str("# TYPE argentor_tool_latency_ms_total counter\n");
        let mut sorted_tools: Vec<_> = inner.tools.iter().collect();
        sorted_tools.sort_by_key(|(k, _)| (*k).clone());
        for (name, acc) in &sorted_tools {
            out.push_str(&format!(
                "argentor_tool_latency_ms_total{{tool=\"{}\"}} {}\n",
                name, acc.total_latency_ms
            ));
        }

        // -- active agents (gauge) -------------------------------------------
        out.push_str("# HELP argentor_active_agents Current number of active agents\n");
        out.push_str("# TYPE argentor_active_agents gauge\n");
        out.push_str(&format!("argentor_active_agents {}\n", inner.active_agents));

        // -- security events -------------------------------------------------
        out.push_str("# HELP argentor_security_events_total Security events blocked\n");
        out.push_str("# TYPE argentor_security_events_total counter\n");
        let mut sorted_sec: Vec<_> = inner.security_events.iter().collect();
        sorted_sec.sort_by_key(|(k, _)| k.prometheus_label());
        for (event_type, count) in &sorted_sec {
            out.push_str(&format!(
                "argentor_security_events_total{{type=\"{}\"}} {}\n",
                event_type.prometheus_label(),
                count
            ));
        }

        // -- compliance ------------------------------------------------------
        out.push_str("# HELP argentor_compliance_checks_total Compliance check results\n");
        out.push_str("# TYPE argentor_compliance_checks_total counter\n");
        out.push_str(&format!(
            "argentor_compliance_checks_total{{result=\"passed\"}} {}\n",
            inner.compliance_passed
        ));
        out.push_str(&format!(
            "argentor_compliance_checks_total{{result=\"failed\"}} {}\n",
            inner.compliance_failed
        ));

        // -- uptime ----------------------------------------------------------
        out.push_str("# HELP argentor_uptime_seconds Seconds since collector was created\n");
        out.push_str("# TYPE argentor_uptime_seconds gauge\n");
        out.push_str(&format!(
            "argentor_uptime_seconds {}\n",
            inner.start_time.elapsed().as_secs()
        ));

        out
    }

    /// Reset all counters and start the uptime clock fresh.
    pub fn reset(&self) {
        let mut inner = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *inner = Inner::new();
    }
}

// ---------------------------------------------------------------------------
// Summary types (serializable)
// ---------------------------------------------------------------------------

/// Point-in-time snapshot of all metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    /// Total number of completed tool calls across all agents.
    pub total_tool_calls: u64,
    /// Total number of failed tool calls.
    pub total_errors: u64,
    /// Total tokens consumed (input + output).
    pub total_tokens: u64,
    /// Total input (prompt) tokens consumed.
    pub total_input_tokens: u64,
    /// Total output (completion) tokens consumed.
    pub total_output_tokens: u64,
    /// Number of agents currently processing a task.
    pub active_agents: u64,
    /// Current task queue depth (reserved for orchestrator use).
    pub queue_depth: u64,
    /// Total number of security events that were blocked.
    pub security_events_blocked: u64,
    /// Number of compliance checks that passed.
    pub compliance_checks_passed: u64,
    /// Number of compliance checks that failed.
    pub compliance_checks_failed: u64,
    /// Per-agent breakdown.
    pub per_agent: HashMap<String, AgentMetricsSummary>,
    /// Per-tool breakdown.
    pub per_tool: HashMap<String, ToolMetricsSummary>,
    /// Seconds since the collector was created (or last reset).
    pub uptime_seconds: u64,
}

/// Per-agent metric summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetricsSummary {
    /// Number of completed tool calls by this agent.
    pub tool_calls: u64,
    /// Number of failed tool calls by this agent.
    pub tool_errors: u64,
    /// Input (prompt) tokens consumed by this agent.
    pub input_tokens: u64,
    /// Output (completion) tokens consumed by this agent.
    pub output_tokens: u64,
    /// Cumulative latency in milliseconds for this agent.
    pub total_latency_ms: u64,
    /// Number of tasks this agent has started.
    pub tasks_started: u64,
    /// Number of tasks this agent has completed.
    pub tasks_completed: u64,
}

/// Per-tool metric summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetricsSummary {
    /// Total number of times this tool was called.
    pub calls: u64,
    /// Number of failed invocations of this tool.
    pub errors: u64,
    /// Cumulative latency in milliseconds for this tool.
    pub total_latency_ms: u64,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn collector() -> AgentMetricsCollector {
        AgentMetricsCollector::new()
    }

    // 1. Fresh collector has zeroed summary
    #[test]
    fn test_new_collector_is_empty() {
        let c = collector();
        let s = c.summary();
        assert_eq!(s.total_tool_calls, 0);
        assert_eq!(s.total_errors, 0);
        assert_eq!(s.total_tokens, 0);
        assert_eq!(s.active_agents, 0);
        assert_eq!(s.security_events_blocked, 0);
        assert_eq!(s.compliance_checks_passed, 0);
        assert_eq!(s.compliance_checks_failed, 0);
        assert!(s.per_agent.is_empty());
        assert!(s.per_tool.is_empty());
    }

    // 2. Tool call completed updates global and per-agent/per-tool counters
    #[test]
    fn test_tool_call_completed() {
        let c = collector();
        c.record(MetricEvent::ToolCallCompleted {
            agent_role: "coder".into(),
            tool_name: "file_write".into(),
            duration_ms: 120,
            success: true,
        });
        c.record(MetricEvent::ToolCallCompleted {
            agent_role: "coder".into(),
            tool_name: "file_write".into(),
            duration_ms: 80,
            success: false,
        });

        let s = c.summary();
        assert_eq!(s.total_tool_calls, 2);
        assert_eq!(s.total_errors, 1);

        let agent = s.per_agent.get("coder").unwrap();
        assert_eq!(agent.tool_calls, 2);
        assert_eq!(agent.tool_errors, 1);
        assert_eq!(agent.total_latency_ms, 200);

        let tool = s.per_tool.get("file_write").unwrap();
        assert_eq!(tool.calls, 2);
        assert_eq!(tool.errors, 1);
        assert_eq!(tool.total_latency_ms, 200);
    }

    // 3. Token tracking
    #[test]
    fn test_tokens_used() {
        let c = collector();
        c.record(MetricEvent::TokensUsed {
            agent_role: "planner".into(),
            input_tokens: 1000,
            output_tokens: 500,
        });
        c.record(MetricEvent::TokensUsed {
            agent_role: "planner".into(),
            input_tokens: 200,
            output_tokens: 100,
        });

        let s = c.summary();
        assert_eq!(s.total_input_tokens, 1200);
        assert_eq!(s.total_output_tokens, 600);
        assert_eq!(s.total_tokens, 1800);

        let agent = s.per_agent.get("planner").unwrap();
        assert_eq!(agent.input_tokens, 1200);
        assert_eq!(agent.output_tokens, 600);
    }

    // 4. Agent lifecycle (started / completed) tracks active count
    #[test]
    fn test_agent_lifecycle() {
        let c = collector();
        c.record(MetricEvent::AgentStarted {
            agent_role: "coder".into(),
            task_id: "t1".into(),
        });
        c.record(MetricEvent::AgentStarted {
            agent_role: "tester".into(),
            task_id: "t2".into(),
        });
        assert_eq!(c.summary().active_agents, 2);

        c.record(MetricEvent::AgentCompleted {
            agent_role: "coder".into(),
            task_id: "t1".into(),
            duration_ms: 5000,
        });
        assert_eq!(c.summary().active_agents, 1);

        let agent = c.summary().per_agent.get("coder").unwrap().clone();
        assert_eq!(agent.tasks_started, 1);
        assert_eq!(agent.tasks_completed, 1);
        assert_eq!(agent.total_latency_ms, 5000);
    }

    // 5. Active agents never underflows
    #[test]
    fn test_active_agents_saturating() {
        let c = collector();
        c.record(MetricEvent::AgentCompleted {
            agent_role: "ghost".into(),
            task_id: "t0".into(),
            duration_ms: 0,
        });
        assert_eq!(c.summary().active_agents, 0);
    }

    // 6. Security events
    #[test]
    fn test_security_events() {
        let c = collector();
        c.record(MetricEvent::SecurityEvent {
            event_type: SecurityEventType::SsrfBlocked,
            details: "blocked http://169.254.169.254".into(),
        });
        c.record(MetricEvent::SecurityEvent {
            event_type: SecurityEventType::SsrfBlocked,
            details: "blocked http://10.0.0.1".into(),
        });
        c.record(MetricEvent::SecurityEvent {
            event_type: SecurityEventType::PathTraversalBlocked,
            details: "../../etc/passwd".into(),
        });

        let s = c.summary();
        assert_eq!(s.security_events_blocked, 3);
    }

    // 7. Compliance checks
    #[test]
    fn test_compliance_checks() {
        let c = collector();
        c.record(MetricEvent::ComplianceCheck {
            framework: "GDPR".into(),
            passed: true,
            details: "data minimization OK".into(),
        });
        c.record(MetricEvent::ComplianceCheck {
            framework: "ISO27001".into(),
            passed: false,
            details: "missing encryption at rest".into(),
        });
        c.record(MetricEvent::ComplianceCheck {
            framework: "DPGA".into(),
            passed: true,
            details: "open source check OK".into(),
        });

        let s = c.summary();
        assert_eq!(s.compliance_checks_passed, 2);
        assert_eq!(s.compliance_checks_failed, 1);
    }

    // 8. Reset clears everything
    #[test]
    fn test_reset() {
        let c = collector();
        c.record(MetricEvent::ToolCallCompleted {
            agent_role: "a".into(),
            tool_name: "t".into(),
            duration_ms: 10,
            success: true,
        });
        c.record(MetricEvent::SecurityEvent {
            event_type: SecurityEventType::PermissionDenied,
            details: "denied".into(),
        });
        assert_eq!(c.summary().total_tool_calls, 1);

        c.reset();
        let s = c.summary();
        assert_eq!(s.total_tool_calls, 0);
        assert_eq!(s.security_events_blocked, 0);
        assert!(s.per_agent.is_empty());
    }

    // 9. Prometheus export contains expected metric lines
    #[test]
    fn test_prometheus_export_format() {
        let c = collector();
        c.record(MetricEvent::ToolCallCompleted {
            agent_role: "coder".into(),
            tool_name: "file_write".into(),
            duration_ms: 42,
            success: true,
        });
        c.record(MetricEvent::SecurityEvent {
            event_type: SecurityEventType::SsrfBlocked,
            details: "blocked".into(),
        });

        let prom = c.prometheus_export();

        // Verify presence of expected lines
        assert!(prom.contains("# HELP argentor_tool_calls_total"));
        assert!(prom.contains("# TYPE argentor_tool_calls_total counter"));
        assert!(prom.contains("argentor_tool_calls_total{agent=\"coder\",tool=\"file_write\"} 1"));
        assert!(prom.contains("# HELP argentor_security_events_total"));
        assert!(prom.contains("argentor_security_events_total{type=\"ssrf_blocked\"} 1"));
        assert!(prom.contains("argentor_active_agents 0"));
        assert!(prom.contains("argentor_uptime_seconds"));
    }

    // 10. Clone shares state (Arc)
    #[test]
    fn test_clone_shares_state() {
        let c1 = collector();
        let c2 = c1.clone();

        c1.record(MetricEvent::ToolCallCompleted {
            agent_role: "a".into(),
            tool_name: "t".into(),
            duration_ms: 1,
            success: true,
        });

        // c2 sees the event recorded via c1
        assert_eq!(c2.summary().total_tool_calls, 1);
    }

    // 11. Summary is serializable to JSON
    #[test]
    fn test_summary_serializable() {
        let c = collector();
        c.record(MetricEvent::TokensUsed {
            agent_role: "x".into(),
            input_tokens: 10,
            output_tokens: 20,
        });

        let json = serde_json::to_string(&c.summary()).unwrap();
        assert!(json.contains("\"total_tokens\":30"));

        // Round-trip
        let deserialized: MetricsSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.total_tokens, 30);
    }

    // 12. Multiple agents and tools tracked independently
    #[test]
    fn test_multi_agent_multi_tool() {
        let c = collector();
        c.record(MetricEvent::ToolCallCompleted {
            agent_role: "coder".into(),
            tool_name: "file_write".into(),
            duration_ms: 10,
            success: true,
        });
        c.record(MetricEvent::ToolCallCompleted {
            agent_role: "tester".into(),
            tool_name: "shell".into(),
            duration_ms: 50,
            success: true,
        });
        c.record(MetricEvent::ToolCallCompleted {
            agent_role: "tester".into(),
            tool_name: "shell".into(),
            duration_ms: 30,
            success: false,
        });

        let s = c.summary();
        assert_eq!(s.total_tool_calls, 3);
        assert_eq!(s.total_errors, 1);

        let coder = s.per_agent.get("coder").unwrap();
        assert_eq!(coder.tool_calls, 1);
        assert_eq!(coder.tool_errors, 0);

        let tester = s.per_agent.get("tester").unwrap();
        assert_eq!(tester.tool_calls, 2);
        assert_eq!(tester.tool_errors, 1);

        let shell = s.per_tool.get("shell").unwrap();
        assert_eq!(shell.calls, 2);
        assert_eq!(shell.total_latency_ms, 80);
    }

    // 13. SecurityEventType prometheus labels are stable
    #[test]
    fn test_security_event_type_labels() {
        assert_eq!(
            SecurityEventType::PermissionDenied.prometheus_label(),
            "permission_denied"
        );
        assert_eq!(
            SecurityEventType::PathTraversalBlocked.prometheus_label(),
            "path_traversal_blocked"
        );
        assert_eq!(
            SecurityEventType::SsrfBlocked.prometheus_label(),
            "ssrf_blocked"
        );
        assert_eq!(
            SecurityEventType::ShellInjectionBlocked.prometheus_label(),
            "shell_injection_blocked"
        );
        assert_eq!(
            SecurityEventType::RateLimitExceeded.prometheus_label(),
            "rate_limit_exceeded"
        );
        assert_eq!(
            SecurityEventType::AuthenticationFailed.prometheus_label(),
            "authentication_failed"
        );
        assert_eq!(
            SecurityEventType::InputSanitized.prometheus_label(),
            "input_sanitized"
        );
    }

    // 14. ToolCallStarted does not inflate counters
    #[test]
    fn test_tool_call_started_does_not_count() {
        let c = collector();
        c.record(MetricEvent::ToolCallStarted {
            agent_role: "a".into(),
            tool_name: "t".into(),
        });
        let s = c.summary();
        assert_eq!(s.total_tool_calls, 0);
        assert_eq!(s.total_errors, 0);
    }
}
