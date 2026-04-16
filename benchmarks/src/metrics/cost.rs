//! Cost computation from token counts and model pricing.
//!
//! Pricing snapshots (per 1M tokens, USD) — April 2026:
//!
//! | Model | Input | Output |
//! |-------|-------|--------|
//! | claude-sonnet-4 | $3.00 | $15.00 |
//! | claude-haiku-4-5 | $1.00 | $5.00 |
//! | claude-opus-4-6 | $15.00 | $75.00 |
//! | gpt-4o | $2.50 | $10.00 |
//! | gpt-4o-mini | $0.15 | $0.60 |
//! | gemini-2.0-flash | $0.10 | $0.40 |
//! | mock | $0.00 | $0.00 |

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostMetric {
    pub input_usd: f64,
    pub output_usd: f64,
    pub total_usd: f64,
    pub model: String,
}

pub fn compute(model: &str, input_tokens: u64, output_tokens: u64) -> CostMetric {
    let (in_rate, out_rate) = pricing(model);
    let input_usd = (input_tokens as f64) * in_rate / 1_000_000.0;
    let output_usd = (output_tokens as f64) * out_rate / 1_000_000.0;
    CostMetric {
        input_usd,
        output_usd,
        total_usd: input_usd + output_usd,
        model: model.to_string(),
    }
}

/// Returns (input_rate_per_million, output_rate_per_million) in USD.
fn pricing(model: &str) -> (f64, f64) {
    let m = model.to_lowercase();
    // Match-by-prefix to tolerate version suffixes
    if m.starts_with("claude-sonnet") {
        (3.00, 15.00)
    } else if m.starts_with("claude-haiku") {
        (1.00, 5.00)
    } else if m.starts_with("claude-opus") {
        (15.00, 75.00)
    } else if m.starts_with("gpt-4o-mini") {
        (0.15, 0.60)
    } else if m.starts_with("gpt-4o") || m.starts_with("gpt-4") {
        (2.50, 10.00)
    } else if m.starts_with("gemini-2") {
        (0.10, 0.40)
    } else {
        // Unknown model → zero cost (mock, test, or new)
        (0.0, 0.0)
    }
}

/// Project a per-task cost to a daily spend given requests/day.
pub fn project_daily(cost_per_task: f64, requests_per_day: u64) -> f64 {
    cost_per_task * requests_per_day as f64
}

/// Project a per-task cost to a monthly spend (30 days/month convention).
pub fn project_monthly(cost_per_task: f64, requests_per_day: u64) -> f64 {
    project_daily(cost_per_task, requests_per_day) * 30.0
}

/// Project a per-task cost to an annual spend (365 days).
pub fn project_annual(cost_per_task: f64, requests_per_day: u64) -> f64 {
    project_daily(cost_per_task, requests_per_day) * 365.0
}

/// Predefined workload scales (requests per day).
#[derive(Debug, Clone, Copy)]
pub enum Scale {
    /// Hobby / small product — 1K req/day.
    Small,
    /// Mid-size SaaS — 100K req/day.
    Mid,
    /// Large product — 1M req/day.
    Large,
    /// Enterprise — 100M req/day.
    Enterprise,
}

impl Scale {
    pub fn requests_per_day(self) -> u64 {
        match self {
            Scale::Small => 1_000,
            Scale::Mid => 100_000,
            Scale::Large => 1_000_000,
            Scale::Enterprise => 100_000_000,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Scale::Small => "small (1K/day)",
            Scale::Mid => "mid (100K/day)",
            Scale::Large => "large (1M/day)",
            Scale::Enterprise => "enterprise (100M/day)",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "small" => Some(Scale::Small),
            "mid" => Some(Scale::Mid),
            "large" => Some(Scale::Large),
            "enterprise" => Some(Scale::Enterprise),
            _ => None,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn sonnet_cost_correct() {
        let c = compute("claude-sonnet-4", 1_000_000, 500_000);
        assert!((c.input_usd - 3.00).abs() < 0.001);
        assert!((c.output_usd - 7.50).abs() < 0.001);
        assert!((c.total_usd - 10.50).abs() < 0.001);
    }

    #[test]
    fn mock_cost_zero() {
        let c = compute("mock", 1000, 500);
        assert_eq!(c.total_usd, 0.0);
    }

    #[test]
    fn unknown_model_zero_cost() {
        let c = compute("some-future-model", 1000, 500);
        assert_eq!(c.total_usd, 0.0);
    }

    #[test]
    fn haiku_cheaper_than_sonnet() {
        let s = compute("claude-sonnet-4", 100_000, 100_000);
        let h = compute("claude-haiku-4-5", 100_000, 100_000);
        assert!(h.total_usd < s.total_usd);
    }

    #[test]
    fn gpt4o_mini_cheapest_openai() {
        let mini = compute("gpt-4o-mini", 100_000, 100_000);
        let full = compute("gpt-4o", 100_000, 100_000);
        assert!(mini.total_usd < full.total_usd);
    }

    #[test]
    fn prefix_matching_tolerates_versions() {
        let a = compute("claude-sonnet-4-20250514", 1_000_000, 1_000_000);
        let b = compute("claude-sonnet-4", 1_000_000, 1_000_000);
        assert_eq!(a.total_usd, b.total_usd);
    }

    #[test]
    fn daily_projection_scales_linearly() {
        assert_eq!(project_daily(0.001, 100_000), 100.0);
    }

    #[test]
    fn monthly_is_30x_daily() {
        let d = project_daily(0.001, 100_000);
        let m = project_monthly(0.001, 100_000);
        assert!((m - d * 30.0).abs() < 1e-9);
    }

    #[test]
    fn annual_is_365x_daily() {
        let d = project_daily(0.001, 100_000);
        let a = project_annual(0.001, 100_000);
        assert!((a - d * 365.0).abs() < 1e-9);
    }

    #[test]
    fn scale_parse_round_trip() {
        assert!(matches!(Scale::parse("small"), Some(Scale::Small)));
        assert!(matches!(Scale::parse("MID"), Some(Scale::Mid)));
        assert!(matches!(Scale::parse("large"), Some(Scale::Large)));
        assert!(matches!(
            Scale::parse("Enterprise"),
            Some(Scale::Enterprise)
        ));
        assert!(Scale::parse("foo").is_none());
    }

    #[test]
    fn scale_rpd_correct() {
        assert_eq!(Scale::Small.requests_per_day(), 1_000);
        assert_eq!(Scale::Mid.requests_per_day(), 100_000);
        assert_eq!(Scale::Large.requests_per_day(), 1_000_000);
        assert_eq!(Scale::Enterprise.requests_per_day(), 100_000_000);
    }
}
