//! HTTP/WebSocket gateway with authentication, rate limiting, and webhook support.
//!
//! This crate provides the public-facing server that accepts HTTP and WebSocket
//! connections, applies authentication and rate-limiting middleware, and routes
//! requests to the agent runner.
//!
//! # Main types
//!
//! - [`GatewayServer`] — Builds and starts the Axum-based HTTP/WS server.
//! - [`AuthConfig`] — API-key authentication configuration.
//! - [`ConnectionManager`] — Tracks active WebSocket connections.
//! - [`MessageRouter`] — Routes inbound messages to the appropriate handler.

/// JWT/OAuth2 authentication module.
pub mod auth;
/// Bridge between ChannelManager and the gateway pipeline.
pub mod channel_bridge;
/// WebSocket connection management.
pub mod connection;
/// REST API endpoints for the orchestrator control plane.
pub mod control_plane;
/// Embedded web dashboard for monitoring and management.
pub mod dashboard;
/// Authentication and rate-limiting middleware.
pub mod middleware;
/// JSON file-based persistence for control plane state.
pub mod persistence;
/// REST API endpoints for managing credentials, token pools, and the proxy orchestrator.
pub mod proxy_management;
/// REST API endpoints for managing agents, sessions, skills, and connections.
pub mod rest_api;
/// HTTP and WebSocket route definitions.
pub mod router;
/// Gateway server builder and runner.
pub mod server;
/// Webhook endpoint configuration and handling.
pub mod webhook;
/// WebSocket-based human approval channel.
pub mod ws_approval;
/// Graceful shutdown manager with cleanup hooks and connection draining.
pub mod graceful_shutdown;
/// OpenAPI 3.0 specification generator for Argentor REST API.
pub mod openapi;
/// X-RateLimit-* response headers for API consumers.
pub mod rate_limit_headers;

pub use auth::{
    AuthConfig as JwtAuthConfig, AuthMiddlewareState, AuthMode, AuthService, AuthenticatedUser,
    JwtClaims,
};
pub use channel_bridge::ChannelBridge;
pub use control_plane::{control_plane_router, ControlPlaneState};
pub use dashboard::dashboard_router;
pub use middleware::AuthConfig;
pub use persistence::PersistentStore;
pub use proxy_management::{proxy_management_router, ProxyManagementState};
pub use rest_api::{api_router, RestApiState};
pub use server::GatewayServer;
pub use webhook::{SessionStrategy, WebhookConfig, WebhookState};
pub use ws_approval::WsApprovalChannel;
pub use graceful_shutdown::{
    HookResult, ShutdownHook, ShutdownManager, ShutdownPhase, ShutdownReport,
};
pub use openapi::{
    ApiEndpoint, ApiParameter, ApiResponse, HttpMethod, OpenApiGenerator, argentor_openapi_spec,
};
pub use rate_limit_headers::{RateLimitHeaders, RateLimitInfo};
