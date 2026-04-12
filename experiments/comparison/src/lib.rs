//! Comparison experiment harness for measuring Argentor against published competitor data.
//!
//! Each scenario produces a `Measurement` which can be serialized to JSON and aggregated
//! into a comparison report.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// A single measurement from a benchmark scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Measurement {
    pub scenario: String,
    pub metric: String,
    pub value: f64,
    pub unit: String,
    pub samples: usize,
    pub min: f64,
    pub max: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
}

/// Aggregate a set of duration samples into a Measurement (in milliseconds).
pub fn measurement_from_durations(
    scenario: &str,
    metric: &str,
    samples: &[Duration],
) -> Measurement {
    let mut values: Vec<f64> = samples.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let n = values.len();
    let sum: f64 = values.iter().sum();
    let mean = sum / n as f64;

    Measurement {
        scenario: scenario.to_string(),
        metric: metric.to_string(),
        value: mean,
        unit: "ms".to_string(),
        samples: n,
        min: values.first().copied().unwrap_or(0.0),
        max: values.last().copied().unwrap_or(0.0),
        p50: values[n / 2],
        p95: values[(n * 95) / 100],
        p99: values[(n * 99) / 100],
    }
}

/// Print a measurement as a human-readable table row.
pub fn print_measurement(m: &Measurement) {
    println!(
        "  {:<35} {:>10.3} {} (min={:.3}, p50={:.3}, p95={:.3}, p99={:.3}, max={:.3}, n={})",
        m.metric, m.value, m.unit, m.min, m.p50, m.p95, m.p99, m.max, m.samples,
    );
}

/// Print a section header.
pub fn print_header(title: &str) {
    println!();
    println!("==================================================================");
    println!("  {title}");
    println!("==================================================================");
}

/// Memory measurement utilities.
pub mod memory {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

    pub fn current_rss_kb() -> u64 {
        let mut sys = System::new();
        let pid = Pid::from_u32(std::process::id());
        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::everything(),
        );
        sys.process(pid).map(|p| p.memory() / 1024).unwrap_or(0)
    }
}
