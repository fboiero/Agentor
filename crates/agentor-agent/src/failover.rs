use crate::backends::LlmBackend;
use crate::llm::LlmResponse;
use crate::stream::StreamEvent;
use agentor_core::{AgentorError, AgentorResult, Message};
use agentor_skills::SkillDescriptor;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{info, warn};

/// Type alias for the injectable sleep function used in tests.
#[cfg(test)]
type SleepFn = Box<
    dyn Fn(u64) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Send + Sync,
>;

/// Configures retry behaviour for failover across LLM backends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of retries per backend before moving to the next one.
    pub max_retries: u32,
    /// Base delay in milliseconds for exponential backoff.
    pub backoff_base_ms: u64,
    /// Maximum delay in milliseconds (cap for exponential backoff).
    pub backoff_max_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            backoff_base_ms: 500,
            backoff_max_ms: 30_000,
        }
    }
}

/// Determines whether an error is transient and worth retrying.
///
/// Returns `true` for rate-limit (429), authentication (401), timeout, and
/// server errors (5xx, 500, 502, 503, 504). Returns `false` for client
/// errors like 400 (bad request) which are not expected to succeed on retry.
pub fn is_retryable(err: &AgentorError) -> bool {
    let msg = err.to_string();
    let lower = msg.to_lowercase();

    // Non-retryable patterns checked first
    if lower.contains("400") {
        return false;
    }

    // Retryable patterns
    lower.contains("429")
        || lower.contains("401")
        || lower.contains("timeout")
        || lower.contains("5xx")
        || lower.contains("500")
        || lower.contains("502")
        || lower.contains("503")
        || lower.contains("504")
}

/// Computes the backoff delay for a given attempt using exponential backoff
/// capped at `backoff_max_ms`.
fn compute_backoff(policy: &RetryPolicy, attempt: u32) -> u64 {
    let delay = policy.backoff_base_ms.saturating_mul(2u64.saturating_pow(attempt));
    delay.min(policy.backoff_max_ms)
}

/// An `LlmBackend` implementation that wraps multiple backends and performs
/// automatic failover with exponential-backoff retries.
///
/// For each request it tries backends in order. Within each backend it retries
/// up to `max_retries` times for transient (retryable) errors. If all retries
/// on a backend are exhausted, or a non-retryable error is encountered, it
/// moves to the next backend. If every backend fails, the last error is
/// returned.
pub struct FailoverBackend {
    backends: Vec<Box<dyn LlmBackend>>,
    policy: RetryPolicy,
    /// Injectable sleep function for testing (allows skipping real delays).
    #[cfg(test)]
    sleep_fn: Option<SleepFn>,
}

impl FailoverBackend {
    /// Create a new failover backend with the given backends and retry policy.
    ///
    /// # Panics
    /// Panics if `backends` is empty.
    pub fn new(backends: Vec<Box<dyn LlmBackend>>, policy: RetryPolicy) -> Self {
        assert!(!backends.is_empty(), "FailoverBackend requires at least one backend");
        Self {
            backends,
            policy,
            #[cfg(test)]
            sleep_fn: None,
        }
    }

    /// Perform a sleep for the given duration in milliseconds.
    async fn do_sleep(&self, ms: u64) {
        #[cfg(test)]
        if let Some(ref f) = self.sleep_fn {
            f(ms).await;
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
    }
}

#[async_trait]
impl LlmBackend for FailoverBackend {
    async fn chat(
        &self,
        system_prompt: Option<&str>,
        messages: &[Message],
        tools: &[SkillDescriptor],
    ) -> AgentorResult<LlmResponse> {
        let mut last_err: Option<AgentorError> = None;

        for (backend_idx, backend) in self.backends.iter().enumerate() {
            for attempt in 0..=self.policy.max_retries {
                match backend.chat(system_prompt, messages, tools).await {
                    Ok(resp) => return Ok(resp),
                    Err(e) => {
                        if !is_retryable(&e) {
                            warn!(
                                backend = backend_idx,
                                attempt,
                                error = %e,
                                "Non-retryable error, moving to next backend"
                            );
                            last_err = Some(e);
                            break; // move to next backend
                        }

                        if attempt < self.policy.max_retries {
                            let delay = compute_backoff(&self.policy, attempt);
                            info!(
                                backend = backend_idx,
                                attempt,
                                delay_ms = delay,
                                error = %e,
                                "Retryable error, backing off"
                            );
                            self.do_sleep(delay).await;
                        }
                        last_err = Some(e);
                    }
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            AgentorError::Agent("All failover backends exhausted".into())
        }))
    }

