//! Circuit breaker for LLM provider resilience.
//!
//! Protects against cascading failures by tracking provider health and
//! preventing calls to providers that are experiencing errors.
//!
//! # Main types
//!
//! - [`CircuitBreaker`] — State machine (Closed → Open → HalfOpen).
//! - [`CircuitState`] — Current state of the breaker.
//! - [`CircuitConfig`] — Configuration thresholds.
//! - [`CircuitBreakerRegistry`] — Manages breakers for multiple providers.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// CircuitState
// ---------------------------------------------------------------------------

/// The state of a circuit breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CircuitState {
    /// Normal operation — requests pass through.
    Closed,
    /// Provider is failing — requests are rejected.
    Open,
    /// Testing recovery — one request allowed through.
    HalfOpen,
}

// ---------------------------------------------------------------------------
// CircuitConfig
// ---------------------------------------------------------------------------

/// Configuration for a circuit breaker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitConfig {
    /// Number of consecutive failures before opening the circuit.
    pub failure_threshold: u32,
    /// Duration the circuit stays open before transitioning to half-open.
    pub recovery_timeout: Duration,
    /// Number of successes needed in half-open state to close the circuit.
    pub success_threshold: u32,
    /// Maximum number of failures to track in the sliding window.
    pub window_size: u32,
}

impl Default for CircuitConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(30),
            success_threshold: 2,
            window_size: 10,
        }
    }
}

impl CircuitConfig {
    /// Create a new config with the given failure threshold.
    pub fn new(failure_threshold: u32) -> Self {
        Self {
            failure_threshold,
            ..Default::default()
        }
    }

    /// Set the recovery timeout.
    pub fn with_recovery_timeout(mut self, timeout: Duration) -> Self {
        self.recovery_timeout = timeout;
        self
    }

    /// Set the success threshold for half-open recovery.
    pub fn with_success_threshold(mut self, threshold: u32) -> Self {
        self.success_threshold = threshold;
        self
    }
}

// ---------------------------------------------------------------------------
// CircuitBreaker
// ---------------------------------------------------------------------------

/// A circuit breaker for a single provider.
#[derive(Debug)]
pub struct CircuitBreaker {
    config: CircuitConfig,
    state: CircuitState,
    consecutive_failures: u32,
    consecutive_successes: u32,
    total_failures: u64,
    total_successes: u64,
    total_rejected: u64,
    last_failure_time: Option<Instant>,
    opened_at: Option<Instant>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given configuration.
    pub fn new(config: CircuitConfig) -> Self {
        Self {
            config,
            state: CircuitState::Closed,
            consecutive_failures: 0,
            consecutive_successes: 0,
            total_failures: 0,
            total_successes: 0,
            total_rejected: 0,
            last_failure_time: None,
            opened_at: None,
        }
    }

    /// Create a circuit breaker with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(CircuitConfig::default())
    }

    /// Check if a request should be allowed through.
    pub fn allow_request(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if recovery timeout has elapsed
                if let Some(opened) = self.opened_at {
                    if opened.elapsed() >= self.config.recovery_timeout {
                        self.state = CircuitState::HalfOpen;
                        self.consecutive_successes = 0;
                        return true; // Allow one probe request
                    }
                }
                self.total_rejected += 1;
                false
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Record a successful request.
    pub fn record_success(&mut self) {
        self.total_successes += 1;
        self.consecutive_failures = 0;
        self.consecutive_successes += 1;

        match self.state {
            CircuitState::HalfOpen => {
                if self.consecutive_successes >= self.config.success_threshold {
                    self.state = CircuitState::Closed;
                    self.opened_at = None;
                }
            }
            CircuitState::Open => {
                // Shouldn't happen, but handle gracefully
                self.state = CircuitState::HalfOpen;
            }
            CircuitState::Closed => {}
        }
    }

    /// Record a failed request.
    pub fn record_failure(&mut self) {
        self.total_failures += 1;
        self.consecutive_failures += 1;
        self.consecutive_successes = 0;
        self.last_failure_time = Some(Instant::now());

        match self.state {
            CircuitState::Closed => {
                if self.consecutive_failures >= self.config.failure_threshold {
                    self.state = CircuitState::Open;
                    self.opened_at = Some(Instant::now());
                }
            }
            CircuitState::HalfOpen => {
                // One failure in half-open → back to open
                self.state = CircuitState::Open;
                self.opened_at = Some(Instant::now());
            }
            CircuitState::Open => {}
        }
    }

    /// Get the current state.
    pub fn state(&self) -> CircuitState {
        self.state
    }

    /// Get a snapshot of the breaker status.
    pub fn status(&self) -> CircuitBreakerStatus {
        CircuitBreakerStatus {
            state: self.state,
            consecutive_failures: self.consecutive_failures,
            total_failures: self.total_failures,
            total_successes: self.total_successes,
            total_rejected: self.total_rejected,
            failure_threshold: self.config.failure_threshold,
        }
    }

    /// Force the circuit to a specific state (for testing/admin).
    pub fn force_state(&mut self, state: CircuitState) {
        self.state = state;
        if state == CircuitState::Open {
            self.opened_at = Some(Instant::now());
        } else if state == CircuitState::Closed {
            self.opened_at = None;
            self.consecutive_failures = 0;
        }
    }

    /// Reset all counters and return to closed state.
    pub fn reset(&mut self) {
        self.state = CircuitState::Closed;
        self.consecutive_failures = 0;
        self.consecutive_successes = 0;
        self.total_failures = 0;
        self.total_successes = 0;
        self.total_rejected = 0;
        self.last_failure_time = None;
        self.opened_at = None;
    }
}

