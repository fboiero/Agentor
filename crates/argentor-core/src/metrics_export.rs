//! Multi-format metrics export for monitoring integration.
//!
//! Converts metric snapshots to various formats for consumption by
//! external monitoring systems.
//!
//! # Main types
//!
//! - [`MetricsExporter`] — Exports metrics to JSON, CSV, and OpenMetrics.
//! - [`ExportFormat`] — Supported export formats.
//! - [`MetricPoint`] — A single metric data point.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// ExportFormat
// ---------------------------------------------------------------------------

/// Supported metrics export formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    /// JSON format (human-readable, structured).
    Json,
    /// CSV format (tabular, spreadsheet-compatible).
    Csv,
    /// OpenMetrics text format (Prometheus-compatible).
    OpenMetrics,
    /// Line protocol format (InfluxDB-compatible).
    LineProtocol,
}

// ---------------------------------------------------------------------------
// MetricType
// ---------------------------------------------------------------------------

/// Classification of a metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricType {
    /// A monotonically increasing counter.
    Counter,
    /// A value that can go up or down.
    Gauge,
    /// A distribution of values.
    Histogram,
}

// ---------------------------------------------------------------------------
// MetricPoint
// ---------------------------------------------------------------------------

/// A single metric data point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricPoint {
    /// Metric name (e.g., `argentor_tool_calls_total`).
    pub name: String,
    /// Metric type.
    pub metric_type: MetricType,
    /// Metric value.
    pub value: f64,
    /// Labels/tags attached to this metric.
    pub labels: HashMap<String, String>,
    /// Optional help text.
    pub help: Option<String>,
    /// Timestamp in Unix milliseconds (0 = current time).
    pub timestamp_ms: u64,
}

impl MetricPoint {
    /// Create a new counter metric.
    pub fn counter(name: impl Into<String>, value: f64) -> Self {
        Self {
            name: name.into(),
            metric_type: MetricType::Counter,
            value,
            labels: HashMap::new(),
            help: None,
            timestamp_ms: 0,
        }
    }

    /// Create a new gauge metric.
    pub fn gauge(name: impl Into<String>, value: f64) -> Self {
        Self {
            name: name.into(),
            metric_type: MetricType::Gauge,
            value,
            labels: HashMap::new(),
            help: None,
            timestamp_ms: 0,
        }
    }

    /// Add a label.
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Set help text.
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Set timestamp.
    pub fn with_timestamp(mut self, ts_ms: u64) -> Self {
        self.timestamp_ms = ts_ms;
        self
    }
}

// ---------------------------------------------------------------------------
// MetricsExporter
// ---------------------------------------------------------------------------

/// Exports metric points to various formats.
pub struct MetricsExporter;

impl MetricsExporter {
    /// Export metrics in the specified format.
    pub fn export(metrics: &[MetricPoint], format: ExportFormat) -> String {
        match format {
            ExportFormat::Json => Self::export_json(metrics),
            ExportFormat::Csv => Self::export_csv(metrics),
            ExportFormat::OpenMetrics => Self::export_open_metrics(metrics),
            ExportFormat::LineProtocol => Self::export_line_protocol(metrics),
        }
    }

    /// Export as JSON.
    fn export_json(metrics: &[MetricPoint]) -> String {
        serde_json::to_string_pretty(metrics).unwrap_or_else(|_| "[]".to_string())
    }

    /// Export as CSV.
    fn export_csv(metrics: &[MetricPoint]) -> String {
        let mut out = String::from("name,type,value,labels\n");
        for m in metrics {
            let labels_str = if m.labels.is_empty() {
                String::new()
            } else {
                let pairs: Vec<String> =
                    m.labels.iter().map(|(k, v)| format!("{k}={v}")).collect();
                pairs.join(";")
            };
            let type_str = match m.metric_type {
                MetricType::Counter => "counter",
                MetricType::Gauge => "gauge",
                MetricType::Histogram => "histogram",
            };
            out.push_str(&format!("{},{},{},{}\n", m.name, type_str, m.value, labels_str));
        }
        out
    }

