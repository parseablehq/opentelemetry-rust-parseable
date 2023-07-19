use opentelemetry::sdk::Resource;
use opentelemetry::KeyValue;
use opentelemetry::{runtime, sdk::trace};
use std::env;
use tokio::time::{sleep, Duration};
use tracing_subscriber::prelude::*;
use tracing_subscriber::{filter::LevelFilter, layer::SubscriberExt, EnvFilter};

use crate::ParseableExporterBuilder;

fn get_resources(service: &str) -> Resource {
    let kvs = [
        KeyValue::new(
            "vhost",
            std::env::var("Q_VHOST")
                .unwrap_or("Not Set".into())
                .replace('/', ""),
        ),
        KeyValue::new(
            "build_number",
            std::env::var("BUILD_NUMBER").unwrap_or("local build".into()),
        ),
        KeyValue::new(
            "build_date_time",
            std::env::var("BUILD_DATE_TIME").unwrap_or("local build".into()),
        ),
        KeyValue::new("user.real_name", whoami::realname()),
        KeyValue::new("user.user_name", whoami::username()),
        KeyValue::new("host.platform", whoami::platform().to_string()),
        KeyValue::new(
            opentelemetry_semantic_conventions::resource::HOST_ARCH,
            whoami::arch().to_string(),
        ),
        KeyValue::new(
            opentelemetry_semantic_conventions::resource::SERVICE_NAME,
            service.to_string(),
        ),
        KeyValue::new(
            opentelemetry_semantic_conventions::resource::HOST_NAME,
            whoami::hostname(),
        ),
    ];
    Resource::new(kvs)
}

fn service_name() -> Option<String> {
    env::current_exe()
        .ok()
        .as_ref()
        .map(std::path::Path::new)
        .and_then(std::path::Path::file_name)
        .and_then(std::ffi::OsStr::to_str)
        .map(|s| {
            // if the exe contains a '-', take only the preceding part
            s.split_once('_').map(|(s, _)| s).unwrap_or(s).to_string()
        })
}

#[inline]
pub async fn telemetry_startup() {
    let service_name = service_name().expect("Unable to get service name");
    if std::env::var("RUST_LOG").ok().is_none() {
        std::env::set_var("RUST_LOG", "info");
    }

    // parseable exporter
    let config = trace::config().with_resource(get_resources(&service_name));
    let tracer = ParseableExporterBuilder::default()
        .with_service_name(&service_name)
        .install_batch(runtime::Tokio, config)
        .expect("Unable to build parseable exporter");

    let collector = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_filter(EnvFilter::from_default_env()),
    );
    let collector = collector.with(
        tracing_opentelemetry::layer()
            .with_tracer(tracer)
            .with_filter(LevelFilter::INFO),
    );
    if tracing::subscriber::set_global_default(collector).is_err() {
        eprintln!(
            "Error setting tracing subscriber, probably another subscriber has already been set?"
        );
    }
}

#[inline]
pub async fn telemetry_shutdown() {
    sleep(Duration::from_secs(2)).await;
}

mod telemetry_test {

    use tokio::time::{sleep, Duration};
    use tracing::*;

    #[instrument]
    async fn bar() {
        sleep(Duration::from_secs(5)).await;
    }

    #[instrument]
    async fn fooz() {
        println!("Testing Spans?");
        bar().await;

        event!(Level::INFO, "midpoint");

        bar().await;

        error!("1");
        warn!("2");
        info!("3");
        debug!("4");
        trace!("5");
    }

    #[instrument]
    async fn inside_test() {
        let test = serde_json::json!({
            "name": "test object",
            "value": 42
        });

        info!(message="This is a test message", developer="giovanni", intent="unit_test", intent_key="unit_test_key", data=%test);
    }

    #[instrument]
    async fn outside_test() {
        let _idea = "bright";
        inside_test().await
    }

    #[tokio::test]
    #[instrument]
    async fn run() {
        //println!("Name: {name}");
        crate::telemetry::telemetry_startup().await;

        fooz().await;
        println!("Test");
        outside_test().await;
        fooz().await;

        crate::telemetry::telemetry_shutdown().await;
    }
}