/// Snapshot of circuit breaker state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerStatus {
    /// Current state.
    pub state: CircuitState,
    /// Consecutive failures since last success.
    pub consecutive_failures: u32,
    /// Total failures ever recorded.
    pub total_failures: u64,
    /// Total successes ever recorded.
    pub total_successes: u64,
    /// Total requests rejected by the open circuit.
    pub total_rejected: u64,
    /// Configured failure threshold.
    pub failure_threshold: u32,
}

// ---------------------------------------------------------------------------
// CircuitBreakerRegistry
// ---------------------------------------------------------------------------

/// Manages circuit breakers for multiple providers/services.
///
/// Clone is cheap (inner state is behind `Arc<RwLock>`).
#[derive(Debug, Clone)]
pub struct CircuitBreakerRegistry {
    breakers: Arc<RwLock<HashMap<String, CircuitBreaker>>>,
    default_config: CircuitConfig,
}

impl Default for CircuitBreakerRegistry {
    fn default() -> Self {
        Self::new(CircuitConfig::default())
    }
}

impl CircuitBreakerRegistry {
    /// Create a new registry with the given default configuration.
    pub fn new(default_config: CircuitConfig) -> Self {
        Self {
            breakers: Arc::new(RwLock::new(HashMap::new())),
            default_config,
        }
    }

    /// Register a breaker for a specific provider with custom config.
    pub fn register(&self, name: impl Into<String>, config: CircuitConfig) {
        self.breakers
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(name.into(), CircuitBreaker::new(config));
    }

    /// Check if a provider allows requests (auto-registers with defaults).
    pub fn allow_request(&self, provider: &str) -> bool {
        let mut breakers = self
            .breakers
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let breaker = breakers
            .entry(provider.to_string())
            .or_insert_with(|| CircuitBreaker::new(self.default_config.clone()));
        breaker.allow_request()
    }

    /// Record a success for a provider.
    pub fn record_success(&self, provider: &str) {
        let mut breakers = self
            .breakers
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(breaker) = breakers.get_mut(provider) {
            breaker.record_success();
        }
    }

    /// Record a failure for a provider.
    pub fn record_failure(&self, provider: &str) {
        let mut breakers = self
            .breakers
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let breaker = breakers
            .entry(provider.to_string())
            .or_insert_with(|| CircuitBreaker::new(self.default_config.clone()));
        breaker.record_failure();
    }

    /// Get the status of a specific provider's breaker.
    pub fn status(&self, provider: &str) -> Option<CircuitBreakerStatus> {
        self.breakers
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(provider)
            .map(CircuitBreaker::status)
    }

    /// Get all provider statuses.
    pub fn all_statuses(&self) -> HashMap<String, CircuitBreakerStatus> {
        self.breakers
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .map(|(k, v)| (k.clone(), v.status()))
            .collect()
    }

