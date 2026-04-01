//! X-RateLimit-* response headers for API consumers.
//!
//! Provides standard rate limit headers so API consumers can track
//! their quota usage and implement client-side throttling.
//!
//! # Main types
//!
//! - [`RateLimitInfo`] — Rate limit state for a request.
//! - [`RateLimitHeaders`] — Converts rate limit info to HTTP headers.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// RateLimitInfo
// ---------------------------------------------------------------------------

/// Rate limit state information for a single request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitInfo {
    /// Maximum number of requests allowed in the window.
    pub limit: u64,
    /// Number of remaining requests in the current window.
    pub remaining: u64,
    /// Unix timestamp when the rate limit window resets.
    pub reset_at: u64,
    /// Window duration in seconds.
    pub window_seconds: u64,
    /// Whether this request was rate-limited.
    pub is_limited: bool,
    /// Optional retry-after value in seconds.
    pub retry_after: Option<u64>,
}

impl RateLimitInfo {
    /// Create a new rate limit info for a successful (non-limited) request.
    pub fn allowed(limit: u64, remaining: u64, reset_at: u64, window_seconds: u64) -> Self {
        Self {
            limit,
            remaining,
            reset_at,
            window_seconds,
            is_limited: false,
            retry_after: None,
        }
    }

    /// Create rate limit info for a rate-limited request.
    pub fn limited(limit: u64, reset_at: u64, window_seconds: u64, retry_after: u64) -> Self {
        Self {
            limit,
            remaining: 0,
            reset_at,
            window_seconds,
            is_limited: true,
            retry_after: Some(retry_after),
        }
    }

    /// Calculate the utilization percentage (0.0 to 100.0).
    pub fn utilization_percent(&self) -> f64 {
        if self.limit == 0 {
            return 0.0;
        }
        let used = self.limit.saturating_sub(self.remaining);
        (used as f64 / self.limit as f64) * 100.0
    }
}

// ---------------------------------------------------------------------------
// RateLimitHeaders
// ---------------------------------------------------------------------------

/// Converts rate limit info to standard HTTP response headers.
///
/// Supports both the draft IETF standard (`RateLimit-*`) and the
/// widely-used `X-RateLimit-*` convention.
pub struct RateLimitHeaders;

/// Header name constants.
pub const HEADER_LIMIT: &str = "X-RateLimit-Limit";
/// Remaining requests header.
pub const HEADER_REMAINING: &str = "X-RateLimit-Remaining";
/// Reset timestamp header.
pub const HEADER_RESET: &str = "X-RateLimit-Reset";
/// Retry-After header (standard HTTP).
pub const HEADER_RETRY_AFTER: &str = "Retry-After";
/// IETF draft RateLimit header.
pub const HEADER_RATELIMIT: &str = "RateLimit";
/// IETF draft RateLimit-Policy header.
pub const HEADER_RATELIMIT_POLICY: &str = "RateLimit-Policy";

impl RateLimitHeaders {
    /// Convert rate limit info to a map of HTTP header name/value pairs.
    pub fn to_headers(info: &RateLimitInfo) -> HashMap<String, String> {
        let mut headers = HashMap::new();

        // X-RateLimit-* headers (widely supported)
        headers.insert(HEADER_LIMIT.to_string(), info.limit.to_string());
        headers.insert(HEADER_REMAINING.to_string(), info.remaining.to_string());
        headers.insert(HEADER_RESET.to_string(), info.reset_at.to_string());

        // IETF draft RateLimit header
        // Format: limit=N, remaining=N, reset=N
        headers.insert(
            HEADER_RATELIMIT.to_string(),
            format!(
                "limit={}, remaining={}, reset={}",
                info.limit, info.remaining, info.reset_at
            ),
        );

        // IETF draft RateLimit-Policy
        headers.insert(
            HEADER_RATELIMIT_POLICY.to_string(),
            format!("{};w={}", info.limit, info.window_seconds),
        );

        // Retry-After (only when limited)
        if let Some(retry) = info.retry_after {
            headers.insert(HEADER_RETRY_AFTER.to_string(), retry.to_string());
        }

        headers
    }

    /// Parse rate limit info from HTTP response headers.
    pub fn from_headers(headers: &HashMap<String, String>) -> Option<RateLimitInfo> {
        let limit = headers.get(HEADER_LIMIT)?.parse::<u64>().ok()?;
        let remaining = headers.get(HEADER_REMAINING)?.parse::<u64>().ok()?;
        let reset_at = headers.get(HEADER_RESET)?.parse::<u64>().ok()?;

        let retry_after = headers
            .get(HEADER_RETRY_AFTER)
            .and_then(|v| v.parse::<u64>().ok());

        let window_seconds = headers
            .get(HEADER_RATELIMIT_POLICY)
            .and_then(|v| v.split(";w=").nth(1).and_then(|w| w.parse::<u64>().ok()))
            .unwrap_or(60);

        Some(RateLimitInfo {
            limit,
            remaining,
            reset_at,
            window_seconds,
            is_limited: remaining == 0 && retry_after.is_some(),
            retry_after,
        })
    }

