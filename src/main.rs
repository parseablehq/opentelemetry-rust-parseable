use std::time::Duration;

use opentelemetry_parseable::telemetry;
use tokio::time::sleep;
use tracing::{event, info, instrument, Level};

#[tokio::main]
async fn main() {
    telemetry::telemetry_startup().await;

    foo().await;

    info!("This is a test message");

    event!(Level::INFO, "This is a span");
    telemetry::telemetry_shutdown().await;
}

#[instrument]
async fn foo() {
    sleep(Duration::from_secs(2)).await;
}
