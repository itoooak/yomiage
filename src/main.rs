mod config;
mod converter;
mod server;
mod store;
mod worker;

use std::sync::Arc;

use anyhow::Context;
use config::Config;
use store::Store;
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_span_events(FmtSpan::CLOSE)
        .init();

    let config = Arc::new(Config::load()?);
    let store = Store::open(&config.data_dir).context("failed to open store")?;

    tokio::spawn(worker::run(config.clone(), store.clone()));

    server::run(config, store)
        .await
        .context("failed to run HTTP server")
}
