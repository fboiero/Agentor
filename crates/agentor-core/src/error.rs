use thiserror::Error;

pub type AgentorResult<T> = Result<T, AgentorError>;

#[derive(Error, Debug)]
pub enum AgentorError {
    #[error("Gateway error: {0}")]
    Gateway(String),

    #[error("Agent error: {0}")]
    Agent(String),

    #[error("Skill error: {0}")]
    Skill(String),

    #[error("Channel error: {0}")]
    Channel(String),

    #[error("Security error: {0}")]
    Security(String),

    #[error("Session error: {0}")]
    Session(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(String),
}
