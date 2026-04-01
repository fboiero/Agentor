//! Token pool management for API tokens per provider.
//!
//! This module manages pools of API tokens per provider, handling rate limiting,
//! rotation, usage quotas, and automatic failover between tokens. It works together
//! with the credential vault to provide intelligent token selection.
//!
//! # Overview
//!
//! Each provider (e.g. "openai", "anthropic") can have multiple API tokens registered
//! in the pool. When an agent needs a token, [`TokenPool::select`] picks the best
//! available token based on the configured [`SelectionStrategy`], taking into account
//! rate limits, daily quotas, token tiers, and weights.
//!
//! # Strategies
//!
//! - [`SelectionStrategy::MostRemaining`] — prefer the token with the most remaining quota.
//! - [`SelectionStrategy::RoundRobin`] — cycle through available tokens evenly.
//! - [`SelectionStrategy::WeightedRandom`] — select proportionally to token weights.
//! - [`SelectionStrategy::TierPriority`] — prefer higher-tier tokens first.

use argentor_core::{ArgentorError, ArgentorResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

/// Duration of the sliding rate-limit window in seconds.
const RATE_WINDOW_SECONDS: i64 = 60;

/// A single API token with usage tracking and rate limiting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PooledToken {
    /// Unique identifier for this token entry.
    pub id: String,
    /// Provider name (e.g. "openai", "anthropic").
    pub provider: String,
    /// The actual API token/key value.
    pub token_value: String,
    /// Tier classification affecting selection priority.
    pub tier: TokenTier,
    /// Sliding-window rate limiter for this token.
    pub rate_limit: RateWindow,
    /// Optional daily call quota (None means unlimited).
    pub daily_quota: Option<u64>,
    /// Number of calls made today.
    pub daily_usage: u64,
    /// When the daily quota resets (next midnight UTC).
    pub quota_reset_at: DateTime<Utc>,
    /// Lifetime total usage across all days.
    pub total_usage: u64,
    /// Lifetime total errors across all days.
    pub total_errors: u64,
    /// Timestamp of last successful use.
    pub last_used: Option<DateTime<Utc>>,
    /// Last error: timestamp and message.
    pub last_error: Option<(DateTime<Utc>, String)>,
    /// Whether this token is enabled for selection.
    pub enabled: bool,
    /// Weight for weighted selection; higher values are preferred.
    pub weight: u32,
}

/// Token tier affects selection priority.
///
/// Higher-priority tiers are preferred when using [`SelectionStrategy::TierPriority`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TokenTier {
    /// Backup tokens — only used when all others are exhausted.
    Backup = 0,
    /// Free tier tokens — lowest priority, strictest limits.
    Free = 1,
    /// Development/testing tokens — lower limits than production.
    Development = 2,
    /// Production tokens — highest priority.
    Production = 3,
}

/// Sliding window rate limiter for token usage.
///
/// Tracks call timestamps within the last 60 seconds and enforces a
/// maximum calls-per-minute limit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateWindow {
    /// Maximum number of calls allowed per 60-second window.
    pub max_per_minute: u32,
    /// Timestamps of recent calls within the window.
    pub window_calls: Vec<DateTime<Utc>>,
}

impl RateWindow {
    /// Create a new rate window with the given max calls per minute.
    pub fn new(max_per_minute: u32) -> Self {
        Self {
            max_per_minute,
            window_calls: Vec::new(),
        }
    }

    /// Record a call at the given timestamp.
    pub fn record_call(&mut self, now: DateTime<Utc>) {
        self.window_calls.push(now);
    }

    /// Check if the token is currently rate-limited.
    ///
    /// Returns `true` if the number of calls in the last 60 seconds
    /// is greater than or equal to `max_per_minute`.
    pub fn is_limited(&self, now: DateTime<Utc>) -> bool {
        self.calls_in_window(now) >= self.max_per_minute
    }

    /// Count the number of calls within the last 60-second window.
    pub fn calls_in_window(&self, now: DateTime<Utc>) -> u32 {
        let cutoff = now - chrono::Duration::seconds(RATE_WINDOW_SECONDS);
        self.window_calls.iter().filter(|ts| **ts > cutoff).count() as u32
    }

    /// Remove entries older than 60 seconds from the window.
    pub fn cleanup(&mut self, now: DateTime<Utc>) {
        let cutoff = now - chrono::Duration::seconds(RATE_WINDOW_SECONDS);
        self.window_calls.retain(|ts| *ts > cutoff);
    }
}

/// Strategy for selecting which token to use from the pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelectionStrategy {
    /// Use the token with the most remaining daily quota.
    MostRemaining,
    /// Round-robin across available tokens.
    RoundRobin,
    /// Weighted selection based on token weight fields.
    ///
    /// Uses a deterministic weighted round-robin approach: the counter
    /// cycles through a virtual ring sized to the sum of all weights.
    WeightedRandom,
    /// Always prefer highest-tier tokens first, within same tier use most-remaining.
    TierPriority,
}

