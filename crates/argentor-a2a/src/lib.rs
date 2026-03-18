//! Google Agent-to-Agent (A2A) interoperability protocol for the Argentor framework.
//!
//! This crate implements the A2A protocol, a JSON-RPC based standard for
//! agent-to-agent communication. It enables Argentor agents to interoperate
//! with any A2A-compliant agent, regardless of the underlying framework.
//!
//! # Key concepts
//!
//! - **Agent Cards** — JSON metadata describing agent capabilities, served at
//!   `/.well-known/agent.json`.
//! - **Tasks** — Units of work with a defined lifecycle
//!   (submitted -> working -> completed/failed/canceled).
//! - **Messages** — Communication within tasks (user messages, agent responses).
//! - **Artifacts** — Files or structured data produced by tasks.
//!
//! # Main types
//!
//! - [`AgentCard`] — Metadata describing an A2A agent's identity and capabilities.
//! - [`A2ATask`] — A task with lifecycle state, messages, and artifacts.
//! - [`A2AServer`] — Axum-based server implementing the A2A protocol endpoints.
//! - [`A2AClient`] — HTTP client for calling remote A2A agents (requires `client` feature).
//! - [`AgentCardBuilder`] — Fluent builder for constructing [`AgentCard`] instances.

/// A2A protocol types (Agent Cards, Tasks, Messages, Artifacts, JSON-RPC).
pub mod protocol;

/// A2A server — axum router serving agent card and JSON-RPC task endpoints.
pub mod server;

/// A2A client — HTTP client for interacting with remote A2A agents.
#[cfg(feature = "client")]
pub mod client;

/// Agent card builder and discovery helpers.
pub mod discovery;

pub use discovery::AgentCardBuilder;
pub use protocol::{
    A2ARequest, A2AResponse, A2ATask, AgentCapabilities, AgentCard, AgentSkill, AuthScheme,
    AuthenticationInfo, FileContent, JsonRpcEnvelope, MessagePart, MessageRole, TaskArtifact,
    TaskMessage, TaskStatus, TaskStreamEvent,
};
pub use server::{A2AServer, A2AServerState, StreamingTaskHandler, TaskHandler};

#[cfg(feature = "client")]
pub use client::A2AClient;
