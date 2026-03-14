use thiserror::Error;

/// A convenience `Result` alias using [`AgentorError`].
pub type AgentorResult<T> = Result<T, AgentorError>;

/// Top-level error type for the Agentor framework.
///
/// Each variant corresponds to a subsystem that can produce errors.
#[derive(Error, Debug)]
pub enum AgentorError {
    /// An error from the API gateway layer.
    #[error("Gateway error: {0}")]
    Gateway(String),

    /// An error originating from the agent execution loop.
    #[error("Agent error: {0}")]
    Agent(String),

    /// An error raised by a skill during invocation.
    #[error("Skill error: {0}")]
    Skill(String),

    /// An error from a communication channel (e.g. WebSocket, CLI).
    #[error("Channel error: {0}")]
    Channel(String),

    /// A security-related error (permissions, TLS, rate limiting).
    #[error("Security error: {0}")]
    Security(String),

    /// An error related to session persistence or lookup.
    #[error("Session error: {0}")]
    Session(String),

    /// An error in configuration parsing or validation.
    #[error("Config error: {0}")]
    Config(String),

    /// A serialization or deserialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// A standard I/O error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// An error from an outbound HTTP request (e.g. LLM API call).
    #[error("HTTP error: {0}")]
    Http(String),
}