/// The token pool — manages multiple tokens per provider.
///
/// Thread-safe via `Arc<RwLock<...>>` for the internal maps and `AtomicU64`
/// for the round-robin counter.
pub struct TokenPool {
    /// All tokens indexed by their unique ID.
    tokens: Arc<RwLock<HashMap<String, PooledToken>>>,
    /// Mapping from provider name to list of token IDs.
    providers: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// Current selection strategy.
    strategy: SelectionStrategy,
    /// Counter for round-robin and weighted-random strategies.
    round_robin_index: Arc<AtomicU64>,
}

/// Summary of pool health for a specific provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolHealth {
    /// Provider name.
    pub provider: String,
    /// Total number of tokens registered for this provider.
    pub total_tokens: usize,
    /// Number of tokens currently available for selection.
    pub available_tokens: usize,
    /// Number of tokens whose daily quota is fully exhausted.
    pub exhausted_tokens: usize,
    /// Number of tokens currently rate-limited.
    pub rate_limited_tokens: usize,
    /// Number of tokens that are disabled.
    pub disabled_tokens: usize,
    /// Sum of remaining daily quota across all tokens.
    pub total_daily_remaining: u64,
    /// Estimated total calls still available (rate + quota headroom).
    pub estimated_calls_available: u64,
}

/// Global pool statistics across all providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStats {
    /// Total number of tokens in the pool.
    pub total_tokens: usize,
    /// Total number of distinct providers.
    pub total_providers: usize,
    /// Sum of total_usage across all tokens.
    pub total_usage: u64,
    /// Sum of total_errors across all tokens.
    pub total_errors: u64,
    /// Per-provider health summaries.
    pub per_provider: HashMap<String, PoolHealth>,
}

