//! Optional OpenTelemetry tracing through Axiom OTLP/HTTP.
//!
//! Telemetry stays disabled unless `AXIOM_TOKEN` is set. Tokens are read from
//! the environment and are never written to a project or a log.

use std::collections::HashMap;
use std::env;

use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{SpanExporter, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::{BatchSpanProcessor, SdkTracerProvider};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const DEFAULT_ENDPOINT: &str = "https://api.axiom.co";
const DEFAULT_DATASET: &str = "slate-traces";
const DEFAULT_SERVICE: &str = "slate-desktop";

pub struct TelemetryGuard {
    provider: SdkTracerProvider,
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        let _ = self.provider.shutdown();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AxiomConfig {
    token: String,
    dataset: String,
    traces_endpoint: String,
    service_name: String,
}

impl AxiomConfig {
    fn from_env() -> Option<Self> {
        let token = env::var("AXIOM_TOKEN")
            .ok()
            .filter(|token| !token.is_empty())?;
        let base = env::var("AXIOM_OTLP_ENDPOINT")
            .or_else(|_| env::var("OTEL_EXPORTER_OTLP_ENDPOINT"))
            .unwrap_or_else(|_| DEFAULT_ENDPOINT.to_string());
        let dataset = env::var("AXIOM_DATASET").unwrap_or_else(|_| DEFAULT_DATASET.to_string());
        let service_name =
            env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| DEFAULT_SERVICE.to_string());
        Some(Self::new(token, dataset, base, service_name))
    }

    fn new(token: String, dataset: String, endpoint: String, service_name: String) -> Self {
        let endpoint = endpoint.trim_end_matches('/');
        let traces_endpoint = if endpoint.ends_with("/v1/traces") {
            endpoint.to_string()
        } else {
            format!("{endpoint}/v1/traces")
        };
        Self {
            token,
            dataset,
            traces_endpoint,
            service_name,
        }
    }

    fn headers(&self) -> HashMap<String, String> {
        HashMap::from([
            (
                "authorization".to_string(),
                format!("Bearer {}", self.token),
            ),
            ("x-axiom-dataset".to_string(), self.dataset.clone()),
        ])
    }
}

/// Install an Axiom span exporter when `AXIOM_TOKEN` exists.
///
/// Keep the returned guard alive for the process lifetime. Dropping it flushes
/// the batch span processor.
pub fn init() -> Option<TelemetryGuard> {
    let Some(config) = AxiomConfig::from_env() else {
        if env::var_os("RUST_LOG").is_some() {
            let filter =
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
            let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
        }
        return None;
    };
    let exporter = SpanExporter::builder()
        .with_http()
        .with_endpoint(&config.traces_endpoint)
        .with_headers(config.headers())
        .build()
        .ok()?;
    let provider = SdkTracerProvider::builder()
        .with_resource(
            Resource::builder()
                .with_service_name(config.service_name)
                .build(),
        )
        .with_span_processor(BatchSpanProcessor::builder(exporter).build())
        .build();
    global::set_tracer_provider(provider.clone());

    let tracer = provider.tracer("slate-desktop");
    let layer = tracing_opentelemetry::layer().with_tracer(tracer);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    if tracing_subscriber::registry()
        .with(filter)
        .with(layer)
        .try_init()
        .is_err()
    {
        return None;
    }

    Some(TelemetryGuard { provider })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axiom_endpoint_and_headers_are_built_once() {
        let config = AxiomConfig::new(
            "xaat-test".into(),
            "slate-traces".into(),
            "https://api.axiom.co/".into(),
            "slate-desktop".into(),
        );
        assert_eq!(config.traces_endpoint, "https://api.axiom.co/v1/traces");
        let headers = config.headers();
        assert_eq!(
            headers.get("authorization").map(String::as_str),
            Some("Bearer xaat-test")
        );
        assert_eq!(
            headers.get("x-axiom-dataset").map(String::as_str),
            Some("slate-traces")
        );
    }

    #[test]
    fn an_explicit_traces_path_is_not_duplicated() {
        let config = AxiomConfig::new(
            "token".into(),
            "dataset".into(),
            "https://api.axiom.co/v1/traces".into(),
            "service".into(),
        );
        assert_eq!(config.traces_endpoint, "https://api.axiom.co/v1/traces");
    }
}
