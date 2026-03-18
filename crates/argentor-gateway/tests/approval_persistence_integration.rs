#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Integration tests for WsApprovalChannel and PersistentStore + ControlPlaneState.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use argentor_core::approval::{ApprovalChannel, ApprovalDecision, ApprovalRequest, RiskLevel};
use argentor_gateway::connection::ConnectionManager;
use argentor_gateway::control_plane::{
    AgentDefinitionInfo, AgentHealthInfo, ControlPlaneEvent, ControlPlaneState, DeploymentInfo,
};
use argentor_gateway::persistence::{
    load_control_plane_state, save_control_plane_state, ControlPlaneSnapshot,
};
use argentor_gateway::{PersistentStore, WsApprovalChannel};
use chrono::Utc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Create a unique temporary directory for a test.
fn test_dir(label: &str) -> PathBuf {
    let id = Uuid::new_v4();
    std::env::temp_dir().join(format!("argentor_integ_{label}_{id}"))
}

/// Build a sample `DeploymentInfo` with sensible defaults.
fn sample_deployment(name: &str) -> DeploymentInfo {
    let id = Uuid::new_v4();
    DeploymentInfo {
        id,
        name: name.into(),
        role: "coder".into(),
        replicas: 1,
        status: "running".into(),
        auto_restart: true,
        instances: vec![],
        created_at: Utc::now(),
        updated_at: Utc::now(),
        total_tasks: 0,
        total_errors: 0,
        tags: HashMap::new(),
    }
}

/// Build a sample `AgentDefinitionInfo`.
fn sample_agent_definition(name: &str) -> AgentDefinitionInfo {
    let id = Uuid::new_v4();
    AgentDefinitionInfo {
        id,
        name: name.into(),
        role: "tester".into(),
        version: "0.1.0".into(),
        description: format!("Agent definition: {name}"),
        capabilities: vec!["test".into()],
        tags: HashMap::new(),
        created_at: Utc::now(),
    }
}

/// Build a sample `AgentHealthInfo`.
fn sample_health(agent_name: &str) -> AgentHealthInfo {
    let agent_id = Uuid::new_v4();
    AgentHealthInfo {
        agent_id,
        agent_name: agent_name.into(),
        status: "healthy".into(),
        last_heartbeat: Some(Utc::now()),
        restart_count: 0,
        uptime_secs: 3600,
        probes: vec![],
    }
}

/// Build a sample `ControlPlaneEvent`.
fn sample_event(message: &str) -> ControlPlaneEvent {
    ControlPlaneEvent {
        id: Uuid::new_v4(),
        timestamp: Utc::now(),
        event_type: "deployed".into(),
        deployment_id: Some(Uuid::new_v4()),
        message: message.into(),
    }
}

/// Build a sample `ApprovalRequest`.
fn sample_approval_request(task_id: &str) -> ApprovalRequest {
    ApprovalRequest {
        task_id: task_id.into(),
        description: format!("Approval for {task_id}"),
        risk_level: RiskLevel::Medium,
        context: "test context".into(),
    }
}

// ===========================================================================
// Part 1: WsApprovalChannel Tests
// ===========================================================================

#[tokio::test]
async fn ws_approval_channel_creation() {
    let connections = ConnectionManager::new();
    let channel = WsApprovalChannel::new(connections, Duration::from_secs(60));
    assert_eq!(channel.pending_count().await, 0);
}

#[tokio::test]
async fn ws_approval_channel_default_timeout_creation() {
    let connections = ConnectionManager::new();
    let channel = WsApprovalChannel::default_timeout(connections);
    // Channel created with default 5-minute timeout — no pending requests.
    assert_eq!(channel.pending_count().await, 0);
}

#[tokio::test]
async fn ws_approval_pending_requests_tracked() {
    let connections = ConnectionManager::new();
    let channel = Arc::new(WsApprovalChannel::new(connections, Duration::from_secs(10)));

    let request = sample_approval_request("track-1");
    let ch = channel.clone();
    let _handle = tokio::spawn(async move { ch.request_approval(request).await });

    // Wait briefly for the spawned task to register the pending request.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(channel.pending_count().await, 1);
}

#[tokio::test]
async fn ws_approval_approve_pending_request() {
    let connections = ConnectionManager::new();
    let channel = Arc::new(WsApprovalChannel::new(connections, Duration::from_secs(10)));

    let request = sample_approval_request("approve-1");
    let ch = channel.clone();
    let handle = tokio::spawn(async move { ch.request_approval(request).await });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Approve the pending request.
    channel
        .handle_approval_response(
            "approve-1",
            ApprovalDecision {
                approved: true,
                reason: Some("LGTM".into()),
                reviewer: "admin".into(),
            },
        )
        .await;

    let result = handle.await.unwrap().unwrap();
    assert!(result.approved);
    assert_eq!(result.reviewer, "admin");
    assert_eq!(result.reason.as_deref(), Some("LGTM"));
    assert_eq!(channel.pending_count().await, 0);
}

