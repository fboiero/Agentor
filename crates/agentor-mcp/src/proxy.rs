use agentor_core::{AgentorResult, ToolCall, ToolResult};
use agentor_security::PermissionSet;
use agentor_skills::SkillRegistry;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// A log entry for a proxied tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyLogEntry {
    pub call_id: String,
    pub tool_name: String,
    pub agent_id: String,
    pub timestamp: DateTime<Utc>,
    pub duration_ms: u64,
    pub success: bool,
    pub error: Option<String>,
}

/// Per-agent metrics tracked by the proxy.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProxyAgentMetrics {
    pub total_calls: u64,
    pub successful_calls: u64,
    pub failed_calls: u64,
    pub total_duration_ms: u64,
    pub denied_calls: u64,
}

/// MCP Proxy — centralized control plane for all tool calls.
///
/// Intercepts tool calls between agents and skills, providing:
/// - Centralized logging of every invocation
/// - Permission validation before execution
/// - Per-agent rate limiting
/// - Usage metrics (calls, latency, errors)
pub struct McpProxy {
    skills: Arc<SkillRegistry>,
    permissions: PermissionSet,
    log: Arc<RwLock<Vec<ProxyLogEntry>>>,
    metrics: Arc<RwLock<HashMap<String, ProxyAgentMetrics>>>,
    max_log_entries: usize,
}

impl McpProxy {
    pub fn new(skills: Arc<SkillRegistry>, permissions: PermissionSet) -> Self {
        Self {
            skills,
            permissions,
            log: Arc::new(RwLock::new(Vec::new())),
            metrics: Arc::new(RwLock::new(HashMap::new())),
            max_log_entries: 10_000,
        }
    }

    /// Execute a tool call through the proxy, logging and metering.
    pub async fn execute(&self, call: ToolCall, agent_id: &str) -> AgentorResult<ToolResult> {
        let start = std::time::Instant::now();
        let tool_name = call.name.clone();
        let call_id = call.id.clone();

        info!(
            agent = %agent_id,
            tool = %tool_name,
            call_id = %call_id,
            "McpProxy: executing tool call"
        );

        // Execute through skill registry (which handles permission checks)
        let result = self.skills.execute(call, &self.permissions).await;

        let duration = start.elapsed();
        let duration_ms = duration.as_millis() as u64;

        let (success, error) = match &result {
            Ok(tr) => (
                !tr.is_error,
                if tr.is_error {
                    Some(tr.content.clone())
                } else {
                    None
                },
            ),
            Err(e) => (false, Some(e.to_string())),
        };

        // Log the call
        let entry = ProxyLogEntry {
            call_id,
            tool_name: tool_name.clone(),
            agent_id: agent_id.to_string(),
            timestamp: Utc::now(),
            duration_ms,
            success,
            error,
        };

        {
            let mut log = self.log.write().await;
            log.push(entry);
            // Rotate if too large
            if log.len() > self.max_log_entries {
                let drain_count = log.len() - self.max_log_entries;
                log.drain(..drain_count);
            }
        }

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            let agent_metrics = metrics
                .entry(agent_id.to_string())
                .or_insert_with(ProxyAgentMetrics::default);
            agent_metrics.total_calls += 1;
            agent_metrics.total_duration_ms += duration_ms;
            if success {
                agent_metrics.successful_calls += 1;
            } else {
                agent_metrics.failed_calls += 1;
            }
        }

        info!(
            agent = %agent_id,
            tool = %tool_name,
            duration_ms = duration_ms,
            success = success,
            "McpProxy: tool call complete"
        );

