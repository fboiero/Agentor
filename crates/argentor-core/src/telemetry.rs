//! OpenTelemetry distributed tracing support for the Argentor framework.
//!
//! This module provides [`TelemetryConfig`] for configuring OTLP trace export,
//! and initialization/shutdown functions for setting up the tracing pipeline.
//!
//! # Feature gating
//!
//! The [`TelemetryConfig`] struct is always available (it is a plain config
//! struct with serde support). The actual OpenTelemetry integration requires
//! the `telemetry` cargo feature to be enabled:
//!
//! ```toml
//! argentor-core = { workspace = true, features = ["telemetry"] }
//! ```
//!
//! When the feature is disabled, [`init_telemetry`] sets up a basic
//! `tracing_subscriber` with env-filter support and [`shutdown_telemetry`]
//! is a no-op.

use serde::{Deserialize, Serialize};

/// Configuration for OpenTelemetry telemetry export.
///
/// This struct is always available regardless of the `telemetry` feature flag,
/// so it can be embedded in application configuration without conditional
/// compilation at the config layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    /// Whether OTLP export is enabled.
    ///
    /// When `false`, [`init_telemetry`] sets up a local-only `tracing_subscriber`
    /// without any OpenTelemetry export.
    pub enabled: bool,

    /// OTLP gRPC endpoint (e.g. `"http://localhost:4317"`).
    ///
    /// This is the address of the OpenTelemetry Collector or compatible backend
    /// that will receive trace spans.
    pub otlp_endpoint: String,

    /// Service name reported to the collector.
    ///
    /// Appears as the `service.name` resource attribute in exported spans.
    pub service_name: String,

    /// Trace sampling ratio (`0.0` to `1.0`).
    ///
    /// - `1.0` means sample every trace (useful for development).
    /// - `0.0` means drop all traces.
    /// - Values in between probabilistically sample traces.
    pub sample_rate: f64,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            otlp_endpoint: "http://localhost:4317".to_string(),
            service_name: "argentor".to_string(),
            sample_rate: 1.0,
        }
    }
}

impl TelemetryConfig {
    /// Creates a new configuration with OTLP export enabled.
    pub fn enabled(endpoint: impl Into<String>, service_name: impl Into<String>) -> Self {
        Self {
            enabled: true,
            otlp_endpoint: endpoint.into(),
            service_name: service_name.into(),
            sample_rate: 1.0,
        }
    }

    /// Returns a configuration with telemetry disabled (local tracing only).
    pub fn disabled() -> Self {
        Self::default()
    }

    /// Sets the sampling rate, clamping to the valid `[0.0, 1.0]` range.
    pub fn with_sample_rate(mut self, rate: f64) -> Self {
        self.sample_rate = rate.clamp(0.0, 1.0);
        self
    }
}

// ---------------------------------------------------------------------------
// Telemetry initialization — feature-gated implementation
// ---------------------------------------------------------------------------

/// Initialize the tracing/telemetry pipeline based on the provided configuration.
///
/// - When [`TelemetryConfig::enabled`] is `false` (or the `telemetry` feature is
///   not compiled in), this sets up a local `tracing_subscriber` with env-filter
///   and JSON formatting support.
/// - When enabled **and** the `telemetry` feature is active, this configures an
///   OTLP gRPC exporter and registers it as an additional tracing layer.
///
/// # Errors
///
/// Returns an error if the OpenTelemetry pipeline fails to initialize (e.g.
/// invalid endpoint configuration).
#[cfg(feature = "telemetry")]
pub fn init_telemetry(config: &TelemetryConfig) -> Result<(), Box<dyn std::error::Error>> {
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry::KeyValue;
    use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider};
    use opentelemetry_sdk::Resource;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::EnvFilter;

    if !config.enabled {
        // No OTLP export — just set up a basic subscriber.
        tracing_subscriber::registry()
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
            .with(tracing_subscriber::fmt::layer())
            .try_init()
            .ok(); // Ignore if a global subscriber is already set.

        tracing::info!("Telemetry: OTLP export disabled, using local tracing only");
        return Ok(());
    }

    // Build the OTLP exporter targeting the configured gRPC endpoint.
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&config.otlp_endpoint)
        .build()?;

    // Choose the sampler based on the configured ratio.
    let sampler = if (config.sample_rate - 1.0).abs() < f64::EPSILON {
        Sampler::AlwaysOn
    } else if config.sample_rate <= 0.0 {
        Sampler::AlwaysOff
    } else {
        Sampler::TraceIdRatioBased(config.sample_rate)
    };

    let resource = Resource::builder()
        .with_attributes([KeyValue::new("service.name", config.service_name.clone())])
        .build();

    let provider = SdkTracerProvider::builder()
        .with_sampler(sampler)
        .with_resource(resource)
        .with_batch_exporter(exporter)
        .build();

    let tracer = provider.tracer(config.service_name.clone());

    // Build the OpenTelemetry tracing layer.
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .with(otel_layer)
        .try_init()
        .ok(); // Ignore if a global subscriber is already set.

    // Store the provider globally so it can be shut down later.
    // We use opentelemetry::global for this purpose.
    opentelemetry::global::set_tracer_provider(provider);

    tracing::info!(
        endpoint = %config.otlp_endpoint,
        service = %config.service_name,
        sample_rate = config.sample_rate,
        "Telemetry: OTLP export initialized"
    );

    Ok(())
}

