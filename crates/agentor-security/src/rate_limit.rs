use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use uuid::Uuid;

struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

/// Token bucket rate limiter per session.
pub struct RateLimiter {
    max_tokens: f64,
    refill_rate: f64, // tokens per second
    buckets: Mutex<HashMap<Uuid, Bucket>>,
}

impl RateLimiter {
    /// Create a new rate limiter.
    /// - `max_tokens`: maximum burst size
    /// - `refill_rate`: tokens added per second
    pub fn new(max_tokens: f64, refill_rate: f64) -> Self {
        Self {
            max_tokens,
            refill_rate,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Try to consume one token for the given session.
    /// Returns `true` if allowed, `false` if rate limited.
    pub async fn check(&self, session_id: Uuid) -> bool {
        let mut buckets = self.buckets.lock().await;
        let now = Instant::now();

        let bucket = buckets.entry(session_id).or_insert(Bucket {
            tokens: self.max_tokens,
            last_refill: now,
        });

        // Refill tokens based on elapsed time
        let elapsed = now.duration_since(bucket.last_refill);
        bucket.tokens =
            (bucket.tokens + elapsed.as_secs_f64() * self.refill_rate).min(self.max_tokens);
        bucket.last_refill = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Remove expired buckets (no activity for the given duration).
    pub async fn cleanup(&self, max_idle: Duration) {
        let mut buckets = self.buckets.lock().await;
        let now = Instant::now();
        buckets.retain(|_, b| now.duration_since(b.last_refill) < max_idle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_allows() {
        let limiter = RateLimiter::new(5.0, 1.0);
        let session = Uuid::new_v4();
        // Should allow first 5 requests
        for _ in 0..5 {
            assert!(limiter.check(session).await);
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_blocks() {
        let limiter = RateLimiter::new(2.0, 0.1);
        let session = Uuid::new_v4();
        assert!(limiter.check(session).await);
        assert!(limiter.check(session).await);
        // Third should be blocked (not enough tokens refilled)
        assert!(!limiter.check(session).await);
    }
}
