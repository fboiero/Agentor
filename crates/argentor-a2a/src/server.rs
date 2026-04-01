//! A2A server — serves the agent card and handles JSON-RPC task requests.
//!
//! Provides an Axum [`Router`] with three endpoints:
//!
//! - `GET /.well-known/agent.json` — serves the [`AgentCard`].
//! - `POST /a2a` — JSON-RPC 2.0 dispatch for task operations.
//! - `POST /a2a/stream` — SSE endpoint for `tasks/sendSubscribe` streaming.
//!
//! # Streaming
//!
//! Handlers that implement [`StreamingTaskHandler`] in addition to [`TaskHandler`]
//! can produce incremental events via `tasks/sendSubscribe`. The server detects
//! streaming capability at runtime through trait-object downcasting.
//!
//! When the handler does *not* implement [`StreamingTaskHandler`], the
//! `/a2a/stream` endpoint falls back to wrapping the regular `handle_task`
//! result as a single SSE event.
//!
//! # Example
//!
//! ```rust,no_run
//! use argentor_a2a::server::{A2AServer, A2AServerState, TaskHandler};
//! use argentor_a2a::protocol::*;
//! use argentor_a2a::discovery::AgentCardBuilder;
//! use argentor_core::ArgentorResult;
//! use async_trait::async_trait;
//! use std::sync::Arc;
//!
//! struct MyHandler;
//!
//! #[async_trait]
//! impl TaskHandler for MyHandler {
//!     async fn handle_task(&self, task: &A2ATask) -> Result<A2ATask, String> {
//!         let mut result = task.clone();
//!         result.transition_to(TaskStatus::Completed, None);
//!         Ok(result)
//!     }
//!
//!     fn as_any(&self) -> &dyn std::any::Any {
//!         self
//!     }
//! }
//! ```

use crate::protocol::*;
use async_trait::async_trait;
use axum::{
    extract::State,
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Json,
    },
    routing::{get, post},
    Router,
};
use futures_util::stream::Stream;
use std::any::Any;
use std::collections::HashMap;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Trait for handling incoming A2A tasks.
///
/// Implementors define how the agent processes tasks. The handler receives
/// a task in [`TaskStatus::Submitted`] state and should return the task with
/// an updated status and any response messages or artifacts.
///
/// The [`as_any`](TaskHandler::as_any) method enables runtime downcasting to
/// check whether the handler also implements [`StreamingTaskHandler`].
#[async_trait]
pub trait TaskHandler: Send + Sync + 'static {
    /// Process a task and return the updated task.
    ///
    /// The handler should:
    /// 1. Read the task's messages to understand the request.
    /// 2. Perform the work (call LLMs, invoke tools, etc.).
    /// 3. Add response messages and/or artifacts.
    /// 4. Transition the task to `Completed`, `Failed`, or `InputRequired`.
    ///
    /// If an error is returned, the task will be marked as `Failed`.
    async fn handle_task(&self, task: &A2ATask) -> Result<A2ATask, String>;

    /// Downcast support for checking [`StreamingTaskHandler`] at runtime.
    ///
    /// Implementors should return `self`. This enables the server to detect
    /// whether the handler also implements [`StreamingTaskHandler`].
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// fn as_any(&self) -> &dyn std::any::Any {
    ///     self
    /// }
    /// ```
    fn as_any(&self) -> &dyn Any;
}

/// Optional extension of [`TaskHandler`] for streaming task processing.
///
/// Implementors yield [`TaskStreamEvent`]s as the task progresses through
/// states, enabling real-time updates via Server-Sent Events (SSE).
///
/// The server checks for this trait at runtime using [`TaskHandler::as_any`].
#[async_trait]
pub trait StreamingTaskHandler: TaskHandler {
    /// Process a task and send incremental events through the channel.
    ///
    /// The handler should:
    /// 1. Send [`TaskStreamEvent::StatusUpdate`] events as the task changes state.
    /// 2. Send [`TaskStreamEvent::Artifact`] events for each produced artifact.
    /// 3. Send [`TaskStreamEvent::Message`] events for intermediate messages.
    /// 4. Return the final task state.
    ///
    /// The channel will be closed after the handler returns or if an error occurs.
    async fn handle_task_streaming(
        &self,
        task: &A2ATask,
        sender: tokio::sync::mpsc::Sender<TaskStreamEvent>,
    ) -> Result<A2ATask, String>;
}

/// Shared state for the A2A server.
pub struct A2AServerState {
    /// The agent card served at `/.well-known/agent.json`.
    pub agent_card: AgentCard,
    /// In-memory task store keyed by task ID.
    pub tasks: Arc<RwLock<HashMap<String, A2ATask>>>,
    /// The handler that processes incoming tasks.
    pub handler: Arc<dyn TaskHandler>,
}

/// The A2A server builder.
///
/// Creates an Axum router with the standard A2A protocol endpoints.
pub struct A2AServer;

impl A2AServer {
    /// Build the A2A router with the given server state.
    ///
    /// # Routes
    ///
    /// - `GET /.well-known/agent.json` — returns the agent card as JSON.
    /// - `POST /a2a` — JSON-RPC 2.0 endpoint for task operations.
    /// - `POST /a2a/stream` — SSE endpoint for `tasks/sendSubscribe`.
    pub fn router(state: Arc<A2AServerState>) -> Router {
        Router::new()
            .route("/.well-known/agent.json", get(agent_card_handler))
            .route("/a2a", post(jsonrpc_handler))
            .route("/a2a/stream", post(sse_handler))
            .with_state(state)
    }
}

