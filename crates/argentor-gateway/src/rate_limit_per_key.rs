//! Per-API-key rate limiting for the Argentor gateway.
//!
//! Provides sliding-window rate limiting on a per-key basis with configurable
//! limits for requests per minute, requests per hour, and tokens per day.
//!
//! # Main types
//!
//! - [`PerKeyRateLimiter`] — The rate limiter that tracks usage per API key.
//! - [`RateLimitConfig`] — Per-key rate limit configuration.
//! - [`RateLimitResult`] — The outcome of a rate limit check (allow or deny).
//! - [`KeyUsageStats`] — Current usage statistics for a key.

use chrono::{DateTime, Utc};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Rate limit configuration for an API key.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests allowed per minute.
    pub requests_per_minute: u32,
    /// Maximum requests allowed per hour.
    pub requests_per_hour: u32,
    /// Maximum token consumption allowed per day.
    pub tokens_per_day: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: 60,
            requests_per_hour: 1000,
            tokens_per_day: 1_000_000,
        }
    }
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// Result of a rate limit check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RateLimitResult {
    /// The request is allowed.
    Allow,
    /// The request is denied. `retry_after` is the number of seconds to wait.
    Deny {
        /// Reason the request was denied.
        reason: DenyReason,
        /// Suggested seconds to wait before retrying.
        retry_after: u64,
    },
}

/// Why a request was denied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DenyReason {
    /// Exceeded requests-per-minute limit.
    MinuteRateExceeded,
    /// Exceeded requests-per-hour limit.
    HourRateExceeded,
    /// Exceeded daily token quota.
    DailyTokenQuotaExceeded,
}

impl std::fmt::Display for DenyReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MinuteRateExceeded => write!(f, "requests per minute limit exceeded"),
            Self::HourRateExceeded => write!(f, "requests per hour limit exceeded"),
            Self::DailyTokenQuotaExceeded => write!(f, "daily token quota exceeded"),
        }
    }
}

/// Current usage statistics for an API key.
#[derive(Debug, Clone)]
pub struct KeyUsageStats {
    /// Requests in the current minute window.
    pub requests_this_minute: u32,
    /// Requests in the current hour window.
    pub requests_this_hour: u32,
    /// Tokens consumed today.
    pub tokens_today: u64,
    /// Configured limits for this key.
    pub config: RateLimitConfig,
}

// ---------------------------------------------------------------------------
// Sliding window
// ---------------------------------------------------------------------------

/// A sliding window that tracks timestamps of events within a fixed duration.
#[derive(Debug, Clone)]
struct SlidingWindow {
    /// Timestamps of events within the window (monotonically increasing).
    timestamps: VecDeque<DateTime<Utc>>,
    /// Window duration in seconds.
    window_seconds: i64,
}

impl SlidingWindow {
    fn new(window_seconds: i64) -> Self {
        Self {
            timestamps: VecDeque::new(),
            window_seconds,
        }
    }

    /// Remove timestamps older than the window and return current count.
    fn count(&mut self, now: DateTime<Utc>) -> u32 {
        let cutoff = now - chrono::Duration::seconds(self.window_seconds);
        while let Some(front) = self.timestamps.front() {
            if *front < cutoff {
                self.timestamps.pop_front();
            } else {
                break;
            }
        }
        self.timestamps.len() as u32
    }

    /// Record an event at the given time.
    fn record(&mut self, now: DateTime<Utc>) {
        self.timestamps.push_back(now);
    }

    /// Seconds until the oldest entry expires, or 0 if the window is empty.
    fn seconds_until_slot(&self, now: DateTime<Utc>) -> u64 {
        if let Some(front) = self.timestamps.front() {
            let expires_at = *front + chrono::Duration::seconds(self.window_seconds);
            if expires_at > now {
                (expires_at - now).num_seconds().max(1) as u64
            } else {
                1
            }
        } else {
            0
        }
    }

    /// True if the window has had no events for the entire window duration.
    fn is_idle(&self, now: DateTime<Utc>) -> bool {
        match self.timestamps.back() {
            Some(last) => {
                let idle_since = now - *last;
                idle_since.num_seconds() >= self.window_seconds
            }
            None => true,
        }
    }
}

// ---------------------------------------------------------------------------
// Per-key state
// ---------------------------------------------------------------------------

/// Internal state for a single API key.
struct KeyState {
    minute_window: SlidingWindow,
    hour_window: SlidingWindow,
    daily_tokens: u64,
    daily_reset: DateTime<Utc>,
}

