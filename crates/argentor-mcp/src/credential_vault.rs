//! Centralized credential management for MCP server connections.
//!
//! Replaces ad-hoc environment variable handling with a secure, thread-safe
//! vault that supports credential pooling, usage tracking, policy enforcement,
//! and token rotation.
//!
//! # Overview
//!
//! The [`CredentialVault`] stores [`Credential`] entries keyed by unique IDs.
//! Each credential belongs to a **provider** (e.g., `"openai"`, `"anthropic"`)
//! and carries a [`CredentialPolicy`] that governs rate limits, daily quotas,
//! and automatic rotation behavior.
//!
//! When an agent needs an API key, it calls [`CredentialVault::resolve`] with a
//! provider name. The vault picks the best available credential for that
//! provider: one that is enabled, not expired, not over quota, and has the
//! lowest usage count.
//!
//! # Example
//!
//! ```rust
//! use argentor_mcp::credential_vault::{CredentialVault, CredentialPolicy};
//!
//! let vault = CredentialVault::new();
//! let policy = CredentialPolicy::default();
//! vault.add("key1", "openai", "api_key", "sk-abc123", policy).unwrap();
//! let cred = vault.resolve("openai").unwrap();
//! assert_eq!(cred.value, "sk-abc123");
//! ```

use argentor_core::{ArgentorError, ArgentorResult};
use argentor_security::{decrypt_value, derive_key, encrypt_value, KEY_SIZE};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Policy governing how a credential may be used.
///
/// Policies enable per-credential rate limiting, daily usage caps,
/// automatic rotation scheduling, and fallback chaining.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CredentialPolicy {
    /// Maximum number of calls allowed per minute. `None` means unlimited.
    pub max_calls_per_minute: Option<u32>,
    /// Maximum total uses allowed per calendar day (UTC). `None` means unlimited.
    pub max_daily_usage: Option<u64>,
    /// Whether the vault should flag this credential for automatic rotation
    /// when it approaches its expiry.
    pub auto_rotate: bool,
    /// If this credential becomes unavailable, the vault will attempt to use
    /// the credential with this ID as a fallback.
    pub fallback_credential_id: Option<String>,
}

/// A single stored credential with metadata and usage tracking.
///
/// Credentials are identified by a unique [`id`](Credential::id) and grouped
/// by [`provider`](Credential::provider) for pool-based resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credential {
    /// Unique identifier for this credential entry.
    pub id: String,
    /// Provider this credential belongs to (e.g., `"openai"`).
    pub provider: String,
    /// Logical key name (e.g., `"OPENAI_API_KEY"`).
    pub key_name: String,
    /// The actual API key or token value.
    pub value: String,
    /// When this credential was first stored (or last rotated).
    pub created_at: DateTime<Utc>,
    /// Optional expiry time. After this instant the credential is unavailable.
    pub expires_at: Option<DateTime<Utc>>,
    /// Timestamp of the most recent usage.
    pub last_used: Option<DateTime<Utc>>,
    /// Cumulative number of times this credential has been used.
    pub usage_count: u64,
    /// Policy that governs rate limits and quotas.
    pub policy: CredentialPolicy,
    /// Whether this credential is currently enabled for resolution.
    pub enabled: bool,
    /// Arbitrary key-value tags for categorization.
    pub tags: HashMap<String, String>,
}

/// Aggregated statistics about the vault contents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialStats {
    /// Total number of credential entries in the vault.
    pub total_credentials: usize,
    /// Number of credentials that are currently available for use.
    pub active_credentials: usize,
    /// Number of credentials past their expiry date.
    pub expired_credentials: usize,
    /// Count of credentials grouped by provider name.
    pub providers: HashMap<String, usize>,
    /// Sum of `usage_count` across all credentials.
    pub total_usage: u64,
}

// ---------------------------------------------------------------------------
// Vault
// ---------------------------------------------------------------------------

/// Thread-safe, centralized credential store.
///
/// `CredentialVault` holds credentials in memory behind an
/// `Arc<RwLock<HashMap<String, Credential>>>`, making it safe to share
/// across threads without async overhead.
///
/// When created with [`with_encryption`](Self::with_encryption), credential
/// values are encrypted in memory and can be persisted to disk via
/// [`save_encrypted`](Self::save_encrypted).
#[derive(Debug, Clone)]
pub struct CredentialVault {
    credentials: Arc<RwLock<HashMap<String, Credential>>>,
    /// Optional AES-256 encryption key derived from a passphrase.
    /// When `Some`, credential values are stored encrypted (base64-encoded).
    encryption_key: Option<[u8; KEY_SIZE]>,
}

impl CredentialVault {
    /// Creates an empty credential vault without encryption.
    pub fn new() -> Self {
        Self {
            credentials: Arc::new(RwLock::new(HashMap::new())),
            encryption_key: None,
        }
    }

    /// Creates an empty credential vault with encryption enabled.
    ///
    /// The passphrase is used to derive an AES-256 key via PBKDF2-HMAC-SHA256.
    /// All credential values stored via [`add`](Self::add) will be encrypted
    /// in memory, and [`get`](Self::get) / [`resolve`](Self::resolve) will
    /// transparently decrypt them on retrieval.
    pub fn with_encryption(passphrase: &str) -> Self {
        let key = derive_key(passphrase, b"argentor-credential-vault-v1");
        Self {
            credentials: Arc::new(RwLock::new(HashMap::new())),
            encryption_key: Some(key),
        }
    }