    /// Return the HTTP status code for a rate-limited response.
    pub fn status_code(info: &RateLimitInfo) -> u16 {
        if info.is_limited {
            429
        } else {
            200
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // 1. Allowed request headers
    #[test]
    fn test_allowed_headers() {
        let info = RateLimitInfo::allowed(100, 95, 1700000000, 60);
        let headers = RateLimitHeaders::to_headers(&info);
        assert_eq!(headers.get(HEADER_LIMIT).unwrap(), "100");
        assert_eq!(headers.get(HEADER_REMAINING).unwrap(), "95");
        assert_eq!(headers.get(HEADER_RESET).unwrap(), "1700000000");
        assert!(!headers.contains_key(HEADER_RETRY_AFTER));
    }

    // 2. Limited request headers
    #[test]
    fn test_limited_headers() {
        let info = RateLimitInfo::limited(100, 1700000060, 60, 30);
        let headers = RateLimitHeaders::to_headers(&info);
        assert_eq!(headers.get(HEADER_REMAINING).unwrap(), "0");
        assert_eq!(headers.get(HEADER_RETRY_AFTER).unwrap(), "30");
    }

    // 3. Status code
    #[test]
    fn test_status_code() {
        let allowed = RateLimitInfo::allowed(100, 50, 0, 60);
        assert_eq!(RateLimitHeaders::status_code(&allowed), 200);

        let limited = RateLimitInfo::limited(100, 0, 60, 30);
        assert_eq!(RateLimitHeaders::status_code(&limited), 429);
    }

    // 4. Round-trip headers
    #[test]
    fn test_roundtrip() {
        let info = RateLimitInfo::allowed(1000, 999, 1700000060, 60);
        let headers = RateLimitHeaders::to_headers(&info);
        let parsed = RateLimitHeaders::from_headers(&headers).unwrap();
        assert_eq!(parsed.limit, 1000);
        assert_eq!(parsed.remaining, 999);
        assert_eq!(parsed.reset_at, 1700000060);
    }

    // 5. Parse missing headers returns None
    #[test]
    fn test_parse_missing() {
        let headers = HashMap::new();
        assert!(RateLimitHeaders::from_headers(&headers).is_none());
    }

    // 6. Utilization percentage
    #[test]
    fn test_utilization() {
        let info = RateLimitInfo::allowed(100, 75, 0, 60);
        assert!((info.utilization_percent() - 25.0).abs() < 0.01);
    }

    // 7. Utilization at 100%
    #[test]
    fn test_utilization_full() {
        let info = RateLimitInfo::allowed(100, 0, 0, 60);
        assert!((info.utilization_percent() - 100.0).abs() < 0.01);
    }

    // 8. Utilization zero limit
    #[test]
    fn test_utilization_zero_limit() {
        let info = RateLimitInfo::allowed(0, 0, 0, 60);
        assert_eq!(info.utilization_percent(), 0.0);
    }

    // 9. IETF RateLimit header format
    #[test]
    fn test_ietf_ratelimit_header() {
        let info = RateLimitInfo::allowed(100, 50, 1700000000, 60);
        let headers = RateLimitHeaders::to_headers(&info);
        let rl = headers.get(HEADER_RATELIMIT).unwrap();
        assert!(rl.contains("limit=100"));
        assert!(rl.contains("remaining=50"));
    }

    // 10. IETF RateLimit-Policy header
    #[test]
    fn test_ietf_policy_header() {
        let info = RateLimitInfo::allowed(100, 50, 0, 300);
        let headers = RateLimitHeaders::to_headers(&info);
        let policy = headers.get(HEADER_RATELIMIT_POLICY).unwrap();
        assert_eq!(policy, "100;w=300");
    }

    // 11. Parse window from policy
    #[test]
    fn test_parse_window() {
        let info = RateLimitInfo::allowed(100, 50, 1000, 120);
        let headers = RateLimitHeaders::to_headers(&info);
        let parsed = RateLimitHeaders::from_headers(&headers).unwrap();
        assert_eq!(parsed.window_seconds, 120);
    }

    // 12. RateLimitInfo serializable
    #[test]
    fn test_info_serializable() {
        let info = RateLimitInfo::allowed(100, 50, 1000, 60);
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"limit\":100"));
        let restored: RateLimitInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.limit, 100);
    }

    // 13. Limited info fields
    #[test]
    fn test_limited_info_fields() {
        let info = RateLimitInfo::limited(100, 2000, 60, 45);
        assert!(info.is_limited);
        assert_eq!(info.remaining, 0);
        assert_eq!(info.retry_after, Some(45));
    }

    // 14. Parse with retry-after
    #[test]
    fn test_parse_with_retry() {
        let info = RateLimitInfo::limited(100, 2000, 60, 30);
        let headers = RateLimitHeaders::to_headers(&info);
        let parsed = RateLimitHeaders::from_headers(&headers).unwrap();
        assert_eq!(parsed.retry_after, Some(30));
    }
}