impl KeyState {
    fn new(now: DateTime<Utc>) -> Self {
        Self {
            minute_window: SlidingWindow::new(60),
            hour_window: SlidingWindow::new(3600),
            daily_tokens: 0,
            daily_reset: next_day_start(now),
        }
    }

    /// Reset the daily token counter if we've passed the reset time.
    fn maybe_reset_daily(&mut self, now: DateTime<Utc>) {
        if now >= self.daily_reset {
            self.daily_tokens = 0;
            self.daily_reset = next_day_start(now);
        }
    }
}

/// Compute the start of the next UTC day from the given timestamp.
fn next_day_start(now: DateTime<Utc>) -> DateTime<Utc> {
    let tomorrow = now.date_naive().succ_opt().unwrap_or(now.date_naive());
    // Safety: midnight (0, 0, 0) is always a valid time.
    #[allow(clippy::unwrap_used)]
    tomorrow.and_hms_opt(0, 0, 0).unwrap().and_utc()
}

// ---------------------------------------------------------------------------
// PerKeyRateLimiter
// ---------------------------------------------------------------------------

/// Per-API-key rate limiter with configurable limits per key.
///
/// Thread-safe via `std::sync::Mutex` (non-async, since lock hold times are short).
pub struct PerKeyRateLimiter {
    limiters: Mutex<HashMap<String, KeyState>>,
    default_config: RateLimitConfig,
    custom_configs: HashMap<String, RateLimitConfig>,
}

impl PerKeyRateLimiter {
    /// Create a new per-key rate limiter with the given default configuration.
    pub fn new(default_config: RateLimitConfig) -> Self {
        Self {
            limiters: Mutex::new(HashMap::new()),
            default_config,
            custom_configs: HashMap::new(),
        }
    }

    /// Add a custom rate limit configuration for a specific API key.
    ///
    /// Returns `self` for builder-style chaining.
    pub fn with_custom_limit(mut self, key: &str, config: RateLimitConfig) -> Self {
        self.custom_configs.insert(key.to_string(), config);
        self
    }

    /// Resolve the effective configuration for a key.
    fn config_for(&self, api_key: &str) -> &RateLimitConfig {
        self.custom_configs
            .get(api_key)
            .unwrap_or(&self.default_config)
    }

    /// Check whether a request from the given API key should be allowed.
    ///
    /// This also records the request in the sliding windows on success.
    pub fn check(&self, api_key: &str) -> RateLimitResult {
        self.check_at(api_key, Utc::now())
    }

    /// Check with an explicit timestamp (useful for testing).
    pub fn check_at(&self, api_key: &str, now: DateTime<Utc>) -> RateLimitResult {
        let config = self.config_for(api_key).clone();
        let mut map = self
            .limiters
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let state = map
            .entry(api_key.to_string())
            .or_insert_with(|| KeyState::new(now));

        state.maybe_reset_daily(now);

        // Check minute window
        let minute_count = state.minute_window.count(now);
        if minute_count >= config.requests_per_minute {
            let retry_after = state.minute_window.seconds_until_slot(now);
            return RateLimitResult::Deny {
                reason: DenyReason::MinuteRateExceeded,
                retry_after,
            };
        }

        // Check hour window
        let hour_count = state.hour_window.count(now);
        if hour_count >= config.requests_per_hour {
            let retry_after = state.hour_window.seconds_until_slot(now);
            return RateLimitResult::Deny {
                reason: DenyReason::HourRateExceeded,
                retry_after,
            };
        }

        // Check daily token quota
        if state.daily_tokens >= config.tokens_per_day {
            let retry_after = (state.daily_reset - now).num_seconds().max(1) as u64;
            return RateLimitResult::Deny {
                reason: DenyReason::DailyTokenQuotaExceeded,
                retry_after,
            };
        }

        // All checks passed — record the request
        state.minute_window.record(now);
        state.hour_window.record(now);

        RateLimitResult::Allow
    }

    /// Record token consumption for the given API key.
    pub fn record_usage(&self, api_key: &str, tokens: u64) {
        self.record_usage_at(api_key, tokens, Utc::now());
    }

    /// Record token consumption with an explicit timestamp (useful for testing).
    pub fn record_usage_at(&self, api_key: &str, tokens: u64, now: DateTime<Utc>) {
        let mut map = self
            .limiters
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let state = map
            .entry(api_key.to_string())
            .or_insert_with(|| KeyState::new(now));
        state.maybe_reset_daily(now);
        state.daily_tokens = state.daily_tokens.saturating_add(tokens);
    }

