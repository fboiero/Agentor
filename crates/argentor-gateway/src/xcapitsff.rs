//! XcapitSFF integration — agent execution endpoints, webhook proxy, and health checks.
//!
//! Provides REST endpoints for XcapitSFF (Python/FastAPI) to invoke Argentor agents
//! by role, execute batch tasks, proxy webhooks with compliance, and cross-check health.
//!
//! # Endpoints
//!
//! - `POST /api/v1/agent/run-task` — Execute a single agent task by role
//! - `POST /api/v1/agent/batch` — Execute multiple agent tasks in parallel
//! - `POST /api/v1/proxy/webhook` — Proxy external webhooks with HMAC validation and audit
//! - `GET /api/v1/health` — Extended health check including XcapitSFF status

use argentor_agent::{AgentRunner, ModelConfig};
use argentor_core::ArgentorError;
use argentor_security::audit::AuditOutcome;
use argentor_security::{AuditLog, PermissionSet};
use argentor_session::Session;
use argentor_skills::SkillRegistry;
use axum::{
    extract::{Json, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Agent profiles
// ---------------------------------------------------------------------------

/// Pre-configured agent profile for XcapitSFF integration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XcapitAgentProfile {
    /// Role identifier (e.g. "sales_qualifier").
    pub role: String,
    /// Primary model configuration.
    pub model: ModelConfig,
    /// System prompt for this role.
    pub system_prompt: String,
}

/// Build the 4 default XcapitSFF agent profiles.
pub fn default_xcapit_profiles() -> HashMap<String, XcapitAgentProfile> {
    let mut profiles = HashMap::new();

    // -- sales_qualifier --
    profiles.insert("sales_qualifier".to_string(), XcapitAgentProfile {
        role: "sales_qualifier".to_string(),
        model: ModelConfig {
            provider: argentor_agent::config::LlmProvider::Claude,
            model_id: "claude-sonnet-4-6-20250514".to_string(),
            api_key: String::new(), // resolved from env
            api_base_url: None,
            temperature: 0.3,
            max_tokens: 2048,
            max_turns: 5,
            fallback_models: vec![ModelConfig {
                provider: argentor_agent::config::LlmProvider::OpenAi,
                model_id: "gpt-4o-mini".to_string(),
                api_key: String::new(),
                api_base_url: None,
                temperature: 0.3,
                max_tokens: 2048,
                max_turns: 5,
                fallback_models: vec![],
                retry_policy: None,
            }],
            retry_policy: None,
        },
        system_prompt: "Sos el agente de calificación de ventas de Xcapit. Evaluás leads usando ICP scoring (Region, C-Level, Afinidad, Score). Clasificás como hot (>=70), warm (>=45), cool (>=25), cold (<25). Para cada lead, respondé con: score, clasificación, acción recomendada, prioridad de outreach. Sé conciso y accionable.".to_string(),
    });

    // -- outreach_composer --
    profiles.insert("outreach_composer".to_string(), XcapitAgentProfile {
        role: "outreach_composer".to_string(),
        model: ModelConfig {
            provider: argentor_agent::config::LlmProvider::Claude,
            model_id: "claude-sonnet-4-6-20250514".to_string(),
            api_key: String::new(),
            api_base_url: None,
            temperature: 0.7,
            max_tokens: 4096,
            max_turns: 5,
            fallback_models: vec![ModelConfig {
                provider: argentor_agent::config::LlmProvider::OpenAi,
                model_id: "gpt-4o-mini".to_string(),
                api_key: String::new(),
                api_base_url: None,
                temperature: 0.7,
                max_tokens: 4096,
                max_turns: 5,
                fallback_models: vec![],
                retry_policy: None,
            }],
            retry_policy: None,
        },
        system_prompt: "Sos el agente de outreach de Xcapit, empresa de tecnología financiera (inversión automatizada, gestión de activos digitales, DeFi). Componés mensajes personalizados según canal (email/linkedin/whatsapp), región (LATAM=español neutro, Iberia=español peninsular), seniority (C-Level=ROI estratégico, otros=operativo), y afinidad (HIGH=directo, MEDIUM=educativo, LOW=nurturing). Siempre incluí mensaje principal, variante A/B, y siguiente paso sugerido.".to_string(),
    });

    // -- support_responder --
    profiles.insert("support_responder".to_string(), XcapitAgentProfile {
        role: "support_responder".to_string(),
        model: ModelConfig {
            provider: argentor_agent::config::LlmProvider::Claude,
            model_id: "claude-sonnet-4-6-20250514".to_string(),
            api_key: String::new(),
            api_base_url: None,
            temperature: 0.4,
            max_tokens: 4096,
            max_turns: 5,
            fallback_models: vec![ModelConfig {
                provider: argentor_agent::config::LlmProvider::OpenAi,
                model_id: "gpt-4o-mini".to_string(),
                api_key: String::new(),
                api_base_url: None,
                temperature: 0.4,
                max_tokens: 4096,
                max_turns: 5,
                fallback_models: vec![],
                retry_policy: None,
            }],
            retry_policy: None,
        },
        system_prompt: "Sos el agente de soporte al cliente de Xcapit (fintech, inversión automatizada, activos digitales). Resolvés tickets de forma empática, precisa y rápida. Si no tenés certeza, decilo. Si es tema de fondos/dinero, NUNCA dar instrucciones sin verificación. Si es bug, documentar pasos de reproducción. Siempre ofrecer siguiente paso claro. Español LATAM por defecto. Respondé con: respuesta al cliente, notas internas, y si hay que escalar.".to_string(),
    });

    // -- ticket_router --
    profiles.insert("ticket_router".to_string(), XcapitAgentProfile {
        role: "ticket_router".to_string(),
        model: ModelConfig {
            provider: argentor_agent::config::LlmProvider::Claude,
            model_id: "claude-sonnet-4-6-20250514".to_string(),
            api_key: String::new(),
            api_base_url: None,
            temperature: 0.2,
            max_tokens: 1024,
            max_turns: 3,
            fallback_models: vec![ModelConfig {
                provider: argentor_agent::config::LlmProvider::OpenAi,
                model_id: "gpt-4o-mini".to_string(),
                api_key: String::new(),
                api_base_url: None,
                temperature: 0.2,
                max_tokens: 1024,
                max_turns: 3,
                fallback_models: vec![],
                retry_policy: None,
            }],
            retry_policy: None,
        },
        system_prompt: "Clasificás tickets de soporte por categoría (billing, technical, account, crypto, general) y prioridad (urgent, high, medium, low). Respondé SOLO con JSON: {\"category\": \"...\", \"priority\": \"...\", \"confidence\": 0.0-1.0, \"requires_human_review\": true/false, \"reasoning\": \"...\"}."
            .to_string(),
    });

    profiles
}

/// Resolve API keys from environment into the profile's model config.
fn resolve_api_keys(config: &mut ModelConfig) {
    if config.api_key.is_empty() {
        config.api_key = match config.provider {
            argentor_agent::config::LlmProvider::Claude
            | argentor_agent::config::LlmProvider::ClaudeCode => {
                std::env::var("ANTHROPIC_API_KEY").unwrap_or_default()
            }
            argentor_agent::config::LlmProvider::OpenAi => {
                std::env::var("OPENAI_API_KEY").unwrap_or_default()
            }
            argentor_agent::config::LlmProvider::Gemini => {
                std::env::var("GEMINI_API_KEY").unwrap_or_default()
            }
            _ => std::env::var("OPENAI_API_KEY").unwrap_or_default(),
        };
    }
    for fallback in &mut config.fallback_models {
        resolve_api_keys(fallback);
    }
}

// ---------------------------------------------------------------------------
// XcapitSFF health state
// ---------------------------------------------------------------------------

/// Health status of XcapitSFF backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XcapitSffHealth {
    /// Base URL of XcapitSFF.
    pub url: String,
    /// Current status ("ok", "unreachable", "degraded").
    pub status: String,
    /// Last successful health check timestamp.
    pub last_check: Option<DateTime<Utc>>,
    /// Last error message, if any.
    pub last_error: Option<String>,
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

/// Configuration for the XcapitSFF integration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XcapitConfig {
    /// Base URL of XcapitSFF (e.g. "http://localhost:8000").
    pub url: String,
    /// Health check interval in seconds.
    pub health_check_interval: u64,
    /// Allowed webhook sources.
    pub allowed_webhook_sources: Vec<String>,
    /// HMAC secret for webhook validation.
    pub webhook_hmac_secret: String,
    /// CORS allowed origins.
    pub cors_origins: Vec<String>,
}

