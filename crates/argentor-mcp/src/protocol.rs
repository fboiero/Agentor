//! MCP (Model Context Protocol) JSON-RPC 2.0 message types.

use serde::{Deserialize, Serialize};

/// JSON-RPC 2.0 request sent to an MCP server.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcRequest {
    /// Protocol version (always `"2.0"`).
    pub jsonrpc: &'static str,
    /// Monotonically increasing request identifier.
    pub id: u64,
    /// RPC method name (e.g., `"tools/list"`, `"tools/call"`).
    pub method: String,
    /// Optional parameters for the method.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    /// Create a new JSON-RPC 2.0 request with the given id, method, and params.
    pub fn new(id: u64, method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method: method.into(),
            params,
        }
    }
}

/// JSON-RPC 2.0 response received from an MCP server.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcResponse {
    /// Protocol version.
    #[allow(dead_code)]
    pub jsonrpc: String,
    /// Request id this response corresponds to.
    pub id: Option<u64>,
    /// Successful result payload (mutually exclusive with `error`).
    pub result: Option<serde_json::Value>,
    /// Error payload (mutually exclusive with `result`).
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 error object returned inside a [`JsonRpcResponse`].
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcError {
    /// Numeric error code (negative values are reserved by the spec).
    pub code: i64,
    /// Human-readable error message.
    pub message: String,
    /// Optional structured data attached to the error.
    pub data: Option<serde_json::Value>,
}

/// MCP tool definition from the `tools/list` response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpToolDef {
    /// Unique name of the tool.
    pub name: String,
    /// Human-readable description of what the tool does.
    #[serde(default)]
    pub description: String,
    /// JSON Schema describing the tool's input parameters.
    #[serde(default = "default_input_schema", rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

fn default_input_schema() -> serde_json::Value {
    serde_json::json!({"type": "object", "properties": {}})
}

/// Result of invoking a tool via `tools/call`.
#[derive(Debug, Clone, Deserialize)]
pub struct McpToolResult {
    /// Content blocks returned by the tool.
    #[serde(default)]
    pub content: Vec<McpContent>,
    /// `true` when the tool execution ended in an error.
    #[serde(default, rename = "isError")]
    pub is_error: bool,
}

/// A content block inside an MCP tool result.
#[derive(Debug, Clone, Deserialize)]
pub struct McpContent {
    /// MIME-style type indicator (e.g., `"text"`).
    #[serde(rename = "type")]
    pub content_type: String,
    /// Textual payload of the content block.
    #[serde(default)]
    pub text: String,
}

/// Capabilities advertised by an MCP server during initialization.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ServerCapabilities {
    /// Tool-related capabilities, if the server exposes tools.
    #[serde(default)]
    pub tools: Option<serde_json::Value>,
    /// Resource-related capabilities, if the server exposes resources.
    #[serde(default)]
    pub resources: Option<serde_json::Value>,
    /// Prompt-related capabilities, if the server exposes prompts.
    #[serde(default)]
    pub prompts: Option<serde_json::Value>,
}

/// Response to the MCP `initialize` handshake.
#[derive(Debug, Clone, Deserialize)]
pub struct InitializeResult {
    /// MCP protocol version supported by the server.
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    /// Server capability declarations.
    #[serde(default)]
    pub capabilities: ServerCapabilities,
    /// Identifying information about the MCP server.
    #[serde(default, rename = "serverInfo")]
    pub server_info: Option<ServerInfo>,
}

/// Identifying information about an MCP server.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerInfo {
    /// Human-readable server name.
    pub name: String,
    /// Server version string.
    #[serde(default)]
    pub version: String,
}