    async fn chat_stream(
        &self,
        system_prompt: Option<&str>,
        messages: &[Message],
        tools: &[SkillDescriptor],
    ) -> AgentorResult<(
        mpsc::Receiver<StreamEvent>,
        JoinHandle<AgentorResult<LlmResponse>>,
    )> {
        let mut last_err: Option<AgentorError> = None;

        for (backend_idx, backend) in self.backends.iter().enumerate() {
            for attempt in 0..=self.policy.max_retries {
                match backend.chat_stream(system_prompt, messages, tools).await {
                    Ok(result) => return Ok(result),
                    Err(e) => {
                        if !is_retryable(&e) {
                            warn!(
                                backend = backend_idx,
                                attempt,
                                error = %e,
                                "Non-retryable stream error, moving to next backend"
                            );
                            last_err = Some(e);
                            break; // move to next backend
                        }

                        if attempt < self.policy.max_retries {
                            let delay = compute_backoff(&self.policy, attempt);
                            info!(
                                backend = backend_idx,
                                attempt,
                                delay_ms = delay,
                                error = %e,
                                "Retryable stream error, backing off"
                            );
                            self.do_sleep(delay).await;
                        }
                        last_err = Some(e);
                    }
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            AgentorError::Agent("All failover backends exhausted (stream)".into())
        }))
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    /// A mock backend that returns a sequence of results.
    struct MockBackend {
        /// Results to return in order; pops from front on each call.
        results: tokio::sync::Mutex<Vec<Result<LlmResponse, AgentorError>>>,
        call_count: AtomicU32,
    }

    impl MockBackend {
        fn new(results: Vec<Result<LlmResponse, AgentorError>>) -> Self {
            Self {
                results: tokio::sync::Mutex::new(results),
                call_count: AtomicU32::new(0),
            }
        }

        #[allow(dead_code)]
        fn calls(&self) -> u32 {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl LlmBackend for MockBackend {
        async fn chat(
            &self,
            _system_prompt: Option<&str>,
            _messages: &[Message],
            _tools: &[SkillDescriptor],
        ) -> AgentorResult<LlmResponse> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            let mut results = self.results.lock().await;
            if results.is_empty() {
                Err(AgentorError::Agent("MockBackend: no more results".into()))
            } else {
                results.remove(0)
            }
        }

        async fn chat_stream(
            &self,
            _system_prompt: Option<&str>,
            _messages: &[Message],
            _tools: &[SkillDescriptor],
        ) -> AgentorResult<(
            mpsc::Receiver<StreamEvent>,
            JoinHandle<AgentorResult<LlmResponse>>,
        )> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            let mut results = self.results.lock().await;
            if results.is_empty() {
                return Err(AgentorError::Agent("MockBackend: no more results".into()));
            }
            match results.remove(0) {
                Ok(resp) => {
                    let (tx, rx) = mpsc::channel(1);
                    let handle = tokio::spawn(async move {
                        drop(tx);
                        Ok(resp)
                    });
                    Ok((rx, handle))
                }
                Err(e) => Err(e),
            }
        }
    }

    fn instant_policy() -> RetryPolicy {
        RetryPolicy {
            max_retries: 3,
            backoff_base_ms: 0,
            backoff_max_ms: 0,
        }
    }

    // ── Test 1: retry succeeds on second attempt ─────────────────────────

    #[tokio::test]
    async fn retry_succeeds_on_second_try() {
        let backend = Arc::new(MockBackend::new(vec![
            Err(AgentorError::Http("429 Too Many Requests".into())),
            Ok(LlmResponse::Text("ok".into())),
        ]));

        let failover = FailoverBackend {
            backends: vec![Box::new(MockBackend::new(vec![
                Err(AgentorError::Http("429 Too Many Requests".into())),
                Ok(LlmResponse::Text("ok".into())),
            ]))],
            policy: instant_policy(),
            sleep_fn: Some(Box::new(|_| Box::pin(async {}))),
        };

        let result = failover.chat(None, &[], &[]).await;
        assert!(result.is_ok());
        match result.unwrap() {
            LlmResponse::Text(t) => assert_eq!(t, "ok"),
            other => panic!("Expected Text, got {other:?}"),
        }
        let _ = backend;
    }

    // ── Test 2: all backends fail, returns last error ────────────────────

    #[tokio::test]
    async fn all_backends_fail_returns_last_error() {
        let failover = FailoverBackend {
            backends: vec![
                Box::new(MockBackend::new(vec![
                    Err(AgentorError::Http("500 Internal Server Error".into())),
                    Err(AgentorError::Http("500 Internal Server Error".into())),
                    Err(AgentorError::Http("500 Internal Server Error".into())),
                    Err(AgentorError::Http("500 Internal Server Error".into())),
                ])),
                Box::new(MockBackend::new(vec![
                    Err(AgentorError::Http("503 Service Unavailable".into())),
                    Err(AgentorError::Http("503 Service Unavailable".into())),
                    Err(AgentorError::Http("503 Service Unavailable".into())),
                    Err(AgentorError::Http("503 Service Unavailable".into())),
                ])),
            ],
            policy: instant_policy(),
            sleep_fn: Some(Box::new(|_| Box::pin(async {}))),
        };

        let result = failover.chat(None, &[], &[]).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("503"), "Expected last error (503), got: {err_msg}");
    }