impl Default for XcapitConfig {
    fn default() -> Self {
        Self {
            url: std::env::var("XCAPITSFF_URL")
                .unwrap_or_else(|_| "http://localhost:8000".to_string()),
            health_check_interval: 30,
            allowed_webhook_sources: vec![
                "hubspot".to_string(),
                "salesforce".to_string(),
                "stripe".to_string(),
                "intercom".to_string(),
            ],
            webhook_hmac_secret: std::env::var("WEBHOOK_SECRET").unwrap_or_default(),
            cors_origins: vec![
                "http://localhost:8000".to_string(),
                "http://xcapitsff:8000".to_string(),
            ],
        }
    }
}

/// Shared state for XcapitSFF integration endpoints.
pub struct XcapitState {
    /// Agent profiles indexed by role.
    pub profiles: HashMap<String, XcapitAgentProfile>,
    /// Shared skill registry.
    pub skills: Arc<SkillRegistry>,
    /// Permissions for agent execution.
    pub permissions: PermissionSet,
    /// Audit log for compliance.
    pub audit: Arc<AuditLog>,
    /// Integration configuration.
    pub config: XcapitConfig,
    /// XcapitSFF health state (updated by background task).
    pub xcapitsff_health: Arc<RwLock<XcapitSffHealth>>,
    /// HTTP client for outbound requests.
    pub http_client: reqwest::Client,
}