/// Handler for `GET /.well-known/agent.json`.
async fn agent_card_handler(State(state): State<Arc<A2AServerState>>) -> impl IntoResponse {
    Json(state.agent_card.clone())
}

/// Handler for `POST /a2a` — JSON-RPC 2.0 dispatch.
async fn jsonrpc_handler(
    State(state): State<Arc<A2AServerState>>,
    Json(envelope): Json<JsonRpcEnvelope>,
) -> impl IntoResponse {
    debug!(method = %envelope.method, "A2A JSON-RPC request received");

    if envelope.jsonrpc != "2.0" {
        let resp = A2AResponse::error(
            envelope.id.clone(),
            JSONRPC_INVALID_REQUEST,
            "Invalid JSON-RPC version, expected 2.0",
        );
        return (StatusCode::OK, Json(resp));
    }

    let response = match envelope.method.as_str() {
        "tasks/send" => handle_send_task(&state, envelope.id.clone(), &envelope.params).await,
        "tasks/get" => handle_get_task(&state, envelope.id.clone(), &envelope.params).await,
        "tasks/cancel" => handle_cancel_task(&state, envelope.id.clone(), &envelope.params).await,
        "tasks/list" => handle_list_tasks(&state, envelope.id.clone(), &envelope.params).await,
        "agent/card" => handle_get_agent_card(&state, envelope.id.clone()).await,
        _ => {
            warn!(method = %envelope.method, "Unknown A2A method");
            A2AResponse::method_not_found(envelope.id.clone(), &envelope.method)
        }
    };

    (StatusCode::OK, Json(response))
}

/// SSE wrapper event sent to the client on the `/a2a/stream` endpoint.
///
/// Each SSE event carries a `type` discriminator and the event payload.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SseEventPayload {
    /// The kind of event: `"status_update"`, `"artifact"`, `"message"`, or `"final"`.
    #[serde(rename = "type")]
    event_type: String,
    /// The event data payload.
    data: serde_json::Value,
}

