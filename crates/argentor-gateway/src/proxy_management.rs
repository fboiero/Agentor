//! REST API endpoints for managing the MCP proxy infrastructure.
//!
//! Provides HTTP endpoints for managing credentials, token pools, and
//! monitoring the proxy orchestrator. All routes are mounted under
//! `/api/v1/proxy-management/`.

use axum::{
    extract::{Json, Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use argentor_mcp::credential_vault::{CredentialPolicy, CredentialVault};
use argentor_mcp::proxy_orchestrator::OrchestratorMetrics;
use argentor_mcp::token_pool::{TokenPool, TokenTier};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Unified error type for proxy-management handlers.
#[derive(Debug)]
pub enum ProxyManagementError {
    /// The requested resource was not found.
    NotFound(String),
    /// The request body or parameters were invalid.
    BadRequest(String),
    /// A resource already exists with the given identifier.
    Conflict(String),
    /// An internal error occurred while processing the request.
    Internal(String),
}

impl std::fmt::Display for ProxyManagementError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "Not found: {msg}"),
            Self::BadRequest(msg) => write!(f, "Bad request: {msg}"),
            Self::Conflict(msg) => write!(f, "Conflict: {msg}"),
            Self::Internal(msg) => write!(f, "Internal error: {msg}"),
        }
    }
}