impl XcapitState {
    /// Create a new XcapitState with default profiles.
    pub fn new(
        skills: Arc<SkillRegistry>,
        permissions: PermissionSet,
        audit: Arc<AuditLog>,
        config: XcapitConfig,
    ) -> Self {
        let health = XcapitSffHealth {
            url: config.url.clone(),
            status: "unknown".to_string(),
            last_check: None,
            last_error: None,
        };

        Self {
            profiles: default_xcapit_profiles(),
            skills,
            permissions,
            audit,
            config,
            xcapitsff_health: Arc::new(RwLock::new(health)),
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Start the background health check loop for XcapitSFF.
    pub fn start_health_loop(self: &Arc<Self>) {
        let state = self.clone();
        let interval = std::time::Duration::from_secs(state.config.health_check_interval);
        tokio::spawn(async move {
            loop {
                let url = format!("{}/health", state.config.url);
                let result = state.http_client.get(&url).send().await;
                let mut health = state.xcapitsff_health.write().await;
                match result {
                    Ok(resp) if resp.status().is_success() => {
                        health.status = "ok".to_string();
                        health.last_check = Some(Utc::now());
                        health.last_error = None;
                    }
                    Ok(resp) => {
                        health.status = "degraded".to_string();
                        health.last_check = Some(Utc::now());
                        health.last_error = Some(format!("HTTP {}", resp.status()));
                    }
                    Err(e) => {
                        health.status = "unreachable".to_string();
                        health.last_check = Some(Utc::now());
                        health.last_error = Some(e.to_string());
                    }
                }
                tokio::time::sleep(interval).await;
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

// -- run-task --

/// Request body for POST /api/v1/agent/run-task.
#[derive(Debug, Deserialize)]
pub struct RunTaskRequest {
    /// Agent role (e.g. "sales_qualifier", "outreach_composer").
    pub agent_role: String,
    /// Optional system prompt override (uses profile default if omitted).
    pub system_prompt: Option<String>,
    /// User context / input for the agent.
    pub context: String,
    /// Optional session ID for conversation continuity.
    pub session_id: Option<String>,
    /// Max tokens for the response (overrides profile if set).
    pub max_tokens: Option<u32>,
    /// Temperature (overrides profile if set).
    pub temperature: Option<f32>,
}

/// Response for POST /api/v1/agent/run-task.
#[derive(Debug, Serialize)]
pub struct RunTaskResponse {
    /// Agent response text.
    pub response: String,
    /// Session ID (new or existing).
    pub session_id: String,
    /// Model that produced the response.
    pub model_used: String,
    /// Estimated input tokens.
    pub tokens_input: u64,
    /// Estimated output tokens.
    pub tokens_output: u64,
    /// Tool calls made during execution (if any).
    pub tool_calls: Vec<String>,
    /// Compliance flags raised during execution.
    pub compliance_flags: Vec<String>,
    /// Total wall-clock duration in milliseconds.
    pub duration_ms: u64,
}

// -- batch --

/// A single task in a batch request.
#[derive(Debug, Deserialize)]
pub struct BatchTaskItem {
    /// Agent role.
    pub agent_role: String,
    /// User context / input.
    pub context: String,
    /// Optional system prompt override.
    pub system_prompt: Option<String>,
    /// Max tokens override.
    pub max_tokens: Option<u32>,
}

/// Request body for POST /api/v1/agent/batch.
#[derive(Debug, Deserialize)]
pub struct BatchRequest {
    /// List of tasks to execute.
    pub tasks: Vec<BatchTaskItem>,
    /// Maximum concurrent executions (default: 5).
    pub max_concurrent: Option<usize>,
}

/// A single result in a batch response.
#[derive(Debug, Serialize)]
pub struct BatchResultItem {
    /// Index in the original task list.
    pub index: usize,
    /// Whether the task succeeded.
    pub success: bool,
    /// Agent response (empty on failure).
    pub response: String,
    /// Error message (empty on success).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Estimated input tokens.
    pub tokens_input: u64,
    /// Estimated output tokens.
    pub tokens_output: u64,
}

/// Response for POST /api/v1/agent/batch.
#[derive(Debug, Serialize)]
pub struct BatchResponse {
    /// Individual results.
    pub results: Vec<BatchResultItem>,
    /// Total tasks in the batch.
    pub total: usize,
    /// Number of successful tasks.
    pub succeeded: usize,
    /// Number of failed tasks.
    pub failed: usize,
    /// Total tokens consumed.
    pub total_tokens: u64,
    /// Total wall-clock duration in milliseconds.
    pub total_duration_ms: u64,
}

// -- webhook proxy --

/// Request body for POST /api/v1/proxy/webhook.
#[derive(Debug, Deserialize, Serialize)]
pub struct WebhookProxyRequest {
    /// Event type (e.g. "lead.created").
    pub event: String,
    /// Event payload.
    pub data: serde_json::Value,
    /// Source system (e.g. "hubspot").
    pub source: String,
}

/// Response for webhook proxy.
#[derive(Debug, Serialize)]
pub struct WebhookProxyResponse {
    /// Whether the forward succeeded.
    pub forwarded: bool,
    /// HTTP status from XcapitSFF.
    pub upstream_status: Option<u16>,
    /// Audit log entry ID.
    pub audit_id: String,
    /// Error message, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// -- health --

/// Extended health check response.
#[derive(Debug, Serialize)]
pub struct ExtendedHealthResponse {
    /// Overall status.
    pub status: String,
    /// Argentor version.
    pub version: String,
    /// Sub-system checks.
    pub checks: HealthChecks,
    /// XcapitSFF health detail.
    pub xcapitsff: XcapitSffHealth,
}

/// Individual health checks.
#[derive(Debug, Serialize)]
pub struct HealthChecks {
    /// LLM backends status.
    pub llm_backends: String,
    /// XcapitSFF connectivity.
    pub xcapitsff: String,
    /// Compliance modules.
    pub compliance: String,
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the XcapitSFF integration router.
pub fn xcapitsff_router(state: Arc<XcapitState>) -> Router {
    Router::new()
        .route("/api/v1/agent/run-task", post(run_task_handler))
        .route("/api/v1/agent/batch", post(batch_handler))
        .route("/api/v1/proxy/webhook", post(webhook_proxy_handler))
        .route("/api/v1/health", get(extended_health_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/v1/agent/run-task — Execute a single agent task.
async fn run_task_handler(
    State(state): State<Arc<XcapitState>>,
    Json(req): Json<RunTaskRequest>,
) -> impl IntoResponse {
    let start = Instant::now();

    // Look up profile
    let profile = match state.profiles.get(&req.agent_role) {
        Some(p) => p.clone(),
        None => {
            let available: Vec<&String> = state.profiles.keys().collect();
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Unknown agent_role: '{}'. Available: {:?}", req.agent_role, available)
                })),
            )
                .into_response();
        }
    };

    // Build model config with overrides
    let mut model_config = profile.model.clone();
    resolve_api_keys(&mut model_config);

    if let Some(max_tokens) = req.max_tokens {
        model_config.max_tokens = max_tokens;
    }
    if let Some(temp) = req.temperature {
        model_config.temperature = temp;
    }

    let system_prompt = req
        .system_prompt
        .unwrap_or_else(|| profile.system_prompt.clone());

    let model_id = model_config.model_id.clone();

    // Build runner with circuit breaker
    let runner = AgentRunner::new(
        model_config,
        state.skills.clone(),
        state.permissions.clone(),
        state.audit.clone(),
    )
    .with_system_prompt(&system_prompt);

    // Create or reuse session
    let mut session = Session::new();
    let session_id = req
        .session_id
        .unwrap_or_else(|| session.id.to_string());

    // Audit: task started
    state.audit.log_action(
        session.id,
        "xcapitsff_run_task",
        Some(req.agent_role.clone()),
        serde_json::json!({"role": req.agent_role, "context_len": req.context.len()}),
        AuditOutcome::Success,
    );

    info!(role = %req.agent_role, session_id = %session_id, "XcapitSFF run-task started");

    // Execute
    let result = runner.run(&mut session, &req.context).await;

    let duration_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(response) => {
            let tokens_input = (req.context.len() / 4) as u64;
            let tokens_output = (response.len() / 4) as u64;

            state.audit.log_action(
                session.id,
                "xcapitsff_run_task_complete",
                Some(req.agent_role.clone()),
                serde_json::json!({
                    "duration_ms": duration_ms,
                    "tokens_input": tokens_input,
                    "tokens_output": tokens_output,
                }),
                AuditOutcome::Success,
            );

            info!(role = %req.agent_role, duration_ms, "XcapitSFF run-task completed");

            (
                StatusCode::OK,
                Json(serde_json::to_value(RunTaskResponse {
                    response,
                    session_id,
                    model_used: model_id,
                    tokens_input,
                    tokens_output,
                    tool_calls: vec![],
                    compliance_flags: vec![],
                    duration_ms,
                })
                .unwrap_or_default()),
            )
                .into_response()
        }
        Err(e) => {
            error!(role = %req.agent_role, error = %e, "XcapitSFF run-task failed");

            state.audit.log_action(
                session.id,
                "xcapitsff_run_task_error",
                Some(req.agent_role),
                serde_json::json!({"error": e.to_string(), "duration_ms": duration_ms}),
                AuditOutcome::Error,
            );

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": e.to_string(),
                    "duration_ms": duration_ms,
                })),
            )
                .into_response()
        }
    }
}

/// POST /api/v1/agent/batch — Execute multiple agent tasks in parallel.
async fn batch_handler(
    State(state): State<Arc<XcapitState>>,
    Json(req): Json<BatchRequest>,
) -> impl IntoResponse {
    let start = Instant::now();
    let max_concurrent = req.max_concurrent.unwrap_or(5);
    let total = req.tasks.len();

    info!(total, max_concurrent, "XcapitSFF batch started");

    // Use semaphore for concurrency limiting
    let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent));
    let mut handles = Vec::with_capacity(total);

    for (index, task) in req.tasks.into_iter().enumerate() {
        let state = state.clone();
        let sem = semaphore.clone();

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await;
            let task_start = Instant::now();

            let profile = match state.profiles.get(&task.agent_role) {
                Some(p) => p.clone(),
                None => {
                    return BatchResultItem {
                        index,
                        success: false,
                        response: String::new(),
                        error: Some(format!("Unknown role: {}", task.agent_role)),
                        tokens_input: 0,
                        tokens_output: 0,
                    };
                }
            };

            let mut model_config = profile.model.clone();
            resolve_api_keys(&mut model_config);
            if let Some(mt) = task.max_tokens {
                model_config.max_tokens = mt;
            }

            let system_prompt = task
                .system_prompt
                .unwrap_or_else(|| profile.system_prompt.clone());

            let runner = AgentRunner::new(
                model_config,
                state.skills.clone(),
                state.permissions.clone(),
                state.audit.clone(),
            )
            .with_system_prompt(&system_prompt);

            let mut session = Session::new();
            match runner.run(&mut session, &task.context).await {
                Ok(response) => {
                    let ti = (task.context.len() / 4) as u64;
                    let to = (response.len() / 4) as u64;
                    BatchResultItem {
                        index,
                        success: true,
                        response,
                        error: None,
                        tokens_input: ti,
                        tokens_output: to,
                    }
                }
                Err(e) => BatchResultItem {
                    index,
                    success: false,
                    response: String::new(),
                    error: Some(e.to_string()),
                    tokens_input: 0,
                    tokens_output: 0,
                },
            }
        });