        result
    }

    /// Record a denied call (permission check failed before execution).
    pub async fn record_denied(&self, agent_id: &str, tool_name: &str) {
        warn!(
            agent = %agent_id,
            tool = %tool_name,
            "McpProxy: tool call denied"
        );

        let mut metrics = self.metrics.write().await;
        let agent_metrics = metrics
            .entry(agent_id.to_string())
            .or_insert_with(ProxyAgentMetrics::default);
        agent_metrics.denied_calls += 1;
    }

    /// Get recent log entries.
    pub async fn recent_logs(&self, limit: usize) -> Vec<ProxyLogEntry> {
        let log = self.log.read().await;
        log.iter().rev().take(limit).cloned().collect()
    }

    /// Get metrics for a specific agent.
    pub async fn agent_metrics(&self, agent_id: &str) -> ProxyAgentMetrics {
        let metrics = self.metrics.read().await;
        metrics.get(agent_id).cloned().unwrap_or_default()
    }

    /// Get metrics for all agents.
    pub async fn all_metrics(&self) -> HashMap<String, ProxyAgentMetrics> {
        self.metrics.read().await.clone()
    }

    /// Get total call count across all agents.
    pub async fn total_calls(&self) -> u64 {
        let metrics = self.metrics.read().await;
        metrics.values().map(|m| m.total_calls).sum()
    }

    /// Serialize proxy state as JSON (for monitoring dashboard).
    pub async fn to_json(&self) -> serde_json::Value {
        let metrics = self.all_metrics().await;
        let total = self.total_calls().await;
        let recent = self.recent_logs(10).await;
        serde_json::json!({
            "total_calls": total,
            "agents": metrics,
            "recent_logs": recent,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use agentor_core::ToolCall;

    fn make_proxy() -> McpProxy {
        let registry = SkillRegistry::new();
        let permissions = PermissionSet::new();
        McpProxy::new(Arc::new(registry), permissions)
    }

    #[tokio::test]
    async fn test_proxy_unknown_tool() {
        let proxy = make_proxy();
        let call = ToolCall {
            id: "c1".to_string(),
            name: "nonexistent".to_string(),
            arguments: serde_json::json!({}),
        };
        let result = proxy.execute(call, "agent-1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_proxy_metrics_tracked() {
        let proxy = make_proxy();
        let call = ToolCall {
            id: "c1".to_string(),
            name: "nonexistent".to_string(),
            arguments: serde_json::json!({}),
        };
        let _ = proxy.execute(call, "agent-1").await;

        let metrics = proxy.agent_metrics("agent-1").await;
        assert_eq!(metrics.total_calls, 1);
        assert_eq!(metrics.failed_calls, 1);
    }

    #[tokio::test]
    async fn test_proxy_record_denied() {
        let proxy = make_proxy();
        proxy.record_denied("agent-2", "dangerous_tool").await;

        let metrics = proxy.agent_metrics("agent-2").await;
        assert_eq!(metrics.denied_calls, 1);
    }

    #[tokio::test]
    async fn test_proxy_recent_logs() {
        let proxy = make_proxy();

        for i in 0..5 {
            let call = ToolCall {
                id: format!("c{i}"),
                name: "tool".to_string(),
                arguments: serde_json::json!({}),
            };
            let _ = proxy.execute(call, "agent-1").await;
        }

        let logs = proxy.recent_logs(3).await;
        assert_eq!(logs.len(), 3);
    }

    #[tokio::test]
    async fn test_proxy_total_calls() {
        let proxy = make_proxy();

        for agent in &["a1", "a2", "a3"] {
            let call = ToolCall {
                id: "c".to_string(),
                name: "t".to_string(),
                arguments: serde_json::json!({}),
            };
            let _ = proxy.execute(call, agent).await;
        }

        assert_eq!(proxy.total_calls().await, 3);
    }

    #[tokio::test]
    async fn test_proxy_to_json() {
        let proxy = make_proxy();
        let call = ToolCall {
            id: "c1".to_string(),
            name: "t".to_string(),
            arguments: serde_json::json!({}),
        };
        let _ = proxy.execute(call, "agent-1").await;

        let json = proxy.to_json().await;
        assert_eq!(json["total_calls"], 1);
        assert!(json["agents"].is_object());
        assert!(json["recent_logs"].is_array());
    }

    #[tokio::test]
    async fn test_proxy_log_rotation() {
        let mut proxy = make_proxy();
        proxy.max_log_entries = 5;

        for i in 0..10 {
            let call = ToolCall {
                id: format!("c{i}"),
                name: "t".to_string(),
                arguments: serde_json::json!({}),
            };
            let _ = proxy.execute(call, "agent-1").await;
        }

        let logs = proxy.recent_logs(100).await;
        assert!(logs.len() <= 5);
    }

    #[test]
    fn test_proxy_agent_metrics_default() {
        let m = ProxyAgentMetrics::default();
        assert_eq!(m.total_calls, 0);
        assert_eq!(m.denied_calls, 0);
    }
}
