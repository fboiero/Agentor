use crate::connection::{Connection, ConnectionManager};
use crate::control_plane::{control_plane_router, ControlPlaneState};
use crate::dashboard::dashboard_router;
use crate::middleware::{auth_middleware, rate_limit_middleware, AuthConfig, MiddlewareState};
use crate::playground::playground_router;
use crate::proxy_management::{proxy_management_router, ProxyManagementState};
use crate::rest_api::{api_router, RestApiState};
use crate::router::{InboundMessage, MessageRouter};
use crate::webhook::{webhook_handler, WebhookConfig, WebhookState};
use argentor_a2a::server::{A2AServer, A2AServerState};
use argentor_agent::AgentRunner;
use argentor_security::observability::AgentMetricsCollector;
use argentor_security::RateLimiter;
use argentor_session::SessionStore;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    http::{header, StatusCode},
    middleware as axum_mw,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info};
use uuid::Uuid;

/// Shared application state.
pub struct AppState {
    /// The message router that handles agent interactions.
    pub router: Arc<MessageRouter>,
    /// Tracks active WebSocket connections.
    pub connections: Arc<ConnectionManager>,
    /// Webhook configuration, if any.
    pub webhooks: Option<WebhookState>,
    /// Optional metrics collector for Prometheus-compatible `/metrics` endpoint.
    pub metrics: Option<AgentMetricsCollector>,
    /// Optional control-plane state for orchestrator management endpoints.
    pub control_plane: Option<Arc<ControlPlaneState>>,
    /// Optional REST API state for session/skill/agent management endpoints.
    pub rest_api: Option<Arc<RestApiState>>,
}

/// The main gateway server.
pub struct GatewayServer;

impl GatewayServer {
    /// Build the gateway without auth or rate limiting (backwards compatible).
    pub fn build(agent: Arc<AgentRunner>, sessions: Arc<dyn SessionStore>) -> Router {
        Self::build_with_middleware(agent, sessions, None, AuthConfig::new(vec![]), None, None)
    }

    /// Build the gateway with optional rate limiting, auth middleware, webhooks, and metrics.
    ///
    /// This method is kept for backward compatibility. It delegates to [`build_full`] with
    /// `control_plane` and `rest_api` set to `None`.
    pub fn build_with_middleware(
        agent: Arc<AgentRunner>,
        sessions: Arc<dyn SessionStore>,
        rate_limiter: Option<Arc<RateLimiter>>,
        auth_config: AuthConfig,
        webhooks: Option<Vec<WebhookConfig>>,
        metrics: Option<AgentMetricsCollector>,
    ) -> Router {
        Self::build_full(
            agent,
            sessions,
            rate_limiter,
            auth_config,
            webhooks,
            metrics,
            None,
            None,
        )
    }

    /// Build the full gateway with all optional subsystems.
    ///
    /// Accepts every parameter from [`build_with_middleware`] plus optional subsystem
    /// states. When provided, their routers are merged into the main application so
    /// their routes become available alongside `/ws`, `/health`, etc.
    ///
    /// - `control_plane` — mounts control-plane routes (`/api/v1/control-plane/…`).
    /// - `rest_api` — mounts REST API routes (`/api/v1/sessions`, `/api/v1/skills`, …).
    #[allow(clippy::too_many_arguments)]
    pub fn build_full(
        agent: Arc<AgentRunner>,
        sessions: Arc<dyn SessionStore>,
        rate_limiter: Option<Arc<RateLimiter>>,
        auth_config: AuthConfig,
        webhooks: Option<Vec<WebhookConfig>>,
        metrics: Option<AgentMetricsCollector>,
        control_plane: Option<Arc<ControlPlaneState>>,
        rest_api: Option<Arc<RestApiState>>,
    ) -> Router {
        Self::build_complete(
            agent,
            sessions,
            rate_limiter,
            auth_config,
            webhooks,
            metrics,
            control_plane,
            rest_api,
            None,
            None,
        )
    }

