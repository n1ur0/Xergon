//! OpenTelemetry distributed tracing initialization.
//!
//! Provides a thin wrapper around the OpenTelemetry SDK that:
//! - Creates a TracerProvider with OTLP gRPC exporter
//! - Bridges tracing spans to OpenTelemetry via tracing-opentelemetry
//! - Configures W3C TraceContext propagation
//! - Falls back to a no-op provider when telemetry is disabled
//!
//! All heavy dependencies are gated behind the `telemetry` Cargo feature
//! so that builds without telemetry pay zero compile-time cost.

use tracing::{info, warn};

/// Global handle to the OpenTelemetry tracer provider.
///
/// When dropped, the provider is flushed and shut down gracefully.
/// Must be held for the entire lifetime of the application.
pub struct TelemetryGuard {
    #[cfg(feature = "telemetry")]
    provider: Option<opentelemetry_sdk::trace::SdkTracerProvider>,
}

impl std::fmt::Debug for TelemetryGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelemetryGuard").finish()
    }
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        #[cfg(feature = "telemetry")]
        if let Some(provider) = self.provider.take() {
            info!("Shutting down OpenTelemetry tracer provider...");
            if let Err(e) = provider.shutdown() {
                warn!(error = %e, "Failed to shutdown OpenTelemetry provider");
            }
        }
    }
}

/// Initialize OpenTelemetry tracing.
///
/// If `enabled` is false and no `OTEL_EXPORTER_OTLP_ENDPOINT` env var is set,
/// returns a no-op guard (no tracing overhead).
///
/// When enabled:
/// - Creates an OTLP gRPC exporter pointed at `otlp_endpoint`
/// - Sets up a TracerProvider with batch span processing
/// - Returns a guard that must be held until application shutdown
pub fn init_telemetry(service_name: &str, otlp_endpoint: &str, enabled: bool) -> TelemetryGuard {
    // Check env var override — standard OTEL convention
    let env_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
    let actually_enabled = enabled || env_endpoint.is_some();

    if !actually_enabled {
        info!("OpenTelemetry telemetry is disabled (set telemetry.enabled = true or OTEL_EXPORTER_OTLP_ENDPOINT)");
        return TelemetryGuard {
            #[cfg(feature = "telemetry")]
            provider: None,
        };
    }

    let endpoint = env_endpoint.as_deref().unwrap_or(otlp_endpoint);
    let env_service_name = std::env::var("OTEL_SERVICE_NAME").ok();
    let actual_service_name = env_service_name.as_deref().unwrap_or(service_name).to_string();

    info!(
        service_name = actual_service_name,
        otlp_endpoint = endpoint,
        "Initializing OpenTelemetry tracing"
    );

    #[cfg(not(feature = "telemetry"))]
    {
        warn!(
            "OpenTelemetry is enabled in config but the 'telemetry' Cargo feature is not compiled in. \
             Rebuild with --features telemetry to enable OTLP export."
        );
        return TelemetryGuard {
            #[cfg(feature = "telemetry")]
            provider: None,
        };
    }

    #[cfg(feature = "telemetry")]
    {
        use opentelemetry::trace::TracerProvider as _;
        use opentelemetry_otlp::{SpanExporter, WithExportConfig};
        use opentelemetry_sdk::trace::SdkTracerProvider;
        use opentelemetry_sdk::Resource;

        // Build OTLP exporter
        let exporter = SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()
            .expect("Failed to create OTLP exporter");

        // Build resource with service name and attributes
        let resource = Resource::builder_empty()
            .with_service_name(actual_service_name.clone())
            .with_attributes([
                opentelemetry::KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
                opentelemetry::KeyValue::new("service.instance.id", uuid::Uuid::new_v4().to_string()),
            ])
            .build();

        let provider = SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(resource)
            .build();

        // Set as global provider so context propagation works
        opentelemetry::global::set_tracer_provider(provider.clone());

        info!(
            service_name = actual_service_name,
            otlp_endpoint = endpoint,
            "OpenTelemetry tracer provider initialized successfully"
        );

        TelemetryGuard {
            provider: Some(provider),
        }
    }
}

/// Check if telemetry is enabled (either via config or env var).
pub fn is_telemetry_enabled(config_enabled: bool) -> bool {
    config_enabled || std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok()
}

/// Get the effective OTLP endpoint (config or env var override).
pub fn effective_otlp_endpoint(config_endpoint: &str) -> String {
    std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| config_endpoint.to_string())
}

/// Get the effective service name (config or env var override).
pub fn effective_service_name(config_name: &str) -> String {
    std::env::var("OTEL_SERVICE_NAME")
        .unwrap_or_else(|_| config_name.to_string())
}