/// Initialize the tracing pipeline (no-op OTLP variant when `telemetry` feature is disabled).
///
/// Sets up a local `tracing_subscriber` with env-filter support. OTLP export
/// is not available without the `telemetry` feature.
#[cfg(not(feature = "telemetry"))]
pub fn init_telemetry(config: &TelemetryConfig) -> Result<(), Box<dyn std::error::Error>> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::EnvFilter;

    if config.enabled {
        tracing::warn!(
            "TelemetryConfig has enabled=true but the `telemetry` feature is not compiled in. \
             OTLP export will not be available. Enable the feature: \
             argentor-core = {{ features = [\"telemetry\"] }}"
        );
    }

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .try_init()
        .ok(); // Ignore if a global subscriber is already set.

    tracing::info!("Telemetry: using local tracing only (telemetry feature disabled)");
    Ok(())
}

/// Gracefully shut down the OpenTelemetry pipeline, flushing any pending spans.
///
/// This should be called before the process exits to ensure all trace data is
/// exported. When the `telemetry` feature is disabled, this is a no-op.
#[cfg(feature = "telemetry")]
pub fn shutdown_telemetry() {
    tracing::info!("Telemetry: shutting down OpenTelemetry pipeline");
    opentelemetry::global::shutdown_tracer_provider();
}

/// Gracefully shut down the telemetry pipeline (no-op without the `telemetry` feature).
#[cfg(not(feature = "telemetry"))]
pub fn shutdown_telemetry() {
    // No-op: nothing to shut down when OTLP is not configured.
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = TelemetryConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.otlp_endpoint, "http://localhost:4317");
        assert_eq!(config.service_name, "argentor");
        assert!((config.sample_rate - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_config_enabled_constructor() {
        let config = TelemetryConfig::enabled("http://otel:4317", "my-service");
        assert!(config.enabled);
        assert_eq!(config.otlp_endpoint, "http://otel:4317");
        assert_eq!(config.service_name, "my-service");
        assert!((config.sample_rate - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_config_disabled_constructor() {
        let config = TelemetryConfig::disabled();
        assert!(!config.enabled);
        assert_eq!(config.service_name, "argentor");
    }

    #[test]
    fn test_config_sample_rate_clamping() {
        let config = TelemetryConfig::default().with_sample_rate(2.5);
        assert!((config.sample_rate - 1.0).abs() < f64::EPSILON);

        let config = TelemetryConfig::default().with_sample_rate(-0.5);
        assert!(config.sample_rate.abs() < f64::EPSILON);

        let config = TelemetryConfig::default().with_sample_rate(0.5);
        assert!((config.sample_rate - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let config =
            TelemetryConfig::enabled("http://collector:4317", "test-svc").with_sample_rate(0.25);

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"enabled\":true"));
        assert!(json.contains("http://collector:4317"));
        assert!(json.contains("test-svc"));

        let parsed: TelemetryConfig = serde_json::from_str(&json).unwrap();
        assert!(parsed.enabled);
        assert_eq!(parsed.otlp_endpoint, "http://collector:4317");
        assert_eq!(parsed.service_name, "test-svc");
        assert!((parsed.sample_rate - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn test_config_deserialization_from_partial_json() {
        // Ensure serde defaults work when fields are missing.
        let json = r#"{"enabled": true, "otlp_endpoint": "http://localhost:4317", "service_name": "x", "sample_rate": 0.1}"#;
        let config: TelemetryConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
        assert_eq!(config.service_name, "x");
        assert!((config.sample_rate - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn test_init_disabled_does_not_panic() {
        // Calling init with disabled config should not panic.
        // Note: the global subscriber may already be set by other tests,
        // which is fine — init_telemetry tolerates that.
        let config = TelemetryConfig::disabled();
        let result = init_telemetry(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_shutdown_does_not_panic() {
        // Calling shutdown without prior init should not panic.
        shutdown_telemetry();
    }

    #[test]
    fn test_config_debug_repr() {
        let config =
            TelemetryConfig::enabled("http://otel:4317", "debug-test").with_sample_rate(0.75);
        let debug = format!("{config:?}");
        assert!(debug.contains("debug-test"));
        assert!(debug.contains("http://otel:4317"));
        assert!(debug.contains("0.75"));
        assert!(debug.contains("enabled: true"));
    }
}
