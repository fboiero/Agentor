//! Headless mode: reads NDJSON from stdin, writes NDJSON to stdout.
//!
//! Used by Python/TypeScript SDKs to wrap Argentor as a subprocess.
//!
//! # Usage
//!
//! ```text
//! argentor --headless
//! echo '{"type":"init","model":"claude-4"}\n{"type":"query","prompt":"hello"}' | argentor --headless
//! ```
//!
//! # Protocol
//!
//! Inbound messages (stdin, one JSON per line):
//! - `{"type":"init", ...}` — Initialize session with model/tools.
//! - `{"type":"query", "prompt":"..."}` — Run a query.
//! - `{"type":"tool_result", "call_id":"...", "result":"..."}` — Provide a tool result.
//! - `{"type":"ping"}` — Health check.
//!
//! Outbound messages (stdout, one JSON per line):
//! - `{"type":"ready"}` — Session initialized.
//! - `{"type":"response", "text":"..."}` — LLM response text.
//! - `{"type":"tool_call", "call_id":"...", "name":"...", "args":{...}}` — Tool call request.
//! - `{"type":"error", "message":"..."}` — Error.
//! - `{"type":"pong"}` — Health check response.

use argentor_core::ArgentorResult;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Protocol messages
// ---------------------------------------------------------------------------

/// Inbound message from the SDK (stdin).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InboundMessage {
    /// Initialize a session.
    Init {
        /// Model identifier.
        #[serde(default)]
        model: Option<String>,
        /// Permission mode.
        #[serde(default)]
        permission_mode: Option<String>,
        /// Working directory override.
        #[serde(default)]
        working_dir: Option<String>,
    },
    /// Run a query prompt.
    Query {
        /// The user prompt.
        prompt: String,
        /// Optional session ID for multi-turn conversations.
        #[serde(default)]
        session_id: Option<String>,
    },
    /// Provide a tool result back to the agent.
    ToolResult {
        /// The call ID that this result corresponds to.
        call_id: String,
        /// The result content.
        result: String,
        /// Whether the tool call was successful.
        #[serde(default = "default_true")]
        success: bool,
    },
    /// Health check ping.
    Ping,
}

/// Outbound message to the SDK (stdout).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutboundMessage {
    /// Session is ready.
    Ready {
        /// Session ID.
        session_id: String,
        /// Model in use.
        model: String,
        /// Number of available tools.
        tool_count: usize,
    },
    /// LLM response text.
    Response {
        /// The response text.
        text: String,
        /// Token usage for this response.
        #[serde(default)]
        tokens_used: Option<usize>,
    },
    /// Agent is requesting a tool call.
    ToolCall {
        /// Unique call ID.
        call_id: String,
        /// Tool name.
        name: String,
        /// Tool arguments as JSON.
        args: serde_json::Value,
    },
    /// Error message.
    Error {
        /// Error description.
        message: String,
        /// Error code (optional).
        #[serde(default)]
        code: Option<String>,
    },
    /// Health check response.
    Pong,
}

fn default_true() -> bool {
    true
}

// ---------------------------------------------------------------------------
// Protocol handler
// ---------------------------------------------------------------------------

/// State machine for the headless NDJSON protocol.
pub struct ProtocolHandler {
    initialized: bool,
    model: String,
    session_id: Option<String>,
}

impl ProtocolHandler {
    /// Create a new protocol handler.
    pub fn new() -> Self {
        Self {
            initialized: false,
            model: "default".to_string(),
            session_id: None,
        }
    }

