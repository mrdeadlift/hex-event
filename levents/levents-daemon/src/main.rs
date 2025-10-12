use anyhow::Result;
use levents_core::{DaemonConfig, LiveDaemon};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let daemon = LiveDaemon::new(DaemonConfig::default());
    let batch = daemon.bootstrap().await?;
    info!(events = batch.events.len(), "daemon bootstrap complete");

    Ok(())
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
