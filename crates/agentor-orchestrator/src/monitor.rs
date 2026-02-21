use crate::types::{AgentMetrics, AgentRole, AgentState, WorkerStatus};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Tracks state and metrics for all agents in the multi-agent system.
pub struct AgentMonitor {
    states: Arc<RwLock<HashMap<AgentRole, AgentState>>>,
}

impl AgentMonitor {
    pub fn new() -> Self {
        let mut states = HashMap::new();
        for role in &[
            AgentRole::Orchestrator,
            AgentRole::Spec,
            AgentRole::Coder,
            AgentRole::Tester,
            AgentRole::Reviewer,
        ] {
            states.insert(
                *role,
                AgentState {
                    role: *role,
                    current_task: None,
                    status: WorkerStatus::Idle,
                    metrics: AgentMetrics::default(),
                },
            );
        }
        Self {
            states: Arc::new(RwLock::new(states)),
        }
    }

    /// Mark an agent as working on a task.
    pub async fn start_task(&self, role: AgentRole, task_id: Uuid) {
        let mut states = self.states.write().await;
        if let Some(state) = states.get_mut(&role) {
            state.current_task = Some(task_id);
            state.status = WorkerStatus::Working;
        }
    }

    /// Mark an agent as idle (task completed or failed).
    pub async fn finish_task(&self, role: AgentRole) {
        let mut states = self.states.write().await;
        if let Some(state) = states.get_mut(&role) {
            state.current_task = None;
            state.status = WorkerStatus::Idle;
        }
    }

    /// Mark an agent as waiting for human approval.
    pub async fn waiting_for_approval(&self, role: AgentRole) {
        let mut states = self.states.write().await;
        if let Some(state) = states.get_mut(&role) {
            state.status = WorkerStatus::WaitingForApproval;
        }
    }

    /// Record an error for an agent.
    pub async fn record_error(&self, role: AgentRole) {
        let mut states = self.states.write().await;
        if let Some(state) = states.get_mut(&role) {
            state.metrics.errors += 1;
            state.status = WorkerStatus::Error;
        }
    }

    /// Record metrics for a completed turn.
    pub async fn record_turn(&self, role: AgentRole, tool_calls: u32, tokens: u64) {
        let mut states = self.states.write().await;
        if let Some(state) = states.get_mut(&role) {
            state.metrics.total_turns += 1;
            state.metrics.total_tool_calls += tool_calls;
            state.metrics.tokens_used += tokens;
        }
    }

    /// Record execution duration for an agent.
    pub async fn record_duration(&self, role: AgentRole, duration_ms: u64) {
        let mut states = self.states.write().await;
        if let Some(state) = states.get_mut(&role) {
            state.metrics.duration_ms += duration_ms;
        }
    }

    /// Get a snapshot of all agent states.
    pub async fn snapshot(&self) -> Vec<AgentState> {
        let states = self.states.read().await;
        states.values().cloned().collect()
    }

    /// Get the state of a specific agent.
    pub async fn get_state(&self, role: AgentRole) -> Option<AgentState> {
        let states = self.states.read().await;
        states.get(&role).cloned()
    }

    /// Get aggregate metrics across all agents.
    pub async fn aggregate_metrics(&self) -> AgentMetrics {
        let states = self.states.read().await;
        let mut total = AgentMetrics::default();
        for state in states.values() {
            total.total_turns += state.metrics.total_turns;
            total.total_tool_calls += state.metrics.total_tool_calls;
            total.errors += state.metrics.errors;
            total.duration_ms += state.metrics.duration_ms;
            total.tokens_used += state.metrics.tokens_used;
        }
        total
    }

    /// Serialize the current state as JSON (for WebSocket dashboard).
    pub async fn to_json(&self) -> serde_json::Value {
        let states = self.snapshot().await;
        let aggregate = self.aggregate_metrics().await;
        serde_json::json!({
            "agents": states,
            "aggregate": aggregate,
        })
    }
}

impl Default for AgentMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_initial_state() {
        let monitor = AgentMonitor::new();
        let states = monitor.snapshot().await;
        assert_eq!(states.len(), 5);
        for state in &states {
            assert_eq!(state.status, WorkerStatus::Idle);
            assert!(state.current_task.is_none());
        }
    }

    #[tokio::test]
    async fn test_start_and_finish_task() {
        let monitor = AgentMonitor::new();
        let task_id = Uuid::new_v4();

        monitor.start_task(AgentRole::Coder, task_id).await;
        let state = monitor.get_state(AgentRole::Coder).await.unwrap();
        assert_eq!(state.status, WorkerStatus::Working);
        assert_eq!(state.current_task, Some(task_id));

        monitor.finish_task(AgentRole::Coder).await;
        let state = monitor.get_state(AgentRole::Coder).await.unwrap();
        assert_eq!(state.status, WorkerStatus::Idle);
        assert!(state.current_task.is_none());
    }

    #[tokio::test]
    async fn test_record_metrics() {
        let monitor = AgentMonitor::new();
        monitor.record_turn(AgentRole::Coder, 3, 1500).await;
        monitor.record_turn(AgentRole::Coder, 2, 1000).await;
        monitor.record_duration(AgentRole::Coder, 5000).await;

        let state = monitor.get_state(AgentRole::Coder).await.unwrap();
        assert_eq!(state.metrics.total_turns, 2);
        assert_eq!(state.metrics.total_tool_calls, 5);
        assert_eq!(state.metrics.tokens_used, 2500);
        assert_eq!(state.metrics.duration_ms, 5000);
    }

    #[tokio::test]
    async fn test_record_error() {
        let monitor = AgentMonitor::new();
        monitor.record_error(AgentRole::Tester).await;
        let state = monitor.get_state(AgentRole::Tester).await.unwrap();
        assert_eq!(state.metrics.errors, 1);
        assert_eq!(state.status, WorkerStatus::Error);
    }

    #[tokio::test]
    async fn test_waiting_for_approval() {
        let monitor = AgentMonitor::new();
        monitor.waiting_for_approval(AgentRole::Reviewer).await;
        let state = monitor.get_state(AgentRole::Reviewer).await.unwrap();
        assert_eq!(state.status, WorkerStatus::WaitingForApproval);
    }

    #[tokio::test]
    async fn test_aggregate_metrics() {
        let monitor = AgentMonitor::new();
        monitor.record_turn(AgentRole::Coder, 3, 1000).await;
        monitor.record_turn(AgentRole::Tester, 2, 500).await;
        monitor.record_error(AgentRole::Tester).await;

        let agg = monitor.aggregate_metrics().await;
        assert_eq!(agg.total_turns, 2);
        assert_eq!(agg.total_tool_calls, 5);
        assert_eq!(agg.tokens_used, 1500);
        assert_eq!(agg.errors, 1);
    }

    #[tokio::test]
    async fn test_to_json() {
        let monitor = AgentMonitor::new();
        monitor.record_turn(AgentRole::Spec, 1, 200).await;
        let json = monitor.to_json().await;
        assert!(json["agents"].is_array());
        assert!(json["aggregate"].is_object());
    }
}