    /// Build the complete gateway with every optional subsystem including proxy management, A2A, and XcapitSFF.
    ///
    /// This is the most comprehensive builder. All other `build_*` methods delegate here.
    ///
    /// - `proxy_management` — mounts proxy management routes (`/api/v1/proxy-management/…`).
    /// - `a2a` — mounts A2A protocol routes (`/.well-known/agent.json`, `/a2a`).
    /// - `xcapitsff` — mounts XcapitSFF integration routes (`/api/v1/agent/…`, `/api/v1/proxy/…`).
    #[allow(clippy::too_many_arguments)]
    pub fn build_complete(
        agent: Arc<AgentRunner>,
        sessions: Arc<dyn SessionStore>,
        rate_limiter: Option<Arc<RateLimiter>>,
        auth_config: AuthConfig,
        webhooks: Option<Vec<WebhookConfig>>,
        metrics: Option<AgentMetricsCollector>,
        control_plane: Option<Arc<ControlPlaneState>>,
        rest_api: Option<Arc<RestApiState>>,
        proxy_management: Option<Arc<ProxyManagementState>>,
        a2a: Option<Arc<A2AServerState>>,
    ) -> Router {
        let connections = ConnectionManager::new();
        let router = Arc::new(MessageRouter::new(agent, sessions, connections.clone()));

        let webhook_state = webhooks.map(|configs| WebhookState { webhooks: configs });

        let state = Arc::new(AppState {
            router,
            connections,
            webhooks: webhook_state,
            metrics,
            control_plane: control_plane.clone(),
            rest_api: rest_api.clone(),
        });

        let mut app = Router::new()
            .route("/ws", get(ws_handler))
            .route("/health", get(health_handler))
            .route("/metrics", get(prometheus_metrics_handler))
            .route("/openapi.json", get(openapi_handler));

        // Add webhook route if webhooks are configured
        if state.webhooks.is_some() {
            app = app.route("/webhook/{name}", post(webhook_handler));
        }

        let mut app = app.with_state(state);

        // Merge the web dashboard and playground.
        app = app.merge(dashboard_router());
        app = app.merge(playground_router());

        // Merge control-plane routes if the state is provided.
        if let Some(cp_state) = control_plane {
            app = app.merge(control_plane_router(cp_state));
        }

        // Merge REST API routes if the state is provided.
        if let Some(ra_state) = rest_api {
            app = app.merge(api_router(ra_state));
        }

        // Merge proxy management routes if the state is provided.
        if let Some(pm_state) = proxy_management {
            app = app.merge(proxy_management_router(pm_state));
        }

        // Merge A2A protocol routes if the state is provided.
        if let Some(a2a_state) = a2a {
            app = app.merge(A2AServer::router(a2a_state));
        }

        // Apply middleware if configured
        if rate_limiter.is_some() || auth_config.is_enabled() {
            let mw_state = Arc::new(MiddlewareState {
                rate_limiter: rate_limiter
                    .unwrap_or_else(|| Arc::new(RateLimiter::new(1000.0, 1000.0))),
                auth: auth_config,
            });

            app.layer(axum_mw::from_fn_with_state(
                mw_state.clone(),
                rate_limit_middleware,
            ))
            .layer(axum_mw::from_fn_with_state(mw_state, auth_middleware))
        } else {
            app
        }
    }

    /// Build the gateway with Prometheus metrics enabled.
    pub fn build_with_metrics(
        agent: Arc<AgentRunner>,
        sessions: Arc<dyn SessionStore>,
        metrics: AgentMetricsCollector,
    ) -> Router {
        Self::build_with_middleware(
            agent,
            sessions,
            None,
            AuthConfig::new(vec![]),
            None,
            Some(metrics),
        )
    }

    /// Build the gateway with webhook support (convenience builder).
    ///
    /// This is equivalent to calling `build_with_middleware` with no auth or rate limiting
    /// but with webhook configurations.
    pub fn with_webhooks(
        agent: Arc<AgentRunner>,
        sessions: Arc<dyn SessionStore>,
        configs: Vec<WebhookConfig>,
    ) -> Router {
        Self::build_with_middleware(
            agent,
            sessions,
            None,
            AuthConfig::new(vec![]),
            Some(configs),
            None,
        )
    }
}

async fn health_handler() -> impl IntoResponse {
    serde_json::json!({"status": "ok", "service": "argentor"}).to_string()
}

/// OpenAPI 3.0 spec endpoint.
async fn openapi_handler() -> impl IntoResponse {
    let spec = crate::openapi::argentor_openapi_spec();
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::to_string_pretty(&spec).unwrap_or_default(),
    )
        .into_response()
}

/// Prometheus-compatible metrics endpoint.
///
/// Returns metrics in the Prometheus text exposition format when a collector
/// is configured, or a JSON error otherwise.
async fn prometheus_metrics_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match &state.metrics {
        Some(collector) => {
            let body = collector.prometheus_export();
            (
                StatusCode::OK,
                [(
                    header::CONTENT_TYPE,
                    "text/plain; version=0.0.4; charset=utf-8",
                )],
                body,
            )
                .into_response()
        }
        None => {
            let body = serde_json::json!({
                "error": "Metrics collector not configured",
                "hint": "Use GatewayServer::build_with_metrics() to enable Prometheus metrics"
            })
            .to_string();
            (
                StatusCode::SERVICE_UNAVAILABLE,
                [(header::CONTENT_TYPE, "application/json")],
                body,
            )
                .into_response()
        }
    }
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let connection_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Channel for sending messages back to the WebSocket
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    let conn = Connection {
        id: connection_id,
        session_id,
        tx,
    };
    state.connections.add(conn).await;

    info!(
        connection_id = %connection_id,
        session_id = %session_id,
        "WebSocket connected"
    );

    // Send initial session info
    let welcome = serde_json::json!({
        "type": "connected",
        "session_id": session_id,
        "connection_id": connection_id,
    });
    let _ = state
        .connections
        .send_to_session(session_id, &welcome.to_string())
        .await;

    // Task: forward messages from channel to WebSocket
    use axum::extract::ws::Message as WsMessage;
    use futures_util::SinkExt;
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sender.send(WsMessage::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Task: receive messages from WebSocket and route them
    use futures_util::StreamExt;
    let router = state.router.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            match msg {
                Message::Text(text) => {
                    let inbound: InboundMessage = match serde_json::from_str(&text) {
                        Ok(m) => m,
                        Err(_) => InboundMessage {
                            session_id: Some(session_id),
                            content: text.to_string(),
                        },
                    };

                    if let Err(e) = router
                        .handle_message_streaming(inbound, connection_id)
                        .await
                    {
                        error!(error = %e, "Failed to handle message");
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    state.connections.remove(connection_id).await;
    info!(connection_id = %connection_id, "WebSocket disconnected");
}