impl TokenPool {
    /// Create a new empty token pool with the given selection strategy.
    pub fn new(strategy: SelectionStrategy) -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashMap::new())),
            providers: Arc::new(RwLock::new(HashMap::new())),
            strategy,
            round_robin_index: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Add a new token to the pool.
    ///
    /// # Errors
    ///
    /// Returns an error if a token with the same `id` already exists.
    #[allow(clippy::too_many_arguments)]
    pub fn add_token(
        &self,
        id: impl Into<String>,
        provider: impl Into<String>,
        value: impl Into<String>,
        tier: TokenTier,
        max_per_minute: u32,
        daily_quota: Option<u64>,
        weight: u32,
    ) -> ArgentorResult<()> {
        let id = id.into();
        let provider = provider.into();
        let value = value.into();

        let token = PooledToken {
            id: id.clone(),
            provider: provider.clone(),
            token_value: value,
            tier,
            rate_limit: RateWindow::new(max_per_minute),
            daily_quota,
            daily_usage: 0,
            quota_reset_at: next_midnight_utc(),
            total_usage: 0,
            total_errors: 0,
            last_used: None,
            last_error: None,
            enabled: true,
            weight,
        };

        {
            let mut tokens = self
                .tokens
                .write()
                .map_err(|e| ArgentorError::Security(format!("Token pool lock poisoned: {e}")))?;
            if tokens.contains_key(&id) {
                return Err(ArgentorError::Config(format!(
                    "Token with id '{id}' already exists in pool"
                )));
            }
            tokens.insert(id.clone(), token);
        }

        {
            let mut providers = self
                .providers
                .write()
                .map_err(|e| ArgentorError::Security(format!("Provider lock poisoned: {e}")))?;
            providers.entry(provider).or_insert_with(Vec::new).push(id);
        }

        Ok(())
    }

    /// Remove a token from the pool by ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the token ID is not found.
    pub fn remove_token(&self, id: &str) -> ArgentorResult<()> {
        let provider = {
            let mut tokens = self
                .tokens
                .write()
                .map_err(|e| ArgentorError::Security(format!("Token pool lock poisoned: {e}")))?;
            let token = tokens.remove(id).ok_or_else(|| {
                ArgentorError::Config(format!("Token with id '{id}' not found in pool"))
            })?;
            token.provider
        };

        {
            let mut providers = self
                .providers
                .write()
                .map_err(|e| ArgentorError::Security(format!("Provider lock poisoned: {e}")))?;
            if let Some(ids) = providers.get_mut(&provider) {
                ids.retain(|i| i != id);
                if ids.is_empty() {
                    providers.remove(&provider);
                }
            }
        }

        Ok(())
    }

    /// Select the best available token for the given provider.
    ///
    /// Returns the token value (the actual API key) for immediate use.
    ///
    /// # Errors
    ///
    /// Returns an error if no tokens are available for the provider (all disabled,
    /// rate-limited, or quota-exhausted).
    pub fn select(&self, provider: &str) -> ArgentorResult<String> {
        let tokens = self
            .tokens
            .read()
            .map_err(|e| ArgentorError::Security(format!("Token pool lock poisoned: {e}")))?;
        let providers = self
            .providers
            .read()
            .map_err(|e| ArgentorError::Security(format!("Provider lock poisoned: {e}")))?;

        let token_ids = providers.get(provider).ok_or_else(|| {
            ArgentorError::Config(format!("No tokens registered for provider '{provider}'"))
        })?;

        let now = Utc::now();
        let available: Vec<&PooledToken> = token_ids
            .iter()
            .filter_map(|id| tokens.get(id))
            .filter(|t| is_token_available(t, now))
            .collect();

        if available.is_empty() {
            return Err(ArgentorError::Security(format!(
                "No available tokens for provider '{provider}': all tokens are disabled, rate-limited, or quota-exhausted"
            )));
        }

        let selected = match self.strategy {
            SelectionStrategy::MostRemaining => select_most_remaining(&available),
            SelectionStrategy::RoundRobin => {
                let idx = self.round_robin_index.fetch_add(1, Ordering::Relaxed);
                select_round_robin(&available, idx)
            }
            SelectionStrategy::WeightedRandom => {
                let idx = self.round_robin_index.fetch_add(1, Ordering::Relaxed);
                select_weighted(&available, idx)
            }
            SelectionStrategy::TierPriority => select_tier_priority(&available),
        };

        Ok(selected.token_value.clone())
    }

    /// Record a successful usage of the token with the given ID.
    ///
    /// Updates daily usage, total usage, last-used timestamp, and the rate window.
    ///
    /// # Errors
    ///
    /// Returns an error if the token ID is not found.
    pub fn record_usage(&self, id: &str) -> ArgentorResult<()> {
        let mut tokens = self
            .tokens
            .write()
            .map_err(|e| ArgentorError::Security(format!("Token pool lock poisoned: {e}")))?;
        let token = tokens.get_mut(id).ok_or_else(|| {
            ArgentorError::Config(format!("Token with id '{id}' not found in pool"))
        })?;

        let now = Utc::now();
        maybe_reset_daily(token, now);

        token.daily_usage += 1;
        token.total_usage += 1;
        token.last_used = Some(now);
        token.rate_limit.record_call(now);
        token.rate_limit.cleanup(now);

        Ok(())
    }

    /// Record an error for the token with the given ID.
    ///
    /// Increments the error counter and stores the error message with timestamp.
    ///
    /// # Errors
    ///
    /// Returns an error if the token ID is not found.
    pub fn record_error(&self, id: &str, error_msg: &str) -> ArgentorResult<()> {
        let mut tokens = self
            .tokens
            .write()
            .map_err(|e| ArgentorError::Security(format!("Token pool lock poisoned: {e}")))?;
        let token = tokens.get_mut(id).ok_or_else(|| {
            ArgentorError::Config(format!("Token with id '{id}' not found in pool"))
        })?;

        let now = Utc::now();
        token.total_errors += 1;
        token.last_error = Some((now, error_msg.to_string()));

        Ok(())
    }

    /// Check if a token is currently rate-limited (sliding window).
    ///
    /// Returns `false` if the token ID is not found.
    pub fn is_rate_limited(&self, id: &str) -> bool {
        let tokens = match self.tokens.read() {
            Ok(t) => t,
            Err(_) => return false,
        };
        match tokens.get(id) {
            Some(token) => token.rate_limit.is_limited(Utc::now()),
            None => false,
        }
    }

    /// Check if a token's daily quota is exhausted.
    ///
    /// Returns `false` if the token has no daily quota or if the token ID is not found.
    pub fn is_quota_exhausted(&self, id: &str) -> bool {
        let tokens = match self.tokens.read() {
            Ok(t) => t,
            Err(_) => return false,
        };
        match tokens.get(id) {
            Some(token) => is_quota_exhausted_inner(token),
            None => false,
        }
    }

    /// Check if a token is available for use (enabled, not rate-limited, not exhausted).
    ///
    /// Returns `false` if the token ID is not found.
    pub fn is_available(&self, id: &str) -> bool {
        let tokens = match self.tokens.read() {
            Ok(t) => t,
            Err(_) => return false,
        };
        match tokens.get(id) {
            Some(token) => is_token_available(token, Utc::now()),
            None => false,
        }
    }

    /// List all available token IDs for a given provider.
    pub fn available_for_provider(&self, provider: &str) -> Vec<String> {
        let tokens = match self.tokens.read() {
            Ok(t) => t,
            Err(_) => return Vec::new(),
        };
        let providers = match self.providers.read() {
            Ok(p) => p,
            Err(_) => return Vec::new(),
        };

        let now = Utc::now();
        providers
            .get(provider)
            .map(|ids| {
                ids.iter()
                    .filter(|id| {
                        tokens
                            .get(*id)
                            .map(|t| is_token_available(t, now))
                            .unwrap_or(false)
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Reset daily quotas for all tokens whose reset time has passed.
    ///
    /// Call this periodically (e.g. once per minute) to ensure quotas roll over at midnight.
    pub fn reset_daily_quotas(&self) {
        let mut tokens = match self.tokens.write() {
            Ok(t) => t,
            Err(_) => return,
        };
        let now = Utc::now();
        for token in tokens.values_mut() {
            maybe_reset_daily(token, now);
        }
    }

    /// Returns all tokens stored in the pool.
    ///
    /// This returns clones of all token entries. Use with caution in
    /// production code — the returned values contain plaintext secrets.
    pub fn list_all(&self) -> Vec<PooledToken> {
        let tokens = match self.tokens.read() {
            Ok(t) => t,
            Err(_) => return Vec::new(),
        };
        tokens.values().cloned().collect()
    }

    /// Get the health summary for a specific provider.
    pub fn pool_health(&self, provider: &str) -> PoolHealth {
        let tokens = match self.tokens.read() {
            Ok(t) => t,
            Err(_) => {
                return PoolHealth {
                    provider: provider.to_string(),
                    total_tokens: 0,
                    available_tokens: 0,
                    exhausted_tokens: 0,
                    rate_limited_tokens: 0,
                    disabled_tokens: 0,
                    total_daily_remaining: 0,
                    estimated_calls_available: 0,
                }
            }
        };
        let providers = match self.providers.read() {
            Ok(p) => p,
            Err(_) => {
                return PoolHealth {
                    provider: provider.to_string(),
                    total_tokens: 0,
                    available_tokens: 0,
                    exhausted_tokens: 0,
                    rate_limited_tokens: 0,
                    disabled_tokens: 0,
                    total_daily_remaining: 0,
                    estimated_calls_available: 0,
                }
            }
        };

        let now = Utc::now();
        let empty_vec = Vec::new();
        let token_ids = providers.get(provider).unwrap_or(&empty_vec);

        let mut total_tokens = 0usize;
        let mut available_tokens = 0usize;
        let mut exhausted_tokens = 0usize;
        let mut rate_limited_tokens = 0usize;
        let mut disabled_tokens = 0usize;
        let mut total_daily_remaining: u64 = 0;

        for id in token_ids {
            if let Some(token) = tokens.get(id) {
                total_tokens += 1;

                if !token.enabled {
                    disabled_tokens += 1;
                }

                if token.rate_limit.is_limited(now) {
                    rate_limited_tokens += 1;
                }

                if is_quota_exhausted_inner(token) {
                    exhausted_tokens += 1;
                }

                if is_token_available(token, now) {
                    available_tokens += 1;
                }

                if let Some(quota) = token.daily_quota {
                    let remaining = quota.saturating_sub(token.daily_usage);
                    total_daily_remaining += remaining;
                } else {
                    // Unlimited quota — add a large sentinel value
                    total_daily_remaining = total_daily_remaining.saturating_add(u64::MAX / 2);
                }
            }
        }

        PoolHealth {
            provider: provider.to_string(),
            total_tokens,
            available_tokens,
            exhausted_tokens,
            rate_limited_tokens,
            disabled_tokens,
            total_daily_remaining,
            estimated_calls_available: total_daily_remaining,
        }
    }

    /// Get global pool statistics across all providers.
    pub fn stats(&self) -> PoolStats {
        let tokens = match self.tokens.read() {
            Ok(t) => t,
            Err(_) => {
                return PoolStats {
                    total_tokens: 0,
                    total_providers: 0,
                    total_usage: 0,
                    total_errors: 0,
                    per_provider: HashMap::new(),
                }
            }
        };
        let providers = match self.providers.read() {
            Ok(p) => p,
            Err(_) => {
                return PoolStats {
                    total_tokens: 0,
                    total_providers: 0,
                    total_usage: 0,
                    total_errors: 0,
                    per_provider: HashMap::new(),
                }
            }
        };

        let total_tokens = tokens.len();
        let total_providers = providers.len();
        let total_usage: u64 = tokens.values().map(|t| t.total_usage).sum();
        let total_errors: u64 = tokens.values().map(|t| t.total_errors).sum();

        // Drop locks before calling pool_health (which re-acquires them)
        drop(tokens);
        drop(providers);

        let providers_read = match self.providers.read() {
            Ok(p) => p,
            Err(_) => {
                return PoolStats {
                    total_tokens,
                    total_providers,
                    total_usage,
                    total_errors,
                    per_provider: HashMap::new(),
                }
            }
        };
        let provider_names: Vec<String> = providers_read.keys().cloned().collect();
        drop(providers_read);

        let mut per_provider = HashMap::new();
        for prov in &provider_names {
            per_provider.insert(prov.clone(), self.pool_health(prov));
        }

        PoolStats {
            total_tokens,
            total_providers,
            total_usage,
            total_errors,
            per_provider,
        }
    }

    /// Enable or disable a token.
    ///
    /// # Errors
    ///
    /// Returns an error if the token ID is not found.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> ArgentorResult<()> {
        let mut tokens = self
            .tokens
            .write()
            .map_err(|e| ArgentorError::Security(format!("Token pool lock poisoned: {e}")))?;
        let token = tokens.get_mut(id).ok_or_else(|| {
            ArgentorError::Config(format!("Token with id '{id}' not found in pool"))
        })?;
        token.enabled = enabled;
        Ok(())
    }

    /// Change the selection strategy at runtime.
    pub fn set_strategy(&mut self, strategy: SelectionStrategy) {
        self.strategy = strategy;
    }

    /// Check if a provider has at least one available token.
    pub fn provider_has_capacity(&self, provider: &str) -> bool {
        !self.available_for_provider(provider).is_empty()
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Check if a token is available: enabled, not rate-limited, and not quota-exhausted.
fn is_token_available(token: &PooledToken, now: DateTime<Utc>) -> bool {
    token.enabled && !token.rate_limit.is_limited(now) && !is_quota_exhausted_inner(token)
}

/// Check if a token's daily quota is exhausted.
fn is_quota_exhausted_inner(token: &PooledToken) -> bool {
    match token.daily_quota {
        Some(quota) => token.daily_usage >= quota,
        None => false,
    }
}

/// Remaining daily quota for a token (u64::MAX if unlimited).
fn remaining_quota(token: &PooledToken) -> u64 {
    match token.daily_quota {
        Some(quota) => quota.saturating_sub(token.daily_usage),
        None => u64::MAX,
    }
}

/// If the token's quota reset time has passed, reset daily counters.
fn maybe_reset_daily(token: &mut PooledToken, now: DateTime<Utc>) {
    if now >= token.quota_reset_at {
        token.daily_usage = 0;
        token.quota_reset_at = next_midnight_utc_from(now);
    }
}

/// Compute next midnight UTC from the current time.
fn next_midnight_utc() -> DateTime<Utc> {
    next_midnight_utc_from(Utc::now())
}

/// Compute next midnight UTC from a given time.
fn next_midnight_utc_from(now: DateTime<Utc>) -> DateTime<Utc> {
    use chrono::{Duration, NaiveTime, Timelike};
    let today_midnight = now
        .date_naive()
        .and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap_or_default())
        .and_utc();
    if now.hour() == 0 && now.minute() == 0 && now.second() == 0 {
        // Already at midnight — next midnight is tomorrow
        today_midnight + Duration::days(1)
    } else {
        today_midnight + Duration::days(1)
    }
}

// ---------------------------------------------------------------------------
// Selection strategies
// ---------------------------------------------------------------------------

/// Select the token with the most remaining daily quota.
fn select_most_remaining<'a>(available: &[&'a PooledToken]) -> &'a PooledToken {
    // Safety: caller guarantees `available` is non-empty.
    available
        .iter()
        .max_by_key(|t| remaining_quota(t))
        .copied()
        .unwrap_or(available[0])
}

/// Round-robin selection across available tokens.
fn select_round_robin<'a>(available: &[&'a PooledToken], counter: u64) -> &'a PooledToken {
    let idx = (counter as usize) % available.len();
    available[idx]
}

