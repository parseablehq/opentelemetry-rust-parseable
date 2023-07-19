use base64::{engine::general_purpose as base64encoder, Engine};
use chrono::{DateTime, Utc};
use futures_core::future::BoxFuture;
use http::{HeaderMap, HeaderValue, Method};
use opentelemetry::{
    global,
    sdk::{
        self,
        export::{self, trace::SpanData},
        trace::{BatchConfig, BatchSpanProcessor, TraceRuntime},
    },
    trace::{TraceError, TracerProvider},
    Key, Value,
};

use reqwest::Url;
use serde::Serialize;
use std::{
    env,
    fmt::Debug,
    time::{Duration, SystemTime},
};

pub mod telemetry;

/// Get configuration options for batch exporter
pub fn get_batch_config() -> BatchConfig {
    BatchConfig::default()
        .with_max_queue_size(
            env::var("OTLP_QUEUE_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(100000),
        )
        .with_max_export_batch_size(
            env::var("OTLP_BATCH_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(8192),
        )
        .with_scheduled_delay(Duration::from_millis(
            env::var("OTLP_INTERVAL_MILLIS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1000),
        ))
}

/// A message is a single data request sent to Parseable WS and represents a single event collected.
/// So, from a single SpanData we create multiple ParseableMessage(s)
#[derive(Serialize, Debug, Clone)]
struct TraceMessage {
    resource_attributes: Vec<String>,
    span_name: String,
    attributes: Vec<String>,
    start_time: String,
    end_time: String,
    parent_span_id: String,
    span_id: String,
    trace_id: String,
    event_message: Option<String>,
    event_timestamp: Option<String>,
}

#[derive(Debug)]
pub struct ParseableExporter {
    client: reqwest::Client,
    request_url: Url,
    request_headers: HeaderMap,
}

impl ParseableExporter {
    pub(crate) fn new(
        client: reqwest::Client,
        request_url: Url,
        request_headers: HeaderMap,
    ) -> Self {
        ParseableExporter {
            client,
            request_url,
            request_headers,
        }
    }
}

#[derive(Debug)]
pub enum ParseableApiVersion {
    V1,
}

impl ParseableApiVersion {
    pub fn path(&self) -> &'static str {
        match self {
            ParseableApiVersion::V1 => "api/v1",
        }
    }

    pub fn content_type(&self) -> &'static str {
        "application/json"
    }
}

pub struct ParseableExporterBuilder {
    tls_enabled: bool,
    host: String,
    port: String,
    api_version: ParseableApiVersion,
    username: String,
    password: String,
    service_name: String,
    client: Option<reqwest::Client>,
    metadata: Option<http::HeaderMap>,
    tags: Option<http::HeaderMap>,
}

impl ParseableExporterBuilder {
    pub fn with_tls(mut self) -> Self {
        self.tls_enabled = true;
        self
    }

    pub fn with_host<T: Into<String>>(mut self, host: T) -> Self {
        self.host = host.into();
        self
    }

    pub fn with_port<T: Into<String>>(mut self, port: T) -> Self {
        self.port = port.into();
        self
    }

    pub fn with_api_version(mut self, api_version: ParseableApiVersion) -> Self {
        self.api_version = api_version;
        self
    }

    pub fn with_service_name<T: Into<String>>(mut self, service_name: T) -> Self {
        self.service_name = service_name.into();
        self
    }

    pub fn with_metadata(mut self, metadata: http::HeaderMap) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn with_tags(mut self, tags: http::HeaderMap) -> Self {
        self.tags = Some(tags);
        self
    }

    pub fn with_client(mut self, client: reqwest::Client) -> Self {
        self.client = Some(client);
        self
    }

    pub fn with_username<T: Into<String>>(mut self, username: T) -> Self {
        self.username = username.into();
        self
    }

    pub fn with_password<T: Into<String>>(mut self, password: T) -> Self {
        self.password = password.into();
        self
    }

    pub fn install_batch<R: TraceRuntime>(
        self,
        runtime: R,
        config: sdk::trace::Config,
    ) -> Result<sdk::trace::Tracer, TraceError> {
        let exporter = self.build_exporter()?;
        let bz = BatchSpanProcessor::builder(exporter, runtime)
            .with_batch_config(get_batch_config())
            .build();
        let provider_builder = sdk::trace::TracerProvider::builder()
            .with_span_processor(bz)
            .with_config(config);
        let provider = provider_builder.build();
        let tracer = provider.versioned_tracer(
            "opentelemetry-parseable",
            Some(env!("CARGO_PKG_VERSION")),
            None,
        );
        let _ = global::set_tracer_provider(provider);
        Ok(tracer)
    }

    fn _build_endpoint(&self) -> Result<Url, TraceError> {
        let http_protocol = if self.tls_enabled { "https" } else { "http" };
        let url = format!(
            "{}://{}:{}/{}/ingest",
            http_protocol,
            self.host,
            self.port,
            self.api_version.path()
        );
        url.parse::<Url>()
            .map_err(|e| TraceError::Other(Box::new(e)))
    }

    fn build_exporter(self) -> Result<ParseableExporter, TraceError> {
        let endpoint = self._build_endpoint()?;
        if let Some(client) = self.client {
            // We add here the stream name, that will be the name of the service we are going to trace
            let mut headers = HeaderMap::new();
            let encoded_auth =
                base64encoder::STANDARD.encode(format!("{}:{}", self.username, self.password));
            headers.insert(
                "Authorization",
                HeaderValue::from_str(&format!("Basic {encoded_auth}"))
                    .map_err(|e| TraceError::Other(Box::new(e)))?,
            );
            headers.insert(
                "Content-Type",
                HeaderValue::from_static(self.api_version.content_type()),
            );
            headers.insert(
                "X-P-Stream",
                HeaderValue::from_str(&self.service_name)
                    .map_err(|e| TraceError::Other(Box::new(e)))?,
            );

            // Metadata
            if let Some(metadata) = self.metadata {
                headers.extend(metadata);
            }

            // Tags
            if let Some(tags) = self.tags {
                headers.extend(tags);
            }

            Ok(ParseableExporter::new(client, endpoint, headers))
        } else {
            Err(TraceError::from("No HttpClient provided"))
        }
    }
}

impl Default for ParseableExporterBuilder {
    fn default() -> Self {
        ParseableExporterBuilder {
            tls_enabled: false,
            host: env::var("PARSEABLE_HOST").unwrap_or_else(|_| "0.0.0.0".into()),
            port: env::var("PARSEABLE_PORT").unwrap_or_else(|_| "8000".into()),
            api_version: ParseableApiVersion::V1,
            username: "admin".into(),
            password: "admin".into(),
            service_name: "my-service".into(),
            client: Some(reqwest::Client::new()),
            metadata: None,
            tags: None,
        }
    }
}

impl export::trace::SpanExporter for ParseableExporter {
    fn export(&mut self, batch: Vec<SpanData>) -> BoxFuture<'static, export::trace::ExportResult> {
        let traces = into_trace_messages(batch);
        Box::pin(send_request(
            self.client.clone(),
            self.request_url.clone(),
            self.request_headers.clone(),
            traces,
        ))
    }
}