    /// Export in OpenMetrics text format.
    fn export_open_metrics(metrics: &[MetricPoint]) -> String {
        let mut out = String::with_capacity(1024);
        let mut seen_names: HashMap<String, bool> = HashMap::new();

        for m in metrics {
            // Emit HELP and TYPE only once per metric name
            if !seen_names.contains_key(&m.name) {
                if let Some(help) = &m.help {
                    out.push_str(&format!("# HELP {} {}\n", m.name, help));
                }
                let type_str = match m.metric_type {
                    MetricType::Counter => "counter",
                    MetricType::Gauge => "gauge",
                    MetricType::Histogram => "histogram",
                };
                out.push_str(&format!("# TYPE {} {}\n", m.name, type_str));
                seen_names.insert(m.name.clone(), true);
            }

            // Metric line
            if m.labels.is_empty() {
                out.push_str(&format!("{} {}\n", m.name, format_value(m.value)));
            } else {
                let labels_str: Vec<String> = m
                    .labels
                    .iter()
                    .map(|(k, v)| format!("{k}=\"{v}\""))
                    .collect();
                out.push_str(&format!(
                    "{}{{{}}} {}\n",
                    m.name,
                    labels_str.join(","),
                    format_value(m.value)
                ));
            }
        }

        out.push_str("# EOF\n");
        out
    }

    /// Export in InfluxDB line protocol format.
    fn export_line_protocol(metrics: &[MetricPoint]) -> String {
        let mut out = String::with_capacity(1024);

        for m in metrics {
            let mut line = m.name.clone();

            // Tags (labels)
            if !m.labels.is_empty() {
                let mut sorted: Vec<(&String, &String)> = m.labels.iter().collect();
                sorted.sort_by_key(|(k, _)| *k);
                for (k, v) in &sorted {
                    line.push_str(&format!(",{k}={v}"));
                }
            }

            // Field value
            line.push_str(&format!(" value={}", format_value(m.value)));

            // Timestamp
            if m.timestamp_ms > 0 {
                line.push_str(&format!(" {}", m.timestamp_ms * 1_000_000)); // ns
            }

            out.push_str(&line);
            out.push('\n');
        }

        out
    }
}

