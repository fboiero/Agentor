//! Interactive web-based agent playground.
//!
//! Serves a self-contained HTML SPA that lets users test agents directly in
//! the browser. The playground communicates with the gateway REST API
//! (`POST /api/v1/agent/run-task`) and displays responses with trace data,
//! token counts, and cost estimates.

use axum::{
    http::{header, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};

/// The playground HTML page, embedded at compile time.
const PLAYGROUND_HTML: &str = include_str!("../playground.html");

/// Creates a router that serves the embedded agent playground.
///
/// Mounts `GET /playground` which returns the full SPA HTML page.
/// The playground itself communicates with the agent API using client-side
/// JavaScript `fetch()` calls.
pub fn playground_router() -> Router {
    Router::new().route("/playground", get(playground_handler))
}

/// Serves the embedded HTML playground.
async fn playground_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        PLAYGROUND_HTML,
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    /// Helper: send a GET to the playground router and return (status, body).
    async fn get_playground() -> (StatusCode, String) {
        let app = playground_router();
        let request = Request::builder()
            .method("GET")
            .uri("/playground")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, String::from_utf8_lossy(&body).to_string())
    }

    #[tokio::test]
    async fn test_playground_router_returns_200() {
        let (status, _body) = get_playground().await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn test_playground_content_type_is_html() {
        let app = playground_router();
        let request = Request::builder()
            .method("GET")
            .uri("/playground")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .expect("Content-Type header should be present")
            .to_str()
            .unwrap();
        assert!(
            content_type.contains("text/html"),
            "Content-Type should be text/html, got: {content_type}"
        );
    }

    #[tokio::test]
    async fn test_playground_contains_agent_selector() {
        let (_status, body) = get_playground().await;
        assert!(
            body.contains("agent-selector")
                || body.contains("agent_selector")
                || body.contains("agentSelector"),
            "Playground HTML should contain an agent selector element"
        );
    }

    #[tokio::test]
    async fn test_playground_contains_chat_container() {
        let (_status, body) = get_playground().await;
        assert!(
            body.contains("chat-container")
                || body.contains("chat-messages")
                || body.contains("chatContainer"),
            "Playground HTML should contain a chat container element"
        );
    }

    #[tokio::test]
    async fn test_playground_contains_send_button() {
        let (_status, body) = get_playground().await;
        assert!(
            body.contains("send-btn") || body.contains("sendBtn") || body.contains("Send"),
            "Playground HTML should contain a send button"
        );
    }

    #[tokio::test]
    async fn test_playground_contains_trace_panel() {
        let (_status, body) = get_playground().await;
        assert!(
            body.contains("trace-panel")
                || body.contains("trace_panel")
                || body.contains("tracePanel"),
            "Playground HTML should contain a trace panel"
        );
    }

    #[tokio::test]
    async fn test_playground_contains_html_structure() {
        let (_status, body) = get_playground().await;
        assert!(
            body.contains("<!DOCTYPE html>") || body.contains("<!doctype html>"),
            "Playground should have a DOCTYPE declaration"
        );
        assert!(body.contains("<html"), "Playground should have an html tag");
        assert!(
            body.contains("</html>"),
            "Playground should have a closing html tag"
        );
    }

    #[tokio::test]
    async fn test_playground_contains_clear_and_export_buttons() {
        let (_status, body) = get_playground().await;
        assert!(
            body.contains("clear") || body.contains("Clear"),
            "Playground should have a clear chat button"
        );
        assert!(
            body.contains("export") || body.contains("Export"),
            "Playground should have an export conversation button"
        );
    }
}