    // ── Test 3: non-retryable error skips retries immediately ────────────

    #[tokio::test]
    async fn non_retryable_skips_immediately() {
        let b1 = Arc::new(MockBackend::new(vec![
            Err(AgentorError::Http("400 Bad Request".into())),
            // These should never be reached:
            Ok(LlmResponse::Text("should not reach".into())),
        ]));

        let failover = FailoverBackend {
            backends: vec![
                Box::new(MockBackend::new(vec![
                    Err(AgentorError::Http("400 Bad Request".into())),
                    Ok(LlmResponse::Text("should not reach".into())),
                ])),
                Box::new(MockBackend::new(vec![
                    Ok(LlmResponse::Text("fallback ok".into())),
                ])),
            ],
            policy: instant_policy(),
            sleep_fn: Some(Box::new(|_| Box::pin(async {}))),
        };

        let result = failover.chat(None, &[], &[]).await;
        assert!(result.is_ok());
        match result.unwrap() {
            LlmResponse::Text(t) => assert_eq!(t, "fallback ok"),
            other => panic!("Expected Text, got {other:?}"),
        }
        let _ = b1;
    }

    // ── Test 4: backoff timing computation ───────────────────────────────

    #[test]
    fn backoff_computation() {
        let policy = RetryPolicy {
            max_retries: 5,
            backoff_base_ms: 500,
            backoff_max_ms: 30_000,
        };

        assert_eq!(compute_backoff(&policy, 0), 500);   // 500 * 2^0 = 500
        assert_eq!(compute_backoff(&policy, 1), 1000);  // 500 * 2^1 = 1000
        assert_eq!(compute_backoff(&policy, 2), 2000);  // 500 * 2^2 = 2000
        assert_eq!(compute_backoff(&policy, 3), 4000);  // 500 * 2^3 = 4000
        assert_eq!(compute_backoff(&policy, 4), 8000);  // 500 * 2^4 = 8000
        assert_eq!(compute_backoff(&policy, 5), 16000); // 500 * 2^5 = 16000
        assert_eq!(compute_backoff(&policy, 6), 30_000); // capped at max
    }

    // ── Test 5: is_retryable classification ──────────────────────────────

    #[test]
    fn is_retryable_classification() {
        // Retryable
        assert!(is_retryable(&AgentorError::Http("429 Too Many Requests".into())));
        assert!(is_retryable(&AgentorError::Http("401 Unauthorized".into())));
        assert!(is_retryable(&AgentorError::Http("timeout waiting for response".into())));
        assert!(is_retryable(&AgentorError::Http("500 Internal Server Error".into())));
        assert!(is_retryable(&AgentorError::Http("502 Bad Gateway".into())));
        assert!(is_retryable(&AgentorError::Http("503 Service Unavailable".into())));
        assert!(is_retryable(&AgentorError::Http("504 Gateway Timeout".into())));
        assert!(is_retryable(&AgentorError::Agent("5xx class error".into())));

        // Not retryable
        assert!(!is_retryable(&AgentorError::Http("400 Bad Request".into())));
    }

    // ── Test 6: failover to second backend after first exhausts retries ──

    #[tokio::test]
    async fn failover_to_second_backend() {
        let failover = FailoverBackend {
            backends: vec![
                Box::new(MockBackend::new(vec![
                    Err(AgentorError::Http("502 Bad Gateway".into())),
                    Err(AgentorError::Http("502 Bad Gateway".into())),
                    Err(AgentorError::Http("502 Bad Gateway".into())),
                    Err(AgentorError::Http("502 Bad Gateway".into())),
                ])),
                Box::new(MockBackend::new(vec![
                    Ok(LlmResponse::Text("second backend".into())),
                ])),
            ],
            policy: instant_policy(),
            sleep_fn: Some(Box::new(|_| Box::pin(async {}))),
        };

        let result = failover.chat(None, &[], &[]).await;
        assert!(result.is_ok());
        match result.unwrap() {
            LlmResponse::Text(t) => assert_eq!(t, "second backend"),
            other => panic!("Expected Text, got {other:?}"),
        }
    }

    // ── Test 7: streaming retry succeeds on second try ───────────────────

    #[tokio::test]
    async fn stream_retry_succeeds_on_second_try() {
        let failover = FailoverBackend {
            backends: vec![Box::new(MockBackend::new(vec![
                Err(AgentorError::Http("503 Service Unavailable".into())),
                Ok(LlmResponse::Text("stream ok".into())),
            ]))],
            policy: instant_policy(),
            sleep_fn: Some(Box::new(|_| Box::pin(async {}))),
        };

        let result = failover.chat_stream(None, &[], &[]).await;
        assert!(result.is_ok());
        let (_rx, handle) = result.unwrap();
        let final_resp = handle.await.unwrap().unwrap();
        match final_resp {
            LlmResponse::Text(t) => assert_eq!(t, "stream ok"),
            other => panic!("Expected Text, got {other:?}"),
        }
    }
}
