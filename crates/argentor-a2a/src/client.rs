//! A2A client for communicating with remote A2A-compliant agents.
//!
//! Requires the `client` feature (enabled by default), which brings in `reqwest`.
//!
//! # Example
//!
//! ```rust,no_run
//! use argentor_a2a::client::A2AClient;
//! use argentor_a2a::protocol::TaskMessage;
//!
//! # async fn example() -> argentor_core::ArgentorResult<()> {
//! let client = A2AClient::new("http://localhost:3000");
//! let card = client.get_agent_card().await?;
//! println!("Connected to: {}", card.name);
//!
//! let task = client.send_task(TaskMessage::user_text("Hello, agent!"), None).await?;
//! println!("Task status: {:?}", task.status);
//! # Ok(())
//! # }
//! ```

use crate::protocol::*;
use argentor_core::{ArgentorError, ArgentorResult};
use tracing::debug;

/// HTTP client for interacting with a remote A2A agent.
pub struct A2AClient {
    /// Base URL of the remote A2A agent (e.g. "http://localhost:3000").
    base_url: String,
    /// The underlying HTTP client.
    http_client: reqwest::Client,
}

impl A2AClient {
    /// Create a new A2A client pointing at the given base URL.
    ///
    /// The base URL should not include a trailing slash or path components.
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http_client: reqwest::Client::new(),
        }
    }

    /// Create a new A2A client with a custom `reqwest::Client`.
    ///
    /// Useful for configuring timeouts, TLS certificates, or proxies.
    pub fn with_client(base_url: &str, client: reqwest::Client) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http_client: client,
        }
    }

    /// Fetch the agent card from `/.well-known/agent.json`.
    pub async fn get_agent_card(&self) -> ArgentorResult<AgentCard> {
        let url = format!("{}/.well-known/agent.json", self.base_url);
        debug!(url = %url, "Fetching A2A agent card");

        let resp = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| ArgentorError::Http(format!("Failed to fetch agent card: {e}")))?;

        if !resp.status().is_success() {
            return Err(ArgentorError::Http(format!(
                "Agent card request failed with status: {}",
                resp.status()
            )));
        }

        let card: AgentCard = resp
            .json()
            .await
            .map_err(|e| ArgentorError::Http(format!("Failed to parse agent card: {e}")))?;

        Ok(card)
    }

    /// Send a task to the remote agent.
    ///
    /// Creates a new task with the given message. If `session_id` is provided,
    /// the task will be associated with that session for grouping.
    pub async fn send_task(
        &self,
        message: TaskMessage,
        session_id: Option<String>,
    ) -> ArgentorResult<A2ATask> {
        let mut params = serde_json::json!({
            "message": message,
        });
        if let Some(sid) = session_id {
            params["sessionId"] = serde_json::Value::String(sid);
        }

        let result = self.jsonrpc_call("tasks/send", Some(params)).await?;
        let task: A2ATask = serde_json::from_value(result)
            .map_err(|e| ArgentorError::Http(format!("Failed to parse task response: {e}")))?;

        Ok(task)
    }

    /// Send a message to an existing task.
    pub async fn send_task_message(
        &self,
        task_id: &str,
        message: TaskMessage,
    ) -> ArgentorResult<A2ATask> {
        let params = serde_json::json!({
            "id": task_id,
            "message": message,
        });

        let result = self.jsonrpc_call("tasks/send", Some(params)).await?;
        let task: A2ATask = serde_json::from_value(result)
            .map_err(|e| ArgentorError::Http(format!("Failed to parse task response: {e}")))?;

        Ok(task)
    }

    /// Get the current state of a task by its ID.
    pub async fn get_task(&self, task_id: &str) -> ArgentorResult<A2ATask> {
        let params = serde_json::json!({"id": task_id});

        let result = self.jsonrpc_call("tasks/get", Some(params)).await?;
        let task: A2ATask = serde_json::from_value(result)
            .map_err(|e| ArgentorError::Http(format!("Failed to parse task response: {e}")))?;

        Ok(task)
    }

    /// Cancel a running task.
    pub async fn cancel_task(&self, task_id: &str) -> ArgentorResult<A2ATask> {
        let params = serde_json::json!({"id": task_id});

        let result = self.jsonrpc_call("tasks/cancel", Some(params)).await?;
        let task: A2ATask = serde_json::from_value(result)
            .map_err(|e| ArgentorError::Http(format!("Failed to parse task response: {e}")))?;

        Ok(task)
    }

    /// List tasks, optionally filtered by session ID.
    pub async fn list_tasks(&self, session_id: Option<&str>) -> ArgentorResult<Vec<A2ATask>> {
        let params = match session_id {
            Some(sid) => serde_json::json!({"sessionId": sid}),
            None => serde_json::json!({}),
        };

        let result = self.jsonrpc_call("tasks/list", Some(params)).await?;
        let tasks: Vec<A2ATask> = serde_json::from_value(result)
            .map_err(|e| ArgentorError::Http(format!("Failed to parse tasks list: {e}")))?;

        Ok(tasks)
    }

    /// Get the base URL this client is configured to connect to.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Make a JSON-RPC 2.0 call to the remote agent's `/a2a` endpoint.
    async fn jsonrpc_call(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> ArgentorResult<serde_json::Value> {
        let url = format!("{}/a2a", self.base_url);
        debug!(url = %url, method = %method, "A2A JSON-RPC call");

        let request_body = JsonRpcEnvelope {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(1)),
            method: method.to_string(),
            params,
        };

        let resp = self
            .http_client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| ArgentorError::Http(format!("A2A request failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(ArgentorError::Http(format!(
                "A2A request returned status: {}",
                resp.status()
            )));
        }

        let rpc_response: A2AResponse = resp
            .json()
            .await
            .map_err(|e| ArgentorError::Http(format!("Failed to parse A2A response: {e}")))?;

        if let Some(err) = rpc_response.error {
            return Err(ArgentorError::Http(format!(
                "A2A error {}: {}",
                err.code, err.message
            )));
        }

        rpc_response
            .result
            .ok_or_else(|| ArgentorError::Http("A2A response missing result".to_string()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_client_creation() {
        let client = A2AClient::new("http://localhost:3000");
        assert_eq!(client.base_url(), "http://localhost:3000");
    }

    #[test]
    fn test_client_strips_trailing_slash() {
        let client = A2AClient::new("http://localhost:3000/");
        assert_eq!(client.base_url(), "http://localhost:3000");
    }

    #[test]
    fn test_client_with_custom_client() {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap();
        let client = A2AClient::with_client("http://remote-agent.example.com", http_client);
        assert_eq!(client.base_url(), "http://remote-agent.example.com");
    }

    #[test]
    fn test_jsonrpc_envelope_serialization() {
        let envelope = JsonRpcEnvelope {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(1)),
            method: "tasks/send".to_string(),
            params: Some(serde_json::json!({
                "message": {
                    "role": "user",
                    "parts": [{"type": "text", "text": "Hello"}],
                    "metadata": {}
                }
            })),
        };
        let json = serde_json::to_string(&envelope).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["method"], "tasks/send");
        assert!(parsed["params"]["message"].is_object());
    }

    #[test]
    fn test_a2a_response_parsing_success() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "id": "task-1",
                "status": "completed",
                "messages": [],
                "artifacts": [],
                "metadata": {},
                "history": []
            }
        }"#;
        let resp: A2AResponse = serde_json::from_str(json).unwrap();
        assert!(resp.error.is_none());
        assert!(resp.result.is_some());
    }

    #[test]
    fn test_a2a_response_parsing_error() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32601,
                "message": "Method not found"
            }
        }"#;
        let resp: A2AResponse = serde_json::from_str(json).unwrap();
        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32601);
    }

    #[test]
    fn test_send_task_params_construction() {
        let message = TaskMessage::user_text("Hello");
        let params = serde_json::json!({
            "message": message,
        });
        let parsed: serde_json::Value = params;
        assert_eq!(parsed["message"]["role"], "user");
        assert_eq!(parsed["message"]["parts"][0]["type"], "text");
        assert_eq!(parsed["message"]["parts"][0]["text"], "Hello");
    }

    #[test]
    fn test_send_task_params_with_session() {
        let message = TaskMessage::user_text("Hello");
        let mut params = serde_json::json!({
            "message": message,
        });
        params["sessionId"] = serde_json::Value::String("sess-123".to_string());
        assert_eq!(params["sessionId"], "sess-123");
    }

    #[test]
    fn test_task_params_construction() {
        let params: HashMap<String, serde_json::Value> = HashMap::new();
        let serialized = serde_json::to_value(&params).unwrap();
        assert!(serialized.is_object());
    }
}