    /// Handle an inbound message and produce zero or more outbound messages.
    pub fn handle(&mut self, msg: InboundMessage) -> Vec<OutboundMessage> {
        match msg {
            InboundMessage::Init {
                model,
                permission_mode: _,
                working_dir: _,
            } => {
                self.model = model.unwrap_or_else(|| "default".to_string());
                self.initialized = true;
                let session_id = uuid::Uuid::new_v4().to_string();
                self.session_id = Some(session_id.clone());

                vec![OutboundMessage::Ready {
                    session_id,
                    model: self.model.clone(),
                    tool_count: 0,
                }]
            }
            InboundMessage::Query { prompt, session_id } => {
                if !self.initialized {
                    return vec![OutboundMessage::Error {
                        message: "Session not initialized. Send an 'init' message first."
                            .to_string(),
                        code: Some("NOT_INITIALIZED".to_string()),
                    }];
                }

                // Use provided session_id or the one from init
                let _sid = session_id
                    .or_else(|| self.session_id.clone())
                    .unwrap_or_default();

                // In the full implementation this would call AgentRunner.
                // For now, echo back the prompt as a response.
                vec![OutboundMessage::Response {
                    text: format!(
                        "[headless] Received query ({} chars) for model '{}'",
                        prompt.len(),
                        self.model
                    ),
                    tokens_used: Some(prompt.len() / 4),
                }]
            }
            InboundMessage::ToolResult {
                call_id,
                result: _,
                success: _,
            } => {
                if !self.initialized {
                    return vec![OutboundMessage::Error {
                        message: "Session not initialized.".to_string(),
                        code: Some("NOT_INITIALIZED".to_string()),
                    }];
                }

                // Acknowledge the tool result
                vec![OutboundMessage::Response {
                    text: format!("[headless] Tool result received for call '{call_id}'"),
                    tokens_used: None,
                }]
            }
            InboundMessage::Ping => vec![OutboundMessage::Pong],
        }
    }
}

impl Default for ProtocolHandler {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Encoding / Decoding
// ---------------------------------------------------------------------------

/// Decode an NDJSON line into an inbound message.
pub fn decode_message(line: &str) -> ArgentorResult<InboundMessage> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err(argentor_core::ArgentorError::Agent(
            "Empty message line".to_string(),
        ));
    }
    serde_json::from_str(trimmed).map_err(argentor_core::ArgentorError::from)
}

/// Encode an outbound message as an NDJSON line (with trailing newline).
pub fn encode_message(msg: &OutboundMessage) -> ArgentorResult<String> {
    let mut json = serde_json::to_string(msg)?;
    json.push('\n');
    Ok(json)
}

// ---------------------------------------------------------------------------
// Headless entry point
// ---------------------------------------------------------------------------

