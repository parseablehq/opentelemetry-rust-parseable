use tracing::instrument;

mod common;

mod example {

    use tokio::time::{sleep, Duration};
    use tracing::*;

    #[instrument]
    async fn bar() {
        sleep(Duration::from_secs(5)).await;
    }

    #[instrument]
    pub async fn fooz() {
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
    pub async fn outside_test() {
        let _idea = "bright";
        inside_test().await
    }
}

#[tokio::test]
#[instrument]
async fn run() {
    //println!("Name: {name}");
    common::telemetry_startup().await;
    example::fooz().await;
    example::outside_test().await;
    common::telemetry_shutdown().await;
}
