//! Benchmark report rendering — Markdown and JSON outputs.

use crate::metrics::TaskMetrics;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReport {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub runs: Vec<TaskMetrics>,
}

impl RunReport {
    pub fn new(runs: Vec<TaskMetrics>) -> Self {
        Self {
            timestamp: chrono::Utc::now(),
            runs,
        }
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "# Benchmark Report — {}\n\n",
            self.timestamp.format("%Y-%m-%d %H:%M UTC")
        ));

        out.push_str("| Task | Runner | OK | Latency (ms) | Cost (USD) | Quality | Passed |\n");
        out.push_str("|------|--------|----|--------------|-----------|---------|--------|\n");
        for r in &self.runs {
            out.push_str(&format!(
                "| `{}` | {} | {} | {} | ${:.6} | {:.2} | {} |\n",
                r.task_id,
                r.runner,
                if r.succeeded { "✓" } else { "✗" },
                r.latency.wall_ms,
                r.cost.total_usd,
                r.quality.aggregate_score,
                if r.passed_rubric { "✓" } else { "✗" },
            ));
        }
        out
    }

    pub fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}
