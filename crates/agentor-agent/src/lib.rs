//! Agent runner with LLM backend abstraction, failover, streaming, and context management.
//!
//! This crate implements the core agentic loop that drives Agentor agents,
//! including multi-provider LLM backends, automatic failover, token-aware
//! context windowing, and streaming event support.
//!
//! # Main types
//!
//! - [`AgentRunner`] — Executes the agentic loop: prompt, call tools, respond.
//! - [`ModelConfig`] — Configuration for model provider, name, and parameters.
//! - [`LlmProvider`] — Enum of supported LLM providers (OpenAI, Anthropic, etc.).
//! - [`ContextWindow`] — Token-aware sliding window over conversation history.
//! - [`StreamEvent`] — Events emitted during streamed agent responses.
//! - [`FailoverBackend`] — Multi-backend wrapper with automatic retry and failover.

/// LLM backend implementations.
pub mod backends;
/// Model and provider configuration.
pub mod config;
/// Token-aware context windowing.
pub mod context;
/// Failover and retry logic for LLM backends.
pub mod failover;
/// LLM client trait and HTTP transport.
pub mod llm;
/// Agent runner and agentic loop.
pub mod runner;
/// Streaming event types.
pub mod stream;

pub use backends::LlmBackend;
pub use config::{LlmProvider, ModelConfig};
pub use context::ContextWindow;
pub use failover::{FailoverBackend, RetryPolicy};
pub use llm::LlmClient;
pub use runner::AgentRunner;
pub use stream::StreamEvent;
