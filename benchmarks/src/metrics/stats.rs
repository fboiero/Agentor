//! Statistical aggregation over N samples.
//!
//! Computes mean/median/stddev/p95/p99 and runs paired t-tests for cross-runner
//! comparisons. Uses `statrs` for statistical distributions.

use serde::{Deserialize, Serialize};
use statrs::distribution::{ContinuousCDF, StudentsT};

/// Summary stats for a single metric (latency, cost, quality, ...).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub n: usize,
    pub mean: f64,
    pub median: f64,
    pub stddev: f64,
    pub min: f64,
    pub max: f64,
    pub p95: f64,
    pub p99: f64,
}

impl Stats {
    pub fn from_samples(samples: &[f64]) -> Self {
        if samples.is_empty() {
            return Self::empty();
        }
        let n = samples.len();
        let mean: f64 = samples.iter().sum::<f64>() / n as f64;
        let variance: f64 =
            samples.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n.max(1) as f64;
        let stddev = variance.sqrt();

        let mut sorted = samples.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = if n % 2 == 1 {
            sorted[n / 2]
        } else {
            (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
        };
        let p95 = percentile(&sorted, 0.95);
        let p99 = percentile(&sorted, 0.99);

        Self {
            n,
            mean,
            median,
            stddev,
            min: sorted[0],
            max: sorted[n - 1],
            p95,
            p99,
        }
    }

    fn empty() -> Self {
        Self {
            n: 0,
            mean: 0.0,
            median: 0.0,
            stddev: 0.0,
            min: 0.0,
            max: 0.0,
            p95: 0.0,
            p99: 0.0,
        }
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = (p * (sorted.len() - 1) as f64).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}

/// Paired t-test: given N paired samples `(a_i, b_i)`, tests the null
/// hypothesis that the mean difference is zero.
///
/// Returns `p-value`. A p-value < 0.05 typically means the difference is
/// statistically significant (reject null hypothesis at 95% confidence).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedTTest {
    pub n: usize,
    pub mean_diff: f64,
    pub stddev_diff: f64,
    pub t_statistic: f64,
    pub df: usize,
    pub p_value: f64,
    /// Cohen's d effect size: (mean_diff) / (stddev_diff).
    /// Thresholds: 0.2 = small, 0.5 = medium, 0.8 = large.
    pub effect_size: f64,
}

impl PairedTTest {
    /// Compute paired t-test for samples a and b.
    /// `a` and `b` must have the same length; each index is one pair.
    pub fn compute(a: &[f64], b: &[f64]) -> Option<Self> {
        if a.len() != b.len() || a.len() < 2 {
            return None;
        }
        let n = a.len();
        let diffs: Vec<f64> = a.iter().zip(b.iter()).map(|(x, y)| x - y).collect();
        let mean_diff: f64 = diffs.iter().sum::<f64>() / n as f64;
        let variance: f64 = diffs
            .iter()
            .map(|d| (d - mean_diff).powi(2))
            .sum::<f64>()
            / (n - 1) as f64;
        let stddev_diff = variance.sqrt();

        if stddev_diff == 0.0 {
            // No variation; if mean_diff is also 0, p=1 (accept null). Else undefined.
            return Some(PairedTTest {
                n,
                mean_diff,
                stddev_diff,
                t_statistic: 0.0,
                df: n - 1,
                p_value: 1.0,
                effect_size: 0.0,
            });
        }

        let se = stddev_diff / (n as f64).sqrt();
        let t = mean_diff / se;
        let df = (n - 1) as f64;

        // Two-tailed p-value from Student's t-distribution
        let t_dist = StudentsT::new(0.0, 1.0, df).ok()?;
        let p_value = 2.0 * (1.0 - t_dist.cdf(t.abs()));

        let effect_size = mean_diff / stddev_diff;

        Some(PairedTTest {
            n,
            mean_diff,
            stddev_diff,
            t_statistic: t,
            df: df as usize,
            p_value,
            effect_size,
        })
    }

    /// Interpret the p-value at the 0.05 significance threshold.
    pub fn is_significant(&self) -> bool {
        self.p_value < 0.05
    }

    /// Qualitative label for the effect size.
    pub fn effect_label(&self) -> &'static str {
        let abs = self.effect_size.abs();
        if abs < 0.2 {
            "negligible"
        } else if abs < 0.5 {
            "small"
        } else if abs < 0.8 {
            "medium"
        } else {
            "large"
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn stats_from_samples_basic() {
        let s = Stats::from_samples(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(s.n, 5);
        assert!((s.mean - 3.0).abs() < 1e-9);
        assert!((s.median - 3.0).abs() < 1e-9);
        assert!((s.min - 1.0).abs() < 1e-9);
        assert!((s.max - 5.0).abs() < 1e-9);
    }

    #[test]
    fn stats_empty_samples() {
        let s = Stats::from_samples(&[]);
        assert_eq!(s.n, 0);
        assert_eq!(s.mean, 0.0);
    }

    #[test]
    fn stats_median_even_count() {
        let s = Stats::from_samples(&[1.0, 2.0, 3.0, 4.0]);
        assert!((s.median - 2.5).abs() < 1e-9);
    }

    #[test]
    fn paired_t_test_identical_samples() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = a.clone();
        let t = PairedTTest::compute(&a, &b).unwrap();
        assert!((t.mean_diff).abs() < 1e-9);
        assert_eq!(t.p_value, 1.0);
        assert!(!t.is_significant());
    }

    #[test]
    fn paired_t_test_clearly_different() {
        // Independent variation in each series so stddev_diff > 0 and the
        // test can actually compute a p-value (not the degenerate case).
        let a: Vec<f64> = vec![50.0, 51.0, 49.0, 50.5, 50.2, 49.8, 51.2, 50.1, 49.9, 50.3];
        let b: Vec<f64> = vec![70.5, 71.2, 69.8, 70.1, 71.0, 69.5, 70.8, 70.3, 71.1, 69.7];
        let t = PairedTTest::compute(&a, &b).unwrap();
        assert!(t.mean_diff < -18.0 && t.mean_diff > -22.0);
        assert!(t.is_significant(), "p_value was {}", t.p_value);
        assert_eq!(t.effect_label(), "large");
    }

    #[test]
    fn paired_t_test_rejects_mismatched_lengths() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        assert!(PairedTTest::compute(&a, &b).is_none());
    }

    #[test]
    fn paired_t_test_rejects_too_few_samples() {
        let a = vec![1.0];
        let b = vec![2.0];
        assert!(PairedTTest::compute(&a, &b).is_none());
    }

    #[test]
    fn effect_label_thresholds() {
        for (d, want) in [
            (0.1, "negligible"),
            (0.3, "small"),
            (0.6, "medium"),
            (1.0, "large"),
            (-1.0, "large"), // abs
        ] {
            let t = PairedTTest {
                n: 10,
                mean_diff: d,
                stddev_diff: 1.0,
                t_statistic: d,
                df: 9,
                p_value: 0.5,
                effect_size: d,
            };
            assert_eq!(t.effect_label(), want, "d={d}");
        }
    }

    #[test]
    fn percentile_helpers() {
        let sorted: Vec<f64> = (0..100).map(|i| i as f64).collect();
        assert!((percentile(&sorted, 0.50) - 50.0).abs() < 1.0);
        assert!((percentile(&sorted, 0.95) - 94.0).abs() < 1.0);
        assert!((percentile(&sorted, 0.99) - 98.0).abs() < 1.0);
    }
}