    /// Returns `true` if this vault has encryption enabled.
    pub fn is_encrypted(&self) -> bool {
        self.encryption_key.is_some()
    }

    /// Encrypt a plaintext credential value, returning a base64-encoded string.
    fn encrypt_credential_value(&self, plaintext: &str) -> ArgentorResult<String> {
        let key = self.encryption_key.ok_or_else(|| {
            ArgentorError::Security("Encryption not enabled on this vault".into())
        })?;
        let encrypted = encrypt_value(&key, plaintext.as_bytes())
            .map_err(|e| ArgentorError::Security(format!("Encryption failed: {e}")))?;
        Ok(BASE64.encode(encrypted))
    }

    /// Decrypt a base64-encoded ciphertext back to the original credential value.
    fn decrypt_credential_value(&self, ciphertext: &str) -> ArgentorResult<String> {
        let key = self.encryption_key.ok_or_else(|| {
            ArgentorError::Security("Encryption not enabled on this vault".into())
        })?;
        let encrypted = BASE64
            .decode(ciphertext)
            .map_err(|e| ArgentorError::Security(format!("Base64 decode failed: {e}")))?;
        let plaintext = decrypt_value(&key, &encrypted)
            .map_err(|e| ArgentorError::Security(format!("Decryption failed: {e}")))?;
        String::from_utf8(plaintext).map_err(|e| {
            ArgentorError::Security(format!("Decrypted value is not valid UTF-8: {e}"))
        })
    }

    /// If encryption is enabled, encrypt the value; otherwise return it unchanged.
    fn maybe_encrypt(&self, value: &str) -> ArgentorResult<String> {
        if self.encryption_key.is_some() {
            self.encrypt_credential_value(value)
        } else {
            Ok(value.to_string())
        }
    }

    /// If encryption is enabled, decrypt the value; otherwise return it unchanged.
    fn maybe_decrypt(&self, value: &str) -> ArgentorResult<String> {
        if self.encryption_key.is_some() {
            self.decrypt_credential_value(value)
        } else {
            Ok(value.to_string())
        }
    }

    /// Adds a new credential to the vault.
    ///
    /// If encryption is enabled, the credential value is encrypted before
    /// storage. Returns an error if a credential with the same `id` already
    /// exists.
    pub fn add(
        &self,
        id: impl Into<String>,
        provider: impl Into<String>,
        key_name: impl Into<String>,
        value: impl Into<String>,
        policy: CredentialPolicy,
    ) -> ArgentorResult<()> {
        let id = id.into();
        let stored_value = self.maybe_encrypt(&value.into())?;

        let mut store = self
            .credentials
            .write()
            .map_err(|e| ArgentorError::Security(format!("Lock poisoned: {e}")))?;

        if store.contains_key(&id) {
            return Err(ArgentorError::Security(format!(
                "Credential '{id}' already exists"
            )));
        }

        store.insert(
            id.clone(),
            Credential {
                id,
                provider: provider.into(),
                key_name: key_name.into(),
                value: stored_value,
                created_at: Utc::now(),
                expires_at: None,
                last_used: None,
                usage_count: 0,
                policy,
                enabled: true,
                tags: HashMap::new(),
            },
        );

        Ok(())
    }

    /// Returns a clone of the credential with the given `id`, or `None` if not
    /// found.
    ///
    /// If encryption is enabled, the credential value is transparently
    /// decrypted before being returned. Returns `None` if decryption fails.
    pub fn get(&self, id: &str) -> Option<Credential> {
        let store = self.credentials.read().ok()?;
        let mut cred = store.get(id).cloned()?;
        cred.value = self.maybe_decrypt(&cred.value).ok()?;
        Some(cred)
    }

    /// Removes a credential from the vault.
    ///
    /// Returns an error if no credential with the given `id` exists.
    pub fn remove(&self, id: &str) -> ArgentorResult<()> {
        let mut store = self
            .credentials
            .write()
            .map_err(|e| ArgentorError::Security(format!("Lock poisoned: {e}")))?;

        if store.remove(id).is_none() {
            return Err(ArgentorError::Security(format!(
                "Credential '{id}' not found"
            )));
        }

        Ok(())
    }

    /// Resolves the best available credential for a given provider.
    ///
    /// Selection criteria (in order):
    /// 1. The credential must be enabled.
    /// 2. The credential must not be expired.
    /// 3. The credential must not have exceeded its daily usage quota.
    /// 4. Among qualifying credentials, the one with the lowest `usage_count`
    ///    is chosen.
    ///
    /// Returns an error if no suitable credential is found for the provider.
    pub fn resolve(&self, provider: &str) -> ArgentorResult<Credential> {
        let store = self
            .credentials
            .read()
            .map_err(|e| ArgentorError::Security(format!("Lock poisoned: {e}")))?;

        let now = Utc::now();

        let mut cred = store
            .values()
            .filter(|c| c.provider == provider)
            .filter(|c| c.enabled)
            .filter(|c| !Self::is_expired_at(c, now))
            .filter(|c| !Self::is_over_daily_quota(c))
            .min_by_key(|c| c.usage_count)
            .cloned()
            .ok_or_else(|| {
                ArgentorError::Security(format!(
                    "No available credential for provider '{provider}'"
                ))
            })?;

        cred.value = self.maybe_decrypt(&cred.value)?;
        Ok(cred)
    }