/// Run the headless NDJSON protocol loop.
///
/// Reads lines from stdin, processes them, and writes responses to stdout.
/// Exits when stdin reaches EOF.
pub async fn run_headless() -> ArgentorResult<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut handler = ProtocolHandler::new();

    let mut line = String::new();
    loop {
        line.clear();
        let bytes = reader.read_line(&mut line).await?;
        if bytes == 0 {
            break; // EOF
        }

        // Skip empty lines
        if line.trim().is_empty() {
            continue;
        }

        let inbound = match decode_message(&line) {
            Ok(msg) => msg,
            Err(e) => {
                let err = OutboundMessage::Error {
                    message: format!("Failed to decode message: {e}"),
                    code: Some("DECODE_ERROR".to_string()),
                };
                let encoded = encode_message(&err)?;
                stdout.write_all(encoded.as_bytes()).await?;
                stdout.flush().await?;
                continue;
            }
        };

        let responses = handler.handle(inbound);

        for msg in responses {
            let encoded = encode_message(&msg)?;
            stdout.write_all(encoded.as_bytes()).await?;
            stdout.flush().await?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // -- Decode tests --

    #[test]
    fn test_decode_init() {
        let line = r#"{"type":"init","model":"claude-4"}"#;
        let msg = decode_message(line).unwrap();
        match msg {
            InboundMessage::Init { model, .. } => {
                assert_eq!(model, Some("claude-4".to_string()));
            }
            _ => panic!("Expected Init"),
        }
    }

    #[test]
    fn test_decode_query() {
        let line = r#"{"type":"query","prompt":"hello world"}"#;
        let msg = decode_message(line).unwrap();
        match msg {
            InboundMessage::Query { prompt, session_id } => {
                assert_eq!(prompt, "hello world");
                assert!(session_id.is_none());
            }
            _ => panic!("Expected Query"),
        }
    }

    #[test]
    fn test_decode_query_with_session() {
        let line = r#"{"type":"query","prompt":"hi","session_id":"abc-123"}"#;
        let msg = decode_message(line).unwrap();
        match msg {
            InboundMessage::Query { session_id, .. } => {
                assert_eq!(session_id, Some("abc-123".to_string()));
            }
            _ => panic!("Expected Query"),
        }
    }

    #[test]
    fn test_decode_tool_result() {
        let line = r#"{"type":"tool_result","call_id":"c1","result":"42","success":true}"#;
        let msg = decode_message(line).unwrap();
        match msg {
            InboundMessage::ToolResult {
                call_id,
                result,
                success,
            } => {
                assert_eq!(call_id, "c1");
                assert_eq!(result, "42");
                assert!(success);
            }
            _ => panic!("Expected ToolResult"),
        }
    }

    #[test]
    fn test_decode_ping() {
        let line = r#"{"type":"ping"}"#;
        let msg = decode_message(line).unwrap();
        assert!(matches!(msg, InboundMessage::Ping));
    }

    #[test]
    fn test_decode_empty_line() {
        let result = decode_message("");
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_invalid_json() {
        let result = decode_message("{not json}");
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_unknown_type() {
        let result = decode_message(r#"{"type":"unknown_thing"}"#);
        assert!(result.is_err());
    }

    // -- Encode tests --

    #[test]
    fn test_encode_ready() {
        let msg = OutboundMessage::Ready {
            session_id: "s1".to_string(),
            model: "claude-4".to_string(),
            tool_count: 5,
        };
        let encoded = encode_message(&msg).unwrap();
        assert!(encoded.ends_with('\n'));
        assert!(encoded.contains(r#""type":"ready""#));
        assert!(encoded.contains(r#""session_id":"s1""#));
    }

    #[test]
    fn test_encode_response() {
        let msg = OutboundMessage::Response {
            text: "hello".to_string(),
            tokens_used: Some(10),
        };
        let encoded = encode_message(&msg).unwrap();
        assert!(encoded.contains(r#""type":"response""#));
        assert!(encoded.contains(r#""text":"hello""#));
    }

    #[test]
    fn test_encode_error() {
        let msg = OutboundMessage::Error {
            message: "bad request".to_string(),
            code: Some("BAD_REQUEST".to_string()),
        };
        let encoded = encode_message(&msg).unwrap();
        assert!(encoded.contains(r#""type":"error""#));
        assert!(encoded.contains("bad request"));
    }

    #[test]
    fn test_encode_pong() {
        let msg = OutboundMessage::Pong;
        let encoded = encode_message(&msg).unwrap();
        assert!(encoded.contains(r#""type":"pong""#));
    }

    #[test]
    fn test_encode_tool_call() {
        let msg = OutboundMessage::ToolCall {
            call_id: "c42".to_string(),
            name: "calculator".to_string(),
            args: serde_json::json!({"expression": "1+1"}),
        };
        let encoded = encode_message(&msg).unwrap();
        assert!(encoded.contains(r#""type":"tool_call""#));
        assert!(encoded.contains("calculator"));
        assert!(encoded.contains("c42"));
    }

    // -- Protocol handler tests --

    #[test]
    fn test_handler_ping_before_init() {
        let mut handler = ProtocolHandler::new();
        let responses = handler.handle(InboundMessage::Ping);
        assert_eq!(responses.len(), 1);
        assert!(matches!(responses[0], OutboundMessage::Pong));
    }

    #[test]
    fn test_handler_query_before_init() {
        let mut handler = ProtocolHandler::new();
        let responses = handler.handle(InboundMessage::Query {
            prompt: "hello".to_string(),
            session_id: None,
        });
        assert_eq!(responses.len(), 1);
        match &responses[0] {
            OutboundMessage::Error { code, .. } => {
                assert_eq!(code.as_deref(), Some("NOT_INITIALIZED"));
            }
            _ => panic!("Expected error"),
        }
    }

    #[test]
    fn test_handler_init_then_query() {
        let mut handler = ProtocolHandler::new();

        // Init
        let init_resp = handler.handle(InboundMessage::Init {
            model: Some("claude-4".to_string()),
            permission_mode: None,
            working_dir: None,
        });
        assert_eq!(init_resp.len(), 1);
        match &init_resp[0] {
            OutboundMessage::Ready {
                model, tool_count, ..
            } => {
                assert_eq!(model, "claude-4");
                assert_eq!(*tool_count, 0);
            }
            _ => panic!("Expected Ready"),
        }

        // Query
        let query_resp = handler.handle(InboundMessage::Query {
            prompt: "What is 2+2?".to_string(),
            session_id: None,
        });
        assert_eq!(query_resp.len(), 1);
        match &query_resp[0] {
            OutboundMessage::Response { text, tokens_used } => {
                assert!(text.contains("12 chars"));
                assert!(tokens_used.is_some());
            }
            _ => panic!("Expected Response"),
        }
    }

    #[test]
    fn test_handler_init_default_model() {
        let mut handler = ProtocolHandler::new();
        let responses = handler.handle(InboundMessage::Init {
            model: None,
            permission_mode: None,
            working_dir: None,
        });
        assert_eq!(responses.len(), 1);
        match &responses[0] {
            OutboundMessage::Ready { model, .. } => {
                assert_eq!(model, "default");
            }
            _ => panic!("Expected Ready"),
        }
    }

    #[test]
    fn test_handler_tool_result_before_init() {
        let mut handler = ProtocolHandler::new();
        let responses = handler.handle(InboundMessage::ToolResult {
            call_id: "c1".to_string(),
            result: "42".to_string(),
            success: true,
        });
        assert_eq!(responses.len(), 1);
        assert!(matches!(responses[0], OutboundMessage::Error { .. }));
    }

    #[test]
    fn test_handler_tool_result_after_init() {
        let mut handler = ProtocolHandler::new();
        handler.handle(InboundMessage::Init {
            model: None,
            permission_mode: None,
            working_dir: None,
        });
        let responses = handler.handle(InboundMessage::ToolResult {
            call_id: "call-abc".to_string(),
            result: "done".to_string(),
            success: true,
        });
        assert_eq!(responses.len(), 1);
        match &responses[0] {
            OutboundMessage::Response { text, .. } => {
                assert!(text.contains("call-abc"));
            }
            _ => panic!("Expected Response"),
        }
    }

    // -- Roundtrip tests --

    #[test]
    fn test_encode_decode_roundtrip() {
        let original = OutboundMessage::Response {
            text: "hello world".to_string(),
            tokens_used: Some(3),
        };
        let encoded = encode_message(&original).unwrap();
        // Can decode as serde_json::Value
        let value: serde_json::Value = serde_json::from_str(encoded.trim()).unwrap();
        assert_eq!(value["type"], "response");
        assert_eq!(value["text"], "hello world");
        assert_eq!(value["tokens_used"], 3);
    }

    #[test]
    fn test_inbound_roundtrip() {
        let original = InboundMessage::Query {
            prompt: "test query".to_string(),
            session_id: Some("sess-1".to_string()),
        };
        let json = serde_json::to_string(&original).unwrap();
        let decoded: InboundMessage = serde_json::from_str(&json).unwrap();
        match decoded {
            InboundMessage::Query { prompt, session_id } => {
                assert_eq!(prompt, "test query");
                assert_eq!(session_id, Some("sess-1".to_string()));
            }
            _ => panic!("Expected Query"),
        }
    }

    #[test]
    fn test_decode_with_trailing_whitespace() {
        let line = "  {\"type\":\"ping\"}  \n";
        let msg = decode_message(line).unwrap();
        assert!(matches!(msg, InboundMessage::Ping));
    }
}