/// Convert span data into flattened trace data.  
fn into_trace_messages(spans: Vec<SpanData>) -> Vec<TraceMessage> {
    let mut trace_messages = Vec::with_capacity(spans.len());

    for span in spans {
        let start_time = to_timestamp_string(span.start_time);
        let end_time = to_timestamp_string(span.end_time);
        let trace_message = TraceMessage {
            resource_attributes: extract_attributes(span.resource.iter()),
            span_name: span.name.to_string(),
            attributes: extract_attributes(span.attributes.iter()),
            start_time,
            end_time,
            parent_span_id: span.parent_span_id.to_string(),
            span_id: span.span_context.span_id().to_string(),
            trace_id: span.span_context.trace_id().to_string(),
            event_message: None,
            event_timestamp: None,
        };

        if span.events.is_empty() {
            trace_messages.push(trace_message);
        } else {
            trace_messages.extend(span.events.into_iter().map(|event| {
                let mut trace_message = trace_message.clone();
                trace_message.attributes.extend(extract_attributes(
                    event.attributes.iter().map(|kv| (&kv.key, &kv.value)),
                ));
                trace_message.event_message = Some(event.name.to_string());
                trace_message.event_timestamp = Some(to_timestamp_string(event.timestamp));
                trace_message
            }))
        }
    }

    trace_messages
}

fn to_timestamp_string(timestamp: SystemTime) -> String {
    DateTime::<Utc>::from(timestamp).to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn extract_attributes<'a>(attributes: impl Iterator<Item = (&'a Key, &'a Value)>) -> Vec<String> {
    attributes
        .map(|(key, value)| format!("{}={}", key, value))
        .collect()
}

async fn send_request<T: Serialize + Debug>(
    client: reqwest::Client,
    url: Url,
    headers: HeaderMap,
    data: T,
) -> export::trace::ExportResult {
    let req = client
        .request(Method::POST, url)
        .headers(headers.clone())
        .json(&data)
        .build()
        .map_err(|e| TraceError::Other(Box::new(e)))?;
    client
        .execute(req)
        .await
        .map_err(|e| TraceError::Other(Box::new(e)))?;
    Ok(())
}
