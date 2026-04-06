use crate::router::InboundMessage;
use crate::server::AppState;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::Deserialize;
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;

/// Configuration for a single webhook endpoint.
#[derive(Deserialize, Clone, Debug)]
pub struct WebhookConfig {
    /// Unique name used in the URL path: POST /webhook/{name}
    pub name: String,
    /// Shared secret for validating incoming requests via X-Webhook-Secret header.
    pub secret: Option<String>,
    /// Template string with `{{payload}}` placeholder that gets replaced with the request body.
    pub agent_prompt_template: String,
    /// Strategy for session management.
    #[serde(default)]
    pub session_strategy: SessionStrategy,
}

/// Determines how sessions are managed for webhook requests.
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(tag = "type", content = "value")]
pub enum SessionStrategy {
    /// Create a new session for every incoming request.
    #[default]
    New,
    /// Reuse sessions based on the value of a specific HTTP header.
    ByHeader(String),
}

/// Shared state for the webhook handler.
pub struct WebhookState {
    /// Configured webhook endpoints.
    pub webhooks: Vec<WebhookConfig>,
}

/// Validate that a request secret matches the configured secret using constant-time comparison.
///
/// Returns `true` if both secrets are equal, using a constant-time algorithm
/// to prevent timing side-channel attacks.
pub fn validate_secret(config_secret: &str, request_secret: &str) -> bool {
    let a = config_secret.as_bytes();
    let b = request_secret.as_bytes();

    if a.len() != b.len() {
        return false;
    }

    // Constant-time comparison to avoid timing attacks
    let mut result: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

/// Replace `{{payload}}` in the template with the given payload string.
pub fn render_template(template: &str, payload: &str) -> String {
    template.replace("{{payload}}", payload)
}

/// Axum handler for incoming webhook POST requests.
///
/// Route: `POST /webhook/{name}`
///
/// Looks up the webhook config by name, validates the secret if configured,
/// renders the prompt template with the request body, forwards the rendered
/// message to the `MessageRouter` for agent processing, and returns the
/// agent response to the caller.
pub async fn webhook_handler(
    Path(name): Path<String>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    body: String,
) -> impl IntoResponse {
    // Ensure webhooks are configured
    let webhook_state = match &state.webhooks {
        Some(ws) => ws,
        None => {
            return (
                StatusCode::NOT_FOUND,
                serde_json::json!({"error": "webhooks not configured"}).to_string(),
            );
        }
    };

    // Find the webhook config by name
    let config = match webhook_state.webhooks.iter().find(|w| w.name == name) {
        Some(c) => c,
        None => {
            warn!(webhook = %name, "Webhook not found");
            return (
                StatusCode::NOT_FOUND,
                serde_json::json!({"error": "webhook not found"}).to_string(),
            );
        }
    };

    // Validate secret if configured
    if let Some(ref secret) = config.secret {
        let request_secret = headers
            .get("x-webhook-secret")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if !validate_secret(secret, request_secret) {
            warn!(webhook = %name, "Webhook secret validation failed");
            return (
                StatusCode::UNAUTHORIZED,
                serde_json::json!({"error": "invalid secret"}).to_string(),
            );
        }
    }

    // Render prompt template
    let rendered = render_template(&config.agent_prompt_template, &body);

    // Determine session ID based on strategy
    let session_id = match &config.session_strategy {
        SessionStrategy::New => None,
        SessionStrategy::ByHeader(header_name) => headers
            .get(header_name)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| Uuid::parse_str(v).ok()),
    };

    info!(
        webhook = %name,
        rendered_len = rendered.len(),
        session_strategy = ?config.session_strategy,
        session_id = ?session_id,
        "Webhook received, forwarding to agent"
    );

    // Forward the rendered message to the MessageRouter for agent processing
    let inbound = InboundMessage {
        session_id,
        content: rendered,
    };

    match state.router.handle_webhook_message(inbound).await {
        Ok(response) => (
            StatusCode::OK,
            serde_json::json!({
                "status": "processed",
                "webhook": name,
                "response": response
            })
            .to_string(),
        ),
        Err(e) => {
            error!(webhook = %name, error = %e, "Webhook processing failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({
                    "status": "error",
                    "webhook": name,
                    "error": e.to_string()
                })
                .to_string(),
            )
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_secret_valid() {
        assert!(validate_secret("my-secret-key", "my-secret-key"));
    }

    #[test]
    fn test_validate_secret_invalid() {
        assert!(!validate_secret("my-secret-key", "wrong-key"));
    }

    #[test]
    fn test_validate_secret_different_lengths() {
        assert!(!validate_secret("short", "a-much-longer-secret"));
    }

    #[test]
    fn test_render_template_with_placeholder() {
        let template = "Process this webhook: {{payload}}";
        let payload = r#"{"event":"push","repo":"argentor"}"#;
        let result = render_template(template, payload);
        assert_eq!(
            result,
            r#"Process this webhook: {"event":"push","repo":"argentor"}"#
        );
    }

    #[test]
    fn test_render_template_without_placeholder() {
        let template = "No placeholder here";
        let payload = "some payload";
        let result = render_template(template, payload);
        assert_eq!(result, "No placeholder here");
    }

    #[test]
    fn test_render_template_multiple_placeholders() {
        let template = "First: {{payload}} | Second: {{payload}}";
        let payload = "data";
        let result = render_template(template, payload);
        assert_eq!(result, "First: data | Second: data");
    }

    #[test]
    fn test_webhook_config_deserialization() {
        let json = r#"{
            "name": "github",
            "secret": "gh-secret-123",
            "agent_prompt_template": "GitHub event: {{payload}}",
            "session_strategy": {"type": "New"}
        }"#;

        let config: WebhookConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.name, "github");
        assert_eq!(config.secret, Some("gh-secret-123".to_string()));
        assert_eq!(config.agent_prompt_template, "GitHub event: {{payload}}");
        assert_eq!(config.session_strategy, SessionStrategy::New);
    }

    #[test]
    fn test_webhook_config_deserialization_no_secret() {
        let json = r#"{
            "name": "slack",
            "agent_prompt_template": "Slack message: {{payload}}"
        }"#;

        let config: WebhookConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.name, "slack");
        assert!(config.secret.is_none());
        assert_eq!(config.session_strategy, SessionStrategy::New);
    }

    #[test]
    fn test_webhook_config_deserialization_by_header() {
        let json = r#"{
            "name": "ci",
            "agent_prompt_template": "CI event: {{payload}}",
            "session_strategy": {"type": "ByHeader", "value": "X-CI-Pipeline-ID"}
        }"#;

        let config: WebhookConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.name, "ci");
        assert_eq!(
            config.session_strategy,
            SessionStrategy::ByHeader("X-CI-Pipeline-ID".to_string())
        );
    }

    #[test]
    fn test_session_strategy_default() {
        let strategy = SessionStrategy::default();
        assert_eq!(strategy, SessionStrategy::New);
    }
}
