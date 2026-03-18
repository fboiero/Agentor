//! REST API endpoints for the orchestrator control plane.
//!
//! Provides HTTP endpoints for deploying, monitoring, scaling, and managing
//! agents. All routes are mounted under `/api/v1/control-plane/`.

use axum::{
    extract::{Json, Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Valid roles
// ---------------------------------------------------------------------------

/// Known agent roles that match `AgentRole` variants in the orchestrator.
const VALID_ROLES: &[&str] = &[
    "orchestrator",
    "spec",
    "coder",
    "tester",
    "reviewer",
    "architect",
    "security_auditor",
    "devops",
    "document_writer",
];

/// Returns `true` if the role string is a known built-in role or a
/// `custom:<name>` role.
fn is_valid_role(role: &str) -> bool {
    VALID_ROLES.contains(&role) || role.starts_with("custom:")
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Unified error type for control-plane handlers.
#[derive(Debug)]
pub enum ControlPlaneError {
    /// The requested resource was not found.
    NotFound(String),
    /// The request body or parameters were invalid.
    BadRequest(String),
    /// A resource already exists with the given identifier.
    Conflict(String),
    /// An internal error occurred while processing the request.
    Internal(String),
}

impl std::fmt::Display for ControlPlaneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "Not found: {msg}"),
            Self::BadRequest(msg) => write!(f, "Bad request: {msg}"),
            Self::Conflict(msg) => write!(f, "Conflict: {msg}"),
            Self::Internal(msg) => write!(f, "Internal error: {msg}"),
        }
    }
}

impl IntoResponse for ControlPlaneError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match &self {
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            Self::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            Self::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
        };

        let body = serde_json::json!({ "error": message });
        (status, Json(body)).into_response()
    }
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

/// Central state for the control plane API.
///
/// All mutable collections are behind `RwLock` so handlers can read/write
/// concurrently.
pub struct ControlPlaneState {
    /// Active deployments keyed by deployment ID.
    pub deployments: Arc<RwLock<HashMap<Uuid, DeploymentInfo>>>,
    /// Registered agent definitions keyed by agent ID.
    pub agent_definitions: Arc<RwLock<HashMap<Uuid, AgentDefinitionInfo>>>,
    /// Health state per agent, keyed by agent ID.
    pub health_states: Arc<RwLock<HashMap<Uuid, AgentHealthInfo>>>,
    /// Chronological list of control-plane events.
    pub events: Arc<RwLock<Vec<ControlPlaneEvent>>>,
    /// When the control plane was started.
    pub started_at: DateTime<Utc>,
}

impl ControlPlaneState {
    /// Create a new, empty control-plane state.
    pub fn new() -> Self {
        Self {
            deployments: Arc::new(RwLock::new(HashMap::new())),
            agent_definitions: Arc::new(RwLock::new(HashMap::new())),
            health_states: Arc::new(RwLock::new(HashMap::new())),
            events: Arc::new(RwLock::new(Vec::new())),
            started_at: Utc::now(),
        }
    }
}

impl Default for ControlPlaneState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// Request body for creating a new deployment.
#[derive(Debug, Serialize, Deserialize)]
pub struct DeployRequest {
    /// Human-readable name for the deployment.
    pub name: String,
    /// Agent role (e.g. "coder", "tester", "custom:my_role").
    pub role: String,
    /// Number of replicas to deploy (default: 1).
    pub replicas: Option<u32>,
    /// Whether failed instances should be restarted automatically (default: true).
    pub auto_restart: Option<bool>,
    /// Maximum concurrent tasks per instance.
    pub max_concurrent_tasks: Option<u32>,
    /// Arbitrary key-value tags.
    pub tags: Option<HashMap<String, String>>,
}

/// Full deployment information, stored and returned by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentInfo {
    /// Unique deployment identifier.
    pub id: Uuid,
    /// Human-readable deployment name.
    pub name: String,
    /// Agent role.
    pub role: String,
    /// Desired number of replicas.
    pub replicas: u32,
    /// Current overall status: "running", "stopped", "degraded", "failed".
    pub status: String,
    /// Whether auto-restart is enabled.
    pub auto_restart: bool,
    /// Per-replica instance details.
    pub instances: Vec<InstanceInfo>,
    /// When the deployment was created.
    pub created_at: DateTime<Utc>,
    /// When the deployment was last updated.
    pub updated_at: DateTime<Utc>,
    /// Total tasks completed across all instances.
    pub total_tasks: u64,
    /// Total errors across all instances.
    pub total_errors: u64,
    /// Arbitrary key-value tags.
    pub tags: HashMap<String, String>,
}

