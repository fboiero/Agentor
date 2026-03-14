use serde::{Deserialize, Serialize};

/// A request from the LLM to invoke a specific tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique identifier assigned by the LLM for this tool call.
    pub id: String,
    /// Name of the tool to invoke.
    pub name: String,
    /// JSON arguments to pass to the tool.
    pub arguments: serde_json::Value,
}

/// The result returned after executing a [`ToolCall`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// The ID of the [`ToolCall`] this result corresponds to.
    pub call_id: String,
    /// The textual output produced by the tool.
    pub content: String,
    /// Whether the tool execution ended in an error.
    pub is_error: bool,
}

impl ToolResult {
    /// Creates a successful tool result.
    pub fn success(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            call_id: call_id.into(),
            content: content.into(),
            is_error: false,
        }
    }

    /// Creates an error tool result.
    pub fn error(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            call_id: call_id.into(),
            content: content.into(),
            is_error: true,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_result_success() {
        let result = ToolResult::success("call_1", "output");
        assert!(!result.is_error);
        assert_eq!(result.content, "output");
    }

    #[test]
    fn test_tool_result_error() {
        let result = ToolResult::error("call_1", "failed");
        assert!(result.is_error);
    }
}