impl IntoResponse for ProxyManagementError {
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

/// State for proxy management endpoints.
///
/// Holds references to the credential vault, token pool, and an optional
/// snapshot of orchestrator metrics. The vault and pool use `std::sync::RwLock`
/// internally, so their methods can be called directly from async handlers
/// without holding locks across await points.
pub struct ProxyManagementState {
    /// Credential vault for API key management.
    pub vault: Arc<CredentialVault>,
    /// Token pool for per-provider token management.
    pub pool: Arc<TokenPool>,
    /// Proxy orchestrator metrics snapshot (read-only view).
    ///
    /// Updated periodically by the orchestrator; handlers read it without
    /// touching the orchestrator's internal `std::sync::RwLock`.
    pub orchestrator_metrics: Arc<RwLock<Option<OrchestratorMetrics>>>,
}

impl ProxyManagementState {
    /// Create a new proxy management state.
    pub fn new(
        vault: Arc<CredentialVault>,
        pool: Arc<TokenPool>,
    ) -> Self {
        Self {
            vault,
            pool,
            orchestrator_metrics: Arc::new(RwLock::new(None)),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Redact a secret value for display purposes.
///
/// If the value is longer than 8 characters, show the first 4 and last 3
/// characters separated by `"..."`. Otherwise, return `"***"`.
pub fn redact_value(value: &str) -> String {
    if value.len() > 8 {
        let start = &value[..4];
        let end = &value[value.len() - 3..];
        format!("{start}...{end}")
    } else {
        "***".to_string()
    }
}

/// Parse a tier string into a [`TokenTier`].
///
/// Supported values: `"production"`, `"development"`, `"free"`, `"backup"`.
/// Returns `None` for unrecognized values.
fn parse_tier(s: &str) -> Option<TokenTier> {
    match s.to_lowercase().as_str() {
        "production" => Some(TokenTier::Production),
        "development" => Some(TokenTier::Development),
        "free" => Some(TokenTier::Free),
        "backup" => Some(TokenTier::Backup),
        _ => None,
    }
}

/// Format a [`TokenTier`] as a human-readable string.
fn tier_to_string(tier: &TokenTier) -> String {
    match tier {
        TokenTier::Production => "production".to_string(),
        TokenTier::Development => "development".to_string(),
        TokenTier::Free => "free".to_string(),
        TokenTier::Backup => "backup".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// Response type for credential listings (value always redacted).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialInfo {
    /// Unique credential identifier.
    pub id: String,
    /// Provider name.
    pub provider: String,
    /// Logical key name.
    pub key_name: String,
    /// Redacted preview of the credential value.
    pub value_preview: String,
    /// When the credential was created or last rotated.
    pub created_at: String,
    /// Optional expiry time.
    pub expires_at: Option<String>,
    /// Cumulative usage count.
    pub usage_count: u64,
    /// Whether the credential is enabled.
    pub enabled: bool,
}

/// Request body for adding a new credential.
#[derive(Debug, Deserialize)]
pub struct AddCredentialRequest {
    /// Unique identifier for the credential.
    pub id: String,
    /// Provider name (e.g. "openai", "anthropic").
    pub provider: String,
    /// Logical key name (e.g. "OPENAI_API_KEY").
    pub key_name: String,
    /// The actual secret value.
    pub value: String,
    /// Optional maximum calls per minute.
    pub max_calls_per_minute: Option<u32>,
    /// Optional maximum daily usage.
    pub max_daily_usage: Option<u64>,
}

/// Request body for rotating a credential.
#[derive(Debug, Deserialize)]
pub struct RotateRequest {
    /// The new secret value to replace the current one.
    pub new_value: String,
}

/// Request body for enabling/disabling a credential or token.
#[derive(Debug, Deserialize)]
pub struct EnabledRequest {
    /// Whether the resource should be enabled.
    pub enabled: bool,
}

/// Credential stats response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialStatsResponse {
    /// Total number of credentials.
    pub total_credentials: usize,
    /// Number of active (enabled + not expired) credentials.
    pub active_credentials: usize,
    /// Number of expired credentials.
    pub expired_credentials: usize,
    /// Credentials per provider.
    pub providers: HashMap<String, usize>,
    /// Total usage across all credentials.
    pub total_usage: u64,
}

/// Response type for token listings (value always redacted).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    /// Unique token identifier.
    pub id: String,
    /// Provider name.
    pub provider: String,
    /// Redacted preview of the token value.
    pub value_preview: String,
    /// Token tier classification.
    pub tier: String,
    /// Maximum calls per minute.
    pub max_per_minute: u32,
    /// Optional daily quota.
    pub daily_quota: Option<u64>,
    /// Current daily usage.
    pub daily_usage: u64,
    /// Lifetime total usage.
    pub total_usage: u64,
    /// Lifetime total errors.
    pub total_errors: u64,
    /// Whether the token is enabled.
    pub enabled: bool,
    /// Token weight for weighted selection.
    pub weight: u32,
}

/// Request body for adding a new token.
#[derive(Debug, Deserialize)]
pub struct AddTokenRequest {
    /// Unique identifier for the token.
    pub id: String,
    /// Provider name (e.g. "openai", "anthropic").
    pub provider: String,
    /// The actual token/API key value.
    pub value: String,
    /// Tier classification: "production", "development", "free", "backup".
    pub tier: String,
    /// Maximum calls per minute.
    pub max_per_minute: u32,
    /// Optional daily quota.
    pub daily_quota: Option<u64>,
    /// Optional weight for weighted selection (default: 1).
    pub weight: Option<u32>,
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the proxy management REST API sub-router.
///
/// All routes are nested under `/api/v1/proxy-management/`.
pub fn proxy_management_router(state: Arc<ProxyManagementState>) -> Router {
    Router::new()
        // Credentials
        .route(
            "/api/v1/proxy-management/credentials",
            get(list_credentials).post(add_credential),
        )
        .route(
            "/api/v1/proxy-management/credentials/stats",
            get(credential_stats),
        )
        .route(
            "/api/v1/proxy-management/credentials/{id}",
            get(get_credential).delete(remove_credential),
        )
        .route(
            "/api/v1/proxy-management/credentials/{id}/rotate",
            post(rotate_credential),
        )
        .route(
            "/api/v1/proxy-management/credentials/{id}/enabled",
            put(set_credential_enabled),
        )
        // Token pool
        .route(
            "/api/v1/proxy-management/tokens",
            get(list_tokens).post(add_token),
        )
        .route(
            "/api/v1/proxy-management/tokens/stats",
            get(token_stats),
        )
        .route(
            "/api/v1/proxy-management/tokens/health/{provider}",
            get(token_pool_health),
        )
        .route(
            "/api/v1/proxy-management/tokens/{id}",
            delete(remove_token),
        )
        // Orchestrator
        .route(
            "/api/v1/proxy-management/orchestrator/metrics",
            get(orchestrator_metrics),
        )
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Handlers — Credentials
// ---------------------------------------------------------------------------

/// List all credentials with redacted values.
async fn list_credentials(
    State(state): State<Arc<ProxyManagementState>>,
) -> Result<Json<Vec<CredentialInfo>>, ProxyManagementError> {
    let all = state.vault.list_all();
    let infos: Vec<CredentialInfo> = all
        .iter()
        .map(|c| CredentialInfo {
            id: c.id.clone(),
            provider: c.provider.clone(),
            key_name: c.key_name.clone(),
            value_preview: redact_value(&c.value),
            created_at: c.created_at.to_rfc3339(),
            expires_at: c.expires_at.map(|e| e.to_rfc3339()),
            usage_count: c.usage_count,
            enabled: c.enabled,
        })
        .collect();
    Ok(Json(infos))
}

/// Add a new credential to the vault.
async fn add_credential(
    State(state): State<Arc<ProxyManagementState>>,
    Json(req): Json<AddCredentialRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ProxyManagementError> {
    if req.id.trim().is_empty() {
        return Err(ProxyManagementError::BadRequest(
            "Credential ID must not be empty".to_string(),
        ));
    }

    if req.provider.trim().is_empty() {
        return Err(ProxyManagementError::BadRequest(
            "Provider must not be empty".to_string(),
        ));
    }

    if req.value.is_empty() {
        return Err(ProxyManagementError::BadRequest(
            "Credential value must not be empty".to_string(),
        ));
    }

    let policy = CredentialPolicy {
        max_calls_per_minute: req.max_calls_per_minute,
        max_daily_usage: req.max_daily_usage,
        auto_rotate: false,
        fallback_credential_id: None,
    };

    state
        .vault
        .add(&req.id, &req.provider, &req.key_name, &req.value, policy)
        .map_err(|e| {
            ProxyManagementError::Conflict(format!("Failed to add credential: {e}"))
        })?;

    info!(credential_id = %req.id, provider = %req.provider, "Credential added");

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "created": true,
            "id": req.id,
            "provider": req.provider,
        })),
    ))
}

/// Get a single credential by ID (value redacted).
async fn get_credential(
    State(state): State<Arc<ProxyManagementState>>,
    Path(id): Path<String>,
) -> Result<Json<CredentialInfo>, ProxyManagementError> {
    let cred = state.vault.get(&id).ok_or_else(|| {
        ProxyManagementError::NotFound(format!("Credential '{id}' not found"))
    })?;

    Ok(Json(CredentialInfo {
        id: cred.id,
        provider: cred.provider,
        key_name: cred.key_name,
        value_preview: redact_value(&cred.value),
        created_at: cred.created_at.to_rfc3339(),
        expires_at: cred.expires_at.map(|e| e.to_rfc3339()),
        usage_count: cred.usage_count,
        enabled: cred.enabled,
    }))
}

/// Remove a credential from the vault.
async fn remove_credential(
    State(state): State<Arc<ProxyManagementState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ProxyManagementError> {
    state.vault.remove(&id).map_err(|e| {
        ProxyManagementError::NotFound(format!("Failed to remove credential: {e}"))
    })?;

    info!(credential_id = %id, "Credential removed");

    Ok(Json(serde_json::json!({
        "deleted": true,
        "id": id,
    })))
}

/// Rotate a credential to a new value.
async fn rotate_credential(
    State(state): State<Arc<ProxyManagementState>>,
    Path(id): Path<String>,
    Json(req): Json<RotateRequest>,
) -> Result<Json<serde_json::Value>, ProxyManagementError> {
    if req.new_value.is_empty() {
        return Err(ProxyManagementError::BadRequest(
            "New value must not be empty".to_string(),
        ));
    }

    state.vault.rotate(&id, &req.new_value).map_err(|e| {
        ProxyManagementError::NotFound(format!("Failed to rotate credential: {e}"))
    })?;

    info!(credential_id = %id, "Credential rotated");

    Ok(Json(serde_json::json!({
        "rotated": true,
        "id": id,
        "value_preview": redact_value(&req.new_value),
    })))
}

/// Enable or disable a credential.
async fn set_credential_enabled(
    State(state): State<Arc<ProxyManagementState>>,
    Path(id): Path<String>,
    Json(req): Json<EnabledRequest>,
) -> Result<Json<serde_json::Value>, ProxyManagementError> {
    state.vault.set_enabled(&id, req.enabled).map_err(|e| {
        ProxyManagementError::NotFound(format!(
            "Failed to set enabled state for credential: {e}"
        ))
    })?;

    info!(credential_id = %id, enabled = req.enabled, "Credential enabled state changed");

    Ok(Json(serde_json::json!({
        "id": id,
        "enabled": req.enabled,
    })))
}

/// Get credential statistics.
async fn credential_stats(
    State(state): State<Arc<ProxyManagementState>>,
) -> Result<Json<CredentialStatsResponse>, ProxyManagementError> {
    let stats = state.vault.stats();

    Ok(Json(CredentialStatsResponse {
        total_credentials: stats.total_credentials,
        active_credentials: stats.active_credentials,
        expired_credentials: stats.expired_credentials,
        providers: stats.providers,
        total_usage: stats.total_usage,
    }))
}

// ---------------------------------------------------------------------------
// Handlers — Token Pool
// ---------------------------------------------------------------------------

/// List all tokens with redacted values.
async fn list_tokens(
    State(state): State<Arc<ProxyManagementState>>,
) -> Result<Json<Vec<TokenInfo>>, ProxyManagementError> {
    let all = state.pool.list_all();
    let infos: Vec<TokenInfo> = all
        .iter()
        .map(|t| TokenInfo {
            id: t.id.clone(),
            provider: t.provider.clone(),
            value_preview: redact_value(&t.token_value),
            tier: tier_to_string(&t.tier),
            max_per_minute: t.rate_limit.max_per_minute,
            daily_quota: t.daily_quota,
            daily_usage: t.daily_usage,
            total_usage: t.total_usage,
            total_errors: t.total_errors,
            enabled: t.enabled,
            weight: t.weight,
        })
        .collect();
    Ok(Json(infos))
}

/// Add a new token to the pool.
async fn add_token(
    State(state): State<Arc<ProxyManagementState>>,
    Json(req): Json<AddTokenRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ProxyManagementError> {
    if req.id.trim().is_empty() {
        return Err(ProxyManagementError::BadRequest(
            "Token ID must not be empty".to_string(),
        ));
    }

    if req.provider.trim().is_empty() {
        return Err(ProxyManagementError::BadRequest(
            "Provider must not be empty".to_string(),
        ));
    }

    if req.value.is_empty() {
        return Err(ProxyManagementError::BadRequest(
            "Token value must not be empty".to_string(),
        ));
    }

    let tier = parse_tier(&req.tier).ok_or_else(|| {
        ProxyManagementError::BadRequest(format!(
            "Invalid tier '{}'. Valid tiers: production, development, free, backup",
            req.tier
        ))
    })?;

    let weight = req.weight.unwrap_or(1);

    state
        .pool
        .add_token(
            &req.id,
            &req.provider,
            &req.value,
            tier,
            req.max_per_minute,
            req.daily_quota,
            weight,
        )
        .map_err(|e| {
            ProxyManagementError::Conflict(format!("Failed to add token: {e}"))
        })?;

    info!(token_id = %req.id, provider = %req.provider, tier = %req.tier, "Token added to pool");

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "created": true,
            "id": req.id,
            "provider": req.provider,
            "tier": req.tier,
        })),
    ))
}