/// Information about a single replica instance within a deployment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceInfo {
    /// Unique instance identifier.
    pub id: Uuid,
    /// Zero-based replica index.
    pub replica_index: u32,
    /// Current status: "running", "stopped", "unhealthy", "starting".
    pub status: String,
    /// When the instance was started.
    pub started_at: DateTime<Utc>,
    /// Last heartbeat received from this instance.
    pub last_heartbeat: Option<DateTime<Utc>>,
    /// Number of tasks completed by this instance.
    pub tasks_completed: u64,
    /// Number of errors in this instance.
    pub errors: u32,
}

/// Request body for scaling a deployment.
#[derive(Debug, Serialize, Deserialize)]
pub struct ScaleRequest {
    /// New desired replica count.
    pub replicas: u32,
}

/// Registered agent definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinitionInfo {
    /// Unique agent definition identifier.
    pub id: Uuid,
    /// Name of the agent definition.
    pub name: String,
    /// Agent role.
    pub role: String,
    /// Semantic version.
    pub version: String,
    /// Human-readable description.
    pub description: String,
    /// List of capability names this agent provides.
    pub capabilities: Vec<String>,
    /// Arbitrary key-value tags.
    pub tags: HashMap<String, String>,
    /// When the definition was created.
    pub created_at: DateTime<Utc>,
}

/// Request body for registering a new agent definition.
#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterAgentRequest {
    /// Name of the agent.
    pub name: String,
    /// Agent role.
    pub role: String,
    /// Semantic version (default: "0.1.0").
    pub version: Option<String>,
    /// Human-readable description (default: "").
    pub description: Option<String>,
    /// Capability names.
    pub capabilities: Option<Vec<String>>,
    /// Arbitrary key-value tags.
    pub tags: Option<HashMap<String, String>>,
}

/// Health information for a specific agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHealthInfo {
    /// Agent identifier.
    pub agent_id: Uuid,
    /// Agent name.
    pub agent_name: String,
    /// Overall health status: "healthy", "unhealthy", "unknown".
    pub status: String,
    /// Last heartbeat received.
    pub last_heartbeat: Option<DateTime<Utc>>,
    /// Number of restarts.
    pub restart_count: u32,
    /// Uptime in seconds.
    pub uptime_secs: i64,
    /// Health probes.
    pub probes: Vec<ProbeInfo>,
}

/// A single health probe result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeInfo {
    /// Probe name.
    pub name: String,
    /// Probe status: "ok", "failing", "unknown".
    pub status: String,
    /// When the probe was last checked.
    pub last_check: Option<DateTime<Utc>>,
    /// Number of consecutive failures.
    pub consecutive_failures: u32,
}

/// A control-plane event recorded for auditability and observability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlPlaneEvent {
    /// Unique event identifier.
    pub id: Uuid,
    /// When the event occurred.
    pub timestamp: DateTime<Utc>,
    /// Event type: "deployed", "undeployed", "scaled", "restarted",
    /// "health_changed", "stopped", "started", "registered", "unregistered".
    pub event_type: String,
    /// Associated deployment ID, if applicable.
    pub deployment_id: Option<Uuid>,
    /// Human-readable description.
    pub message: String,
}

/// Aggregate summary of the control plane.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlPlaneSummary {
    /// Total number of deployments.
    pub total_deployments: usize,
    /// Number of deployments in "running" state.
    pub running_deployments: usize,
    /// Total number of instances across all deployments.
    pub total_instances: usize,
    /// Number of instances in "running" state.
    pub running_instances: usize,
    /// Total registered agent definitions.
    pub total_registered_agents: usize,
    /// Number of healthy agents.
    pub healthy_agents: usize,
    /// Number of unhealthy agents.
    pub unhealthy_agents: usize,
    /// Total tasks completed across all deployments.
    pub total_tasks_completed: u64,
    /// Total errors across all deployments.
    pub total_errors: u64,
    /// Control-plane uptime in seconds.
    pub uptime_secs: i64,
    /// Most recent control-plane events (up to 20).
    pub recent_events: Vec<ControlPlaneEvent>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Record an event in the control-plane event log.
async fn record_event(
    events: &RwLock<Vec<ControlPlaneEvent>>,
    event_type: &str,
    deployment_id: Option<Uuid>,
    message: impl Into<String>,
) {
    let event = ControlPlaneEvent {
        id: Uuid::new_v4(),
        timestamp: Utc::now(),
        event_type: event_type.to_string(),
        deployment_id,
        message: message.into(),
    };
    events.write().await.push(event);
}

