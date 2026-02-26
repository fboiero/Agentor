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

/// WebSocket connection management.
pub mod connection;
/// Authentication and rate-limiting middleware.
pub mod middleware;
/// HTTP and WebSocket route definitions.
pub mod router;
/// Gateway server builder and runner.
pub mod server;
/// Webhook endpoint configuration and handling.
pub mod webhook;
/// WebSocket-based human approval channel.
pub mod ws_approval;

pub use middleware::AuthConfig;
pub use server::GatewayServer;
pub use webhook::{SessionStrategy, WebhookConfig, WebhookState};
pub use ws_approval::WsApprovalChannel;
