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
/// Audit log query and filtering.
pub mod audit_query;
/// Capability and permission definitions.
pub mod capability;
/// Encrypted at-rest storage (AES-256-GCM).
pub mod encrypted_store;
/// Token-bucket rate limiting.
pub mod rate_limit;
/// Role-Based Access Control (RBAC).
pub mod rbac;
/// Input sanitization utilities.
pub mod sanitizer;
/// TLS and mutual-TLS configuration.
pub mod tls;

pub use audit::AuditLog;
pub use audit_query::{query_audit_log, AuditFilter, AuditQueryResult};
pub use capability::{Capability, PermissionSet};
pub use encrypted_store::EncryptedStore;
pub use rate_limit::RateLimiter;
pub use rbac::{RbacDecision, RbacPolicy, Role};
pub use sanitizer::Sanitizer;
pub use tls::TlsConfig;