/// Create instances for a deployment with the given replica count.
fn create_instances(replicas: u32) -> Vec<InstanceInfo> {
    let now = Utc::now();
    (0..replicas)
        .map(|i| InstanceInfo {
            id: Uuid::new_v4(),
            replica_index: i,
            status: "running".to_string(),
            started_at: now,
            last_heartbeat: Some(now),
            tasks_completed: 0,
            errors: 0,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the control-plane REST API sub-router.
///
/// All routes are nested under `/api/v1/control-plane/`.
pub fn control_plane_router(state: Arc<ControlPlaneState>) -> Router {
    Router::new()
        // Deployments
        .route(
            "/api/v1/control-plane/deployments",
            get(list_deployments).post(create_deployment),
        )
        .route(
            "/api/v1/control-plane/deployments/{id}",
            get(get_deployment).delete(delete_deployment),
        )
        .route(
            "/api/v1/control-plane/deployments/{id}/scale",
            post(scale_deployment),
        )
        .route(
            "/api/v1/control-plane/deployments/{id}/restart",
            post(restart_deployment),
        )
        .route(
            "/api/v1/control-plane/deployments/{id}/stop",
            post(stop_deployment),
        )
        .route(
            "/api/v1/control-plane/deployments/{id}/start",
            post(start_deployment),
        )
        // Agent definitions
        .route(
            "/api/v1/control-plane/agents",
            get(list_agents).post(register_agent),
        )
        .route(
            "/api/v1/control-plane/agents/{id}",
            get(get_agent).delete(delete_agent),
        )
        // Health
        .route("/api/v1/control-plane/health", get(get_health_summary))
        .route("/api/v1/control-plane/health/{id}", get(get_agent_health))
        .route(
            "/api/v1/control-plane/health/{id}/heartbeat",
            post(record_heartbeat),
        )
        // Summary & events
        .route("/api/v1/control-plane/summary", get(get_summary))
        .route("/api/v1/control-plane/events", get(list_events))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Handlers — Deployments
// ---------------------------------------------------------------------------

/// List all deployments.
async fn list_deployments(
    State(state): State<Arc<ControlPlaneState>>,
) -> Result<Json<Vec<DeploymentInfo>>, ControlPlaneError> {
    let deployments = state.deployments.read().await;
    let list: Vec<DeploymentInfo> = deployments.values().cloned().collect();
    Ok(Json(list))
}

/// Create a new deployment.
async fn create_deployment(
    State(state): State<Arc<ControlPlaneState>>,
    Json(req): Json<DeployRequest>,
) -> Result<(StatusCode, Json<DeploymentInfo>), ControlPlaneError> {
    // Validate name
    if req.name.trim().is_empty() {
        return Err(ControlPlaneError::BadRequest(
            "Deployment name must not be empty".to_string(),
        ));
    }

    // Validate role
    if !is_valid_role(&req.role) {
        return Err(ControlPlaneError::BadRequest(format!(
            "Invalid role '{}'. Valid roles: {:?}, or 'custom:<name>'",
            req.role, VALID_ROLES
        )));
    }

    let replicas = req.replicas.unwrap_or(1);
    if replicas == 0 {
        return Err(ControlPlaneError::BadRequest(
            "Replicas must be greater than 0".to_string(),
        ));
    }

    let now = Utc::now();
    let id = Uuid::new_v4();
    let instances = create_instances(replicas);

    let deployment = DeploymentInfo {
        id,
        name: req.name.clone(),
        role: req.role.clone(),
        replicas,
        status: "running".to_string(),
        auto_restart: req.auto_restart.unwrap_or(true),
        instances,
        created_at: now,
        updated_at: now,
        total_tasks: 0,
        total_errors: 0,
        tags: req.tags.unwrap_or_default(),
    };

    state
        .deployments
        .write()
        .await
        .insert(id, deployment.clone());

    record_event(
        &state.events,
        "deployed",
        Some(id),
        format!("Deployed '{}' with {} replica(s)", req.name, replicas),
    )
    .await;

    info!(deployment_id = %id, name = %req.name, role = %req.role, replicas, "Deployment created");

    Ok((StatusCode::CREATED, Json(deployment)))
}

/// Get a deployment by ID.
async fn get_deployment(
    State(state): State<Arc<ControlPlaneState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<DeploymentInfo>, ControlPlaneError> {
    let deployments = state.deployments.read().await;
    let deployment = deployments
        .get(&id)
        .cloned()
        .ok_or_else(|| ControlPlaneError::NotFound(format!("Deployment {id} not found")))?;
    Ok(Json(deployment))
}

/// Delete (undeploy) a deployment.
async fn delete_deployment(
    State(state): State<Arc<ControlPlaneState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ControlPlaneError> {
    let removed = state.deployments.write().await.remove(&id);
    match removed {
        Some(dep) => {
            record_event(
                &state.events,
                "undeployed",
                Some(id),
                format!("Undeployed '{}'", dep.name),
            )
            .await;

            info!(deployment_id = %id, name = %dep.name, "Deployment deleted");

            Ok(Json(serde_json::json!({
                "deleted": true,
                "deployment_id": id,
            })))
        }
        None => Err(ControlPlaneError::NotFound(format!(
            "Deployment {id} not found"
        ))),
    }
}

/// Scale a deployment to a new replica count.
async fn scale_deployment(
    State(state): State<Arc<ControlPlaneState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<ScaleRequest>,
) -> Result<Json<DeploymentInfo>, ControlPlaneError> {
    if req.replicas == 0 {
        return Err(ControlPlaneError::BadRequest(
            "Replicas must be greater than 0".to_string(),
        ));
    }

    let mut deployments = state.deployments.write().await;
    let deployment = deployments
        .get_mut(&id)
        .ok_or_else(|| ControlPlaneError::NotFound(format!("Deployment {id} not found")))?;

    let old_replicas = deployment.replicas;
    let now = Utc::now();

    if req.replicas > old_replicas {
        // Scale up — add new instances
        for i in old_replicas..req.replicas {
            deployment.instances.push(InstanceInfo {
                id: Uuid::new_v4(),
                replica_index: i,
                status: "running".to_string(),
                started_at: now,
                last_heartbeat: Some(now),
                tasks_completed: 0,
                errors: 0,
            });
        }
    } else if req.replicas < old_replicas {
        // Scale down — remove instances from the end
        deployment.instances.truncate(req.replicas as usize);
    }

    deployment.replicas = req.replicas;
    deployment.updated_at = now;

    let result = deployment.clone();

    // Drop the write lock before recording the event
    let name = deployment.name.clone();
    drop(deployments);

    record_event(
        &state.events,
        "scaled",
        Some(id),
        format!(
            "Scaled '{}' from {} to {} replica(s)",
            name, old_replicas, req.replicas
        ),
    )
    .await;

    info!(deployment_id = %id, from = old_replicas, to = req.replicas, "Deployment scaled");

    Ok(Json(result))
}

/// Restart all instances in a deployment.
async fn restart_deployment(
    State(state): State<Arc<ControlPlaneState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<DeploymentInfo>, ControlPlaneError> {
    let mut deployments = state.deployments.write().await;
    let deployment = deployments
        .get_mut(&id)
        .ok_or_else(|| ControlPlaneError::NotFound(format!("Deployment {id} not found")))?;

    let now = Utc::now();
    for instance in &mut deployment.instances {
        instance.status = "running".to_string();
        instance.started_at = now;
        instance.last_heartbeat = Some(now);
    }

    deployment.status = "running".to_string();
    deployment.updated_at = now;

    let result = deployment.clone();
    let name = deployment.name.clone();
    drop(deployments);

    record_event(
        &state.events,
        "restarted",
        Some(id),
        format!("Restarted all instances of '{name}'"),
    )
    .await;

    info!(deployment_id = %id, "Deployment restarted");

    Ok(Json(result))
}

/// Stop a deployment (mark all instances as stopped).
async fn stop_deployment(
    State(state): State<Arc<ControlPlaneState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<DeploymentInfo>, ControlPlaneError> {
    let mut deployments = state.deployments.write().await;
    let deployment = deployments
        .get_mut(&id)
        .ok_or_else(|| ControlPlaneError::NotFound(format!("Deployment {id} not found")))?;

    let now = Utc::now();
    for instance in &mut deployment.instances {
        instance.status = "stopped".to_string();
    }

    deployment.status = "stopped".to_string();
    deployment.updated_at = now;

    let result = deployment.clone();
    let name = deployment.name.clone();
    drop(deployments);

    record_event(
        &state.events,
        "stopped",
        Some(id),
        format!("Stopped deployment '{name}'"),
    )
    .await;

    info!(deployment_id = %id, "Deployment stopped");

    Ok(Json(result))
}

/// Start a stopped deployment (mark all instances as running).
async fn start_deployment(
    State(state): State<Arc<ControlPlaneState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<DeploymentInfo>, ControlPlaneError> {
    let mut deployments = state.deployments.write().await;
    let deployment = deployments
        .get_mut(&id)
        .ok_or_else(|| ControlPlaneError::NotFound(format!("Deployment {id} not found")))?;

    let now = Utc::now();
    for instance in &mut deployment.instances {
        instance.status = "running".to_string();
        instance.started_at = now;
        instance.last_heartbeat = Some(now);
    }

    deployment.status = "running".to_string();
    deployment.updated_at = now;

    let result = deployment.clone();
    let name = deployment.name.clone();
    drop(deployments);

    record_event(
        &state.events,
        "started",
        Some(id),
        format!("Started deployment '{name}'"),
    )
    .await;

    info!(deployment_id = %id, "Deployment started");

    Ok(Json(result))
}

// ---------------------------------------------------------------------------
// Handlers — Agent definitions
// ---------------------------------------------------------------------------

/// List all registered agent definitions.
async fn list_agents(
    State(state): State<Arc<ControlPlaneState>>,
) -> Result<Json<Vec<AgentDefinitionInfo>>, ControlPlaneError> {
    let agents = state.agent_definitions.read().await;
    let list: Vec<AgentDefinitionInfo> = agents.values().cloned().collect();
    Ok(Json(list))
}

/// Register a new agent definition.
async fn register_agent(
    State(state): State<Arc<ControlPlaneState>>,
    Json(req): Json<RegisterAgentRequest>,
) -> Result<(StatusCode, Json<AgentDefinitionInfo>), ControlPlaneError> {
    if req.name.trim().is_empty() {
        return Err(ControlPlaneError::BadRequest(
            "Agent name must not be empty".to_string(),
        ));
    }

    if !is_valid_role(&req.role) {
        return Err(ControlPlaneError::BadRequest(format!(
            "Invalid role '{}'. Valid roles: {:?}, or 'custom:<name>'",
            req.role, VALID_ROLES
        )));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    let agent_def = AgentDefinitionInfo {
        id,
        name: req.name.clone(),
        role: req.role.clone(),
        version: req.version.unwrap_or_else(|| "0.1.0".to_string()),
        description: req.description.unwrap_or_default(),
        capabilities: req.capabilities.unwrap_or_default(),
        tags: req.tags.unwrap_or_default(),
        created_at: now,
    };

    state
        .agent_definitions
        .write()
        .await
        .insert(id, agent_def.clone());

    // Also create an initial health entry
    let health = AgentHealthInfo {
        agent_id: id,
        agent_name: req.name.clone(),
        status: "healthy".to_string(),
        last_heartbeat: Some(now),
        restart_count: 0,
        uptime_secs: 0,
        probes: vec![ProbeInfo {
            name: "liveness".to_string(),
            status: "ok".to_string(),
            last_check: Some(now),
            consecutive_failures: 0,
        }],
    };
    state.health_states.write().await.insert(id, health);

    record_event(
        &state.events,
        "registered",
        None,
        format!(
            "Registered agent definition '{}' (role: {})",
            req.name, req.role
        ),
    )
    .await;

    info!(agent_id = %id, name = %req.name, role = %req.role, "Agent definition registered");

    Ok((StatusCode::CREATED, Json(agent_def)))
}

/// Get an agent definition by ID.
async fn get_agent(
    State(state): State<Arc<ControlPlaneState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<AgentDefinitionInfo>, ControlPlaneError> {
    let agents = state.agent_definitions.read().await;
    let agent = agents
        .get(&id)
        .cloned()
        .ok_or_else(|| ControlPlaneError::NotFound(format!("Agent definition {id} not found")))?;
    Ok(Json(agent))
}

/// Unregister an agent definition.
async fn delete_agent(
    State(state): State<Arc<ControlPlaneState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ControlPlaneError> {
    let removed = state.agent_definitions.write().await.remove(&id);
    match removed {
        Some(agent) => {
            // Also remove the health entry
            state.health_states.write().await.remove(&id);

            record_event(
                &state.events,
                "unregistered",
                None,
                format!("Unregistered agent definition '{}'", agent.name),
            )
            .await;

            info!(agent_id = %id, name = %agent.name, "Agent definition unregistered");

            Ok(Json(serde_json::json!({
                "deleted": true,
                "agent_id": id,
            })))
        }
        None => Err(ControlPlaneError::NotFound(format!(
            "Agent definition {id} not found"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Handlers — Health
// ---------------------------------------------------------------------------

/// Get a health summary for all agents.
async fn get_health_summary(
    State(state): State<Arc<ControlPlaneState>>,
) -> Result<Json<Vec<AgentHealthInfo>>, ControlPlaneError> {
    let healths = state.health_states.read().await;
    let list: Vec<AgentHealthInfo> = healths.values().cloned().collect();
    Ok(Json(list))
}

/// Get health info for a specific agent.
async fn get_agent_health(
    State(state): State<Arc<ControlPlaneState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<AgentHealthInfo>, ControlPlaneError> {
    let healths = state.health_states.read().await;
    let health = healths.get(&id).cloned().ok_or_else(|| {
        ControlPlaneError::NotFound(format!("Health info for agent {id} not found"))
    })?;
    Ok(Json(health))
}

/// Record a heartbeat for a specific agent.
async fn record_heartbeat(
    State(state): State<Arc<ControlPlaneState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<AgentHealthInfo>, ControlPlaneError> {
    let mut healths = state.health_states.write().await;
    let health = healths.get_mut(&id).ok_or_else(|| {
        ControlPlaneError::NotFound(format!("Health info for agent {id} not found"))
    })?;

    let now = Utc::now();
    health.last_heartbeat = Some(now);
    health.status = "healthy".to_string();

    // Update liveness probe
    for probe in &mut health.probes {
        if probe.name == "liveness" {
            probe.status = "ok".to_string();
            probe.last_check = Some(now);
            probe.consecutive_failures = 0;
        }
    }

    // Recalculate uptime from started_at
    health.uptime_secs = now.signed_duration_since(state.started_at).num_seconds();

    let result = health.clone();
    Ok(Json(result))
}

// ---------------------------------------------------------------------------
// Handlers — Summary & Events
// ---------------------------------------------------------------------------

/// Get a full control-plane summary.
async fn get_summary(
    State(state): State<Arc<ControlPlaneState>>,
) -> Result<Json<ControlPlaneSummary>, ControlPlaneError> {
    let deployments = state.deployments.read().await;
    let healths = state.health_states.read().await;
    let events = state.events.read().await;
    let agents = state.agent_definitions.read().await;
    let now = Utc::now();

    let total_deployments = deployments.len();
    let running_deployments = deployments
        .values()
        .filter(|d| d.status == "running")
        .count();

    let total_instances: usize = deployments.values().map(|d| d.instances.len()).sum();
    let running_instances: usize = deployments
        .values()
        .flat_map(|d| &d.instances)
        .filter(|i| i.status == "running")
        .count();

    let total_tasks_completed: u64 = deployments.values().map(|d| d.total_tasks).sum();
    let total_errors: u64 = deployments.values().map(|d| d.total_errors).sum();

    let healthy_agents = healths.values().filter(|h| h.status == "healthy").count();
    let unhealthy_agents = healths.values().filter(|h| h.status != "healthy").count();

    let uptime_secs = now.signed_duration_since(state.started_at).num_seconds();

    // Return up to 20 most recent events
    let recent_events: Vec<ControlPlaneEvent> = events.iter().rev().take(20).cloned().collect();

    Ok(Json(ControlPlaneSummary {
        total_deployments,
        running_deployments,
        total_instances,
        running_instances,
        total_registered_agents: agents.len(),
        healthy_agents,
        unhealthy_agents,
        total_tasks_completed,
        total_errors,
        uptime_secs,
        recent_events,
    }))
}

/// List all control-plane events.
async fn list_events(
    State(state): State<Arc<ControlPlaneState>>,
) -> Result<Json<Vec<ControlPlaneEvent>>, ControlPlaneError> {
    let events = state.events.read().await;
    Ok(Json(events.clone()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    /// Create a test control-plane state.
    fn test_state() -> Arc<ControlPlaneState> {
        Arc::new(ControlPlaneState::new())
    }

    /// Helper: parse response body as JSON.
    async fn body_json<T: serde::de::DeserializeOwned>(resp: axum::http::Response<Body>) -> T {
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    /// Helper: build a POST request with JSON body.
    fn post_json(uri: &str, body: &impl Serialize) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(body).unwrap()))
            .unwrap()
    }

    /// Helper: create a deployment and return its ID.
    async fn create_test_deployment(state: &Arc<ControlPlaneState>) -> Uuid {
        let app = control_plane_router(state.clone());
        let req = post_json(
            "/api/v1/control-plane/deployments",
            &DeployRequest {
                name: "test-coder".to_string(),
                role: "coder".to_string(),
                replicas: Some(2),
                auto_restart: None,
                max_concurrent_tasks: None,
                tags: None,
            },
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let dep: DeploymentInfo = body_json(resp).await;
        dep.id
    }

    #[tokio::test]
    async fn test_create_deployment() {
        let state = test_state();
        let app = control_plane_router(state.clone());

        let req = post_json(
            "/api/v1/control-plane/deployments",
            &DeployRequest {
                name: "my-coder".to_string(),
                role: "coder".to_string(),
                replicas: Some(3),
                auto_restart: Some(false),
                max_concurrent_tasks: Some(5),
                tags: Some(HashMap::from([("env".to_string(), "prod".to_string())])),
            },
        );

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let dep: DeploymentInfo = body_json(resp).await;
        assert_eq!(dep.name, "my-coder");
        assert_eq!(dep.role, "coder");
        assert_eq!(dep.replicas, 3);
        assert!(!dep.auto_restart);
        assert_eq!(dep.status, "running");
        assert_eq!(dep.instances.len(), 3);
        assert_eq!(dep.tags.get("env").unwrap(), "prod");

        // Verify it was stored
        let deployments = state.deployments.read().await;
        assert_eq!(deployments.len(), 1);
    }

    #[tokio::test]
    async fn test_list_deployments() {
        let state = test_state();

        // Create two deployments
        create_test_deployment(&state).await;

        let app = control_plane_router(state.clone());
        let req = post_json(
            "/api/v1/control-plane/deployments",
            &DeployRequest {
                name: "test-tester".to_string(),
                role: "tester".to_string(),
                replicas: None,
                auto_restart: None,
                max_concurrent_tasks: None,
                tags: None,
            },
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        // List
        let app = control_plane_router(state.clone());
        let req = Request::builder()
            .uri("/api/v1/control-plane/deployments")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let deps: Vec<DeploymentInfo> = body_json(resp).await;
        assert_eq!(deps.len(), 2);
    }

    #[tokio::test]
    async fn test_get_deployment_by_id() {
        let state = test_state();
        let dep_id = create_test_deployment(&state).await;

        let app = control_plane_router(state.clone());
        let req = Request::builder()
            .uri(format!("/api/v1/control-plane/deployments/{dep_id}"))
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let dep: DeploymentInfo = body_json(resp).await;
        assert_eq!(dep.id, dep_id);
        assert_eq!(dep.name, "test-coder");
    }

    #[tokio::test]
    async fn test_delete_deployment() {
        let state = test_state();
        let dep_id = create_test_deployment(&state).await;

        let app = control_plane_router(state.clone());
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/api/v1/control-plane/deployments/{dep_id}"))
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body: serde_json::Value = body_json(resp).await;
        assert_eq!(body["deleted"], true);

        // Verify it was removed
        let deployments = state.deployments.read().await;
        assert!(deployments.is_empty());
    }

    #[tokio::test]
    async fn test_scale_deployment() {
        let state = test_state();
        let dep_id = create_test_deployment(&state).await;

        // Scale up from 2 to 5
        let app = control_plane_router(state.clone());
        let req = post_json(
            &format!("/api/v1/control-plane/deployments/{dep_id}/scale"),
            &ScaleRequest { replicas: 5 },
        );

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let dep: DeploymentInfo = body_json(resp).await;
        assert_eq!(dep.replicas, 5);
        assert_eq!(dep.instances.len(), 5);

        // Scale down from 5 to 1
        let app = control_plane_router(state.clone());
        let req = post_json(
            &format!("/api/v1/control-plane/deployments/{dep_id}/scale"),
            &ScaleRequest { replicas: 1 },
        );

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let dep: DeploymentInfo = body_json(resp).await;
        assert_eq!(dep.replicas, 1);
        assert_eq!(dep.instances.len(), 1);
    }

    #[tokio::test]
    async fn test_restart_deployment() {
        let state = test_state();
        let dep_id = create_test_deployment(&state).await;

        // Stop first so restart actually changes state
        let app = control_plane_router(state.clone());
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/control-plane/deployments/{dep_id}/stop"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let dep: DeploymentInfo = body_json(resp).await;
        assert_eq!(dep.status, "stopped");

        // Restart
        let app = control_plane_router(state.clone());
        let req = Request::builder()
            .method("POST")
            .uri(format!(
                "/api/v1/control-plane/deployments/{dep_id}/restart"
            ))
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let dep: DeploymentInfo = body_json(resp).await;
        assert_eq!(dep.status, "running");
        for instance in &dep.instances {
            assert_eq!(instance.status, "running");
        }
    }

    #[tokio::test]
    async fn test_register_agent() {
        let state = test_state();
        let app = control_plane_router(state.clone());

        let req = post_json(
            "/api/v1/control-plane/agents",
            &RegisterAgentRequest {
                name: "code-writer".to_string(),
                role: "coder".to_string(),
                version: Some("1.0.0".to_string()),
                description: Some("Writes Rust code".to_string()),
                capabilities: Some(vec!["file_write".to_string(), "shell".to_string()]),
                tags: None,
            },
        );

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let agent: AgentDefinitionInfo = body_json(resp).await;
        assert_eq!(agent.name, "code-writer");
        assert_eq!(agent.role, "coder");
        assert_eq!(agent.version, "1.0.0");
        assert_eq!(agent.capabilities.len(), 2);

        // Verify stored
        let agents = state.agent_definitions.read().await;
        assert_eq!(agents.len(), 1);

        // Verify health entry was created
        let healths = state.health_states.read().await;
        assert_eq!(healths.len(), 1);
    }

    #[tokio::test]
    async fn test_list_agents() {
        let state = test_state();

        // Register two agents
        let app = control_plane_router(state.clone());
        let req = post_json(
            "/api/v1/control-plane/agents",
            &RegisterAgentRequest {
                name: "agent-a".to_string(),
                role: "coder".to_string(),
                version: None,
                description: None,
                capabilities: None,
                tags: None,
            },
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let app = control_plane_router(state.clone());
        let req = post_json(
            "/api/v1/control-plane/agents",
            &RegisterAgentRequest {
                name: "agent-b".to_string(),
                role: "tester".to_string(),
                version: None,
                description: None,
                capabilities: None,
                tags: None,
            },
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        // List
        let app = control_plane_router(state.clone());
        let req = Request::builder()
            .uri("/api/v1/control-plane/agents")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let agents: Vec<AgentDefinitionInfo> = body_json(resp).await;
        assert_eq!(agents.len(), 2);
    }

    #[tokio::test]
    async fn test_get_health_summary_empty() {
        let state = test_state();
        let app = control_plane_router(state.clone());

        let req = Request::builder()
            .uri("/api/v1/control-plane/health")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let healths: Vec<AgentHealthInfo> = body_json(resp).await;
        assert!(healths.is_empty());
    }

    #[tokio::test]
    async fn test_record_heartbeat() {
        let state = test_state();

        // Register an agent first (this creates a health entry)
        let app = control_plane_router(state.clone());
        let req = post_json(
            "/api/v1/control-plane/agents",
            &RegisterAgentRequest {
                name: "heartbeat-agent".to_string(),
                role: "coder".to_string(),
                version: None,
                description: None,
                capabilities: None,
                tags: None,
            },
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let agent: AgentDefinitionInfo = body_json(resp).await;
        let agent_id = agent.id;

        // Record heartbeat
        let app = control_plane_router(state.clone());
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/control-plane/health/{agent_id}/heartbeat"))
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let health: AgentHealthInfo = body_json(resp).await;
        assert_eq!(health.status, "healthy");
        assert!(health.last_heartbeat.is_some());
    }

    #[tokio::test]
    async fn test_get_summary() {
        let state = test_state();

        // Create a deployment and register an agent to have some data
        create_test_deployment(&state).await;

        let app = control_plane_router(state.clone());
        let req = post_json(
            "/api/v1/control-plane/agents",
            &RegisterAgentRequest {
                name: "summary-agent".to_string(),
                role: "tester".to_string(),
                version: None,
                description: None,
                capabilities: None,
                tags: None,
            },
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        // Get summary
        let app = control_plane_router(state.clone());
        let req = Request::builder()
            .uri("/api/v1/control-plane/summary")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let summary: ControlPlaneSummary = body_json(resp).await;
        assert_eq!(summary.total_deployments, 1);
        assert_eq!(summary.running_deployments, 1);
        assert_eq!(summary.total_instances, 2);
        assert_eq!(summary.running_instances, 2);
        assert_eq!(summary.total_registered_agents, 1);
        assert_eq!(summary.healthy_agents, 1);
        assert_eq!(summary.unhealthy_agents, 0);
        assert!(summary.uptime_secs >= 0);
        // Events: 1 for deploy + 1 for register = 2
        assert!(!summary.recent_events.is_empty());
    }

    #[tokio::test]
    async fn test_deploy_with_invalid_role_returns_400() {
        let state = test_state();
        let app = control_plane_router(state.clone());

        let req = post_json(
            "/api/v1/control-plane/deployments",
            &DeployRequest {
                name: "bad-deploy".to_string(),
                role: "nonexistent_role".to_string(),
                replicas: None,
                auto_restart: None,
                max_concurrent_tasks: None,
                tags: None,
            },
        );

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let body: serde_json::Value = body_json(resp).await;
        assert!(body["error"].as_str().unwrap().contains("Invalid role"));
    }

    #[tokio::test]
    async fn test_delete_nonexistent_returns_404() {
        let state = test_state();
        let app = control_plane_router(state.clone());

        let fake_id = Uuid::new_v4();
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/api/v1/control-plane/deployments/{fake_id}"))
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_list_events() {
        let state = test_state();

        // Create a deployment to generate an event
        create_test_deployment(&state).await;

        let app = control_plane_router(state.clone());
        let req = Request::builder()
            .uri("/api/v1/control-plane/events")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let events: Vec<ControlPlaneEvent> = body_json(resp).await;
        assert!(!events.is_empty());
        assert_eq!(events[0].event_type, "deployed");
    }

    #[tokio::test]
    async fn test_stop_and_start_deployment() {
        let state = test_state();
        let dep_id = create_test_deployment(&state).await;

        // Stop
        let app = control_plane_router(state.clone());
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/control-plane/deployments/{dep_id}/stop"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let dep: DeploymentInfo = body_json(resp).await;
        assert_eq!(dep.status, "stopped");
        for inst in &dep.instances {
            assert_eq!(inst.status, "stopped");
        }

        // Start
        let app = control_plane_router(state.clone());
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/control-plane/deployments/{dep_id}/start"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let dep: DeploymentInfo = body_json(resp).await;
        assert_eq!(dep.status, "running");
        for inst in &dep.instances {
            assert_eq!(inst.status, "running");
        }
    }

    #[tokio::test]
    async fn test_scale_deployment_zero_replicas_returns_400() {
        let state = test_state();
        let dep_id = create_test_deployment(&state).await;

        let app = control_plane_router(state.clone());
        let req = post_json(
            &format!("/api/v1/control-plane/deployments/{dep_id}/scale"),
            &ScaleRequest { replicas: 0 },
        );

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_deploy_custom_role() {
        let state = test_state();
        let app = control_plane_router(state.clone());

        let req = post_json(
            "/api/v1/control-plane/deployments",
            &DeployRequest {
                name: "custom-agent".to_string(),
                role: "custom:data_pipeline".to_string(),
                replicas: Some(1),
                auto_restart: None,
                max_concurrent_tasks: None,
                tags: None,
            },
        );

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let dep: DeploymentInfo = body_json(resp).await;
        assert_eq!(dep.role, "custom:data_pipeline");
    }
}
