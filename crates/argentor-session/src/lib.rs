//! Session management and persistence for the Argentor framework.
//!
//! Handles conversation session lifecycle, file-backed persistence,
//! and structured transcript storage for audit and replay.
//!
//! # Main types
//!
//! - [`Session`] — Represents a single conversation session with metadata.
//! - [`SessionStore`] — Trait for session persistence backends.
//! - [`FileSessionStore`] — File-system-backed session store.
//! - [`SqliteSessionStore`] — Atomic JSON-file-per-table session store with index.
//! - [`PersistentUsageStore`] — Append-only JSONL usage tracking per tenant.
//! - [`PersistentPersonaStore`] — JSON-file persona configuration store.
//! - [`TranscriptStore`] — Trait for transcript persistence.
//! - [`FileTranscriptStore`] — File-system-backed transcript store.
//!
//! # Feature flags
//!
//! | Flag     | Effect                                                      |
//! |----------|-------------------------------------------------------------|
//! | `sqlite` | Enables [`SqliteBackend`] — real SQLite-backed persistence. |

/// Git-like conversation tree with branching, forking, and comparison.
pub mod conversation_tree;
/// Core session type and lifecycle.
pub mod session;
/// Real SQLite-backed persistence (sessions, messages, usage, personas).
#[cfg(feature = "sqlite")]
pub mod sqlite_backend;
/// SQLite-style persistence layer for sessions, usage, and personas.
pub mod sqlite_store;
/// Session persistence backends.
pub mod store;
/// Structured transcript storage.
pub mod transcript;

pub use session::Session;
#[cfg(feature = "sqlite")]
pub use sqlite_backend::SqliteBackend;
pub use sqlite_store::{
    PersistentPersonaStore, PersistentUsageStore, PersonaConfig, SqliteSessionStore, UsageRecord,
};
pub use store::{FileSessionStore, SessionStore};
pub use transcript::{FileTranscriptStore, TranscriptEntry, TranscriptEvent, TranscriptStore};