#[tokio::test]
async fn ws_approval_deny_pending_request() {
    let connections = ConnectionManager::new();
    let channel = Arc::new(WsApprovalChannel::new(connections, Duration::from_secs(10)));

    let request = sample_approval_request("deny-1");
    let ch = channel.clone();
    let handle = tokio::spawn(async move { ch.request_approval(request).await });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Deny the pending request.
    channel
        .handle_approval_response(
            "deny-1",
            ApprovalDecision {
                approved: false,
                reason: Some("Too risky".into()),
                reviewer: "security-lead".into(),
            },
        )
        .await;

    let result = handle.await.unwrap().unwrap();
    assert!(!result.approved);
    assert_eq!(result.reviewer, "security-lead");
    assert_eq!(result.reason.as_deref(), Some("Too risky"));
    assert_eq!(channel.pending_count().await, 0);
}

#[tokio::test]
async fn ws_approval_unknown_task_id_ignored() {
    let connections = ConnectionManager::new();
    let channel = WsApprovalChannel::new(connections, Duration::from_secs(5));

    // Responding to a non-existent task ID should not panic.
    channel
        .handle_approval_response(
            "nonexistent-task",
            ApprovalDecision {
                approved: true,
                reason: None,
                reviewer: "nobody".into(),
            },
        )
        .await;

    assert_eq!(channel.pending_count().await, 0);
}

#[tokio::test]
async fn ws_approval_timeout_returns_denied() {
    let connections = ConnectionManager::new();
    let channel = WsApprovalChannel::new(connections, Duration::from_millis(100));

    let request = sample_approval_request("timeout-1");
    let result = channel.request_approval(request).await.unwrap();

    assert!(!result.approved);
    assert_eq!(result.reviewer, "system");
    assert!(result.reason.unwrap().contains("Timed out"));
    assert_eq!(channel.pending_count().await, 0);
}

#[tokio::test]
async fn ws_approval_multiple_concurrent_requests() {
    let connections = ConnectionManager::new();
    let channel = Arc::new(WsApprovalChannel::new(connections, Duration::from_secs(10)));

    // Spawn 3 concurrent approval requests.
    let mut handles = Vec::new();
    for i in 0..3 {
        let ch = channel.clone();
        let req = sample_approval_request(&format!("multi-{i}"));
        handles.push(tokio::spawn(async move { ch.request_approval(req).await }));
    }

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(channel.pending_count().await, 3);

    // Approve first, deny second, approve third.
    channel
        .handle_approval_response(
            "multi-0",
            ApprovalDecision {
                approved: true,
                reason: None,
                reviewer: "r0".into(),
            },
        )
        .await;
    channel
        .handle_approval_response(
            "multi-1",
            ApprovalDecision {
                approved: false,
                reason: Some("nope".into()),
                reviewer: "r1".into(),
            },
        )
        .await;
    channel
        .handle_approval_response(
            "multi-2",
            ApprovalDecision {
                approved: true,
                reason: None,
                reviewer: "r2".into(),
            },
        )
        .await;

    let results: Vec<_> = futures_util::future::join_all(handles)
        .await
        .into_iter()
        .map(|h| h.unwrap().unwrap())
        .collect();

    assert!(results[0].approved);
    assert!(!results[1].approved);
    assert!(results[2].approved);
    assert_eq!(channel.pending_count().await, 0);
}

#[tokio::test]
async fn ws_approval_pending_cleaned_after_timeout() {
    let connections = ConnectionManager::new();
    let channel = Arc::new(WsApprovalChannel::new(
        connections,
        Duration::from_millis(80),
    ));

    let ch = channel.clone();
    let req = sample_approval_request("clean-timeout");
    let handle = tokio::spawn(async move { ch.request_approval(req).await });

    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(channel.pending_count().await, 1);

    // Let it time out.
    let result = handle.await.unwrap().unwrap();
    assert!(!result.approved);
    // After timeout, pending map is cleaned up.
    assert_eq!(channel.pending_count().await, 0);
}

// ===========================================================================
// Part 2: PersistentStore + ControlPlaneState Integration Tests
// ===========================================================================

