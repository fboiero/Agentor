use agentor_security::RateLimiter;
use axum::{
    extract::{Query, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use tracing::warn;
use uuid::Uuid;

/// Auth configuration for the gateway.
#[derive(Clone)]
pub struct AuthConfig {
    /// API keys that are allowed to connect. Empty = no auth required.
    pub api_keys: Vec<String>,
}

impl AuthConfig {
    pub fn new(api_keys: Vec<String>) -> Self {
        Self { api_keys }
    }

    /// Returns true if authentication is enabled (at least one key configured).
    pub fn is_enabled(&self) -> bool {
        !self.api_keys.is_empty()
    }
}

/// Shared middleware state.
#[derive(Clone)]
pub struct MiddlewareState {
    pub rate_limiter: Arc<RateLimiter>,
    pub auth: AuthConfig,
}

/// Auth middleware: validates API key from header or query param.
///
/// Checks `Authorization: Bearer <key>` header first, then `?api_key=<key>` query param.
/// If no API keys are configured, all requests are allowed.
pub async fn auth_middleware(
    State(state): State<Arc<MiddlewareState>>,
    headers: HeaderMap,
    query: Query<AuthQuery>,
    request: Request,
    next: Next,
) -> Response {
    if !state.auth.is_enabled() {
        return next.run(request).await;
    }

    // Check Authorization header: "Bearer <key>"
    let key_from_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    // Check query param: ?api_key=<key>
    let key = key_from_header.or_else(|| query.api_key.clone());

    match key {
        Some(k) if state.auth.api_keys.contains(&k) => next.run(request).await,
        Some(_) => {
            warn!("Rejected request: invalid API key");
            (StatusCode::UNAUTHORIZED, "Invalid API key").into_response()
        }
        None => {
            warn!("Rejected request: missing API key");
            (StatusCode::UNAUTHORIZED, "API key required").into_response()
        }
    }
}

#[derive(serde::Deserialize, Default)]
pub struct AuthQuery {
    pub api_key: Option<String>,
}

/// Rate limiting middleware: limits requests per session.
///
/// Uses a default session ID for unauthenticated requests.
/// In production, the session_id would come from the auth token or connection.
pub async fn rate_limit_middleware(
    State(state): State<Arc<MiddlewareState>>,
    request: Request,
    next: Next,
) -> Response {
    // Use a fixed ID for HTTP requests; WebSocket rate limiting happens per-message in the router
    let session_id = Uuid::nil();

    if !state.rate_limiter.check(session_id).await {
        warn!("Rate limited request");
        return (StatusCode::TOO_MANY_REQUESTS, "Rate limit exceeded").into_response();
    }

    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_config_disabled() {
        let config = AuthConfig::new(vec![]);
        assert!(!config.is_enabled());
    }

    #[test]
    fn test_auth_config_enabled() {
        let config = AuthConfig::new(vec!["key123".to_string()]);
        assert!(config.is_enabled());
    }
}
