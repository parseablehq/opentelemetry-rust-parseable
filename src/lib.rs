use base64::{engine::general_purpose as base64encoder, Engine};
use chrono::{DateTime, Utc};
use futures_core::future::BoxFuture;
use http::{HeaderMap, HeaderValue, Method};
use itertools::Itertools;
use opentelemetry::{
    global,
    sdk::{
        self,
        export::{self, trace::SpanData},
        trace::{BatchSpanProcessor, TraceRuntime},
    },
    trace::{TraceError, TracerProvider},
};
use reqwest::Url;
use serde::Serialize;
use std::{env, fmt::Debug};
use telemetry::get_batch_config;

pub mod telemetry;

/// A message is a single data request sent to Parseable WS and represents a single event collected.
/// So, from a single SpanData we create multiple ParseableMessage(s)
#[derive(Serialize, Debug)]
struct ParseableMessage {
    span_start_time: String,
    span_end_time: String,
    parent_span_id: String,
    span_id: String,
    trace_id: String,
    event_level: String,
    event_caller: String,
    event_message: String,
    event_timestamp: String,
    event_target: String,
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

pub struct ParseableMetadata {
    headers: http::HeaderMap,
}

pub struct ParseableTags {
    headers: http::HeaderMap,
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

impl export::trace::SpanExporter for ParseableExporter {
    fn export(&mut self, batch: Vec<SpanData>) -> BoxFuture<'static, export::trace::ExportResult> {
        let traces = batch
            .into_iter()
            .flat_map(|sd| {
                sd.events
                    .into_iter()
                    .map(|event| {
                        let start_time: DateTime<Utc> = sd.start_time.into();
                        let end_time: DateTime<Utc> = sd.end_time.into();
                        let timestamp: DateTime<Utc> = event.timestamp.into();
                        ParseableMessage {
                            span_start_time: start_time.to_string(),
                            span_end_time: end_time.to_string(),
                            parent_span_id: sd.parent_span_id.to_string(),
                            span_id: sd.span_context.span_id().to_string(),
                            trace_id: sd.span_context.trace_id().to_string(),
                            event_level: "toadd".into(),
                            event_caller: sd.name.to_string(),
                            event_message: event.name.to_string(),
                            event_timestamp: timestamp.naive_utc().to_string(),
                            event_target: "toadd".into(),
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .sorted_unstable_by(|ts1, ts2| Ord::cmp(&ts2.event_timestamp, &ts1.event_timestamp))
            .collect::<Vec<_>>();

        Box::pin(send_request(
            self.client.clone(),
            self.request_url.clone(),
            self.request_headers.clone(),
            traces,
        ))
    }
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

pub struct ParseableExporterBuilder {
    tls_enabled: bool,
    host: String,
    port: String,
    api_version: ParseableApiVersion,
    username: String,
    password: String,
    service_name: String,
    client: Option<reqwest::Client>,
    metadata: Option<ParseableMetadata>,
    tags: Option<ParseableTags>,
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

    pub fn with_metadata(mut self, metadata: ParseableMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn with_tags(mut self, tags: ParseableTags) -> Self {
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
        let exporter = self._build_exporter()?;
        let bz = BatchSpanProcessor::builder(exporter, runtime)
            .with_batch_config(get_batch_config())
            .build();
        let provider_builder = sdk::trace::TracerProvider::builder()
            .with_span_processor(bz)
            .with_config(config);
        // let provider_builder =
        //     sdk::trace::TracerProvider::builder().with_batch_exporter(exporter, runtime);
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

    fn _build_exporter(self) -> Result<ParseableExporter, TraceError> {
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
                headers.extend(metadata.headers);
            }

            // Tags
            if let Some(tags) = self.tags {
                headers.extend(tags.headers);
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