#[tokio::test]
async fn persistence_save_and_load_roundtrip() {
    let dir = test_dir("roundtrip");
    let store = PersistentStore::new(&dir).unwrap();
    let state = ControlPlaneState::new();

    // Populate state.
    let dep = sample_deployment("roundtrip-deploy");
    state.deployments.write().await.insert(dep.id, dep);

    let def = sample_agent_definition("roundtrip-agent");
    state.agent_definitions.write().await.insert(def.id, def);

    // Save and reload into a fresh state.
    save_control_plane_state(&store, &state).await.unwrap();

    let state2 = ControlPlaneState::new();
    let loaded = load_control_plane_state(&store, &state2).await.unwrap();
    assert!(loaded);

    let deployments = state2.deployments.read().await;
    assert_eq!(deployments.len(), 1);
    let dep = deployments.values().next().unwrap();
    assert_eq!(dep.name, "roundtrip-deploy");

    let defs = state2.agent_definitions.read().await;
    assert_eq!(defs.len(), 1);
    let def = defs.values().next().unwrap();
    assert_eq!(def.name, "roundtrip-agent");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn persistence_empty_state_roundtrip() {
    let dir = test_dir("empty_state");
    let store = PersistentStore::new(&dir).unwrap();
    let state = ControlPlaneState::new();

    // Save empty state.
    save_control_plane_state(&store, &state).await.unwrap();

    let state2 = ControlPlaneState::new();
    let loaded = load_control_plane_state(&store, &state2).await.unwrap();
    assert!(loaded);

    assert!(state2.deployments.read().await.is_empty());
    assert!(state2.agent_definitions.read().await.is_empty());
    assert!(state2.health_states.read().await.is_empty());
    assert!(state2.events.read().await.is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn persistence_state_with_deployments() {
    let dir = test_dir("with_deployments");
    let store = PersistentStore::new(&dir).unwrap();
    let state = ControlPlaneState::new();

    // Add multiple deployments.
    {
        let mut deployments = state.deployments.write().await;
        for name in &["alpha", "bravo", "charlie"] {
            let dep = sample_deployment(name);
            deployments.insert(dep.id, dep);
        }
    }

    save_control_plane_state(&store, &state).await.unwrap();

    let state2 = ControlPlaneState::new();
    load_control_plane_state(&store, &state2).await.unwrap();

    let deployments = state2.deployments.read().await;
    assert_eq!(deployments.len(), 3);

    let mut names: Vec<String> = deployments.values().map(|d| d.name.clone()).collect();
    names.sort();
    assert_eq!(names, vec!["alpha", "bravo", "charlie"]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn persistence_state_with_agent_definitions() {
    let dir = test_dir("with_agent_defs");
    let store = PersistentStore::new(&dir).unwrap();
    let state = ControlPlaneState::new();

    {
        let mut defs = state.agent_definitions.write().await;
        let def1 = sample_agent_definition("agent-a");
        let def2 = sample_agent_definition("agent-b");
        defs.insert(def1.id, def1);
        defs.insert(def2.id, def2);
    }

    save_control_plane_state(&store, &state).await.unwrap();

    let state2 = ControlPlaneState::new();
    load_control_plane_state(&store, &state2).await.unwrap();

    let defs = state2.agent_definitions.read().await;
    assert_eq!(defs.len(), 2);

    let mut names: Vec<String> = defs.values().map(|d| d.name.clone()).collect();
    names.sort();
    assert_eq!(names, vec!["agent-a", "agent-b"]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn persistence_state_with_health_data() {
    let dir = test_dir("with_health");
    let store = PersistentStore::new(&dir).unwrap();
    let state = ControlPlaneState::new();

    {
        let mut healths = state.health_states.write().await;
        let h = sample_health("health-agent");
        healths.insert(h.agent_id, h);
    }

    save_control_plane_state(&store, &state).await.unwrap();

    let state2 = ControlPlaneState::new();
    load_control_plane_state(&store, &state2).await.unwrap();

    let healths = state2.health_states.read().await;
    assert_eq!(healths.len(), 1);
    let h = healths.values().next().unwrap();
    assert_eq!(h.agent_name, "health-agent");
    assert_eq!(h.status, "healthy");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn persistence_events_truncated_to_100() {
    let dir = test_dir("events_truncated");
    let store = PersistentStore::new(&dir).unwrap();
    let state = ControlPlaneState::new();

    // Add 150 events — only the last 100 should be persisted.
    {
        let mut events = state.events.write().await;
        for i in 0..150 {
            events.push(sample_event(&format!("event-{i}")));
        }
    }

    save_control_plane_state(&store, &state).await.unwrap();

    let state2 = ControlPlaneState::new();
    load_control_plane_state(&store, &state2).await.unwrap();

    let events = state2.events.read().await;
    assert_eq!(events.len(), 100);

    // The first persisted event should be event-50 (skipped 0..49).
    assert_eq!(events[0].message, "event-50");
    assert_eq!(events[99].message, "event-149");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn persistence_load_populates_hashmaps_correctly() {
    let dir = test_dir("populate_hashmaps");
    let store = PersistentStore::new(&dir).unwrap();
    let state = ControlPlaneState::new();

    let dep = sample_deployment("pop-deploy");
    let dep_id = dep.id;
    state.deployments.write().await.insert(dep.id, dep);

    let def = sample_agent_definition("pop-agent");
    let def_id = def.id;
    state.agent_definitions.write().await.insert(def.id, def);

    let health = sample_health("pop-health");
    let health_agent_id = health.agent_id;
    state
        .health_states
        .write()
        .await
        .insert(health.agent_id, health);

    save_control_plane_state(&store, &state).await.unwrap();

    let state2 = ControlPlaneState::new();
    load_control_plane_state(&store, &state2).await.unwrap();

    // Verify the HashMap keys match the original IDs.
    assert!(state2.deployments.read().await.contains_key(&dep_id));
    assert!(state2.agent_definitions.read().await.contains_key(&def_id));
    assert!(
        state2
            .health_states
            .read()
            .await
            .contains_key(&health_agent_id)
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn persistence_multiple_save_load_cycles() {
    let dir = test_dir("multi_cycle");
    let store = PersistentStore::new(&dir).unwrap();
    let state = ControlPlaneState::new();

    // Cycle 1: save 1 deployment.
    let dep1 = sample_deployment("cycle-1");
    state.deployments.write().await.insert(dep1.id, dep1);
    save_control_plane_state(&store, &state).await.unwrap();

    // Cycle 2: add another deployment, save again.
    let dep2 = sample_deployment("cycle-2");
    state.deployments.write().await.insert(dep2.id, dep2);
    save_control_plane_state(&store, &state).await.unwrap();

    // Load into fresh state — should have 2 deployments from the latest save.
    let state2 = ControlPlaneState::new();
    load_control_plane_state(&store, &state2).await.unwrap();

    let deployments = state2.deployments.read().await;
    assert_eq!(deployments.len(), 2);

    let mut names: Vec<String> = deployments.values().map(|d| d.name.clone()).collect();
    names.sort();
    assert_eq!(names, vec!["cycle-1", "cycle-2"]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn persistence_load_returns_false_when_no_file() {
    let dir = test_dir("no_file");
    let store = PersistentStore::new(&dir).unwrap();
    let state = ControlPlaneState::new();

    let loaded = load_control_plane_state(&store, &state).await.unwrap();
    assert!(!loaded);

    // State should remain empty.
    assert!(state.deployments.read().await.is_empty());
    assert!(state.agent_definitions.read().await.is_empty());
    assert!(state.health_states.read().await.is_empty());
    assert!(state.events.read().await.is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn persistence_save_creates_file_in_directory() {
    let dir = test_dir("creates_file");
    let store = PersistentStore::new(&dir).unwrap();
    let state = ControlPlaneState::new();

    save_control_plane_state(&store, &state).await.unwrap();

    // The snapshot file should exist in the data directory.
    let snapshot_path = dir.join("control_plane.json");
    assert!(snapshot_path.exists());

    // Verify it is valid JSON.
    let contents = std::fs::read_to_string(&snapshot_path).unwrap();
    let parsed: ControlPlaneSnapshot = serde_json::from_str(&contents).unwrap();
    assert_eq!(parsed.version, 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn persistence_full_state_roundtrip() {
    let dir = test_dir("full_roundtrip");
    let store = PersistentStore::new(&dir).unwrap();
    let state = ControlPlaneState::new();

    // Populate all four collections.
    let dep = sample_deployment("full-deploy");
    state.deployments.write().await.insert(dep.id, dep);

    let def = sample_agent_definition("full-agent");
    state.agent_definitions.write().await.insert(def.id, def);

    let health = sample_health("full-health");
    state
        .health_states
        .write()
        .await
        .insert(health.agent_id, health);

    state
        .events
        .write()
        .await
        .push(sample_event("full-event"));

    save_control_plane_state(&store, &state).await.unwrap();

    let state2 = ControlPlaneState::new();
    let loaded = load_control_plane_state(&store, &state2).await.unwrap();
    assert!(loaded);

    assert_eq!(state2.deployments.read().await.len(), 1);
    assert_eq!(state2.agent_definitions.read().await.len(), 1);
    assert_eq!(state2.health_states.read().await.len(), 1);
    assert_eq!(state2.events.read().await.len(), 1);

    let events = state2.events.read().await;
    assert_eq!(events[0].message, "full-event");

    let _ = std::fs::remove_dir_all(&dir);
}
