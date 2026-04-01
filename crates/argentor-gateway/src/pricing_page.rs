//! SaaS pricing page for the Argentor platform.
//!
//! Serves a self-contained HTML pricing page that displays Free, Pro, and
//! Enterprise tiers with monthly/annual toggle, feature comparison, and FAQ.

use axum::{
    http::{header, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};

/// The pricing HTML page, embedded at compile time.
const PRICING_HTML: &str = include_str!("../pricing.html");

/// Creates a router that serves the embedded pricing page.
///
/// Mounts `GET /pricing` which returns the full SPA HTML page.
pub fn pricing_router() -> Router {
    Router::new().route("/pricing", get(pricing_handler))
}

/// Serves the embedded HTML pricing page.
async fn pricing_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        PRICING_HTML,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    /// Helper: send a GET to the pricing router and return (status, headers, body).
    async fn get_pricing() -> (StatusCode, axum::http::HeaderMap, String) {
        let app = pricing_router();
        let request = Request::builder()
            .method("GET")
            .uri("/pricing")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, headers, String::from_utf8_lossy(&body).to_string())
    }

    #[tokio::test]
    async fn test_pricing_returns_200() {
        let (status, _, _) = get_pricing().await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn test_pricing_content_type_is_html() {
        let (_, headers, _) = get_pricing().await;
        let content_type = headers
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
    async fn test_pricing_contains_plan_names() {
        let (_, _, body) = get_pricing().await;
        assert!(body.contains("Free"), "Pricing page should contain 'Free' plan");
        assert!(body.contains("Pro"), "Pricing page should contain 'Pro' plan");
        assert!(
            body.contains("Enterprise"),
            "Pricing page should contain 'Enterprise' plan"
        );
    }

    #[tokio::test]
    async fn test_pricing_contains_pricing_amounts() {
        let (_, _, body) = get_pricing().await;
        assert!(body.contains("$0"), "Pricing page should contain '$0' for Free plan");
        assert!(
            body.contains("$49"),
            "Pricing page should contain '$49' for Pro plan"
        );
        assert!(
            body.contains("$499"),
            "Pricing page should contain '$499' for Enterprise plan"
        );
    }
}
