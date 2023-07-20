# Parseable OpenTelemetry Trace Exporter 

This repository contains OpenTelemetry rust sdk trace exporter that allows you to collect and export traces to Parseable. The exporter can be used directly with opentelemetry crate or used as a tracing subscriber using tracing-opentelemetry.

## Installation

To use this exporter in your Rust project, add the following dependency to your `Cargo.toml` file:

```toml
[dependencies]
opentelemetry-parseable = { git = "https://github.com/parseablehq/opentelemetry-rust-parseable.git"}
```

## Example

### Define Resource for Tracer

```rust
use opentelemetry::KeyValue;
use opentelemetry::sdk::Resource;
// Function to create the resource with key-value pairs
fn tracer_resource(service_name: &str) -> opentelemetry::sdk::Resource {
    let kvs = [
        KeyValue::new("user.real_name", whoami::realname()),
        KeyValue::new("user.user_name", whoami::username()),
        KeyValue::new("host.platform", whoami::platform().to_string()),
        KeyValue::new(
            opentelemetry_semantic_conventions::resource::HOST_ARCH,
            whoami::arch().to_string(),
        ),
        KeyValue::new(
            opentelemetry_semantic_conventions::resource::SERVICE_NAME,
            service_name,
        ),
        KeyValue::new(
            opentelemetry_semantic_conventions::resource::HOST_NAME,
            whoami::hostname(),
        ),
    ];
    Resource::new(kvs)
}
```

### Here is an example of how you can use the exporter with opentelemetry crate

```rust
use opentelemetry::{
    global,
    trace::{Span, Tracer},
};
use opentelemetry::{runtime, sdk::trace};
use tokio::time::{sleep, Duration};

use opentelemetry_parseable::ParseableExporterBuilder;

const SERVICE_NAME: &str = "my-service";

// startup function to initialize the exporter and tracing
fn install_global_tracer() {
    let config = trace::config().with_resource(tracer_resource(SERVICE_NAME));
    ParseableExporterBuilder::default()
        // service name provided here is used as the stream name in Parseable
        .with_service_name(SERVICE_NAME)
        .install_batch(runtime::Tokio, config)
        .expect("Unable to build and install parseable exporter");
}

// Main function to demonstrate the usage of the exporter
#[tokio::main]
async fn main() {
    install_global_tracer();
    // get a tracer from a provider
    let tracer = global::tracer("my_service");
    // start a new span
    let mut span = tracer.start("my_span");
    // set some attributes
    span.set_attribute(KeyValue::new("http.client_ip", "83.164.160.102"));
    // perform some more work...
    // end or drop the span to export
    span.end();
    // Sleep for a while to allow some traces to be captured
    sleep(Duration::from_secs(5)).await;
}

```
### Here is an example of how you can use the exporter with tracing opentelemetry crate

```rust

use opentelemetry::{runtime, sdk::trace};
use tokio::time::{sleep, Duration};
use tracing::*;
use tracing_subscriber::prelude::*;

use opentelemetry_parseable::ParseableExporterBuilder;

const SERVICE_NAME: &str = "my-service";

// startup function to initialize the exporter and tracing
fn install_global_tracer() {
    let config = trace::config().with_resource(tracer_resource(SERVICE_NAME));
    // Create and install the Parseable exporter
    let tracer = ParseableExporterBuilder::default()
        // service name provided here is used as the stream name in Parseable
        .with_service_name(SERVICE_NAME)
        .install_batch(runtime::Tokio, config)
        .expect("Unable to build parseable exporter");

    let collector =
        tracing_subscriber::registry().with(tracing_opentelemetry::layer().with_tracer(tracer));

    // Register the tracing subscriber globally
    if tracing::subscriber::set_global_default(collector).is_err() {
        eprintln!(
            "Error setting tracing subscriber, probably another subscriber has already been set?"
        );
    }
}

#[instrument]
async fn bar() {
    sleep(Duration::from_secs(1)).await;
}

#[instrument]
pub async fn foo() {
    info!("Calling bar");
    bar().await;
    trace!("bar returned");
}

// Main function to demonstrate the usage of the exporter
#[tokio::main]
async fn main() {
    // Initialize the telemetry exporter
    install_global_tracer();
    foo().await;

    // Sleep for a while to allow some traces to be captured
    sleep(Duration::from_secs(5)).await;
}

```

### Configuration
If you don't want to use builder methods to configure parseable instance to target, you can set following environment variables to configure the exporter instead

| Variable | Default |
|----|----|
| PARSEABLE_HOST | 0.0.0.0 |
| PARSEABLE_PORT | 8000 |
| PARSEABLE_USERNAME | admin |
| PARSEABLE_PASSWORD | admin |

Batch exporter can be configured from environment as well 


| Variable | Default |
|----|----|
| OTLP_QUEUE_SIZE | 65536 |
| OTLP_BATCH_SIZE | 8192 |
| OTLP_INTERVAL_MILLIS | 1000 |


## Contributing

If you encounter any issues, have suggestions, or want to contribute to the project, feel free to create an issue or submit a pull request on GitHub.

Happy tracing!