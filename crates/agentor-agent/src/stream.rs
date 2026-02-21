use serde::{Deserialize, Serialize};

/// Events emitted during a streaming LLM response.
///
/// These events allow consumers (e.g. WebSocket handlers) to receive partial
/// results as they arrive from the LLM provider, enabling real-time display of
/// text generation and tool call progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    /// A chunk of text content from the assistant.
    TextDelta {
        text: String,
    },

    /// A new tool call has started.
    ToolCallStart {
        id: String,
        name: String,
    },

    /// An incremental fragment of tool call arguments (JSON string delta).
    ToolCallDelta {
        id: String,
        arguments_delta: String,
    },

    /// A tool call's arguments are now complete.
    ToolCallEnd {
        id: String,
    },

    /// The stream has finished successfully.
    Done,

    /// An error occurred during streaming.
    Error {
        message: String,
    },
}
