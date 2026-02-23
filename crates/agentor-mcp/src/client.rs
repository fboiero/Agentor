//! MCP client — connects to an MCP server via stdio (subprocess) and
//! exchanges JSON-RPC 2.0 messages.

use crate::protocol::*;
use agentor_core::{AgentorError, AgentorResult};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{oneshot, Mutex};
use tracing::{debug, error, info};

/// MCP client that communicates with an MCP server over stdio.
pub struct McpClient {
    stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    _child: Arc<Mutex<Child>>,
    pending: Arc<Mutex<std::collections::HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
    next_id: Arc<AtomicU64>,
    server_name: String,
}

impl McpClient {
    /// Spawn an MCP server subprocess and perform the initialization handshake.
    pub async fn connect(
        command: &str,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> AgentorResult<(Self, Vec<McpToolDef>)> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());

        for (key, val) in env {
            cmd.env(key, val);
        }

        let mut child = cmd.spawn().map_err(|e| {
            AgentorError::Skill(format!("Failed to spawn MCP server '{}': {}", command, e))
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AgentorError::Skill("MCP server stdin not available".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AgentorError::Skill("MCP server stdout not available".into()))?;

        let pending: Arc<Mutex<std::collections::HashMap<u64, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(std::collections::HashMap::new()));

        // Spawn reader task to process responses
        let pending_clone = pending.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        debug!("MCP server stdout closed");
                        break;
                    }
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<JsonRpcResponse>(trimmed) {
                            Ok(resp) => {
                                if let Some(id) = resp.id {
                                    let mut map = pending_clone.lock().await;
                                    if let Some(tx) = map.remove(&id) {
                                        let _ = tx.send(resp);
                                    }
                                }
                                // Notifications (no id) are ignored for now
                            }
                            Err(e) => {
                                debug!(line = %trimmed, error = %e, "Non-JSON-RPC line from MCP server");
                            }
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Error reading MCP server stdout");
                        break;
                    }
                }
            }
        });

        let client = Self {
            stdin: Arc::new(Mutex::new(stdin)),
            _child: Arc::new(Mutex::new(child)),
            pending,
            next_id: Arc::new(AtomicU64::new(1)),
            server_name: command.to_string(),
        };

        // Initialize handshake
        let init_result = client.initialize().await?;
        info!(
            server = %client.server_name,
            version = %init_result.protocol_version,
            "MCP server initialized"
        );

        // Send initialized notification
        client.notify("notifications/initialized", None).await?;

        // Discover tools
        let tools = client.list_tools().await?;
        info!(
            server = %client.server_name,
            tools = tools.len(),
            "MCP tools discovered"
        );

        Ok((client, tools))
    }

    /// Send a JSON-RPC request and wait for the response.
    async fn request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> AgentorResult<JsonRpcResponse> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let req = JsonRpcRequest::new(id, method, params);

        let (tx, rx) = oneshot::channel();
        {
            let mut map = self.pending.lock().await;
            map.insert(id, tx);
        }

        let msg = serde_json::to_string(&req)
            .map_err(|e| AgentorError::Skill(format!("Failed to serialize request: {}", e)))?;

        {
            let mut stdin = self.stdin.lock().await;
            stdin
                .write_all(msg.as_bytes())
                .await
                .map_err(|e| AgentorError::Skill(format!("Failed to write to MCP stdin: {}", e)))?;
            stdin
                .write_all(b"\n")
                .await
                .map_err(|e| AgentorError::Skill(format!("Failed to write newline: {}", e)))?;
            stdin
                .flush()
                .await
                .map_err(|e| AgentorError::Skill(format!("Failed to flush stdin: {}", e)))?;
        }

        let resp = tokio::time::timeout(std::time::Duration::from_secs(30), rx)
            .await
            .map_err(|_| AgentorError::Skill(format!("MCP request '{}' timed out", method)))?
            .map_err(|_| AgentorError::Skill("MCP response channel dropped".into()))?;

        if let Some(err) = &resp.error {
            return Err(AgentorError::Skill(format!(
                "MCP error {}: {}",
                err.code, err.message
            )));
        }

        Ok(resp)
    }

    /// Send a JSON-RPC notification (no response expected).
    async fn notify(&self, method: &str, params: Option<serde_json::Value>) -> AgentorResult<()> {
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params.unwrap_or(serde_json::json!({})),
        });

        let serialized = serde_json::to_string(&msg)
            .map_err(|e| AgentorError::Skill(format!("Failed to serialize notification: {}", e)))?;

        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(serialized.as_bytes())
            .await
            .map_err(|e| AgentorError::Skill(format!("Failed to write notification: {}", e)))?;
        stdin
            .write_all(b"\n")
            .await
            .map_err(|e| AgentorError::Skill(format!("Failed to write newline: {}", e)))?;
        stdin
            .flush()
            .await
            .map_err(|e| AgentorError::Skill(format!("Failed to flush: {}", e)))?;

        Ok(())
    }

    /// Perform the MCP initialize handshake.
    async fn initialize(&self) -> AgentorResult<InitializeResult> {
        let params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "agentor",
                "version": env!("CARGO_PKG_VERSION")
            }
        });

        let resp = self.request("initialize", Some(params)).await?;
        let result: InitializeResult = serde_json::from_value(
            resp.result
                .ok_or_else(|| AgentorError::Skill("Empty initialize result".into()))?,
        )
        .map_err(|e| AgentorError::Skill(format!("Failed to parse initialize result: {}", e)))?;

        Ok(result)
    }

    /// List available tools from the MCP server.
    pub async fn list_tools(&self) -> AgentorResult<Vec<McpToolDef>> {
        let resp = self.request("tools/list", None).await?;
        let result = resp
            .result
            .ok_or_else(|| AgentorError::Skill("Empty tools/list result".into()))?;

        let tools: Vec<McpToolDef> = serde_json::from_value(
            result
                .get("tools")
                .cloned()
                .unwrap_or(serde_json::json!([])),
        )
        .map_err(|e| AgentorError::Skill(format!("Failed to parse tools: {}", e)))?;

        Ok(tools)
    }

    /// Call a tool on the MCP server.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> AgentorResult<McpToolResult> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments,
        });

        let resp = self.request("tools/call", Some(params)).await?;
        let result = resp
            .result
            .ok_or_else(|| AgentorError::Skill("Empty tools/call result".into()))?;

        let tool_result: McpToolResult = serde_json::from_value(result)
            .map_err(|e| AgentorError::Skill(format!("Failed to parse tool result: {}", e)))?;

        Ok(tool_result)
    }

    /// Get the server name.
    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// Health check — verify the server is responsive by calling list_tools.
    pub async fn health_check(&self) -> AgentorResult<()> {
        let _tools = self.list_tools().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_rpc_request_serialization() {
        let req = JsonRpcRequest::new(1, "test/method", Some(serde_json::json!({"key": "value"})));
        let json = serde_json::to_string(&req).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["id"], 1);
        assert_eq!(parsed["method"], "test/method");
        assert_eq!(parsed["params"]["key"], "value");
    }

    #[test]
    fn test_json_rpc_request_no_params() {
        let req = JsonRpcRequest::new(2, "tools/list", None);
        let json = serde_json::to_string(&req).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("params").is_none());
    }

    #[test]
    fn test_json_rpc_response_parse() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, Some(1));
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_json_rpc_error_parse() {
        let json =
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32600,"message":"Invalid request"}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert!(resp.error.is_some());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32600);
        assert_eq!(err.message, "Invalid request");
    }

    #[test]
    fn test_mcp_tool_def_parse() {
        let json = r#"{"name":"read_file","description":"Read a file","inputSchema":{"type":"object","properties":{"path":{"type":"string"}}}}"#;
        let tool: McpToolDef = serde_json::from_str(json).unwrap();
        assert_eq!(tool.name, "read_file");
        assert_eq!(tool.description, "Read a file");
    }

    #[test]
    fn test_mcp_tool_result_parse() {
        let json = r#"{"content":[{"type":"text","text":"file contents here"}],"isError":false}"#;
        let result: McpToolResult = serde_json::from_str(json).unwrap();
        assert!(!result.is_error);
        assert_eq!(result.content.len(), 1);
        assert_eq!(result.content[0].text, "file contents here");
    }

    #[test]
    fn test_initialize_result_parse() {
        let json = r#"{"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"test-server","version":"1.0"}}"#;
        let result: InitializeResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.protocol_version, "2024-11-05");
        assert!(result.capabilities.tools.is_some());
        assert_eq!(result.server_info.unwrap().name, "test-server");
    }
}