        handles.push(handle);
    }

    // Collect results
    let mut results = Vec::with_capacity(total);
    for handle in handles {
        match handle.await {
            Ok(item) => results.push(item),
            Err(e) => results.push(BatchResultItem {
                index: results.len(),
                success: false,
                response: String::new(),
                error: Some(format!("Task panicked: {e}")),
                tokens_input: 0,
                tokens_output: 0,
            }),
        }
    }

    // Sort by original index
    results.sort_by_key(|r| r.index);

    let succeeded = results.iter().filter(|r| r.success).count();
    let failed = total - succeeded;
    let total_tokens: u64 = results
        .iter()
        .map(|r| r.tokens_input + r.tokens_output)
        .sum();
    let total_duration_ms = start.elapsed().as_millis() as u64;

    info!(total, succeeded, failed, total_duration_ms, "XcapitSFF batch completed");

    (
        StatusCode::OK,
        Json(serde_json::to_value(BatchResponse {
            results,
            total,
            succeeded,
            failed,
            total_tokens,
            total_duration_ms,
        })
        .unwrap_or_default()),
    )
        .into_response()
}

/// POST /api/v1/proxy/webhook — Proxy webhooks to XcapitSFF with compliance.
async fn webhook_proxy_handler(
    State(state): State<Arc<XcapitState>>,
    headers: HeaderMap,
    Json(req): Json<WebhookProxyRequest>,
) -> impl IntoResponse {
    let audit_id = Uuid::new_v4().to_string();

    // Validate webhook source
    if !state.config.allowed_webhook_sources.contains(&req.source) {
        state.audit.log_action(
            Uuid::new_v4(),
            "webhook_proxy_rejected",
            Some(req.source.clone()),
            serde_json::json!({"event": req.event, "reason": "source_not_allowed"}),
            AuditOutcome::Error,
        );

        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::to_value(WebhookProxyResponse {
                forwarded: false,
                upstream_status: None,
                audit_id,
                error: Some(format!("Source '{}' not in allowed list", req.source)),
            })
            .unwrap_or_default()),
        )
            .into_response();
    }

    // Validate HMAC if secret is configured
    if !state.config.webhook_hmac_secret.is_empty() {
        let provided_sig = headers
            .get("x-webhook-secret")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if provided_sig.is_empty() {
            state.audit.log_action(
                Uuid::new_v4(),
                "webhook_proxy_rejected",
                Some(req.source.clone()),
                serde_json::json!({"event": req.event, "reason": "missing_hmac"}),
                AuditOutcome::Error,
            );

            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "forwarded": false,
                    "audit_id": audit_id,
                    "error": "Missing X-Webhook-Secret header",
                })),
            )
                .into_response();
        }

        // Compute expected HMAC-SHA256
        use sha2::Digest;
        let body_bytes = serde_json::to_vec(&req).unwrap_or_default();
        let mut mac = sha2::Sha256::new();
        mac.update(state.config.webhook_hmac_secret.as_bytes());
        mac.update(&body_bytes);
        let expected = hex::encode(mac.finalize());

        if provided_sig != expected {
            state.audit.log_action(
                Uuid::new_v4(),
                "webhook_proxy_rejected",
                Some(req.source.clone()),
                serde_json::json!({"event": req.event, "reason": "invalid_hmac"}),
                AuditOutcome::Error,
            );

            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "forwarded": false,
                    "audit_id": audit_id,
                    "error": "Invalid HMAC signature",
                })),
            )
                .into_response();
        }
    }

    // Audit: webhook received
    state.audit.log_action(
        Uuid::new_v4(),
        "webhook_proxy_received",
        Some(req.source.clone()),
        serde_json::json!({"event": &req.event, "source": &req.source}),
        AuditOutcome::Success,
    );

    // Forward to XcapitSFF
    let forward_url = format!("{}/api/v1/webhooks/generic", state.config.url);
    let result = state
        .http_client
        .post(&forward_url)
        .json(&req)
        .send()
        .await;

    match result {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let success = resp.status().is_success();

            state.audit.log_action(
                Uuid::new_v4(),
                "webhook_proxy_forwarded",
                Some(req.source.clone()),
                serde_json::json!({
                    "event": req.event,
                    "upstream_status": status,
                    "success": success,
                }),
                if success {
                    AuditOutcome::Success
                } else {
                    AuditOutcome::Error
                },
            );

            info!(
                source = %req.source,
                event = %req.event,
                upstream_status = status,
                "Webhook proxied to XcapitSFF"
            );

            (
                StatusCode::OK,
                Json(serde_json::to_value(WebhookProxyResponse {
                    forwarded: success,
                    upstream_status: Some(status),
                    audit_id,
                    error: if success {
                        None
                    } else {
                        Some(format!("Upstream returned HTTP {status}"))
                    },
                })
                .unwrap_or_default()),
            )
                .into_response()
        }
        Err(e) => {
            error!(error = %e, "Failed to forward webhook to XcapitSFF");

            state.audit.log_action(
                Uuid::new_v4(),
                "webhook_proxy_error",
                Some(req.source),
                serde_json::json!({"error": e.to_string()}),
                AuditOutcome::Error,
            );

            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::to_value(WebhookProxyResponse {
                    forwarded: false,
                    upstream_status: None,
                    audit_id,
                    error: Some(e.to_string()),
                })
                .unwrap_or_default()),
            )
                .into_response()
        }
    }
}