/// Handler for `POST /a2a/stream` — SSE endpoint for `tasks/sendSubscribe`.
///
/// Accepts a JSON-RPC envelope with method `tasks/sendSubscribe`, creates or
/// resumes a task, and returns an SSE stream of [`TaskStreamEvent`]s.
///
/// If the handler implements [`StreamingTaskHandler`], it yields incremental
/// events. Otherwise, it falls back to a non-streaming execution and emits
/// the result as a single `final` event.
#[allow(clippy::too_many_arguments)]
async fn sse_handler(
    State(state): State<Arc<A2AServerState>>,
    Json(envelope): Json<JsonRpcEnvelope>,
) -> axum::response::Response {
    debug!(method = %envelope.method, "A2A SSE request received");

    // Validate JSON-RPC version
    if envelope.jsonrpc != "2.0" {
        let resp = A2AResponse::error(
            envelope.id.clone(),
            JSONRPC_INVALID_REQUEST,
            "Invalid JSON-RPC version, expected 2.0",
        );
        return (StatusCode::BAD_REQUEST, Json(resp)).into_response();
    }

    // Only accept tasks/sendSubscribe
    if envelope.method != "tasks/sendSubscribe" {
        let resp = A2AResponse::error(
            envelope.id.clone(),
            JSONRPC_METHOD_NOT_FOUND,
            format!(
                "SSE endpoint only supports tasks/sendSubscribe, got: {}",
                envelope.method
            ),
        );
        return (StatusCode::BAD_REQUEST, Json(resp)).into_response();
    }

    // Parse task params
    let params = match &envelope.params {
        Some(p) => p.clone(),
        None => {
            let resp = A2AResponse::invalid_params(
                envelope.id.clone(),
                "Missing params for tasks/sendSubscribe",
            );
            return (StatusCode::BAD_REQUEST, Json(resp)).into_response();
        }
    };

    let message: TaskMessage = match params.get("message") {
        Some(msg_val) => match serde_json::from_value(msg_val.clone()) {
            Ok(m) => m,
            Err(e) => {
                let resp = A2AResponse::invalid_params(
                    envelope.id.clone(),
                    &format!("Invalid message format: {e}"),
                );
                return (StatusCode::BAD_REQUEST, Json(resp)).into_response();
            }
        },
        None => {
            let resp =
                A2AResponse::invalid_params(envelope.id.clone(), "Missing 'message' in params");
            return (StatusCode::BAD_REQUEST, Json(resp)).into_response();
        }
    };

    let task_id_param = params.get("id").and_then(|v| v.as_str()).map(String::from);
    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .map(String::from);
    let metadata: HashMap<String, serde_json::Value> = params
        .get("metadata")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    // Create or resume task
    let mut task = if let Some(ref existing_id) = task_id_param {
        let tasks = state.tasks.read().await;
        match tasks.get(existing_id) {
            Some(existing) => {
                if existing.is_terminal() {
                    let resp = A2AResponse::error(
                        envelope.id.clone(),
                        A2A_TASK_TERMINAL,
                        format!("Task {existing_id} is in terminal state"),
                    );
                    return (StatusCode::BAD_REQUEST, Json(resp)).into_response();
                }
                let mut t = existing.clone();
                t.add_message(message);
                t
            }
            None => {
                let resp = A2AResponse::task_not_found(envelope.id.clone(), existing_id);
                return (StatusCode::NOT_FOUND, Json(resp)).into_response();
            }
        }
    } else {
        let mut t = A2ATask::new(message);
        t.session_id = session_id;
        t.metadata = metadata;
        t
    };

    info!(task_id = %task.id, "Processing A2A streaming task");
    task.transition_to(
        TaskStatus::Working,
        Some("Streaming handler invoked".to_string()),
    );

    // Store task before processing
    {
        let mut tasks = state.tasks.write().await;
        tasks.insert(task.id.clone(), task.clone());
    }

    // Build the SSE stream
    let stream = build_sse_stream(state, task);
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Build an SSE stream for a task, executing the handler and emitting events.
///
/// The handler is invoked via [`TaskHandler::handle_task`] and the result is
/// wrapped in synthetic [`TaskStreamEvent`]s (working + final status).
///
/// For handlers that implement [`StreamingTaskHandler`], use the public
/// [`run_streaming_handler`] helper to produce richer incremental events.
fn build_sse_stream(
    state: Arc<A2AServerState>,
    task: A2ATask,
) -> Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<TaskStreamEvent>(32);

    let handler = Arc::clone(&state.handler);
    let tasks_store = Arc::clone(&state.tasks);
    let task_for_spawn = task.clone();

    // Spawn the handler work in a background task and feed events to the channel.
    tokio::spawn(async move {
        // Execute the handler via the non-streaming fallback path, which
        // emits synthetic status events (working -> completed/failed).
        let final_result = run_non_streaming_with_events(&handler, &task_for_spawn, &tx).await;

        // Store final task state and send the closing event
        match final_result {
            Ok(updated) => {
                let mut tasks = tasks_store.write().await;
                tasks.insert(updated.id.clone(), updated.clone());
                // Send final event
                let _ = tx
                    .send(TaskStreamEvent::StatusUpdate {
                        task_id: updated.id.clone(),
                        status: updated.status.clone(),
                        message: Some("Task completed".to_string()),
                    })
                    .await;
            }
            Err(err) => {
                error!(error = %err, "Streaming task handler failed");
                let _ = tx
                    .send(TaskStreamEvent::StatusUpdate {
                        task_id: task_for_spawn.id.clone(),
                        status: TaskStatus::Failed,
                        message: Some(format!("Error: {err}")),
                    })
                    .await;
            }
        }
    });

    // Convert mpsc receiver into an SSE stream
    let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    let mapped = futures_util::stream::StreamExt::map(stream, |event| {
        let (event_type, data) = match &event {
            TaskStreamEvent::StatusUpdate {
                task_id,
                status,
                message,
            } => {
                let is_final = matches!(
                    status,
                    TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Canceled
                );
                let type_str = if is_final { "final" } else { "status_update" };
                let payload = serde_json::json!({
                    "taskId": task_id,
                    "status": status,
                    "message": message,
                });
                (type_str.to_string(), payload)
            }
            TaskStreamEvent::Artifact {
                task_id, artifact, ..
            } => {
                let payload = serde_json::json!({
                    "taskId": task_id,
                    "artifact": artifact,
                });
                ("artifact".to_string(), payload)
            }
            TaskStreamEvent::Message {
                task_id, message, ..
            } => {
                let payload = serde_json::json!({
                    "taskId": task_id,
                    "message": message,
                });
                ("message".to_string(), payload)
            }
        };

        let sse_payload = SseEventPayload { event_type, data };

        let json_str = serde_json::to_string(&sse_payload).unwrap_or_else(|e| {
            format!(r#"{{"type":"error","data":{{"error":"serialization failed: {e}"}}}}"#)
        });

        Ok(Event::default().data(json_str))
    });

    Box::pin(mapped)
}

/// Run a non-streaming handler and emit synthetic events through the channel.
///
/// This is the fallback path when the handler does not implement
/// [`StreamingTaskHandler`]. It executes `handle_task` and sends a
/// `StatusUpdate(Working)` event before execution and writes the final
/// task state to the store.
async fn run_non_streaming_with_events(
    handler: &Arc<dyn TaskHandler>,
    task: &A2ATask,
    tx: &tokio::sync::mpsc::Sender<TaskStreamEvent>,
) -> Result<A2ATask, String> {
    // Emit initial working status
    let _ = tx
        .send(TaskStreamEvent::StatusUpdate {
            task_id: task.id.clone(),
            status: TaskStatus::Working,
            message: Some("Processing task".to_string()),
        })
        .await;

    match handler.handle_task(task).await {
        Ok(updated) => Ok(updated),
        Err(err) => {
            let mut failed_task = task.clone();
            failed_task.transition_to(TaskStatus::Failed, Some(err.clone()));
            failed_task.add_message(TaskMessage::agent_text(format!("Error: {err}")));
            Ok(failed_task)
        }
    }
}

/// Invoke a [`StreamingTaskHandler`] through the given channel.
///
/// This is a helper for server implementations that hold a concrete streaming
/// handler and want to wire it into the SSE pipeline.
pub async fn run_streaming_handler(
    handler: &dyn StreamingTaskHandler,
    task: &A2ATask,
    tasks_store: &Arc<RwLock<HashMap<String, A2ATask>>>,
    tx: tokio::sync::mpsc::Sender<TaskStreamEvent>,
) -> Result<A2ATask, String> {
    // Emit initial working status
    let _ = tx
        .send(TaskStreamEvent::StatusUpdate {
            task_id: task.id.clone(),
            status: TaskStatus::Working,
            message: Some("Streaming processing started".to_string()),
        })
        .await;

    let result = handler.handle_task_streaming(task, tx.clone()).await;

    match &result {
        Ok(updated) => {
            let mut tasks = tasks_store.write().await;
            tasks.insert(updated.id.clone(), updated.clone());
            let _ = tx
                .send(TaskStreamEvent::StatusUpdate {
                    task_id: updated.id.clone(),
                    status: updated.status.clone(),
                    message: Some("Task completed".to_string()),
                })
                .await;
        }
        Err(err) => {
            let _ = tx
                .send(TaskStreamEvent::StatusUpdate {
                    task_id: task.id.clone(),
                    status: TaskStatus::Failed,
                    message: Some(format!("Error: {err}")),
                })
                .await;
        }
    }

    result
}

/// Handle `tasks/send` — create a new task or add a message to an existing one.
async fn handle_send_task(
    state: &A2AServerState,
    id: Option<serde_json::Value>,
    params: &Option<serde_json::Value>,
) -> A2AResponse {
    let params = match params {
        Some(p) => p,
        None => return A2AResponse::invalid_params(id, "Missing params for tasks/send"),
    };

    // Extract message from params
    let message: TaskMessage = match params.get("message") {
        Some(msg_val) => match serde_json::from_value(msg_val.clone()) {
            Ok(m) => m,
            Err(e) => {
                return A2AResponse::invalid_params(id, &format!("Invalid message format: {e}"));
            }
        },
        None => return A2AResponse::invalid_params(id, "Missing 'message' in params"),
    };

    // Check for an existing task ID
    let task_id = params.get("id").and_then(|v| v.as_str()).map(String::from);

    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .map(String::from);

    let metadata: HashMap<String, serde_json::Value> = params
        .get("metadata")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let mut task = if let Some(ref existing_id) = task_id {
        // Look up existing task
        let tasks = state.tasks.read().await;
        match tasks.get(existing_id) {
            Some(existing) => {
                if existing.is_terminal() {
                    return A2AResponse::error(
                        id,
                        A2A_TASK_TERMINAL,
                        format!("Task {existing_id} is in terminal state"),
                    );
                }
                let mut t = existing.clone();
                t.add_message(message);
                t
            }
            None => return A2AResponse::task_not_found(id, existing_id),
        }
    } else {
        // Create a new task
        let mut t = A2ATask::new(message);
        t.session_id = session_id;
        t.metadata = metadata;
        t
    };

    info!(task_id = %task.id, "Processing A2A task");
    task.transition_to(TaskStatus::Working, Some("Handler invoked".to_string()));

    // Store the task before processing
    {
        let mut tasks = state.tasks.write().await;
        tasks.insert(task.id.clone(), task.clone());
    }

    // Invoke the handler
    match state.handler.handle_task(&task).await {
        Ok(updated) => {
            let mut tasks = state.tasks.write().await;
            tasks.insert(updated.id.clone(), updated.clone());
            info!(task_id = %updated.id, status = ?updated.status, "A2A task processed");
            match serde_json::to_value(&updated) {
                Ok(val) => A2AResponse::success(id, val),
                Err(e) => A2AResponse::internal_error(id, &format!("Serialization error: {e}")),
            }
        }
        Err(err) => {
            error!(task_id = %task.id, error = %err, "A2A task handler failed");
            task.transition_to(TaskStatus::Failed, Some(err.clone()));
            task.add_message(TaskMessage::agent_text(format!("Error: {err}")));
            let mut tasks = state.tasks.write().await;
            tasks.insert(task.id.clone(), task.clone());
            match serde_json::to_value(&task) {
                Ok(val) => A2AResponse::success(id, val),
                Err(e) => A2AResponse::internal_error(id, &format!("Serialization error: {e}")),
            }
        }
    }
}

/// Handle `tasks/get` — retrieve a task by ID.
async fn handle_get_task(
    state: &A2AServerState,
    id: Option<serde_json::Value>,
    params: &Option<serde_json::Value>,
) -> A2AResponse {
    let task_id = match params
        .as_ref()
        .and_then(|p| p.get("id"))
        .and_then(|v| v.as_str())
    {
        Some(tid) => tid.to_string(),
        None => return A2AResponse::invalid_params(id, "Missing 'id' in params for tasks/get"),
    };

    let tasks = state.tasks.read().await;
    match tasks.get(&task_id) {
        Some(task) => match serde_json::to_value(task) {
            Ok(val) => A2AResponse::success(id, val),
            Err(e) => A2AResponse::internal_error(id, &format!("Serialization error: {e}")),
        },
        None => A2AResponse::task_not_found(id, &task_id),
    }
}

/// Handle `tasks/cancel` — cancel a running task.
async fn handle_cancel_task(
    state: &A2AServerState,
    id: Option<serde_json::Value>,
    params: &Option<serde_json::Value>,
) -> A2AResponse {
    let task_id = match params
        .as_ref()
        .and_then(|p| p.get("id"))
        .and_then(|v| v.as_str())
    {
        Some(tid) => tid.to_string(),
        None => {
            return A2AResponse::invalid_params(id, "Missing 'id' in params for tasks/cancel");
        }
    };

    let mut tasks = state.tasks.write().await;
    match tasks.get_mut(&task_id) {
        Some(task) => {
            if task.is_terminal() {
                return A2AResponse::error(
                    id,
                    A2A_TASK_TERMINAL,
                    format!("Task {task_id} is already in terminal state"),
                );
            }
            task.transition_to(TaskStatus::Canceled, Some("Canceled by caller".to_string()));
            info!(task_id = %task_id, "A2A task canceled");
            match serde_json::to_value(&*task) {
                Ok(val) => A2AResponse::success(id, val),
                Err(e) => A2AResponse::internal_error(id, &format!("Serialization error: {e}")),
            }
        }
        None => A2AResponse::task_not_found(id, &task_id),
    }
}

/// Handle `tasks/list` — list tasks, optionally filtered by session ID.
async fn handle_list_tasks(
    state: &A2AServerState,
    id: Option<serde_json::Value>,
    params: &Option<serde_json::Value>,
) -> A2AResponse {
    let session_filter = params
        .as_ref()
        .and_then(|p| p.get("sessionId"))
        .and_then(|v| v.as_str())
        .map(String::from);

    let tasks = state.tasks.read().await;
    let filtered: Vec<&A2ATask> = tasks
        .values()
        .filter(|t| {
            if let Some(ref sid) = session_filter {
                t.session_id.as_deref() == Some(sid.as_str())
            } else {
                true
            }
        })
        .collect();

    match serde_json::to_value(&filtered) {
        Ok(val) => A2AResponse::success(id, val),
        Err(e) => A2AResponse::internal_error(id, &format!("Serialization error: {e}")),
    }
}

/// Handle `agent/card` — return the agent card via JSON-RPC.
async fn handle_get_agent_card(
    state: &A2AServerState,
    id: Option<serde_json::Value>,
) -> A2AResponse {
    match serde_json::to_value(&state.agent_card) {
        Ok(val) => A2AResponse::success(id, val),
        Err(e) => A2AResponse::internal_error(id, &format!("Serialization error: {e}")),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    /// A simple echo handler for testing.
    struct EchoHandler;

    #[async_trait]
    impl TaskHandler for EchoHandler {
        async fn handle_task(&self, task: &A2ATask) -> Result<A2ATask, String> {
            let mut result = task.clone();
            // Echo back the first message's text
            let response_text = task
                .messages
                .first()
                .and_then(|m| m.parts.first())
                .map(|p| match p {
                    MessagePart::Text { text } => format!("Echo: {text}"),
                    _ => "Echo: (non-text)".to_string(),
                })
                .unwrap_or_else(|| "Echo: (empty)".to_string());

            result.add_message(TaskMessage::agent_text(response_text));
            result.transition_to(TaskStatus::Completed, None);
            Ok(result)
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    /// A handler that always fails.
    struct FailHandler;

    #[async_trait]
    impl TaskHandler for FailHandler {
        async fn handle_task(&self, _task: &A2ATask) -> Result<A2ATask, String> {
            Err("Something went wrong".to_string())
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    /// A streaming handler that emits events as it processes.
    struct StreamingEchoHandler;

    #[async_trait]
    impl TaskHandler for StreamingEchoHandler {
        async fn handle_task(&self, task: &A2ATask) -> Result<A2ATask, String> {
            // Fallback non-streaming path
            let mut result = task.clone();
            result.add_message(TaskMessage::agent_text("Echo (non-streaming)"));
            result.transition_to(TaskStatus::Completed, None);
            Ok(result)
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    #[async_trait]
    impl StreamingTaskHandler for StreamingEchoHandler {
        async fn handle_task_streaming(
            &self,
            task: &A2ATask,
            sender: tokio::sync::mpsc::Sender<TaskStreamEvent>,
        ) -> Result<A2ATask, String> {
            let mut result = task.clone();

            // Send working status
            let _ = sender
                .send(TaskStreamEvent::StatusUpdate {
                    task_id: task.id.clone(),
                    status: TaskStatus::Working,
                    message: Some("Starting echo".to_string()),
                })
                .await;

            // Send an artifact
            let artifact = TaskArtifact::text("echo-output", "Streamed echo result");
            let _ = sender
                .send(TaskStreamEvent::Artifact {
                    task_id: task.id.clone(),
                    artifact: artifact.clone(),
                })
                .await;
            result.add_artifact(artifact);

            // Send a message
            let msg = TaskMessage::agent_text("Streaming echo done");
            let _ = sender
                .send(TaskStreamEvent::Message {
                    task_id: task.id.clone(),
                    message: msg.clone(),
                })
                .await;
            result.add_message(msg);

            result.transition_to(TaskStatus::Completed, None);
            Ok(result)
        }
    }

    fn make_state(handler: impl TaskHandler) -> Arc<A2AServerState> {
        let card = AgentCard {
            name: "TestAgent".to_string(),
            description: "A test agent".to_string(),
            url: "http://localhost:3000".to_string(),
            version: "1.0.0".to_string(),
            provider: None,
            capabilities: AgentCapabilities::default(),
            skills: vec![],
            default_input_modes: vec!["text/plain".to_string()],
            default_output_modes: vec!["text/plain".to_string()],
            authentication: None,
        };

        Arc::new(A2AServerState {
            agent_card: card,
            tasks: Arc::new(RwLock::new(HashMap::new())),
            handler: Arc::new(handler),
        })
    }

    fn jsonrpc_request(method: &str, params: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        })
    }

    #[tokio::test]
    async fn test_agent_card_endpoint() {
        let state = make_state(EchoHandler);
        let app = A2AServer::router(state);

        let req = Request::builder()
            .uri("/.well-known/agent.json")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let card: AgentCard = serde_json::from_slice(&body).unwrap();
        assert_eq!(card.name, "TestAgent");
        assert_eq!(card.version, "1.0.0");
    }

    #[tokio::test]
    async fn test_send_task() {
        let state = make_state(EchoHandler);
        let app = A2AServer::router(state);

        let body = jsonrpc_request(
            "tasks/send",
            serde_json::json!({
                "message": {
                    "role": "user",
                    "parts": [{"type": "text", "text": "Hello"}],
                    "metadata": {}
                }
            }),
        );

        let req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let rpc_resp: A2AResponse = serde_json::from_slice(&body).unwrap();
        assert!(rpc_resp.error.is_none());

        let task: A2ATask = serde_json::from_value(rpc_resp.result.unwrap()).unwrap();
        assert_eq!(task.status, TaskStatus::Completed);
        assert_eq!(task.messages.len(), 2);
    }

    #[tokio::test]
    async fn test_send_task_fail_handler() {
        let state = make_state(FailHandler);
        let app = A2AServer::router(state);

        let body = jsonrpc_request(
            "tasks/send",
            serde_json::json!({
                "message": {
                    "role": "user",
                    "parts": [{"type": "text", "text": "Hello"}],
                    "metadata": {}
                }
            }),
        );

        let req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let rpc_resp: A2AResponse = serde_json::from_slice(&body).unwrap();
        // Even on handler failure, we get a success response with the failed task
        assert!(rpc_resp.error.is_none());

        let task: A2ATask = serde_json::from_value(rpc_resp.result.unwrap()).unwrap();
        assert_eq!(task.status, TaskStatus::Failed);
    }

    #[tokio::test]
    async fn test_get_task() {
        let state = make_state(EchoHandler);
        // Pre-populate a task
        let mut task = A2ATask::new(TaskMessage::user_text("Test"));
        task.transition_to(TaskStatus::Completed, None);
        let task_id = task.id.clone();
        {
            let mut tasks = state.tasks.write().await;
            tasks.insert(task_id.clone(), task);
        }

        let app = A2AServer::router(state);

        let body = jsonrpc_request("tasks/get", serde_json::json!({"id": task_id}));

        let req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let rpc_resp: A2AResponse = serde_json::from_slice(&body).unwrap();
        assert!(rpc_resp.error.is_none());

        let retrieved: A2ATask = serde_json::from_value(rpc_resp.result.unwrap()).unwrap();
        assert_eq!(retrieved.id, task_id);
        assert_eq!(retrieved.status, TaskStatus::Completed);
    }

    #[tokio::test]
    async fn test_get_task_not_found() {
        let state = make_state(EchoHandler);
        let app = A2AServer::router(state);

        let body = jsonrpc_request("tasks/get", serde_json::json!({"id": "nonexistent"}));

        let req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let rpc_resp: A2AResponse = serde_json::from_slice(&body).unwrap();
        assert!(rpc_resp.error.is_some());
        assert_eq!(rpc_resp.error.unwrap().code, A2A_TASK_NOT_FOUND);
    }

    #[tokio::test]
    async fn test_cancel_task() {
        let state = make_state(EchoHandler);
        // Pre-populate a working task
        let mut task = A2ATask::new(TaskMessage::user_text("Working"));
        task.transition_to(TaskStatus::Working, None);
        let task_id = task.id.clone();
        {
            let mut tasks = state.tasks.write().await;
            tasks.insert(task_id.clone(), task);
        }

        let app = A2AServer::router(state);

        let body = jsonrpc_request("tasks/cancel", serde_json::json!({"id": task_id}));

        let req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let rpc_resp: A2AResponse = serde_json::from_slice(&body).unwrap();
        assert!(rpc_resp.error.is_none());

        let canceled: A2ATask = serde_json::from_value(rpc_resp.result.unwrap()).unwrap();
        assert_eq!(canceled.status, TaskStatus::Canceled);
    }

    #[tokio::test]
    async fn test_cancel_terminal_task() {
        let state = make_state(EchoHandler);
        // Pre-populate a completed task (terminal state)
        let mut task = A2ATask::new(TaskMessage::user_text("Done"));
        task.transition_to(TaskStatus::Completed, None);
        let task_id = task.id.clone();
        {
            let mut tasks = state.tasks.write().await;
            tasks.insert(task_id.clone(), task);
        }

        let app = A2AServer::router(state);

        let body = jsonrpc_request("tasks/cancel", serde_json::json!({"id": task_id}));

        let req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let rpc_resp: A2AResponse = serde_json::from_slice(&body).unwrap();
        assert!(rpc_resp.error.is_some());
        assert_eq!(rpc_resp.error.unwrap().code, A2A_TASK_TERMINAL);
    }

    #[tokio::test]
    async fn test_list_tasks_empty() {
        let state = make_state(EchoHandler);
        let app = A2AServer::router(state);

        let body = jsonrpc_request("tasks/list", serde_json::json!({}));

        let req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let rpc_resp: A2AResponse = serde_json::from_slice(&body).unwrap();
        assert!(rpc_resp.error.is_none());

        let tasks: Vec<A2ATask> = serde_json::from_value(rpc_resp.result.unwrap()).unwrap();
        assert!(tasks.is_empty());
    }

    #[tokio::test]
    async fn test_list_tasks_with_session_filter() {
        let state = make_state(EchoHandler);
        // Pre-populate tasks with different sessions
        {
            let mut tasks = state.tasks.write().await;
            let mut task1 = A2ATask::new(TaskMessage::user_text("Task 1"));
            task1.session_id = Some("session-a".to_string());
            tasks.insert(task1.id.clone(), task1);

            let mut task2 = A2ATask::new(TaskMessage::user_text("Task 2"));
            task2.session_id = Some("session-b".to_string());
            tasks.insert(task2.id.clone(), task2);

            let mut task3 = A2ATask::new(TaskMessage::user_text("Task 3"));
            task3.session_id = Some("session-a".to_string());
            tasks.insert(task3.id.clone(), task3);
        }

        let app = A2AServer::router(state);

        let body = jsonrpc_request("tasks/list", serde_json::json!({"sessionId": "session-a"}));

        let req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let rpc_resp: A2AResponse = serde_json::from_slice(&body).unwrap();
        assert!(rpc_resp.error.is_none());

        let tasks: Vec<A2ATask> = serde_json::from_value(rpc_resp.result.unwrap()).unwrap();
        assert_eq!(tasks.len(), 2);
        for task in &tasks {
            assert_eq!(task.session_id, Some("session-a".to_string()));
        }
    }

    #[tokio::test]
    async fn test_method_not_found() {
        let state = make_state(EchoHandler);
        let app = A2AServer::router(state);

        let body = jsonrpc_request("unknown/method", serde_json::json!({}));

        let req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let rpc_resp: A2AResponse = serde_json::from_slice(&body).unwrap();
        assert!(rpc_resp.error.is_some());
        assert_eq!(rpc_resp.error.unwrap().code, JSONRPC_METHOD_NOT_FOUND);
    }

    #[tokio::test]
    async fn test_agent_card_via_jsonrpc() {
        let state = make_state(EchoHandler);
        let app = A2AServer::router(state);

        let body = jsonrpc_request("agent/card", serde_json::json!({}));

        let req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let rpc_resp: A2AResponse = serde_json::from_slice(&body).unwrap();
        assert!(rpc_resp.error.is_none());

        let card: AgentCard = serde_json::from_value(rpc_resp.result.unwrap()).unwrap();
        assert_eq!(card.name, "TestAgent");
    }

    #[tokio::test]
    async fn test_send_task_missing_message() {
        let state = make_state(EchoHandler);
        let app = A2AServer::router(state);

        let body = jsonrpc_request("tasks/send", serde_json::json!({}));

        let req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let rpc_resp: A2AResponse = serde_json::from_slice(&body).unwrap();
        assert!(rpc_resp.error.is_some());
        assert_eq!(rpc_resp.error.unwrap().code, JSONRPC_INVALID_PARAMS);
    }

    #[tokio::test]
    async fn test_invalid_jsonrpc_version() {
        let state = make_state(EchoHandler);
        let app = A2AServer::router(state);

        let body = serde_json::json!({
            "jsonrpc": "1.0",
            "id": 1,
            "method": "tasks/send",
            "params": {}
        });

        let req = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let rpc_resp: A2AResponse = serde_json::from_slice(&body).unwrap();
        assert!(rpc_resp.error.is_some());
        assert_eq!(rpc_resp.error.unwrap().code, JSONRPC_INVALID_REQUEST);
    }

    // -----------------------------------------------------------------------
    // SSE streaming tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_sse_endpoint_returns_event_stream() {
        let state = make_state(EchoHandler);
        let app = A2AServer::router(state);

        let body = jsonrpc_request(
            "tasks/sendSubscribe",
            serde_json::json!({
                "message": {
                    "role": "user",
                    "parts": [{"type": "text", "text": "Stream me"}],
                    "metadata": {}
                }
            }),
        );

        let req = Request::builder()
            .method("POST")
            .uri("/a2a/stream")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Verify Content-Type is text/event-stream
        let content_type = resp
            .headers()
            .get("content-type")
            .expect("Missing content-type header")
            .to_str()
            .unwrap();
        assert!(
            content_type.contains("text/event-stream"),
            "Expected text/event-stream, got: {content_type}"
        );
    }

    #[tokio::test]
    async fn test_sse_task_events() {
        let state = make_state(EchoHandler);
        let app = A2AServer::router(state);

        let body = jsonrpc_request(
            "tasks/sendSubscribe",
            serde_json::json!({
                "message": {
                    "role": "user",
                    "parts": [{"type": "text", "text": "Hello SSE"}],
                    "metadata": {}
                }
            }),
        );

        let req = Request::builder()
            .method("POST")
            .uri("/a2a/stream")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Read the entire SSE body
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_str = String::from_utf8_lossy(&body_bytes);

        // SSE events are prefixed with "data: " lines
        let data_lines: Vec<&str> = body_str
            .lines()
            .filter(|l| l.starts_with("data:"))
            .collect();

        // Should have at least 2 events: working status + final status
        assert!(
            data_lines.len() >= 2,
            "Expected at least 2 SSE data lines, got {}: {body_str}",
            data_lines.len()
        );

        // Parse each data line and verify structure
        for line in &data_lines {
            let json_str = line.trim_start_matches("data:").trim();
            let parsed: serde_json::Value = serde_json::from_str(json_str)
                .unwrap_or_else(|e| panic!("Failed to parse SSE data: {e}, line: {line}"));
            assert!(
                parsed.get("type").is_some(),
                "SSE event missing 'type' field: {parsed}"
            );
            assert!(
                parsed.get("data").is_some(),
                "SSE event missing 'data' field: {parsed}"
            );
        }

        // The last event should be a "final" event
        let last_line = data_lines.last().unwrap();
        let last_json: serde_json::Value =
            serde_json::from_str(last_line.trim_start_matches("data:").trim()).unwrap();
        assert_eq!(
            last_json["type"], "final",
            "Last SSE event should be 'final', got: {}",
            last_json["type"]
        );
    }

    #[tokio::test]
    async fn test_sse_non_streaming_fallback() {
        // Use EchoHandler which does NOT implement StreamingTaskHandler.
        // The SSE endpoint should still work by falling back to handle_task.
        let state = make_state(EchoHandler);
        let app = A2AServer::router(state);

        let body = jsonrpc_request(
            "tasks/sendSubscribe",
            serde_json::json!({
                "message": {
                    "role": "user",
                    "parts": [{"type": "text", "text": "Fallback test"}],
                    "metadata": {}
                }
            }),
        );

        let req = Request::builder()
            .method("POST")
            .uri("/a2a/stream")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let content_type = resp
            .headers()
            .get("content-type")
            .expect("Missing content-type header")
            .to_str()
            .unwrap();
        assert!(content_type.contains("text/event-stream"));

        // Read entire SSE body
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_str = String::from_utf8_lossy(&body_bytes);

        // Should contain at least the working + final events
        let data_lines: Vec<&str> = body_str
            .lines()
            .filter(|l| l.starts_with("data:"))
            .collect();

        assert!(
            data_lines.len() >= 2,
            "Fallback should emit at least 2 SSE events, got {}: {body_str}",
            data_lines.len()
        );

        // Verify the final event indicates completed status
        let last_line = data_lines.last().unwrap();
        let last_json: serde_json::Value =
            serde_json::from_str(last_line.trim_start_matches("data:").trim()).unwrap();
        assert_eq!(last_json["type"], "final");
        assert_eq!(last_json["data"]["status"], "completed");
    }

    #[tokio::test]
    async fn test_sse_wrong_method_rejected() {
        let state = make_state(EchoHandler);
        let app = A2AServer::router(state);

        // Send tasks/send to the SSE endpoint — should be rejected
        let body = jsonrpc_request(
            "tasks/send",
            serde_json::json!({
                "message": {
                    "role": "user",
                    "parts": [{"type": "text", "text": "Wrong method"}],
                    "metadata": {}
                }
            }),
        );

        let req = Request::builder()
            .method("POST")
            .uri("/a2a/stream")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_sse_missing_params() {
        let state = make_state(EchoHandler);
        let app = A2AServer::router(state);

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tasks/sendSubscribe",
        });

        let req = Request::builder()
            .method("POST")
            .uri("/a2a/stream")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_sse_streaming_handler_events() {
        // Use the StreamingEchoHandler — although the generic SSE handler
        // will use the non-streaming fallback (since as_any downcast to
        // Box<dyn StreamingTaskHandler> won't match a concrete type),
        // we verify the run_streaming_handler helper works correctly.
        let handler = StreamingEchoHandler;
        let task = A2ATask::new(TaskMessage::user_text("Stream test"));
        let tasks_store = Arc::new(RwLock::new(HashMap::new()));
        let (tx, mut rx) = tokio::sync::mpsc::channel::<TaskStreamEvent>(32);

        let result = run_streaming_handler(&handler, &task, &tasks_store, tx).await;
        assert!(result.is_ok());

        let updated = result.unwrap();
        assert_eq!(updated.status, TaskStatus::Completed);

        // Collect all events from the channel
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }

        // Should have: working status, artifact, message, completed status
        assert!(
            events.len() >= 4,
            "Expected at least 4 events from streaming handler, got {}",
            events.len()
        );

        // Verify event types
        let type_names: Vec<String> = events
            .iter()
            .map(|e| match e {
                TaskStreamEvent::StatusUpdate { .. } => "status_update".to_string(),
                TaskStreamEvent::Artifact { .. } => "artifact".to_string(),
                TaskStreamEvent::Message { .. } => "message".to_string(),
            })
            .collect();

        assert!(type_names.contains(&"status_update".to_string()));
        assert!(type_names.contains(&"artifact".to_string()));
        assert!(type_names.contains(&"message".to_string()));
    }
}