    /// Get current usage statistics for an API key, or `None` if the key has no state.
    pub fn stats(&self, api_key: &str) -> Option<KeyUsageStats> {
        self.stats_at(api_key, Utc::now())
    }

    /// Get stats with an explicit timestamp (useful for testing).
    pub fn stats_at(&self, api_key: &str, now: DateTime<Utc>) -> Option<KeyUsageStats> {
        let mut map = self
            .limiters
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let state = map.get_mut(api_key)?;
        state.maybe_reset_daily(now);

        Some(KeyUsageStats {
            requests_this_minute: state.minute_window.count(now),
            requests_this_hour: state.hour_window.count(now),
            tokens_today: state.daily_tokens,
            config: self.config_for(api_key).clone(),
        })
    }

    /// Remove entries for keys that have been idle (no requests) for longer
    /// than their window durations. Call periodically to free memory.
    pub fn cleanup(&self) {
        self.cleanup_at(Utc::now());
    }

    /// Cleanup with an explicit timestamp (useful for testing).
    pub fn cleanup_at(&self, now: DateTime<Utc>) {
        let mut map = self
            .limiters
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        map.retain(|_, state| {
            // Keep entries that still have activity in either window
            !state.minute_window.is_idle(now) || !state.hour_window.is_idle(now)
        });
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixed_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 4, 1, 12, 0, 0).unwrap()
    }

    fn default_config() -> RateLimitConfig {
        RateLimitConfig {
            requests_per_minute: 5,
            requests_per_hour: 100,
            tokens_per_day: 10_000,
        }
    }

    // 1. Under limit — requests are allowed
    #[test]
    fn test_under_limit_allows() {
        let limiter = PerKeyRateLimiter::new(default_config());
        let now = fixed_time();
        for i in 0..5 {
            let t = now + chrono::Duration::seconds(i);
            assert_eq!(limiter.check_at("key-a", t), RateLimitResult::Allow);
        }
    }

    // 2. At limit — next request is denied
    #[test]
    fn test_at_limit_denies() {
        let limiter = PerKeyRateLimiter::new(default_config());
        let now = fixed_time();
        for i in 0..5 {
            assert_eq!(
                limiter.check_at("key-a", now + chrono::Duration::seconds(i)),
                RateLimitResult::Allow
            );
        }
        // 6th request within the same minute window should be denied
        let result = limiter.check_at("key-a", now + chrono::Duration::seconds(5));
        assert!(matches!(
            result,
            RateLimitResult::Deny {
                reason: DenyReason::MinuteRateExceeded,
                ..
            }
        ));
    }

    // 3. Over limit — denied with retry_after
    #[test]
    fn test_over_limit_retry_after() {
        let limiter = PerKeyRateLimiter::new(default_config());
        let now = fixed_time();
        for i in 0..5 {
            limiter.check_at("key-a", now + chrono::Duration::seconds(i));
        }
        if let RateLimitResult::Deny { retry_after, .. } =
            limiter.check_at("key-a", now + chrono::Duration::seconds(10))
        {
            // The oldest entry (at t=0) expires at t=60, so retry_after should be around 50
            assert!(retry_after > 0);
            assert!(retry_after <= 60);
        } else {
            panic!("Expected Deny");
        }
    }

    // 4. Per-key isolation — key A's usage doesn't affect key B
    #[test]
    fn test_per_key_isolation() {
        let limiter = PerKeyRateLimiter::new(default_config());
        let now = fixed_time();

        // Exhaust key-a's minute limit
        for i in 0..5 {
            limiter.check_at("key-a", now + chrono::Duration::seconds(i));
        }
        assert!(matches!(
            limiter.check_at("key-a", now + chrono::Duration::seconds(5)),
            RateLimitResult::Deny { .. }
        ));

        // key-b should still be allowed
        assert_eq!(
            limiter.check_at("key-b", now + chrono::Duration::seconds(5)),
            RateLimitResult::Allow
        );
    }

    // 5. Custom config per key
    #[test]
    fn test_custom_config_per_key() {
        let limiter = PerKeyRateLimiter::new(default_config()).with_custom_limit(
            "premium-key",
            RateLimitConfig {
                requests_per_minute: 10,
                requests_per_hour: 500,
                tokens_per_day: 100_000,
            },
        );
        let now = fixed_time();

        // Default key can do 5 requests
        for i in 0..5 {
            assert_eq!(
                limiter.check_at("basic-key", now + chrono::Duration::seconds(i)),
                RateLimitResult::Allow
            );
        }
        assert!(matches!(
            limiter.check_at("basic-key", now + chrono::Duration::seconds(5)),
            RateLimitResult::Deny { .. }
        ));

        // Premium key can do 10 requests
        for i in 0..10 {
            assert_eq!(
                limiter.check_at("premium-key", now + chrono::Duration::seconds(i)),
                RateLimitResult::Allow
            );
        }
        assert!(matches!(
            limiter.check_at("premium-key", now + chrono::Duration::seconds(10)),
            RateLimitResult::Deny { .. }
        ));
    }

    // 6. Sliding window accuracy — old entries expire
    #[test]
    fn test_sliding_window_expiry() {
        let limiter = PerKeyRateLimiter::new(default_config());
        let now = fixed_time();

        // Use all 5 slots
        for i in 0..5 {
            limiter.check_at("key-a", now + chrono::Duration::seconds(i));
        }
        // Denied at t=10
        assert!(matches!(
            limiter.check_at("key-a", now + chrono::Duration::seconds(10)),
            RateLimitResult::Deny { .. }
        ));

        // After 61 seconds, the first entry has expired, so we get a slot
        let later = now + chrono::Duration::seconds(61);
        assert_eq!(limiter.check_at("key-a", later), RateLimitResult::Allow);
    }

    // 7. Token quota tracking
    #[test]
    fn test_token_quota_tracking() {
        let limiter = PerKeyRateLimiter::new(RateLimitConfig {
            requests_per_minute: 100,
            requests_per_hour: 1000,
            tokens_per_day: 1000,
        });
        let now = fixed_time();

        // Request is allowed
        assert_eq!(limiter.check_at("key-a", now), RateLimitResult::Allow);

        // Record 1000 tokens — at the limit
        limiter.record_usage_at("key-a", 1000, now);

        // Next request should be denied due to token quota
        let result = limiter.check_at("key-a", now + chrono::Duration::seconds(1));
        assert!(matches!(
            result,
            RateLimitResult::Deny {
                reason: DenyReason::DailyTokenQuotaExceeded,
                ..
            }
        ));
    }

    // 8. Daily reset clears token counter
    #[test]
    fn test_daily_reset() {
        let limiter = PerKeyRateLimiter::new(RateLimitConfig {
            requests_per_minute: 100,
            requests_per_hour: 1000,
            tokens_per_day: 500,
        });
        let now = fixed_time();

        limiter.check_at("key-a", now);
        limiter.record_usage_at("key-a", 500, now);

        // Denied today
        assert!(matches!(
            limiter.check_at("key-a", now + chrono::Duration::seconds(1)),
            RateLimitResult::Deny {
                reason: DenyReason::DailyTokenQuotaExceeded,
                ..
            }
        ));

        // Tomorrow at 00:00:01 UTC — should be allowed again
        let tomorrow = now + chrono::Duration::hours(13); // past midnight since now is 12:00
        assert_eq!(limiter.check_at("key-a", tomorrow), RateLimitResult::Allow);

        // Token counter should be reset
        let stats = limiter.stats_at("key-a", tomorrow).unwrap();
        assert_eq!(stats.tokens_today, 0);
    }

    // 9. Cleanup removes expired entries
    #[test]
    fn test_cleanup_expired_entries() {
        let limiter = PerKeyRateLimiter::new(default_config());
        let now = fixed_time();

        limiter.check_at("key-a", now);
        limiter.check_at("key-b", now);

        // Both keys should exist
        assert!(limiter.stats_at("key-a", now).is_some());
        assert!(limiter.stats_at("key-b", now).is_some());

        // Fast-forward well past all window durations (2 hours)
        let later = now + chrono::Duration::hours(2);
        limiter.cleanup_at(later);

        // Both should be cleaned up
        assert!(limiter.stats_at("key-a", later).is_none());
        assert!(limiter.stats_at("key-b", later).is_none());
    }

    // 10. Cleanup keeps active entries
    #[test]
    fn test_cleanup_keeps_active() {
        let limiter = PerKeyRateLimiter::new(default_config());
        let now = fixed_time();

        limiter.check_at("key-a", now);
        // key-b has a recent request
        let recent = now + chrono::Duration::seconds(30);
        limiter.check_at("key-b", recent);

        // Cleanup just after minute window (65 seconds from `now`)
        let cleanup_time = now + chrono::Duration::seconds(65);
        limiter.cleanup_at(cleanup_time);

        // key-a (last activity at t=0) should be gone — minute window idle and hour window idle
        // Actually, the hour window is 3600s so key-a is still within that
        // key-a is NOT idle in hour window (3600s), so it should be kept
        assert!(limiter.stats_at("key-a", cleanup_time).is_some());

        // Fast forward 2 hours — now key-a should be cleaned
        let much_later = now + chrono::Duration::hours(2);
        limiter.cleanup_at(much_later);
        assert!(limiter.stats_at("key-a", much_later).is_none());
    }

    // 11. Stats accuracy
    #[test]
    fn test_stats_accuracy() {
        let limiter = PerKeyRateLimiter::new(default_config());
        let now = fixed_time();

        // 3 requests
        for i in 0..3 {
            limiter.check_at("key-a", now + chrono::Duration::seconds(i));
        }
        limiter.record_usage_at("key-a", 42, now);

        let stats = limiter
            .stats_at("key-a", now + chrono::Duration::seconds(3))
            .unwrap();
        assert_eq!(stats.requests_this_minute, 3);
        assert_eq!(stats.requests_this_hour, 3);
        assert_eq!(stats.tokens_today, 42);
        assert_eq!(stats.config.requests_per_minute, 5);
    }

    // 12. Hour limit is enforced
    #[test]
    fn test_hour_limit_enforced() {
        let config = RateLimitConfig {
            requests_per_minute: 100, // high minute limit
            requests_per_hour: 10,    // low hour limit
            tokens_per_day: 1_000_000,
        };
        let limiter = PerKeyRateLimiter::new(config);
        let now = fixed_time();

        // Send 10 requests spread across minutes to avoid minute limit
        for i in 0..10 {
            let t = now + chrono::Duration::minutes(i);
            assert_eq!(limiter.check_at("key-a", t), RateLimitResult::Allow);
        }

        // 11th request should hit hour limit
        let t = now + chrono::Duration::minutes(10);
        assert!(matches!(
            limiter.check_at("key-a", t),
            RateLimitResult::Deny {
                reason: DenyReason::HourRateExceeded,
                ..
            }
        ));
    }

    // 13. Concurrent access safety (basic smoke test)
    #[test]
    fn test_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let limiter = Arc::new(PerKeyRateLimiter::new(RateLimitConfig {
            requests_per_minute: 1000,
            requests_per_hour: 10_000,
            tokens_per_day: 1_000_000,
        }));

        let mut handles = vec![];
        for t in 0..10 {
            let limiter = limiter.clone();
            handles.push(thread::spawn(move || {
                let key = format!("thread-key-{}", t % 3);
                for _ in 0..100 {
                    let _ = limiter.check(&key);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // No panics means the mutex held up correctly
    }

    // 14. Unknown key returns None for stats
    #[test]
    fn test_stats_unknown_key() {
        let limiter = PerKeyRateLimiter::new(default_config());
        assert!(limiter.stats("nonexistent").is_none());
    }

    // 15. Record usage without prior check creates entry
    #[test]
    fn test_record_usage_creates_entry() {
        let limiter = PerKeyRateLimiter::new(default_config());
        let now = fixed_time();

        limiter.record_usage_at("new-key", 500, now);
        let stats = limiter.stats_at("new-key", now).unwrap();
        assert_eq!(stats.tokens_today, 500);
        assert_eq!(stats.requests_this_minute, 0);
    }

    // 16. Token usage saturates instead of overflowing
    #[test]
    fn test_token_saturation() {
        let limiter = PerKeyRateLimiter::new(default_config());
        let now = fixed_time();

        limiter.record_usage_at("key-a", u64::MAX - 10, now);
        limiter.record_usage_at("key-a", 100, now);

        let stats = limiter.stats_at("key-a", now).unwrap();
        assert_eq!(stats.tokens_today, u64::MAX);
    }

    // 17. Default config values
    #[test]
    fn test_default_config() {
        let config = RateLimitConfig::default();
        assert_eq!(config.requests_per_minute, 60);
        assert_eq!(config.requests_per_hour, 1000);
        assert_eq!(config.tokens_per_day, 1_000_000);
    }

    // 18. DenyReason Display
    #[test]
    fn test_deny_reason_display() {
        assert_eq!(
            DenyReason::MinuteRateExceeded.to_string(),
            "requests per minute limit exceeded"
        );
        assert_eq!(
            DenyReason::HourRateExceeded.to_string(),
            "requests per hour limit exceeded"
        );
        assert_eq!(
            DenyReason::DailyTokenQuotaExceeded.to_string(),
            "daily token quota exceeded"
        );
    }
}