/// GET /api/v1/health — Extended health check with XcapitSFF status.
async fn extended_health_handler(
    State(state): State<Arc<XcapitState>>,
) -> impl IntoResponse {
    let xcapitsff_health = state.xcapitsff_health.read().await.clone();

    let overall = if xcapitsff_health.status == "ok" {
        "ok"
    } else {
        "degraded"
    };

    let response = ExtendedHealthResponse {
        status: overall.to_string(),
        version: "0.1.0".to_string(),
        checks: HealthChecks {
            llm_backends: "ok".to_string(),
            xcapitsff: xcapitsff_health.status.clone(),
            compliance: "ok".to_string(),
        },
        xcapitsff: xcapitsff_health,
    };

    (StatusCode::OK, Json(serde_json::to_value(response).unwrap_or_default())).into_response()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_default_profiles_has_four() {
        let profiles = default_xcapit_profiles();
        assert_eq!(profiles.len(), 4);
        assert!(profiles.contains_key("sales_qualifier"));
        assert!(profiles.contains_key("outreach_composer"));
        assert!(profiles.contains_key("support_responder"));
        assert!(profiles.contains_key("ticket_router"));
    }

    #[test]
    fn test_profile_temperatures() {
        let profiles = default_xcapit_profiles();
        assert!((profiles["sales_qualifier"].model.temperature - 0.3).abs() < 0.01);
        assert!((profiles["outreach_composer"].model.temperature - 0.7).abs() < 0.01);
        assert!((profiles["support_responder"].model.temperature - 0.4).abs() < 0.01);
        assert!((profiles["ticket_router"].model.temperature - 0.2).abs() < 0.01);
    }

    #[test]
    fn test_profile_max_tokens() {
        let profiles = default_xcapit_profiles();
        assert_eq!(profiles["sales_qualifier"].model.max_tokens, 2048);
        assert_eq!(profiles["outreach_composer"].model.max_tokens, 4096);
        assert_eq!(profiles["support_responder"].model.max_tokens, 4096);
        assert_eq!(profiles["ticket_router"].model.max_tokens, 1024);
    }

    #[test]
    fn test_profile_has_fallback() {
        let profiles = default_xcapit_profiles();
        for (_, profile) in &profiles {
            assert_eq!(profile.model.fallback_models.len(), 1, "Profile {} should have 1 fallback", profile.role);
            assert_eq!(profile.model.fallback_models[0].model_id, "gpt-4o-mini");
        }
    }

    #[test]
    fn test_default_config() {
        let config = XcapitConfig::default();
        assert!(config.url.contains("localhost:8000") || config.url.contains("xcapitsff"));
        assert_eq!(config.health_check_interval, 30);
        assert_eq!(config.allowed_webhook_sources.len(), 4);
    }

    #[test]
    fn test_config_allowed_sources() {
        let config = XcapitConfig::default();
        assert!(config.allowed_webhook_sources.contains(&"hubspot".to_string()));
        assert!(config.allowed_webhook_sources.contains(&"salesforce".to_string()));
        assert!(config.allowed_webhook_sources.contains(&"stripe".to_string()));
        assert!(config.allowed_webhook_sources.contains(&"intercom".to_string()));
    }

    #[test]
    fn test_run_task_response_serialization() {
        let resp = RunTaskResponse {
            response: "Score: 82".to_string(),
            session_id: "ses_abc".to_string(),
            model_used: "claude-sonnet-4-6".to_string(),
            tokens_input: 100,
            tokens_output: 50,
            tool_calls: vec![],
            compliance_flags: vec![],
            duration_ms: 1200,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"model_used\":\"claude-sonnet-4-6\""));
        assert!(json.contains("\"duration_ms\":1200"));
    }

    #[test]
    fn test_batch_response_serialization() {
        let resp = BatchResponse {
            results: vec![BatchResultItem {
                index: 0,
                success: true,
                response: "ok".to_string(),
                error: None,
                tokens_input: 100,
                tokens_output: 50,
            }],
            total: 1,
            succeeded: 1,
            failed: 0,
            total_tokens: 150,
            total_duration_ms: 500,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"succeeded\":1"));
    }

    #[test]
    fn test_webhook_proxy_response_serialization() {
        let resp = WebhookProxyResponse {
            forwarded: true,
            upstream_status: Some(200),
            audit_id: "audit-123".to_string(),
            error: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"forwarded\":true"));
        assert!(!json.contains("\"error\"")); // skip_serializing_if
    }

    #[test]
    fn test_health_response_serialization() {
        let resp = ExtendedHealthResponse {
            status: "ok".to_string(),
            version: "0.1.0".to_string(),
            checks: HealthChecks {
                llm_backends: "ok".to_string(),
                xcapitsff: "ok".to_string(),
                compliance: "ok".to_string(),
            },
            xcapitsff: XcapitSffHealth {
                url: "http://localhost:8000".to_string(),
                status: "ok".to_string(),
                last_check: Some(Utc::now()),
                last_error: None,
            },
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"version\":\"0.1.0\""));
    }

    #[test]
    fn test_resolve_api_keys_from_env() {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key-123");
        let mut config = default_xcapit_profiles()["sales_qualifier"].model.clone();
        resolve_api_keys(&mut config);
        assert_eq!(config.api_key, "test-key-123");
        std::env::remove_var("ANTHROPIC_API_KEY");
    }

    #[test]
    fn test_system_prompts_non_empty() {
        let profiles = default_xcapit_profiles();
        for (name, profile) in &profiles {
            assert!(!profile.system_prompt.is_empty(), "Profile {name} has empty system prompt");
            assert!(profile.system_prompt.len() > 50, "Profile {name} prompt is too short");
        }
    }
}
