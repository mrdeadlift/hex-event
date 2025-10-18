use std::net::SocketAddr;

use anyhow::{Context, Result};
use levents_core::{DaemonConfig, LiveDaemon};

mod grpc;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let daemon = LiveDaemon::new(DaemonConfig::default());

    let addr: SocketAddr = std::env::var("LEVENTS_GRPC_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:50051".to_string())
        .parse()
        .context("failed to parse LEVENTS_GRPC_ADDR")?;

    grpc::serve(daemon, addr).await
}

fn init_tracing() {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    let fmt_layer = fmt::layer().with_target(false);
    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .expect("env filter");

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .init();
}
