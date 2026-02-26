//! Security primitives for the Agentor framework.
//!
//! Provides capabilities, permissions, rate limiting, audit logging,
//! input sanitization, and TLS configuration used throughout the system.
//!
//! # Main types
//!
//! - [`Capability`] — A fine-grained permission token (file, network, shell, etc.).
//! - [`PermissionSet`] — A collection of granted capabilities.
//! - [`RateLimiter`] — Token-bucket rate limiter for request throttling.
//! - [`AuditLog`] — Append-only audit trail persisted to disk.
//! - [`Sanitizer`] — Input sanitization utilities.
//! - [`TlsConfig`] — TLS and mutual-TLS configuration.

/// Audit logging module.
pub mod audit;
/// Capability and permission definitions.
pub mod capability;
/// Token-bucket rate limiting.
pub mod rate_limit;
/// Input sanitization utilities.
pub mod sanitizer;
/// TLS and mutual-TLS configuration.
pub mod tls;

pub use audit::AuditLog;
pub use capability::{Capability, PermissionSet};
pub use rate_limit::RateLimiter;
pub use sanitizer::Sanitizer;
pub use tls::TlsConfig;
