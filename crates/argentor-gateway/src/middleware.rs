use crate::rate_limit_per_key::{PerKeyRateLimiter, RateLimitResult};
use argentor_security::RateLimiter;
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
    /// Create a new auth config with the given API keys.
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
    /// Global rate limiter.
    pub rate_limiter: Arc<RateLimiter>,
    /// Authentication configuration.
    pub auth: AuthConfig,
    /// Optional per-API-key rate limiter.
    pub per_key_rate_limiter: Option<Arc<PerKeyRateLimiter>>,
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
        .map(std::string::ToString::to_string);

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

/// Query parameters for API key authentication.
#[derive(serde::Deserialize, Default)]
pub struct AuthQuery {
    /// Optional API key passed as a query parameter.
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

/// Extract the API key from the request headers.
///
/// Checks `Authorization: Bearer <key>` first, then `X-API-Key`.
fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    // Check Authorization: Bearer <key>
    if let Some(auth) = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        return Some(auth.to_string());
    }

    // Check X-API-Key header
    if let Some(key) = headers.get("x-api-key").and_then(|v| v.to_str().ok()) {
        return Some(key.to_string());
    }

    None
}

/// Per-API-key rate limiting middleware.
///
/// Extracts the API key from `Authorization: Bearer <key>` or `X-API-Key` header
/// and checks it against the per-key rate limiter. Returns 429 Too Many Requests
/// with standard rate limit headers when the key is over its quota.
///
/// If no per-key rate limiter is configured, or if no API key is present in the
/// request, the request is passed through without rate limiting.
pub async fn per_key_rate_limit_middleware(
    State(state): State<Arc<MiddlewareState>>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    let limiter = match &state.per_key_rate_limiter {
        Some(l) => l,
        None => return next.run(request).await,
    };

    let api_key = match extract_api_key(&headers) {
        Some(k) => k,
        None => return next.run(request).await,
    };

    // Safety: all `.parse()` calls below convert numeric `.to_string()` output
    // (pure ASCII digits) into `HeaderValue`, which is infallible for ASCII.
    #[allow(clippy::unwrap_used)]
    match limiter.check(&api_key) {
        RateLimitResult::Allow => {
            // Add rate limit info headers to the response
            let mut response = next.run(request).await;
            if let Some(stats) = limiter.stats(&api_key) {
                let headers = response.headers_mut();
                headers.insert(
                    "X-RateLimit-Limit",
                    stats
                        .config
                        .requests_per_minute
                        .to_string()
                        .parse()
                        .unwrap(),
                );
                let remaining = stats
                    .config
                    .requests_per_minute
                    .saturating_sub(stats.requests_this_minute);
                headers.insert(
                    "X-RateLimit-Remaining",
                    remaining.to_string().parse().unwrap(),
                );
            }
            response
        }
        RateLimitResult::Deny {
            reason,
            retry_after,
        } => {
            warn!(
                api_key = %api_key,
                reason = %reason,
                retry_after = retry_after,
                "Per-key rate limit exceeded"
            );
            let body = serde_json::json!({
                "error": "rate_limit_exceeded",
                "message": reason.to_string(),
                "retry_after": retry_after,
            });
            let mut response = (StatusCode::TOO_MANY_REQUESTS, body.to_string()).into_response();
            response
                .headers_mut()
                .insert("Retry-After", retry_after.to_string().parse().unwrap());
            response
                .headers_mut()
                .insert("X-RateLimit-Remaining", "0".parse().unwrap());
            response
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
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
