pub mod api;
pub mod collector;
pub mod config;
pub mod errors;

#[tokio::main]
async fn main() {
    let config = config::read_config();
    let info = collector::start_collector(config).await.unwrap();
    api::api(info).await;
}
