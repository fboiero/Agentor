//! Session management and persistence for the Agentor framework.
//!
//! Handles conversation session lifecycle, file-backed persistence,
//! and structured transcript storage for audit and replay.
//!
//! # Main types
//!
//! - [`Session`] — Represents a single conversation session with metadata.
//! - [`SessionStore`] — Trait for session persistence backends.
//! - [`FileSessionStore`] — File-system-backed session store.
//! - [`TranscriptStore`] — Trait for transcript persistence.
//! - [`FileTranscriptStore`] — File-system-backed transcript store.

/// Core session type and lifecycle.
pub mod session;
/// Session persistence backends.
pub mod store;
/// Structured transcript storage.
pub mod transcript;

pub use session::Session;
pub use store::{FileSessionStore, SessionStore};
pub use transcript::{FileTranscriptStore, TranscriptEntry, TranscriptEvent, TranscriptStore};
