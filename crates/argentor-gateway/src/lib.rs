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

/// Business analytics endpoints for the SaaS dashboard.
pub mod analytics;
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
/// Graceful shutdown manager with cleanup hooks and connection draining.
pub mod graceful_shutdown;
/// Authentication and rate-limiting middleware.
pub mod middleware;
/// OpenAPI 3.0 specification generator for Argentor REST API.
pub mod openapi;
/// JSON file-based persistence for control plane state.
pub mod persistence;
/// Interactive web-based agent playground for testing agents in a browser.
pub mod playground;
/// REST API endpoints for managing credentials, token pools, and the proxy orchestrator.
pub mod proxy_management;
/// X-RateLimit-* response headers for API consumers.
pub mod rate_limit_headers;
/// REST API endpoints for managing agents, sessions, skills, and connections.
pub mod rest_api;
/// HTTP and WebSocket route definitions.
pub mod router;
/// Gateway server builder and runner.
pub mod server;
/// Trace visualization system for debugging agent execution.
pub mod trace_viewer;
/// Webhook endpoint configuration and handling.
pub mod webhook;
/// Outbound webhook notification system.
pub mod webhook_outbound;
/// WebSocket-based human approval channel.
pub mod ws_approval;
/// XcapitSFF integration — agent execution, webhook proxy, health checks.
pub mod xcapitsff;

pub use analytics::{
    analytics_router, AgentPerformance, AnalyticsDashboard, AnalyticsEngine, AnalyticsState,
    ConversionFunnel, DailyMetric, FunnelStage, InteractionEvent, InteractionOutcome, QualityEvent,
};
pub use auth::{
    AuthConfig as JwtAuthConfig, AuthMiddlewareState, AuthMode, AuthService, AuthenticatedUser,
    JwtClaims,
};
pub use channel_bridge::ChannelBridge;
pub use control_plane::{control_plane_router, ControlPlaneState};
pub use dashboard::dashboard_router;
pub use graceful_shutdown::{
    HookResult, ShutdownHook, ShutdownManager, ShutdownPhase, ShutdownReport,
};
pub use middleware::AuthConfig;
pub use openapi::{
    argentor_openapi_spec, ApiEndpoint, ApiParameter, ApiResponse, HttpMethod, OpenApiGenerator,
};
pub use persistence::PersistentStore;
pub use playground::playground_router;
pub use proxy_management::{proxy_management_router, ProxyManagementState};
pub use rate_limit_headers::{RateLimitHeaders, RateLimitInfo};
pub use rest_api::{api_router, RestApiState};
pub use server::GatewayServer;
pub use trace_viewer::{
    trace_viewer_router, CostBreakdown, StepCost, TimelineLane, TraceFilter, TraceStore,
    TraceSummary as TraceViewerSummary, TraceTimeline, TraceViewerState,
};
pub use webhook::{SessionStrategy, WebhookConfig, WebhookState};
pub use ws_approval::WsApprovalChannel;
pub use xcapitsff::{
    default_xcapit_profiles, xcapitsff_router, BatchRequest, BatchResponse, RunTaskRequest,
    RunTaskResponse, XcapitConfig, XcapitState,
};
