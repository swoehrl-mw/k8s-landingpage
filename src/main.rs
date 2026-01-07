pub mod api;
pub mod collector;
pub mod config;
pub mod errors;

// Avoid musl's default allocator due to lackluster performance
// https://nickb.dev/blog/default-musl-allocator-considered-harmful-to-performance
#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() {
    init_logging();
    let config = config::read_config();
    let info = collector::start_collector(config).await.unwrap();
    api::api(info).await;
}

fn init_logging() {
    use tracing_subscriber::{EnvFilter, prelude::*};
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();

    let subscriber = tracing_subscriber::registry().with(filter);

    let log_mode = std::env::var("LOGGING_MODE").unwrap_or_else(|_| "plain".to_string());
    if log_mode.to_lowercase() == "json" {
        subscriber
            .with(tracing_subscriber::fmt::layer().json())
            .init();
    } else {
        subscriber.with(tracing_subscriber::fmt::layer()).init();
    }
}