/// Remove a token from the pool.
async fn remove_token(
    State(state): State<Arc<ProxyManagementState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ProxyManagementError> {
    state.pool.remove_token(&id).map_err(|e| {
        ProxyManagementError::NotFound(format!("Failed to remove token: {e}"))
    })?;

    info!(token_id = %id, "Token removed from pool");

    Ok(Json(serde_json::json!({
        "deleted": true,
        "id": id,
    })))
}

/// Get pool health for a specific provider.
async fn token_pool_health(
    State(state): State<Arc<ProxyManagementState>>,
    Path(provider): Path<String>,
) -> Result<Json<serde_json::Value>, ProxyManagementError> {
    let health = state.pool.pool_health(&provider);
    let body = serde_json::json!({
        "provider": health.provider,
        "total_tokens": health.total_tokens,
        "available_tokens": health.available_tokens,
        "exhausted_tokens": health.exhausted_tokens,
        "rate_limited_tokens": health.rate_limited_tokens,
        "disabled_tokens": health.disabled_tokens,
        "total_daily_remaining": health.total_daily_remaining,
        "estimated_calls_available": health.estimated_calls_available,
    });
    Ok(Json(body))
}

/// Get global token pool statistics.
async fn token_stats(
    State(state): State<Arc<ProxyManagementState>>,
) -> Result<Json<serde_json::Value>, ProxyManagementError> {
    let stats = state.pool.stats();
    let body = serde_json::json!({
        "total_tokens": stats.total_tokens,
        "total_providers": stats.total_providers,
        "total_usage": stats.total_usage,
        "total_errors": stats.total_errors,
        "per_provider": stats.per_provider,
    });
    Ok(Json(body))
}

// ---------------------------------------------------------------------------
// Handlers — Orchestrator
// ---------------------------------------------------------------------------

/// Get orchestrator metrics snapshot.
async fn orchestrator_metrics(
    State(state): State<Arc<ProxyManagementState>>,
) -> Result<Json<serde_json::Value>, ProxyManagementError> {
    let metrics = state.orchestrator_metrics.read().await;
    match metrics.as_ref() {
        Some(m) => {
            let body = serde_json::json!({
                "total_proxies": m.total_proxies,
                "active_proxies": m.active_proxies,
                "circuit_open_proxies": m.circuit_open_proxies,
                "total_calls": m.total_calls,
                "total_failures": m.total_failures,
                "calls_per_group": m.calls_per_group,
                "routing_rules_count": m.routing_rules_count,
            });
            Ok(Json(body))
        }
        None => Ok(Json(serde_json::json!({
            "message": "No orchestrator metrics available yet",
            "total_proxies": 0,
            "active_proxies": 0,
            "circuit_open_proxies": 0,
            "total_calls": 0,
            "total_failures": 0,
            "calls_per_group": {},
            "routing_rules_count": 0,
        }))),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use argentor_mcp::token_pool::SelectionStrategy;
    use axum::body::Body;
    use axum::http::{self, Request};
    use tower::ServiceExt;

    /// Build a test router with fresh state.
    fn test_app() -> (Router, Arc<ProxyManagementState>) {
        let vault = Arc::new(CredentialVault::new());
        let pool = Arc::new(TokenPool::new(SelectionStrategy::MostRemaining));
        let state = Arc::new(ProxyManagementState::new(vault, pool));
        let router = proxy_management_router(state.clone());
        (router, state)
    }

    /// Send a GET request and return the response body as a JSON Value.
    async fn get_json(app: &Router, uri: &str) -> (StatusCode, serde_json::Value) {
        let request = Request::builder()
            .method(http::Method::GET)
            .uri(uri)
            .body(Body::empty())
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        (status, json)
    }

    /// Send a POST request with a JSON body and return the response.
    async fn post_json(
        app: &Router,
        uri: &str,
        body: serde_json::Value,
    ) -> (StatusCode, serde_json::Value) {
        let request = Request::builder()
            .method(http::Method::POST)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        (status, json)
    }

    /// Send a PUT request with a JSON body and return the response.
    async fn put_json(
        app: &Router,
        uri: &str,
        body: serde_json::Value,
    ) -> (StatusCode, serde_json::Value) {
        let request = Request::builder()
            .method(http::Method::PUT)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        (status, json)
    }

    /// Send a DELETE request and return the response.
    async fn delete_json(app: &Router, uri: &str) -> (StatusCode, serde_json::Value) {
        let request = Request::builder()
            .method(http::Method::DELETE)
            .uri(uri)
            .body(Body::empty())
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        (status, json)
    }

    // -- 1. Redaction helper ---------------------------------------------------

    #[test]
    fn test_redact_value_long() {
        let result = redact_value("sk-1234567890abcdef");
        assert_eq!(result, "sk-1...def");
    }

    #[test]
    fn test_redact_value_short() {
        let result = redact_value("short");
        assert_eq!(result, "***");
    }

    #[test]
    fn test_redact_value_boundary() {
        // Exactly 8 characters — should be "***"
        let result = redact_value("12345678");
        assert_eq!(result, "***");

        // 9 characters — should show first 4 + last 3
        let result = redact_value("123456789");
        assert_eq!(result, "1234...789");
    }

    // -- 2. List credentials (empty) -------------------------------------------

    #[tokio::test]
    async fn test_list_credentials_empty() {
        let (app, _state) = test_app();
        let (status, json) = get_json(&app, "/api/v1/proxy-management/credentials").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json.as_array().unwrap().len(), 0);
    }

    // -- 3. Add and list credentials (value redacted) --------------------------

    #[tokio::test]
    async fn test_add_and_list_credentials() {
        let (app, _state) = test_app();

        let (status, json) = post_json(
            &app,
            "/api/v1/proxy-management/credentials",
            serde_json::json!({
                "id": "cred1",
                "provider": "openai",
                "key_name": "OPENAI_API_KEY",
                "value": "sk-1234567890abcdef",
            }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(json["created"], true);
        assert_eq!(json["id"], "cred1");

        let (status, json) = get_json(&app, "/api/v1/proxy-management/credentials").await;
        assert_eq!(status, StatusCode::OK);
        let creds = json.as_array().unwrap();
        assert_eq!(creds.len(), 1);
        assert_eq!(creds[0]["id"], "cred1");
        assert_eq!(creds[0]["provider"], "openai");
        // Value must be redacted
        assert_eq!(creds[0]["value_preview"], "sk-1...def");
        assert_ne!(creds[0]["value_preview"], "sk-1234567890abcdef");
    }

    // -- 4. Get credential by ID -----------------------------------------------

    #[tokio::test]
    async fn test_get_credential_by_id() {
        let (app, state) = test_app();

        state
            .vault
            .add(
                "c1",
                "anthropic",
                "API_KEY",
                "ak-secret-value-long-enough",
                CredentialPolicy::default(),
            )
            .unwrap();

        let (status, json) =
            get_json(&app, "/api/v1/proxy-management/credentials/c1").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["id"], "c1");
        assert_eq!(json["provider"], "anthropic");
        assert_eq!(json["value_preview"], "ak-s...ugh");
    }

    #[tokio::test]
    async fn test_get_credential_not_found() {
        let (app, _state) = test_app();
        let (status, _json) =
            get_json(&app, "/api/v1/proxy-management/credentials/nonexistent").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    // -- 5. Remove credential --------------------------------------------------

    #[tokio::test]
    async fn test_remove_credential() {
        let (app, state) = test_app();

        state
            .vault
            .add(
                "del1",
                "openai",
                "key",
                "sk-secret-long-enough",
                CredentialPolicy::default(),
            )
            .unwrap();

        let (status, json) =
            delete_json(&app, "/api/v1/proxy-management/credentials/del1").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["deleted"], true);

        // Verify it's gone
        let (status, _json) =
            get_json(&app, "/api/v1/proxy-management/credentials/del1").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    // -- 6. Rotate credential --------------------------------------------------

    #[tokio::test]
    async fn test_rotate_credential() {
        let (app, state) = test_app();

        state
            .vault
            .add(
                "rot1",
                "openai",
                "key",
                "sk-old-value-long-enough",
                CredentialPolicy::default(),
            )
            .unwrap();

        // Record some usage
        state.vault.record_usage("rot1").unwrap();
        state.vault.record_usage("rot1").unwrap();

        let (status, json) = post_json(
            &app,
            "/api/v1/proxy-management/credentials/rot1/rotate",
            serde_json::json!({ "new_value": "sk-new-rotated-value" }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["rotated"], true);
        assert_eq!(json["value_preview"], "sk-n...lue");

        // Verify usage was reset
        let cred = state.vault.get("rot1").unwrap();
        assert_eq!(cred.usage_count, 0);
        assert_eq!(cred.value, "sk-new-rotated-value");
    }

    // -- 7. Enable/disable credential ------------------------------------------

    #[tokio::test]
    async fn test_enable_disable_credential() {
        let (app, state) = test_app();

        state
            .vault
            .add(
                "en1",
                "openai",
                "key",
                "sk-value-long-enough",
                CredentialPolicy::default(),
            )
            .unwrap();

        // Disable
        let (status, json) = put_json(
            &app,
            "/api/v1/proxy-management/credentials/en1/enabled",
            serde_json::json!({ "enabled": false }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["enabled"], false);

        // Verify disabled
        assert!(!state.vault.is_available("en1"));

        // Re-enable
        let (status, json) = put_json(
            &app,
            "/api/v1/proxy-management/credentials/en1/enabled",
            serde_json::json!({ "enabled": true }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["enabled"], true);
        assert!(state.vault.is_available("en1"));
    }

    // -- 8. Credential stats ---------------------------------------------------

    #[tokio::test]
    async fn test_credential_stats() {
        let (app, state) = test_app();

        state
            .vault
            .add("s1", "openai", "k", "v1-long-enough-value", CredentialPolicy::default())
            .unwrap();
        state
            .vault
            .add("s2", "anthropic", "k", "v2-long-enough-value", CredentialPolicy::default())
            .unwrap();

        state.vault.record_usage("s1").unwrap();
        state.vault.record_usage("s1").unwrap();

        let (status, json) =
            get_json(&app, "/api/v1/proxy-management/credentials/stats").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["total_credentials"], 2);
        assert_eq!(json["active_credentials"], 2);
        assert_eq!(json["total_usage"], 2);
    }

    // -- 9. Add and list tokens (value redacted) -------------------------------

    #[tokio::test]
    async fn test_add_and_list_tokens() {
        let (app, _state) = test_app();

        let (status, json) = post_json(
            &app,
            "/api/v1/proxy-management/tokens",
            serde_json::json!({
                "id": "tok1",
                "provider": "openai",
                "value": "sk-token-value-long-enough",
                "tier": "production",
                "max_per_minute": 60,
                "daily_quota": 1000,
                "weight": 10,
            }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(json["created"], true);

        let (status, json) = get_json(&app, "/api/v1/proxy-management/tokens").await;
        assert_eq!(status, StatusCode::OK);
        let tokens = json.as_array().unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0]["id"], "tok1");
        assert_eq!(tokens[0]["tier"], "production");
        // Value must be redacted
        assert_eq!(tokens[0]["value_preview"], "sk-t...ugh");
        assert_ne!(tokens[0]["value_preview"], "sk-token-value-long-enough");
    }

    // -- 10. Token pool health -------------------------------------------------

    #[tokio::test]
    async fn test_token_pool_health() {
        let (app, state) = test_app();

        state
            .pool
            .add_token(
                "h1",
                "openai",
                "sk-health-test-val",
                TokenTier::Production,
                60,
                Some(1000),
                10,
            )
            .unwrap();

        let (status, json) =
            get_json(&app, "/api/v1/proxy-management/tokens/health/openai").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["provider"], "openai");
        assert_eq!(json["total_tokens"], 1);
        assert_eq!(json["available_tokens"], 1);
    }

    // -- 11. Token stats -------------------------------------------------------

    #[tokio::test]
    async fn test_token_stats() {
        let (app, state) = test_app();

        state
            .pool
            .add_token("ts1", "openai", "sk-111", TokenTier::Production, 60, Some(500), 5)
            .unwrap();
        state
            .pool
            .add_token("ts2", "anthropic", "ak-222", TokenTier::Development, 30, None, 3)
            .unwrap();

        state.pool.record_usage("ts1").unwrap();

        let (status, json) =
            get_json(&app, "/api/v1/proxy-management/tokens/stats").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["total_tokens"], 2);
        assert_eq!(json["total_providers"], 2);
        assert_eq!(json["total_usage"], 1);
    }

    // -- 12. Remove token ------------------------------------------------------

    #[tokio::test]
    async fn test_remove_token() {
        let (app, state) = test_app();

        state
            .pool
            .add_token("rt1", "openai", "sk-remove", TokenTier::Free, 10, None, 1)
            .unwrap();

        let (status, json) =
            delete_json(&app, "/api/v1/proxy-management/tokens/rt1").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["deleted"], true);

        // Verify it's gone
        let (status, json) = get_json(&app, "/api/v1/proxy-management/tokens").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json.as_array().unwrap().len(), 0);
    }

    // -- 13. Orchestrator metrics (empty) --------------------------------------

    #[tokio::test]
    async fn test_orchestrator_metrics_empty() {
        let (app, _state) = test_app();

        let (status, json) =
            get_json(&app, "/api/v1/proxy-management/orchestrator/metrics").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["total_proxies"], 0);
    }

    // -- 14. Orchestrator metrics (with snapshot) ------------------------------

    #[tokio::test]
    async fn test_orchestrator_metrics_with_snapshot() {
        let (app, state) = test_app();

        let metrics = OrchestratorMetrics {
            total_proxies: 3,
            active_proxies: 2,
            circuit_open_proxies: 1,
            total_calls: 500,
            total_failures: 10,
            calls_per_group: {
                let mut m = HashMap::new();
                m.insert("github".to_string(), 300);
                m.insert("slack".to_string(), 200);
                m
            },
            routing_rules_count: 5,
        };

        *state.orchestrator_metrics.write().await = Some(metrics);

        let (status, json) =
            get_json(&app, "/api/v1/proxy-management/orchestrator/metrics").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["total_proxies"], 3);
        assert_eq!(json["active_proxies"], 2);
        assert_eq!(json["circuit_open_proxies"], 1);
        assert_eq!(json["total_calls"], 500);
        assert_eq!(json["total_failures"], 10);
        assert_eq!(json["routing_rules_count"], 5);
    }

    // -- 15. Validation: empty credential ID -----------------------------------

    #[tokio::test]
    async fn test_add_credential_empty_id_rejected() {
        let (app, _state) = test_app();

        let (status, json) = post_json(
            &app,
            "/api/v1/proxy-management/credentials",
            serde_json::json!({
                "id": "",
                "provider": "openai",
                "key_name": "key",
                "value": "sk-val",
            }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(json["error"].as_str().unwrap().contains("ID"));
    }

    // -- 16. Validation: invalid token tier ------------------------------------

    #[tokio::test]
    async fn test_add_token_invalid_tier_rejected() {
        let (app, _state) = test_app();

        let (status, json) = post_json(
            &app,
            "/api/v1/proxy-management/tokens",
            serde_json::json!({
                "id": "bad",
                "provider": "openai",
                "value": "sk-val",
                "tier": "ultra",
                "max_per_minute": 60,
            }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(json["error"].as_str().unwrap().contains("tier"));
    }

    // -- 17. Duplicate credential rejected -------------------------------------

    #[tokio::test]
    async fn test_add_duplicate_credential_rejected() {
        let (app, state) = test_app();

        state
            .vault
            .add("dup1", "openai", "key", "val", CredentialPolicy::default())
            .unwrap();

        let (status, _json) = post_json(
            &app,
            "/api/v1/proxy-management/credentials",
            serde_json::json!({
                "id": "dup1",
                "provider": "openai",
                "key_name": "key",
                "value": "other-val",
            }),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
    }
}