/// Format a float value, removing unnecessary decimals.
fn format_value(v: f64) -> String {
    if v == v.floor() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn sample_metrics() -> Vec<MetricPoint> {
        vec![
            MetricPoint::counter("requests_total", 42.0)
                .with_label("method", "GET")
                .with_help("Total requests"),
            MetricPoint::gauge("active_connections", 5.0)
                .with_help("Current active connections"),
            MetricPoint::counter("errors_total", 3.0)
                .with_label("code", "500"),
        ]
    }

    // 1. JSON export
    #[test]
    fn test_json_export() {
        let output = MetricsExporter::export(&sample_metrics(), ExportFormat::Json);
        let parsed: Vec<MetricPoint> = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].name, "requests_total");
    }

    // 2. CSV export
    #[test]
    fn test_csv_export() {
        let output = MetricsExporter::export(&sample_metrics(), ExportFormat::Csv);
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines[0], "name,type,value,labels");
        assert!(lines[1].contains("requests_total"));
        assert!(lines[1].contains("counter"));
    }

    // 3. OpenMetrics export
    #[test]
    fn test_open_metrics_export() {
        let output = MetricsExporter::export(&sample_metrics(), ExportFormat::OpenMetrics);
        assert!(output.contains("# HELP requests_total Total requests"));
        assert!(output.contains("# TYPE requests_total counter"));
        assert!(output.contains("requests_total{method=\"GET\"} 42"));
        assert!(output.contains("# EOF"));
    }

    // 4. Line protocol export
    #[test]
    fn test_line_protocol_export() {
        let output = MetricsExporter::export(&sample_metrics(), ExportFormat::LineProtocol);
        assert!(output.contains("requests_total,method=GET value=42"));
        assert!(output.contains("active_connections value=5"));
    }

    // 5. Empty metrics
    #[test]
    fn test_empty_metrics() {
        let output = MetricsExporter::export(&[], ExportFormat::Json);
        assert_eq!(output, "[]");

        let csv = MetricsExporter::export(&[], ExportFormat::Csv);
        assert!(csv.starts_with("name,type,value,labels"));
    }

    // 6. MetricPoint counter constructor
    #[test]
    fn test_counter_constructor() {
        let m = MetricPoint::counter("test", 10.0);
        assert_eq!(m.metric_type, MetricType::Counter);
        assert_eq!(m.value, 10.0);
    }

    // 7. MetricPoint gauge constructor
    #[test]
    fn test_gauge_constructor() {
        let m = MetricPoint::gauge("test", 5.5);
        assert_eq!(m.metric_type, MetricType::Gauge);
        assert_eq!(m.value, 5.5);
    }

    // 8. Labels on metric point
    #[test]
    fn test_labels() {
        let m = MetricPoint::counter("test", 1.0)
            .with_label("a", "b")
            .with_label("c", "d");
        assert_eq!(m.labels.len(), 2);
    }

    // 9. Help text
    #[test]
    fn test_help_text() {
        let m = MetricPoint::counter("test", 1.0).with_help("Help text");
        assert_eq!(m.help.unwrap(), "Help text");
    }

    // 10. Timestamp
    #[test]
    fn test_timestamp() {
        let m = MetricPoint::counter("test", 1.0).with_timestamp(1234567890);
        assert_eq!(m.timestamp_ms, 1234567890);
    }

    // 11. Line protocol with timestamp
    #[test]
    fn test_line_protocol_timestamp() {
        let metrics = vec![MetricPoint::counter("test", 1.0).with_timestamp(1000)];
        let output = MetricsExporter::export(&metrics, ExportFormat::LineProtocol);
        assert!(output.contains("1000000000")); // ms to ns
    }

    // 12. OpenMetrics dedup HELP/TYPE
    #[test]
    fn test_open_metrics_dedup() {
        let metrics = vec![
            MetricPoint::counter("test", 1.0)
                .with_label("a", "1")
                .with_help("Test"),
            MetricPoint::counter("test", 2.0).with_label("a", "2"),
        ];
        let output = MetricsExporter::export(&metrics, ExportFormat::OpenMetrics);
        assert_eq!(output.matches("# TYPE test counter").count(), 1);
    }

    // 13. Gauge without labels in OpenMetrics
    #[test]
    fn test_open_metrics_no_labels() {
        let metrics = vec![MetricPoint::gauge("temp", 72.5)];
        let output = MetricsExporter::export(&metrics, ExportFormat::OpenMetrics);
        assert!(output.contains("temp 72.5"));
    }

    // 14. CSV escapes labels
    #[test]
    fn test_csv_labels() {
        let metrics = vec![MetricPoint::counter("test", 1.0)
            .with_label("a", "1")
            .with_label("b", "2")];
        let output = MetricsExporter::export(&metrics, ExportFormat::Csv);
        // Labels should be semicolon-separated
        assert!(output.contains("a=1") || output.contains("b=2"));
    }

    // 15. Integer values formatted without decimals
    #[test]
    fn test_format_value_integer() {
        assert_eq!(format_value(42.0), "42");
        assert_eq!(format_value(0.0), "0");
    }

    // 16. Float values keep decimals
    #[test]
    fn test_format_value_float() {
        assert_eq!(format_value(3.14), "3.14");
    }

    // 17. MetricPoint serializable
    #[test]
    fn test_metric_point_serializable() {
        let m = MetricPoint::counter("test", 42.0).with_label("env", "prod");
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"name\":\"test\""));
        let restored: MetricPoint = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.value, 42.0);
    }

    // 18. ExportFormat serializable
    #[test]
    fn test_format_serializable() {
        let f = ExportFormat::Json;
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(json, "\"json\"");
    }

    // 19. Multiple metrics same name different labels
    #[test]
    fn test_same_name_different_labels() {
        let metrics = vec![
            MetricPoint::counter("http_requests", 100.0).with_label("status", "200"),
            MetricPoint::counter("http_requests", 5.0).with_label("status", "500"),
        ];
        let om = MetricsExporter::export(&metrics, ExportFormat::OpenMetrics);
        assert!(om.contains("status=\"200\""));
        assert!(om.contains("status=\"500\""));
    }

    // 20. Line protocol sorted labels
    #[test]
    fn test_line_protocol_sorted_labels() {
        let m = vec![MetricPoint::counter("test", 1.0)
            .with_label("z", "1")
            .with_label("a", "2")];
        let output = MetricsExporter::export(&m, ExportFormat::LineProtocol);
        // Labels should be sorted alphabetically
        let line = output.lines().next().unwrap();
        let comma_pos_a = line.find(",a=").unwrap();
        let comma_pos_z = line.find(",z=").unwrap();
        assert!(comma_pos_a < comma_pos_z);
    }
}
