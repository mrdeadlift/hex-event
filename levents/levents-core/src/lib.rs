//! Core runtime primitives for the levents daemon.

mod lcu;
mod live_client;

use anyhow::Result;
use futures_core::Stream;
use levents_model::{
    Event, EventBatch, EventKind, EventPayload, GoldEvent, HeartbeatEvent, ItemEvent, LevelEvent,
    PhaseEvent, PlayerEvent, PlayerRef, Team,
};
use parking_lot::Mutex;
use reqwest::Client;
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::time::{sleep, Instant};
use tracing::{debug, instrument, warn};

/// Configuration passed to the daemon when bootstrapping.
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub heartbeat_interval: Duration,
    pub live_base_url: String,
    /// Interval used while the player is in combat/high activity.
    pub poll_interval_combat: Duration,
    /// Interval used for normal gameplay moments.
    pub poll_interval_normal: Duration,
    /// Interval used while the game is idle or in low activity.
    pub poll_interval_idle: Duration,
    /// Cooldown before downgrading from combat to normal.
    pub combat_cooldown: Duration,
    /// Cooldown before downgrading from normal activity to idle.
    pub idle_cooldown: Duration,
    /// Backoff used when the Live Client endpoints cannot be reached.
    pub error_backoff: Duration,
    /// Optional override pointing at the League Client lockfile location.
    pub lcu_lockfile: Option<PathBuf>,
    /// Interval used while the lockfile is missing; controls discovery polling.
    pub lcu_discovery_interval: Duration,
    /// Delay before attempting to reconnect after an LCU websocket disconnect.
    pub lcu_retry_delay: Duration,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval: Duration::from_secs(1),
            live_base_url: "https://127.0.0.1:2999".to_string(),
            poll_interval_combat: Duration::from_millis(150),
            poll_interval_normal: Duration::from_millis(750),
            poll_interval_idle: Duration::from_millis(1500),
            combat_cooldown: Duration::from_secs(5),
            idle_cooldown: Duration::from_secs(20),
            error_backoff: Duration::from_secs(1),
            lcu_lockfile: None,
            lcu_discovery_interval: Duration::from_secs(1),
            lcu_retry_delay: Duration::from_secs(2),
        }
    }
}

/// Shared state for the daemon runtime.
#[derive(Clone)]
pub struct LiveDaemon {
    config: DaemonConfig,
    http: Client,
    seq: Arc<Mutex<u64>>,
}

impl LiveDaemon {
    /// Construct the daemon with a default `reqwest` client.
    pub fn new(config: DaemonConfig) -> Self {
        let client = Client::builder()
            .danger_accept_invalid_certs(true)
            .use_rustls_tls()
            .build()
            .expect("infallible TLS configuration");

        Self::with_client(config, client)
    }

    /// Construct the daemon with a caller-provided `reqwest` client (useful for tests).
    pub fn with_client(config: DaemonConfig, http: Client) -> Self {
        Self {
            config,
            http,
            seq: Arc::new(Mutex::new(0)),
        }
    }

    /// Returns a reference to the internal HTTP client.
    pub fn http_client(&self) -> &Client {
        &self.http
    }

    /// Spawn an asynchronous stream that polls the Live Client Data endpoints and emits
    /// normalized event batches with adaptive scheduling.
    pub fn live_events(&self) -> impl Stream<Item = Result<EventBatch>> + Send + 'static {
        live_client::live_event_stream(self.config.clone(), self.http.clone())
    }

    /// Spawn a websocket-backed stream that proxies LCU phase changes.
    pub fn lcu_events(&self) -> impl Stream<Item = Result<EventBatch>> + Send + 'static {
        lcu::lcu_event_stream(self.config.clone(), self.http.clone())
    }

    /// Perform a lightweight bootstrap routine to prove that async runtime wiring works.
    #[instrument(name = "levents.bootstrap", skip(self))]
    pub async fn bootstrap(&self) -> Result<EventBatch> {
        let start = Instant::now();
        // Simulate discovering local client capabilities.
        sleep(Duration::from_millis(10)).await;
        let metadata = self.fetch_metadata().await.unwrap_or_else(|error| {
            warn!(%error, "metadata fetch failed, falling back to stub");
            json!({"source": "stub"})
        });

        let seq = {
            let mut guard = self.seq.lock();
            *guard += 1;
            *guard
        };

        let event = Event {
            kind: EventKind::Heartbeat,
            ts: start.elapsed().as_millis() as u64,
            payload: EventPayload::Heartbeat(HeartbeatEvent { seq }),
        };

        debug!(?metadata, "bootstrap metadata ready");
        Ok(EventBatch {
            events: vec![event],
        })
    }

    /// Fetch basic metadata from the live client REST endpoint. For now this method returns
    /// placeholder data to avoid hard coupling with a running client during initialization.
    async fn fetch_metadata(&self) -> Result<Value> {
        // We issue a HEAD request purely to exercise the async HTTP stack. Errors are fine because
        // the daemon will use a stub fallback during early development.
        let url = format!("{}/liveclientdata/activeplayer", self.config.live_base_url);
        let _ = self.http.head(url).send().await;
        Ok(json!({"status": "unreachable"}))
    }

    /// Construct a synthetic kill event used by smoke-tests.
    pub fn synthetic_kill(&self, summoner: &str) -> Event {
        Event {
            kind: EventKind::Kill,
            ts: 0,
            payload: EventPayload::Player(PlayerEvent {
                player: PlayerRef {
                    summoner_name: summoner.to_string(),
                    team: Team::Order,
                    slot: 0,
                },
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bootstrap_produces_heartbeat() {
        let daemon = LiveDaemon::new(DaemonConfig::default());
        let batch = daemon.bootstrap().await.expect("bootstrap");
        assert_eq!(batch.events.len(), 1);
        assert!(matches!(
            batch.events[0].payload,
            EventPayload::Heartbeat(_)
        ));
    }

    #[test]
    fn synthetic_kill_contains_summoner() {
        let daemon = LiveDaemon::new(DaemonConfig::default());
        let event = daemon.synthetic_kill("Example");
        if let EventPayload::Player(player) = event.payload {
            assert_eq!(player.player.summoner_name, "Example");
        } else {
            panic!("expected player payload");
        }
    }
}
