//! Argentor Cloud — multi-tenant managed runtime for hosted deployments.
//!
//! # Status
//!
//! Scaffolding for future Argentor Cloud SaaS offering. All managers
//! are in-memory stubs in v1.x. Production deployment requires:
//!
//! - PostgreSQL for tenant/quota persistence
//! - Redis for active session caching
//! - Object storage (S3) for audit logs
//! - Stripe/Paddle for billing
//! - CDN for hosted dashboard
//!
//! # Modules
//!
//! - [`tenant`] — Multi-tenant management
//! - [`quota`] — Per-tenant quota enforcement
//! - [`runtime`] — Managed agent runtime with isolation
//! - [`dashboard`] — Hosted dashboard adapter
//! - [`billing`] — Usage-based billing integration
//! - [`scheduler`] — Multi-tenant work scheduler
//! - [`audit`] — Cloud-grade audit logging

/// Cloud-grade audit logging for managed deployments.
pub mod audit;
/// Usage-based billing integration (Stripe/Paddle stubs).
pub mod billing;
/// Hosted dashboard adapter (read-only telemetry view).
pub mod dashboard;
/// Per-tenant quota tracking and enforcement.
pub mod quota;
/// Managed agent runtime with tenant isolation.
pub mod runtime;
/// Multi-tenant work scheduler.
pub mod scheduler;
/// Multi-tenant management — tenants, plans, regions.
pub mod tenant;

pub use audit::{AuditEvent, AuditEventKind, AuditLog, AuditSink};
pub use billing::{BillingError, BillingProvider, Invoice, InvoiceLineItem, UsageMeter};
pub use dashboard::{DashboardAdapter, DashboardSnapshot, TenantMetrics};
pub use quota::{QuotaEnforcer, QuotaError, QuotaUsage};
pub use runtime::{ManagedError, ManagedRunConfig, ManagedRunResult, ManagedRuntime};
pub use scheduler::{CloudScheduler, ScheduledJob, SchedulerError};
pub use tenant::{DataRegion, Tenant, TenantManager, TenantPlan, TenantStatus};
