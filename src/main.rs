use std::time::Duration;

use tokio::time::sleep;
use opentelemetry_parseable::telemetry;
use tracing::{Level, info, event, instrument};

#[tokio::main]
async fn main() {
    telemetry::telemetry_startup().await;

    foo().await;

    info!("This is a test message");

    event!(Level::INFO, "This is a span");
    //telemetry::telemetry_shutdown().await;
}

#[instrument]
async fn foo() {
    sleep(Duration::from_secs(2)).await;
}