/// Weighted selection: distributes calls proportionally to token weights.
///
/// Uses a deterministic weighted round-robin: the counter cycles through a
/// virtual ring whose size equals the sum of all weights.
fn select_weighted<'a>(available: &[&'a PooledToken], counter: u64) -> &'a PooledToken {
    let total_weight: u64 = available.iter().map(|t| u64::from(t.weight)).sum();
    if total_weight == 0 {
        // Fallback to simple round-robin if all weights are zero
        return select_round_robin(available, counter);
    }

    let pos = counter % total_weight;
    let mut cumulative: u64 = 0;
    for token in available {
        cumulative += u64::from(token.weight);
        if pos < cumulative {
            return token;
        }
    }

    // Should never reach here, but return last token as fallback.
    available[available.len() - 1]
}

/// Tier-priority selection: prefer highest tier, within same tier prefer most remaining.
fn select_tier_priority<'a>(available: &[&'a PooledToken]) -> &'a PooledToken {
    available
        .iter()
        .max_by(|a, b| {
            a.tier
                .cmp(&b.tier)
                .then_with(|| remaining_quota(a).cmp(&remaining_quota(b)))
        })
        .copied()
        .unwrap_or(available[0])
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use chrono::Duration;

    /// Helper: create a pool and add some tokens.
    fn setup_pool(strategy: SelectionStrategy) -> TokenPool {
        let pool = TokenPool::new(strategy);
        pool.add_token(
            "t1",
            "openai",
            "sk-111",
            TokenTier::Production,
            60,
            Some(1000),
            10,
        )
        .unwrap();
        pool.add_token(
            "t2",
            "openai",
            "sk-222",
            TokenTier::Development,
            30,
            Some(500),
            5,
        )
        .unwrap();
        pool.add_token(
            "t3",
            "anthropic",
            "ak-333",
            TokenTier::Production,
            100,
            None,
            8,
        )
        .unwrap();
        pool
    }

    #[test]
    fn test_add_token() {
        let pool = TokenPool::new(SelectionStrategy::MostRemaining);
        let result = pool.add_token(
            "t1",
            "openai",
            "sk-111",
            TokenTier::Production,
            60,
            Some(1000),
            10,
        );
        assert!(result.is_ok());

        // Duplicate ID should fail
        let result = pool.add_token("t1", "openai", "sk-999", TokenTier::Free, 10, None, 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_token() {
        let pool = setup_pool(SelectionStrategy::MostRemaining);

        assert!(pool.remove_token("t1").is_ok());
        assert!(pool.remove_token("t1").is_err()); // Already removed

        // t2 should still be available for openai
        let available = pool.available_for_provider("openai");
        assert_eq!(available.len(), 1);
        assert_eq!(available[0], "t2");
    }

    #[test]
    fn test_select_most_remaining() {
        let pool = setup_pool(SelectionStrategy::MostRemaining);

        // t1 has quota 1000, t2 has quota 500, so t1 should be selected
        let value = pool.select("openai").unwrap();
        assert_eq!(value, "sk-111");

        // Consume some of t1's quota to make t2 have more remaining
        {
            let mut tokens = pool.tokens.write().unwrap();
            let t1 = tokens.get_mut("t1").unwrap();
            t1.daily_usage = 900; // 100 remaining
        }
        // Now t2 has 500 remaining vs t1's 100
        let value = pool.select("openai").unwrap();
        assert_eq!(value, "sk-222");
    }

    #[test]
    fn test_select_round_robin() {
        let pool = setup_pool(SelectionStrategy::RoundRobin);

        // Should cycle between t1 and t2 for openai
        let v1 = pool.select("openai").unwrap();
        let v2 = pool.select("openai").unwrap();

        // They should be different (unless they map to same index, but with 2 tokens and
        // sequential counter they will alternate)
        assert!(v1 == "sk-111" || v1 == "sk-222");
        assert!(v2 == "sk-111" || v2 == "sk-222");
        assert_ne!(v1, v2);
    }

    #[test]
    fn test_select_tier_priority() {
        let pool = TokenPool::new(SelectionStrategy::TierPriority);
        pool.add_token(
            "dev1",
            "prov",
            "dev-key",
            TokenTier::Development,
            60,
            Some(1000),
            5,
        )
        .unwrap();
        pool.add_token(
            "prod1",
            "prov",
            "prod-key",
            TokenTier::Production,
            60,
            Some(100),
            5,
        )
        .unwrap();
        pool.add_token(
            "free1",
            "prov",
            "free-key",
            TokenTier::Free,
            60,
            Some(2000),
            5,
        )
        .unwrap();

        // Should always pick Production tier first
        let value = pool.select("prov").unwrap();
        assert_eq!(value, "prod-key");
    }

    #[test]
    fn test_select_tier_priority_same_tier_most_remaining() {
        let pool = TokenPool::new(SelectionStrategy::TierPriority);
        pool.add_token(
            "p1",
            "prov",
            "key-a",
            TokenTier::Production,
            60,
            Some(100),
            5,
        )
        .unwrap();
        pool.add_token(
            "p2",
            "prov",
            "key-b",
            TokenTier::Production,
            60,
            Some(500),
            5,
        )
        .unwrap();

        // Same tier — should pick the one with more remaining (p2 = 500)
        let value = pool.select("prov").unwrap();
        assert_eq!(value, "key-b");
    }

    #[test]
    fn test_rate_limiting_sliding_window() {
        let pool = TokenPool::new(SelectionStrategy::MostRemaining);
        pool.add_token("t1", "prov", "key", TokenTier::Production, 3, None, 10)
            .unwrap();

        // Record 3 calls → should become rate-limited
        pool.record_usage("t1").unwrap();
        pool.record_usage("t1").unwrap();
        pool.record_usage("t1").unwrap();

        assert!(pool.is_rate_limited("t1"));
    }

    #[test]
    fn test_rate_window_expiry() {
        let mut rw = RateWindow::new(2);
        let now = Utc::now();
        let old = now - Duration::seconds(90); // 90 seconds ago — outside window

        rw.record_call(old);
        rw.record_call(old);

        // Should NOT be limited because calls are outside the 60s window
        assert!(!rw.is_limited(now));
        assert_eq!(rw.calls_in_window(now), 0);

        // Cleanup should remove old entries
        rw.cleanup(now);
        assert!(rw.window_calls.is_empty());
    }

    #[test]
    fn test_daily_quota_enforcement() {
        let pool = TokenPool::new(SelectionStrategy::MostRemaining);
        pool.add_token(
            "t1",
            "prov",
            "key",
            TokenTier::Production,
            1000,
            Some(3),
            10,
        )
        .unwrap();

        pool.record_usage("t1").unwrap();
        pool.record_usage("t1").unwrap();
        pool.record_usage("t1").unwrap();

        assert!(pool.is_quota_exhausted("t1"));
        assert!(!pool.is_available("t1"));
    }

    #[test]
    fn test_quota_exhaustion_prevents_selection() {
        let pool = TokenPool::new(SelectionStrategy::MostRemaining);
        pool.add_token(
            "t1",
            "prov",
            "key",
            TokenTier::Production,
            1000,
            Some(1),
            10,
        )
        .unwrap();

        pool.record_usage("t1").unwrap();

        // Now quota is exhausted, select should fail
        let result = pool.select("prov");
        assert!(result.is_err());
    }

    #[test]
    fn test_error_recording() {
        let pool = setup_pool(SelectionStrategy::MostRemaining);

        pool.record_error("t1", "rate limit exceeded").unwrap();
        pool.record_error("t1", "server error 500").unwrap();

        let tokens = pool.tokens.read().unwrap();
        let t1 = tokens.get("t1").unwrap();
        assert_eq!(t1.total_errors, 2);
        assert!(t1.last_error.is_some());
        assert_eq!(t1.last_error.as_ref().unwrap().1, "server error 500");
    }

    #[test]
    fn test_provider_grouping() {
        let pool = setup_pool(SelectionStrategy::MostRemaining);

        let openai_tokens = pool.available_for_provider("openai");
        assert_eq!(openai_tokens.len(), 2);
        assert!(openai_tokens.contains(&"t1".to_string()));
        assert!(openai_tokens.contains(&"t2".to_string()));

        let anthropic_tokens = pool.available_for_provider("anthropic");
        assert_eq!(anthropic_tokens.len(), 1);
        assert!(anthropic_tokens.contains(&"t3".to_string()));
    }

    #[test]
    fn test_pool_health() {
        let pool = setup_pool(SelectionStrategy::MostRemaining);

        let health = pool.pool_health("openai");
        assert_eq!(health.provider, "openai");
        assert_eq!(health.total_tokens, 2);
        assert_eq!(health.available_tokens, 2);
        assert_eq!(health.exhausted_tokens, 0);
        assert_eq!(health.disabled_tokens, 0);
    }

    #[test]
    fn test_pool_health_with_disabled_token() {
        let pool = setup_pool(SelectionStrategy::MostRemaining);
        pool.set_enabled("t1", false).unwrap();

        let health = pool.pool_health("openai");
        assert_eq!(health.available_tokens, 1);
        assert_eq!(health.disabled_tokens, 1);
    }

    #[test]
    fn test_stats_export() {
        let pool = setup_pool(SelectionStrategy::MostRemaining);
        pool.record_usage("t1").unwrap();
        pool.record_usage("t2").unwrap();
        pool.record_error("t3", "oops").unwrap();

        let stats = pool.stats();
        assert_eq!(stats.total_tokens, 3);
        assert_eq!(stats.total_providers, 2);
        assert_eq!(stats.total_usage, 2);
        assert_eq!(stats.total_errors, 1);
        assert!(stats.per_provider.contains_key("openai"));
        assert!(stats.per_provider.contains_key("anthropic"));
    }

    #[test]
    fn test_enable_disable_token() {
        let pool = setup_pool(SelectionStrategy::MostRemaining);

        // Disable t1
        pool.set_enabled("t1", false).unwrap();
        assert!(!pool.is_available("t1"));

        // Only t2 should be available for openai
        let available = pool.available_for_provider("openai");
        assert_eq!(available.len(), 1);
        assert_eq!(available[0], "t2");

        // Re-enable
        pool.set_enabled("t1", true).unwrap();
        assert!(pool.is_available("t1"));
    }

    #[test]
    fn test_no_available_tokens_error() {
        let pool = TokenPool::new(SelectionStrategy::MostRemaining);
        pool.add_token(
            "t1",
            "prov",
            "key",
            TokenTier::Production,
            1000,
            Some(1),
            10,
        )
        .unwrap();
        pool.record_usage("t1").unwrap();

        let result = pool.select("prov");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("No available tokens"));
    }

    #[test]
    fn test_no_provider_error() {
        let pool = TokenPool::new(SelectionStrategy::MostRemaining);
        let result = pool.select("nonexistent");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("No tokens registered"));
    }

    #[test]
    fn test_daily_quota_reset() {
        let pool = TokenPool::new(SelectionStrategy::MostRemaining);
        pool.add_token(
            "t1",
            "prov",
            "key",
            TokenTier::Production,
            1000,
            Some(5),
            10,
        )
        .unwrap();

        // Exhaust the quota
        for _ in 0..5 {
            pool.record_usage("t1").unwrap();
        }
        assert!(pool.is_quota_exhausted("t1"));

        // Simulate quota reset by setting reset_at to the past
        {
            let mut tokens = pool.tokens.write().unwrap();
            let t = tokens.get_mut("t1").unwrap();
            t.quota_reset_at = Utc::now() - Duration::hours(1);
        }

        // Trigger reset
        pool.reset_daily_quotas();

        // Should be available again
        assert!(!pool.is_quota_exhausted("t1"));
        assert!(pool.is_available("t1"));

        // Verify daily_usage was reset
        {
            let tokens = pool.tokens.read().unwrap();
            let t = tokens.get("t1").unwrap();
            assert_eq!(t.daily_usage, 0);
        }
    }

    #[test]
    fn test_multiple_providers() {
        let pool = setup_pool(SelectionStrategy::MostRemaining);

        // Selecting from openai should not affect anthropic
        let openai_val = pool.select("openai").unwrap();
        let anthropic_val = pool.select("anthropic").unwrap();

        assert!(openai_val == "sk-111" || openai_val == "sk-222");
        assert_eq!(anthropic_val, "ak-333");

        assert!(pool.provider_has_capacity("openai"));
        assert!(pool.provider_has_capacity("anthropic"));
        assert!(!pool.provider_has_capacity("nonexistent"));
    }

    #[test]
    fn test_weighted_selection() {
        let pool = TokenPool::new(SelectionStrategy::WeightedRandom);
        // w=3 and w=1 → 3/4 of calls go to heavy, 1/4 to light
        pool.add_token(
            "heavy",
            "prov",
            "heavy-key",
            TokenTier::Production,
            1000,
            None,
            3,
        )
        .unwrap();
        pool.add_token(
            "light",
            "prov",
            "light-key",
            TokenTier::Production,
            1000,
            None,
            1,
        )
        .unwrap();

        let mut heavy_count = 0u32;
        let mut light_count = 0u32;

        // Run through one full cycle of weights (4 calls)
        for _ in 0..4 {
            let val = pool.select("prov").unwrap();
            if val == "heavy-key" {
                heavy_count += 1;
            } else {
                light_count += 1;
            }
        }

        assert_eq!(heavy_count, 3);
        assert_eq!(light_count, 1);
    }

    #[test]
    fn test_record_usage_not_found() {
        let pool = TokenPool::new(SelectionStrategy::MostRemaining);
        let result = pool.record_usage("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_record_error_not_found() {
        let pool = TokenPool::new(SelectionStrategy::MostRemaining);
        let result = pool.record_error("nonexistent", "oops");
        assert!(result.is_err());
    }

    #[test]
    fn test_set_enabled_not_found() {
        let pool = TokenPool::new(SelectionStrategy::MostRemaining);
        let result = pool.set_enabled("nonexistent", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_rate_window_basic() {
        let mut rw = RateWindow::new(5);
        let now = Utc::now();

        assert!(!rw.is_limited(now));
        assert_eq!(rw.calls_in_window(now), 0);

        for _ in 0..5 {
            rw.record_call(now);
        }

        assert!(rw.is_limited(now));
        assert_eq!(rw.calls_in_window(now), 5);
    }

    #[test]
    fn test_set_strategy() {
        let mut pool = TokenPool::new(SelectionStrategy::MostRemaining);
        pool.set_strategy(SelectionStrategy::RoundRobin);
        assert_eq!(pool.strategy, SelectionStrategy::RoundRobin);
    }

    #[test]
    fn test_pool_health_unknown_provider() {
        let pool = TokenPool::new(SelectionStrategy::MostRemaining);
        let health = pool.pool_health("unknown");
        assert_eq!(health.total_tokens, 0);
        assert_eq!(health.available_tokens, 0);
    }

    #[test]
    fn test_remove_last_token_removes_provider() {
        let pool = TokenPool::new(SelectionStrategy::MostRemaining);
        pool.add_token("t1", "solo", "key", TokenTier::Free, 10, None, 1)
            .unwrap();

        pool.remove_token("t1").unwrap();

        let providers = pool.providers.read().unwrap();
        assert!(!providers.contains_key("solo"));
    }

    #[test]
    fn test_unlimited_quota_never_exhausted() {
        let pool = TokenPool::new(SelectionStrategy::MostRemaining);
        pool.add_token("t1", "prov", "key", TokenTier::Production, 1000, None, 10)
            .unwrap();

        // Record many usages — should never be quota-exhausted
        for _ in 0..100 {
            pool.record_usage("t1").unwrap();
        }

        assert!(!pool.is_quota_exhausted("t1"));
    }
}