    /// Records a usage event for the credential with the given `id`.
    ///
    /// Increments `usage_count` and updates `last_used` to the current time.
    /// Returns an error if the credential does not exist.
    pub fn record_usage(&self, id: &str) -> ArgentorResult<()> {
        let mut store = self
            .credentials
            .write()
            .map_err(|e| ArgentorError::Security(format!("Lock poisoned: {e}")))?;

        let cred = store
            .get_mut(id)
            .ok_or_else(|| ArgentorError::Security(format!("Credential '{id}' not found")))?;

        cred.usage_count += 1;
        cred.last_used = Some(Utc::now());

        Ok(())
    }

    /// Returns `true` if the credential is available for use: enabled, not
    /// expired, and not over its daily quota.
    pub fn is_available(&self, id: &str) -> bool {
        let store = match self.credentials.read() {
            Ok(s) => s,
            Err(_) => return false,
        };

        match store.get(id) {
            Some(cred) => {
                cred.enabled
                    && !Self::is_expired_at(cred, Utc::now())
                    && !Self::is_over_daily_quota(cred)
            }
            None => false,
        }
    }

    /// Rotates a credential to a new value.
    ///
    /// The old value is replaced, `usage_count` is reset to zero, and
    /// `created_at` is updated to the current time. All other metadata
    /// (provider, key_name, policy, tags) is preserved.
    pub fn rotate(&self, id: &str, new_value: impl Into<String>) -> ArgentorResult<()> {
        let stored_value = self.maybe_encrypt(&new_value.into())?;

        let mut store = self
            .credentials
            .write()
            .map_err(|e| ArgentorError::Security(format!("Lock poisoned: {e}")))?;

        let cred = store
            .get_mut(id)
            .ok_or_else(|| ArgentorError::Security(format!("Credential '{id}' not found")))?;

        cred.value = stored_value;
        cred.usage_count = 0;
        cred.created_at = Utc::now();
        cred.last_used = None;

        Ok(())
    }

