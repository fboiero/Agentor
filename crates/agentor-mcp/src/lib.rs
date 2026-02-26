//! Model Context Protocol (MCP) client, proxy, and service discovery.
//!
//! Implements the MCP specification over JSON-RPC 2.0 / stdio, enabling
//! Agentor to connect to external tool servers, discover their capabilities,
//! and expose them as skills within the agent framework.
//!
//! # Main types
//!
//! - [`McpClient`] — JSON-RPC 2.0 client for communicating with MCP servers.
//! - [`McpSkill`] — Wraps an MCP tool as an Agentor [`Skill`](agentor_skills::Skill).
//! - [`McpProxy`] — Transparent proxy that multiplexes requests to multiple MCP servers.
//! - [`McpServerManager`] — Manages lifecycle and health of connected MCP servers.

/// MCP JSON-RPC client.
pub mod client;
/// Tool discovery over MCP.
pub mod discovery;
/// MCP server lifecycle manager.
pub mod manager;
/// JSON-RPC 2.0 protocol types.
pub mod protocol;
/// MCP proxy for multi-server multiplexing.
pub mod proxy;
/// MCP tool-to-skill adapter.
pub mod skill;

pub use client::McpClient;
pub use discovery::ToolDiscovery;
pub use manager::{McpServerConfig, McpServerManager, McpServerStatus};
pub use proxy::McpProxy;
pub use skill::McpSkill;