    /// Get the number of registered breakers.
    pub fn count(&self) -> usize {
        self.breakers
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn breaker() -> CircuitBreaker {
        CircuitBreaker::new(CircuitConfig::new(3).with_recovery_timeout(Duration::from_millis(50)))
    }

    // 1. Starts in closed state
    #[test]
    fn test_initial_state() {
        let b = breaker();
        assert_eq!(b.state(), CircuitState::Closed);
    }

    // 2. Allows requests when closed
    #[test]
    fn test_closed_allows() {
        let mut b = breaker();
        assert!(b.allow_request());
    }

    // 3. Opens after threshold failures
    #[test]
    fn test_opens_after_threshold() {
        let mut b = breaker();
        for _ in 0..3 {
            b.record_failure();
        }
        assert_eq!(b.state(), CircuitState::Open);
    }

    // 4. Open state rejects requests
    #[test]
    fn test_open_rejects() {
        let mut b = breaker();
        for _ in 0..3 {
            b.record_failure();
        }
        assert!(!b.allow_request());
    }

    // 5. Transitions to half-open after timeout
    #[test]
    fn test_half_open_after_timeout() {
        let mut b = breaker();
        for _ in 0..3 {
            b.record_failure();
        }
        std::thread::sleep(Duration::from_millis(60));
        assert!(b.allow_request());
        assert_eq!(b.state(), CircuitState::HalfOpen);
    }

    // 6. Success in half-open doesn't immediately close (needs threshold)
    #[test]
    fn test_half_open_needs_threshold() {
        let mut b = CircuitBreaker::new(
            CircuitConfig::new(3)
                .with_recovery_timeout(Duration::from_millis(50))
                .with_success_threshold(2),
        );
        for _ in 0..3 {
            b.record_failure();
        }
        std::thread::sleep(Duration::from_millis(60));
        b.allow_request(); // transitions to half-open
        b.record_success();
        assert_eq!(b.state(), CircuitState::HalfOpen);
        b.record_success();
        assert_eq!(b.state(), CircuitState::Closed);
    }

    // 7. Failure in half-open goes back to open
    #[test]
    fn test_half_open_failure() {
        let mut b = breaker();
        for _ in 0..3 {
            b.record_failure();
        }
        std::thread::sleep(Duration::from_millis(60));
        b.allow_request();
        b.record_failure();
        assert_eq!(b.state(), CircuitState::Open);
    }

    // 8. Success resets consecutive failures
    #[test]
    fn test_success_resets_failures() {
        let mut b = breaker();
        b.record_failure();
        b.record_failure();
        b.record_success();
        assert_eq!(b.state(), CircuitState::Closed);
        // Need 3 more failures to open
        b.record_failure();
        b.record_failure();
        assert_eq!(b.state(), CircuitState::Closed);
    }

    // 9. Status snapshot
    #[test]
    fn test_status() {
        let mut b = breaker();
        b.record_success();
        b.record_failure();

        let s = b.status();
        assert_eq!(s.state, CircuitState::Closed);
        assert_eq!(s.total_successes, 1);
        assert_eq!(s.total_failures, 1);
        assert_eq!(s.consecutive_failures, 1);
    }

    // 10. Rejected count
    #[test]
    fn test_rejected_count() {
        let mut b = breaker();
        for _ in 0..3 {
            b.record_failure();
        }
        b.allow_request(); // rejected
        b.allow_request(); // rejected

        let s = b.status();
        assert_eq!(s.total_rejected, 2);
    }

    // 11. Force state
    #[test]
    fn test_force_state() {
        let mut b = breaker();
        b.force_state(CircuitState::Open);
        assert_eq!(b.state(), CircuitState::Open);

        b.force_state(CircuitState::Closed);
        assert_eq!(b.state(), CircuitState::Closed);
        assert_eq!(b.status().consecutive_failures, 0);
    }

    // 12. Reset
    #[test]
    fn test_reset() {
        let mut b = breaker();
        b.record_failure();
        b.record_failure();
        b.record_failure();
        b.reset();

        assert_eq!(b.state(), CircuitState::Closed);
        assert_eq!(b.status().total_failures, 0);
    }

    // 13. Registry auto-registers
    #[test]
    fn test_registry_auto_register() {
        let reg = CircuitBreakerRegistry::default();
        assert!(reg.allow_request("openai"));
        assert_eq!(reg.count(), 1);
    }

    // 14. Registry tracks per-provider
    #[test]
    fn test_registry_per_provider() {
        let reg = CircuitBreakerRegistry::default();
        for _ in 0..5 {
            reg.record_failure("openai");
        }
        // openai should be open, claude should be fine
        assert!(!reg.allow_request("openai"));
        assert!(reg.allow_request("claude"));
    }

    // 15. Registry all statuses
    #[test]
    fn test_registry_all_statuses() {
        let reg = CircuitBreakerRegistry::default();
        reg.allow_request("a");
        reg.allow_request("b");
        let statuses = reg.all_statuses();
        assert_eq!(statuses.len(), 2);
    }

    // 16. Registry status for specific provider
    #[test]
    fn test_registry_status() {
        let reg = CircuitBreakerRegistry::default();
        reg.allow_request("test");
        reg.record_failure("test");
        let s = reg.status("test").unwrap();
        assert_eq!(s.total_failures, 1);
    }

    // 17. Registry missing provider
    #[test]
    fn test_registry_missing() {
        let reg = CircuitBreakerRegistry::default();
        assert!(reg.status("nonexistent").is_none());
    }

    // 18. Status serializable
    #[test]
    fn test_status_serializable() {
        let mut b = breaker();
        b.record_failure();
        let json = serde_json::to_string(&b.status()).unwrap();
        assert!(json.contains("\"state\":\"closed\""));
    }

    // 19. Config defaults
    #[test]
    fn test_config_defaults() {
        let c = CircuitConfig::default();
        assert_eq!(c.failure_threshold, 5);
        assert_eq!(c.success_threshold, 2);
    }

    // 20. Clone registry shares state
    #[test]
    fn test_registry_clone() {
        let r1 = CircuitBreakerRegistry::default();
        let r2 = r1.clone();
        r1.allow_request("shared");
        assert_eq!(r2.count(), 1);
    }

    // 21. Many failures don't underflow
    #[test]
    fn test_many_failures() {
        let mut b = breaker();
        for _ in 0..100 {
            b.record_failure();
        }
        assert_eq!(b.state(), CircuitState::Open);
        assert_eq!(b.status().total_failures, 100);
    }

    // 22. Register custom config in registry
    #[test]
    fn test_registry_custom_config() {
        let reg = CircuitBreakerRegistry::default();
        reg.register("custom", CircuitConfig::new(1));
        reg.record_failure("custom");
        assert!(!reg.allow_request("custom")); // threshold=1
    }
}