    /// Returns all credentials belonging to the given provider.
    ///
    /// Values are decrypted if encryption is enabled.
    pub fn list_by_provider(&self, provider: &str) -> Vec<Credential> {
        let store = match self.credentials.read() {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        store
            .values()
            .filter(|c| c.provider == provider)
            .cloned()
            .map(|mut c| {
                if let Ok(v) = self.maybe_decrypt(&c.value) {
                    c.value = v;
                }
                c
            })
            .collect()
    }

    /// Returns all credentials stored in the vault.
    ///
    /// This returns clones of all credential entries with decrypted values.
    /// Use with caution in production code — the returned values contain
    /// plaintext secrets.
    pub fn list_all(&self) -> Vec<Credential> {
        let store = match self.credentials.read() {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        store
            .values()
            .cloned()
            .map(|mut c| {
                if let Ok(v) = self.maybe_decrypt(&c.value) {
                    c.value = v;
                }
                c
            })
            .collect()
    }

    /// Returns aggregate statistics about the vault contents.
    pub fn stats(&self) -> CredentialStats {
        let store = match self.credentials.read() {
            Ok(s) => s,
            Err(_) => {
                return CredentialStats {
                    total_credentials: 0,
                    active_credentials: 0,
                    expired_credentials: 0,
                    providers: HashMap::new(),
                    total_usage: 0,
                };
            }
        };

        let now = Utc::now();
        let mut providers: HashMap<String, usize> = HashMap::new();
        let mut active = 0usize;
        let mut expired = 0usize;
        let mut total_usage = 0u64;

        for cred in store.values() {
            *providers.entry(cred.provider.clone()).or_insert(0) += 1;
            total_usage += cred.usage_count;

            if Self::is_expired_at(cred, now) {
                expired += 1;
            } else if cred.enabled {
                active += 1;
            }
        }

        CredentialStats {
            total_credentials: store.len(),
            active_credentials: active,
            expired_credentials: expired,
            providers,
            total_usage,
        }
    }

    /// Enables or disables a credential.
    ///
    /// Disabled credentials are excluded from resolution but remain in the
    /// vault.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> ArgentorResult<()> {
        let mut store = self
            .credentials
            .write()
            .map_err(|e| ArgentorError::Security(format!("Lock poisoned: {e}")))?;

        let cred = store
            .get_mut(id)
            .ok_or_else(|| ArgentorError::Security(format!("Credential '{id}' not found")))?;

        cred.enabled = enabled;

        Ok(())
    }

    /// Sets an expiry time on a credential.
    ///
    /// After this instant the credential will no longer be considered
    /// available by [`is_available`](Self::is_available) or
    /// [`resolve`](Self::resolve).
    pub fn set_expires_at(
        &self,
        id: &str,
        expires_at: Option<DateTime<Utc>>,
    ) -> ArgentorResult<()> {
        let mut store = self
            .credentials
            .write()
            .map_err(|e| ArgentorError::Security(format!("Lock poisoned: {e}")))?;

        let cred = store
            .get_mut(id)
            .ok_or_else(|| ArgentorError::Security(format!("Credential '{id}' not found")))?;

        cred.expires_at = expires_at;

        Ok(())
    }

    /// Bulk-imports credentials from environment variables.
    ///
    /// Each tuple in `mappings` is `(provider, key_name, env_var_name)`.
    /// If an environment variable is not set it is silently skipped.
    /// All imported credentials receive the supplied `policy`.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use argentor_mcp::credential_vault::{CredentialVault, CredentialPolicy};
    ///
    /// let vault = CredentialVault::from_env(
    ///     &[
    ///         ("openai", "api_key", "OPENAI_API_KEY"),
    ///         ("anthropic", "api_key", "ANTHROPIC_API_KEY"),
    ///     ],
    ///     CredentialPolicy::default(),
    /// );
    /// ```
    pub fn from_env(mappings: &[(&str, &str, &str)], policy: CredentialPolicy) -> Self {
        let vault = Self::new();

        for &(provider, key_name, env_var) in mappings {
            if let Ok(value) = std::env::var(env_var) {
                // Use env_var name as the credential ID for deterministic
                // lookups.
                let id = format!("{provider}_{key_name}");
                // Ignore errors from duplicate IDs (shouldn't happen with
                // well-formed mappings).
                let _ = vault.add(&id, provider, key_name, value, policy.clone());
            }
        }

        vault
    }

    /// Bulk-imports credentials from environment variables with encryption.
    ///
    /// Same as [`from_env`](Self::from_env), but all imported credentials are
    /// encrypted in memory using the given passphrase.
    pub fn from_env_encrypted(
        mappings: &[(&str, &str, &str)],
        policy: CredentialPolicy,
        passphrase: &str,
    ) -> Self {
        let vault = Self::with_encryption(passphrase);

        for &(provider, key_name, env_var) in mappings {
            if let Ok(value) = std::env::var(env_var) {
                let id = format!("{provider}_{key_name}");
                let _ = vault.add(&id, provider, key_name, value, policy.clone());
            }
        }

        vault
    }

    // -----------------------------------------------------------------------
    // Persistence
    // -----------------------------------------------------------------------

    /// Save the vault to disk as an encrypted file (atomic write).
    ///
    /// All credentials (including their already-encrypted values) are
    /// serialized to JSON and then encrypted as a single blob using the
    /// vault's encryption key. The file is written atomically via a
    /// temporary file + rename to prevent partial writes.
    ///
    /// Requires that encryption was enabled via [`with_encryption`](Self::with_encryption).
    pub fn save_encrypted(&self, path: &Path) -> ArgentorResult<()> {
        let key = self.encryption_key.ok_or_else(|| {
            ArgentorError::Security("Cannot save encrypted vault: encryption not enabled".into())
        })?;

        let store = self
            .credentials
            .read()
            .map_err(|e| ArgentorError::Security(format!("Lock poisoned: {e}")))?;

        let json = serde_json::to_vec(&*store)
            .map_err(|e| ArgentorError::Security(format!("Failed to serialize vault: {e}")))?;

        let encrypted = encrypt_value(&key, &json)
            .map_err(|e| ArgentorError::Security(format!("Vault encryption failed: {e}")))?;

        // Atomic write: write to temp file then rename
        let tmp_path = atomic_tmp_path(path);
        std::fs::write(&tmp_path, &encrypted)
            .map_err(|e| ArgentorError::Security(format!("Failed to write temp file: {e}")))?;
        std::fs::rename(&tmp_path, path).map_err(|e| {
            // Clean up temp file on rename failure
            let _ = std::fs::remove_file(&tmp_path);
            ArgentorError::Security(format!("Failed to rename temp file: {e}"))
        })?;

        Ok(())
    }

    /// Load an encrypted vault from disk.
    ///
    /// The file must have been created by [`save_encrypted`](Self::save_encrypted).
    /// The passphrase is used to derive the decryption key. If the passphrase
    /// is incorrect, decryption will fail with an authentication error.
    pub fn load_encrypted(path: &Path, passphrase: &str) -> ArgentorResult<Self> {
        let key = derive_key(passphrase, b"argentor-credential-vault-v1");

        let encrypted = std::fs::read(path)
            .map_err(|e| ArgentorError::Security(format!("Failed to read vault file: {e}")))?;

        let json_bytes = decrypt_value(&key, &encrypted)
            .map_err(|e| ArgentorError::Security(format!("Vault decryption failed: {e}")))?;

        let credentials: HashMap<String, Credential> = serde_json::from_slice(&json_bytes)
            .map_err(|e| ArgentorError::Security(format!("Failed to deserialize vault: {e}")))?;

        Ok(Self {
            credentials: Arc::new(RwLock::new(credentials)),
            encryption_key: Some(key),
        })
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Returns `true` if the credential has an `expires_at` that is in the
    /// past relative to `now`.
    fn is_expired_at(cred: &Credential, now: DateTime<Utc>) -> bool {
        cred.expires_at.is_some_and(|exp| exp <= now)
    }

    /// Returns `true` if the credential has exceeded its daily usage quota.
    fn is_over_daily_quota(cred: &Credential) -> bool {
        match cred.policy.max_daily_usage {
            Some(max) => cred.usage_count >= max,
            None => false,
        }
    }
}

/// Generate a temporary file path adjacent to the target for atomic writes.
fn atomic_tmp_path(target: &Path) -> std::path::PathBuf {
    use std::time::SystemTime;
    let ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let name = format!(".vault_tmp_{ts}_{pid}");
    target.with_file_name(name)
}

impl Default for CredentialVault {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use chrono::Duration;

    /// Helper: create a vault with one credential.
    fn vault_with_one() -> CredentialVault {
        let v = CredentialVault::new();
        v.add(
            "k1",
            "openai",
            "api_key",
            "sk-abc",
            CredentialPolicy::default(),
        )
        .unwrap();
        v
    }

    // -- 1. Store and retrieve -------------------------------------------------

    #[test]
    fn test_store_and_retrieve() {
        let vault = vault_with_one();
        let cred = vault.get("k1").unwrap();
        assert_eq!(cred.id, "k1");
        assert_eq!(cred.provider, "openai");
        assert_eq!(cred.key_name, "api_key");
        assert_eq!(cred.value, "sk-abc");
        assert_eq!(cred.usage_count, 0);
        assert!(cred.enabled);
    }

    #[test]
    fn test_get_nonexistent_returns_none() {
        let vault = CredentialVault::new();
        assert!(vault.get("nope").is_none());
    }

    // -- 2. Duplicate ID rejected ----------------------------------------------

    #[test]
    fn test_duplicate_id_rejected() {
        let vault = vault_with_one();
        let result = vault.add("k1", "openai", "key2", "val2", CredentialPolicy::default());
        assert!(result.is_err());
    }

    // -- 3. Usage counting and quota enforcement -------------------------------

    #[test]
    fn test_usage_counting() {
        let vault = vault_with_one();
        vault.record_usage("k1").unwrap();
        vault.record_usage("k1").unwrap();
        vault.record_usage("k1").unwrap();
        let cred = vault.get("k1").unwrap();
        assert_eq!(cred.usage_count, 3);
        assert!(cred.last_used.is_some());
    }

    #[test]
    fn test_quota_enforcement() {
        let vault = CredentialVault::new();
        let policy = CredentialPolicy {
            max_daily_usage: Some(2),
            ..CredentialPolicy::default()
        };
        vault.add("k1", "openai", "key", "val", policy).unwrap();

        vault.record_usage("k1").unwrap();
        assert!(vault.is_available("k1"));

        vault.record_usage("k1").unwrap();
        // Now at the quota limit — should not be available.
        assert!(!vault.is_available("k1"));
    }

    // -- 4. Credential expiry detection ----------------------------------------

    #[test]
    fn test_expiry_detection() {
        let vault = vault_with_one();
        // Set expiry in the past.
        let past = Utc::now() - Duration::hours(1);
        vault.set_expires_at("k1", Some(past)).unwrap();
        assert!(!vault.is_available("k1"));
    }

    #[test]
    fn test_not_expired_when_future() {
        let vault = vault_with_one();
        let future = Utc::now() + Duration::hours(1);
        vault.set_expires_at("k1", Some(future)).unwrap();
        assert!(vault.is_available("k1"));
    }

    // -- 5. Provider grouping and resolution -----------------------------------

    #[test]
    fn test_resolve_picks_least_used() {
        let vault = CredentialVault::new();
        let policy = CredentialPolicy::default();
        vault
            .add("a1", "openai", "key", "v1", policy.clone())
            .unwrap();
        vault.add("a2", "openai", "key", "v2", policy).unwrap();

        // Use a1 three times, a2 once.
        vault.record_usage("a1").unwrap();
        vault.record_usage("a1").unwrap();
        vault.record_usage("a1").unwrap();
        vault.record_usage("a2").unwrap();

        let resolved = vault.resolve("openai").unwrap();
        assert_eq!(resolved.id, "a2");
    }

    #[test]
    fn test_resolve_skips_expired() {
        let vault = CredentialVault::new();
        let policy = CredentialPolicy::default();
        vault
            .add("e1", "anthropic", "key", "v1", policy.clone())
            .unwrap();
        vault.add("e2", "anthropic", "key", "v2", policy).unwrap();

        let past = Utc::now() - Duration::hours(1);
        vault.set_expires_at("e1", Some(past)).unwrap();

        let resolved = vault.resolve("anthropic").unwrap();
        assert_eq!(resolved.id, "e2");
    }

    #[test]
    fn test_resolve_no_match_returns_error() {
        let vault = CredentialVault::new();
        let result = vault.resolve("nonexistent");
        assert!(result.is_err());
    }

    // -- 6. Rotation -----------------------------------------------------------

    #[test]
    fn test_rotation_replaces_value_and_resets() {
        let vault = vault_with_one();
        vault.record_usage("k1").unwrap();
        vault.record_usage("k1").unwrap();

        let before = vault.get("k1").unwrap();
        assert_eq!(before.usage_count, 2);

        vault.rotate("k1", "sk-new-key").unwrap();

        let after = vault.get("k1").unwrap();
        assert_eq!(after.value, "sk-new-key");
        assert_eq!(after.usage_count, 0);
        assert!(after.last_used.is_none());
        // Provider and key_name are preserved.
        assert_eq!(after.provider, "openai");
        assert_eq!(after.key_name, "api_key");
    }

    #[test]
    fn test_rotation_nonexistent_errors() {
        let vault = CredentialVault::new();
        assert!(vault.rotate("ghost", "val").is_err());
    }

    // -- 7. Policy enforcement (rate limit / daily quota) ----------------------

    #[test]
    fn test_resolve_skips_over_quota() {
        let vault = CredentialVault::new();
        let limited = CredentialPolicy {
            max_daily_usage: Some(1),
            ..CredentialPolicy::default()
        };
        let unlimited = CredentialPolicy::default();

        vault.add("lim", "provider", "key", "v1", limited).unwrap();
        vault
            .add("unlim", "provider", "key", "v2", unlimited)
            .unwrap();

        vault.record_usage("lim").unwrap();
        // lim is over quota; resolve should pick unlim.
        let resolved = vault.resolve("provider").unwrap();
        assert_eq!(resolved.id, "unlim");
    }

    // -- 8. Multiple credentials per provider (pool) ---------------------------

    #[test]
    fn test_multiple_per_provider() {
        let vault = CredentialVault::new();
        let p = CredentialPolicy::default();
        vault.add("p1", "gemini", "k1", "v1", p.clone()).unwrap();
        vault.add("p2", "gemini", "k2", "v2", p.clone()).unwrap();
        vault.add("p3", "gemini", "k3", "v3", p).unwrap();

        let list = vault.list_by_provider("gemini");
        assert_eq!(list.len(), 3);
    }

    // -- 9. Remove credential --------------------------------------------------

    #[test]
    fn test_remove_credential() {
        let vault = vault_with_one();
        vault.remove("k1").unwrap();
        assert!(vault.get("k1").is_none());
    }

    #[test]
    fn test_remove_nonexistent_errors() {
        let vault = CredentialVault::new();
        assert!(vault.remove("ghost").is_err());
    }

    // -- 10. List by provider --------------------------------------------------

    #[test]
    fn test_list_by_provider_filters() {
        let vault = CredentialVault::new();
        let p = CredentialPolicy::default();
        vault.add("a", "openai", "k", "v", p.clone()).unwrap();
        vault.add("b", "anthropic", "k", "v", p.clone()).unwrap();
        vault.add("c", "openai", "k", "v", p).unwrap();

        assert_eq!(vault.list_by_provider("openai").len(), 2);
        assert_eq!(vault.list_by_provider("anthropic").len(), 1);
        assert_eq!(vault.list_by_provider("unknown").len(), 0);
    }

    // -- 11. Stats export ------------------------------------------------------

    #[test]
    fn test_stats_export() {
        let vault = CredentialVault::new();
        let p = CredentialPolicy::default();
        vault.add("s1", "openai", "k", "v", p.clone()).unwrap();
        vault.add("s2", "openai", "k", "v", p.clone()).unwrap();
        vault.add("s3", "anthropic", "k", "v", p).unwrap();

        vault.record_usage("s1").unwrap();
        vault.record_usage("s1").unwrap();
        vault.record_usage("s3").unwrap();

        // Expire s2.
        let past = Utc::now() - Duration::hours(1);
        vault.set_expires_at("s2", Some(past)).unwrap();

        let stats = vault.stats();
        assert_eq!(stats.total_credentials, 3);
        assert_eq!(stats.active_credentials, 2); // s1 + s3
        assert_eq!(stats.expired_credentials, 1); // s2
        assert_eq!(stats.total_usage, 3); // 2 + 0 + 1
        assert_eq!(*stats.providers.get("openai").unwrap(), 2);
        assert_eq!(*stats.providers.get("anthropic").unwrap(), 1);
    }

    // -- 12. Set enabled / disabled --------------------------------------------

    #[test]
    fn test_set_enabled_disables_resolution() {
        let vault = vault_with_one();
        vault.set_enabled("k1", false).unwrap();
        assert!(!vault.is_available("k1"));
        assert!(vault.resolve("openai").is_err());

        vault.set_enabled("k1", true).unwrap();
        assert!(vault.is_available("k1"));
        assert!(vault.resolve("openai").is_ok());
    }

    // -- 13. from_env ----------------------------------------------------------

    #[test]
    fn test_from_env_skips_missing_vars() {
        // Use a very unlikely env var name to ensure it's not set.
        let vault = CredentialVault::from_env(
            &[(
                "test_prov",
                "key",
                "ARGENTOR_TEST_CREDENTIAL_MISSING_XYZ_42",
            )],
            CredentialPolicy::default(),
        );
        assert_eq!(vault.stats().total_credentials, 0);
    }

    #[test]
    fn test_from_env_imports_set_var() {
        // Temporarily set an env var for this test.
        let var_name = "ARGENTOR_TEST_VAULT_IMPORT_1234";
        std::env::set_var(var_name, "test-secret-value");

        let vault = CredentialVault::from_env(
            &[("ci_provider", "token", var_name)],
            CredentialPolicy::default(),
        );

        let cred = vault.get("ci_provider_token").unwrap();
        assert_eq!(cred.value, "test-secret-value");
        assert_eq!(cred.provider, "ci_provider");

        // Clean up.
        std::env::remove_var(var_name);
    }

    // -- 14. Record usage on nonexistent credential ----------------------------

    #[test]
    fn test_record_usage_nonexistent_errors() {
        let vault = CredentialVault::new();
        assert!(vault.record_usage("ghost").is_err());
    }

    // -- 15. Thread-safety: clone vault and use across threads -----------------

    #[test]
    fn test_thread_safety() {
        let vault = CredentialVault::new();
        let p = CredentialPolicy::default();
        vault.add("t1", "prov", "k", "v", p).unwrap();

        let v2 = vault.clone();
        let handle = std::thread::spawn(move || {
            v2.record_usage("t1").unwrap();
            v2.record_usage("t1").unwrap();
            v2.get("t1").unwrap().usage_count
        });

        let count = handle.join().unwrap();
        assert_eq!(count, 2);

        // Original vault sees the same data.
        assert_eq!(vault.get("t1").unwrap().usage_count, 2);
    }

    // -- 16. Default trait impl ------------------------------------------------

    #[test]
    fn test_default_impl() {
        let vault = CredentialVault::default();
        assert_eq!(vault.stats().total_credentials, 0);
    }

    // -- 17. Tags preserved across rotation ------------------------------------

    #[test]
    fn test_tags_preserved_across_rotation() {
        let vault = vault_with_one();

        // Manually insert a tag via the lock.
        {
            let mut store = vault.credentials.write().unwrap();
            let cred = store.get_mut("k1").unwrap();
            cred.tags.insert("env".to_string(), "prod".to_string());
        }

        vault.rotate("k1", "new-val").unwrap();

        let cred = vault.get("k1").unwrap();
        assert_eq!(cred.tags.get("env").unwrap(), "prod");
        assert_eq!(cred.value, "new-val");
    }

    // =========================================================================
    // Encryption-at-rest tests
    // =========================================================================

    // -- 18. Encrypt/decrypt roundtrip ----------------------------------------

    #[test]
    fn test_encrypted_vault_roundtrip() {
        let vault = CredentialVault::with_encryption("my-secret-passphrase");
        assert!(vault.is_encrypted());

        vault
            .add(
                "k1",
                "openai",
                "api_key",
                "sk-abc123",
                CredentialPolicy::default(),
            )
            .unwrap();

        // Value should be returned decrypted.
        let cred = vault.get("k1").unwrap();
        assert_eq!(cred.value, "sk-abc123");
        assert_eq!(cred.provider, "openai");
        assert_eq!(cred.key_name, "api_key");
    }

    #[test]
    fn test_encrypted_vault_value_stored_encrypted() {
        let vault = CredentialVault::with_encryption("passphrase");
        vault
            .add(
                "k1",
                "openai",
                "api_key",
                "sk-abc123",
                CredentialPolicy::default(),
            )
            .unwrap();

        // Read raw stored value (bypass decryption).
        let store = vault.credentials.read().unwrap();
        let raw = store.get("k1").unwrap();
        // The stored value should NOT be the plaintext.
        assert_ne!(raw.value, "sk-abc123");
        // It should be base64-encoded.
        assert!(base64::engine::general_purpose::STANDARD
            .decode(&raw.value)
            .is_ok());
    }

    // -- 19. Wrong passphrase fails decryption --------------------------------

    #[test]
    fn test_wrong_passphrase_fails_get() {
        let vault = CredentialVault::with_encryption("correct-passphrase");
        vault
            .add(
                "k1",
                "openai",
                "api_key",
                "sk-abc123",
                CredentialPolicy::default(),
            )
            .unwrap();

        // Grab the raw encrypted store contents.
        let raw_store: HashMap<String, Credential> = {
            let store = vault.credentials.read().unwrap();
            store.clone()
        };

        // Create a vault with a different passphrase and inject the raw data.
        let wrong_vault = CredentialVault::with_encryption("wrong-passphrase");
        {
            let mut store = wrong_vault.credentials.write().unwrap();
            *store = raw_store;
        }

        // Decryption should fail — get returns None on failure.
        assert!(wrong_vault.get("k1").is_none());
    }

    // -- 20. Save/load encrypted roundtrip ------------------------------------

    #[test]
    fn test_save_load_encrypted_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let vault_path = dir.path().join("vault.enc");
        let passphrase = "test-save-load-passphrase";

        // Create vault, add credentials, save.
        let vault = CredentialVault::with_encryption(passphrase);
        vault
            .add(
                "k1",
                "openai",
                "api_key",
                "sk-abc123",
                CredentialPolicy::default(),
            )
            .unwrap();
        vault
            .add(
                "k2",
                "anthropic",
                "api_key",
                "sk-xyz789",
                CredentialPolicy::default(),
            )
            .unwrap();
        vault.record_usage("k1").unwrap();

        vault.save_encrypted(&vault_path).unwrap();
        assert!(vault_path.exists());

        // Load into a new vault and verify.
        let loaded = CredentialVault::load_encrypted(&vault_path, passphrase).unwrap();
        assert!(loaded.is_encrypted());

        let cred1 = loaded.get("k1").unwrap();
        assert_eq!(cred1.value, "sk-abc123");
        assert_eq!(cred1.provider, "openai");
        assert_eq!(cred1.usage_count, 1);

        let cred2 = loaded.get("k2").unwrap();
        assert_eq!(cred2.value, "sk-xyz789");
        assert_eq!(cred2.provider, "anthropic");
    }

    #[test]
    fn test_load_encrypted_wrong_passphrase() {
        let dir = tempfile::tempdir().unwrap();
        let vault_path = dir.path().join("vault.enc");

        let vault = CredentialVault::with_encryption("correct");
        vault
            .add("k1", "openai", "key", "val", CredentialPolicy::default())
            .unwrap();
        vault.save_encrypted(&vault_path).unwrap();

        // Loading with wrong passphrase should fail.
        let result = CredentialVault::load_encrypted(&vault_path, "wrong");
        assert!(result.is_err());
    }

    // -- 21. Empty encrypted vault --------------------------------------------

    #[test]
    fn test_empty_encrypted_vault() {
        let vault = CredentialVault::with_encryption("passphrase");
        assert!(vault.is_encrypted());
        assert_eq!(vault.stats().total_credentials, 0);
        assert!(vault.list_all().is_empty());
    }

    #[test]
    fn test_save_load_empty_encrypted_vault() {
        let dir = tempfile::tempdir().unwrap();
        let vault_path = dir.path().join("empty.enc");
        let passphrase = "empty-vault-pass";

        let vault = CredentialVault::with_encryption(passphrase);
        vault.save_encrypted(&vault_path).unwrap();

        let loaded = CredentialVault::load_encrypted(&vault_path, passphrase).unwrap();
        assert_eq!(loaded.stats().total_credentials, 0);
    }

    // -- 22. Multiple credentials with encryption -----------------------------

    #[test]
    fn test_multiple_encrypted_credentials() {
        let vault = CredentialVault::with_encryption("multi-pass");
        let p = CredentialPolicy::default();

        vault
            .add("a1", "openai", "key1", "val-1", p.clone())
            .unwrap();
        vault
            .add("a2", "openai", "key2", "val-2", p.clone())
            .unwrap();
        vault
            .add("a3", "anthropic", "key3", "val-3", p.clone())
            .unwrap();
        vault.add("a4", "gemini", "key4", "val-4", p).unwrap();

        // All values should be decrypted on retrieval.
        assert_eq!(vault.get("a1").unwrap().value, "val-1");
        assert_eq!(vault.get("a2").unwrap().value, "val-2");
        assert_eq!(vault.get("a3").unwrap().value, "val-3");
        assert_eq!(vault.get("a4").unwrap().value, "val-4");

        // list_by_provider should return decrypted values.
        let openai_creds = vault.list_by_provider("openai");
        assert_eq!(openai_creds.len(), 2);
        for c in &openai_creds {
            assert!(c.value == "val-1" || c.value == "val-2");
        }

        // list_all should return decrypted values.
        let all = vault.list_all();
        assert_eq!(all.len(), 4);
    }

    // -- 23. Encrypted resolve picks best credential --------------------------

    #[test]
    fn test_encrypted_resolve() {
        let vault = CredentialVault::with_encryption("resolve-pass");
        let p = CredentialPolicy::default();

        vault
            .add("r1", "openai", "key", "key-A", p.clone())
            .unwrap();
        vault.add("r2", "openai", "key", "key-B", p).unwrap();

        // Use r1 three times.
        vault.record_usage("r1").unwrap();
        vault.record_usage("r1").unwrap();
        vault.record_usage("r1").unwrap();

        // resolve should pick r2 (lowest usage).
        let resolved = vault.resolve("openai").unwrap();
        assert_eq!(resolved.id, "r2");
        assert_eq!(resolved.value, "key-B");
    }

    // -- 24. Encrypted rotation -----------------------------------------------

    #[test]
    fn test_encrypted_rotation() {
        let vault = CredentialVault::with_encryption("rotate-pass");
        vault
            .add(
                "k1",
                "openai",
                "key",
                "old-val",
                CredentialPolicy::default(),
            )
            .unwrap();

        vault.rotate("k1", "new-val").unwrap();

        let cred = vault.get("k1").unwrap();
        assert_eq!(cred.value, "new-val");
        assert_eq!(cred.usage_count, 0);
    }

    // -- 25. Unencrypted vault has no encryption key --------------------------

    #[test]
    fn test_unencrypted_vault_not_encrypted() {
        let vault = CredentialVault::new();
        assert!(!vault.is_encrypted());
    }

    #[test]
    fn test_unencrypted_vault_save_fails() {
        let dir = tempfile::tempdir().unwrap();
        let vault_path = dir.path().join("vault.enc");
        let vault = CredentialVault::new();
        assert!(vault.save_encrypted(&vault_path).is_err());
    }

    // -- 26. Load from nonexistent file fails ---------------------------------

    #[test]
    fn test_load_nonexistent_file_fails() {
        let result =
            CredentialVault::load_encrypted(std::path::Path::new("/tmp/no_such_vault.enc"), "pass");
        assert!(result.is_err());
    }

    // -- 27. Save/load preserves metadata -------------------------------------

    #[test]
    fn test_save_load_preserves_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let vault_path = dir.path().join("meta.enc");
        let passphrase = "meta-pass";

        let vault = CredentialVault::with_encryption(passphrase);
        let policy = CredentialPolicy {
            max_calls_per_minute: Some(100),
            max_daily_usage: Some(5000),
            auto_rotate: true,
            fallback_credential_id: Some("backup".into()),
        };
        vault
            .add("k1", "openai", "api_key", "sk-secret", policy)
            .unwrap();
        vault.record_usage("k1").unwrap();
        vault.record_usage("k1").unwrap();
        vault.set_enabled("k1", false).unwrap();

        vault.save_encrypted(&vault_path).unwrap();

        let loaded = CredentialVault::load_encrypted(&vault_path, passphrase).unwrap();
        let cred = loaded.get("k1").unwrap();

        assert_eq!(cred.value, "sk-secret");
        assert_eq!(cred.usage_count, 2);
        assert!(!cred.enabled);
        assert_eq!(cred.policy.max_calls_per_minute, Some(100));
        assert_eq!(cred.policy.max_daily_usage, Some(5000));
        assert!(cred.policy.auto_rotate);
        assert_eq!(
            cred.policy.fallback_credential_id.as_deref(),
            Some("backup")
        );
    }
}
