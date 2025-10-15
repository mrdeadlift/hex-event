use super::{DaemonConfig, Event, EventBatch, EventKind, EventPayload, PhaseEvent};
use anyhow::{anyhow, Context, Result};
use async_stream::try_stream;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use futures_util::{SinkExt, StreamExt};
use http::header::{AUTHORIZATION, ORIGIN};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use std::collections::HashSet;
use std::env;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs;
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async_tls_with_config, Connector};
use tracing::{debug, trace, warn};

use rustls::client::{ServerCertVerified, ServerCertVerifier, ServerName};
use rustls::{Certificate, ClientConfig, Error as RustlsError, RootCertStore};

const GAMEFLOW_URI: &str = "/lol-gameflow/v1/gameflow-phase";

pub(super) fn lcu_event_stream(
    config: DaemonConfig,
    http: Client,
) -> impl futures_core::Stream<Item = Result<EventBatch>> + Send {
    try_stream! {
        let mut last_phase: Option<String> = None;

        loop {
            let candidates = lockfile_candidates(&config);
            let (path, auth) = match load_lockfile(&candidates).await {
                Ok(value) => value,
                Err(error) => {
                    trace!(?error, "lockfile unavailable");
                    sleep(config.lcu_discovery_interval).await;
                    continue;
                }
            };

            trace!(path = %path.display(), "lockfile discovered");

            match connect_websocket(&auth).await {
                Ok(mut socket) => {
                    debug!(port = auth.port, "LCU websocket connected");

                    if let Err(error) = subscribe(&mut socket).await {
                        warn!(?error, "failed to subscribe to LCU events");
                        sleep(config.lcu_retry_delay).await;
                        continue;
                    }

                    if let Ok(Some(phase)) = fetch_current_phase(&http, &auth).await {
                        if last_phase.as_deref() != Some(phase.as_str()) {
                            let event = phase_event(&phase);
                            last_phase = Some(phase);
                            yield EventBatch { events: vec![event] };
                        }
                    }

                    loop {
                        match socket.next().await {
                            Some(Ok(Message::Text(text))) => {
                                if let Some(phase) = parse_phase_message(&text) {
                                    if last_phase.as_deref() != Some(phase.as_str()) {
                                        trace!(phase = %phase, "LCU phase update");
                                        let event = phase_event(&phase);
                                        last_phase = Some(phase);
                                        yield EventBatch { events: vec![event] };
                                    }
                                }
                            }
                            Some(Ok(Message::Ping(payload))) => {
                                if let Err(error) = socket.send(Message::Pong(payload)).await {
                                    warn!(?error, "failed to respond to LCU ping");
                                    break;
                                }
                            }
                            Some(Ok(Message::Pong(_))) => {}
                            Some(Ok(Message::Binary(_))) => {}
                            Some(Ok(Message::Frame(_))) => {}
                            Some(Ok(Message::Close(_))) => {
                                trace!("LCU websocket closed by peer");
                                break;
                            }
                            Some(Err(error)) => {
                                warn!(?error, "LCU websocket error");
                                break;
                            }
                            None => break,
                        }
                    }
                }
                Err(error) => {
                    warn!(?error, "failed to connect to LCU websocket");
                }
            }

            sleep(config.lcu_retry_delay).await;
        }
    }
}

#[derive(Debug, Clone)]
struct LockfileAuth {
    port: u16,
    password: String,
    protocol: String,
}

impl LockfileAuth {
    fn websocket_url(&self) -> String {
        let scheme = if self.protocol.eq_ignore_ascii_case("https") {
            "wss"
        } else {
            "ws"
        };
        format!("{scheme}://127.0.0.1:{}/", self.port)
    }

    fn base_url(&self) -> String {
        format!("{}://127.0.0.1:{}", self.protocol, self.port)
    }

    fn basic_token(&self) -> String {
        STANDARD.encode(format!("riot:{}", self.password))
    }
}

async fn connect_websocket(
    auth: &LockfileAuth,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
> {
    let url = auth.websocket_url();
    let mut request = url
        .into_client_request()
        .context("construct websocket request")?;

    let header_value = format!("Basic {}", auth.basic_token());
    request.headers_mut().insert(
        AUTHORIZATION,
        header_value.parse().context("authorization header")?,
    );
    request.headers_mut().insert(
        ORIGIN,
        "https://127.0.0.1".parse().expect("static origin header"),
    );

    let connector = Connector::Rustls(Arc::new(build_tls_config()));
    let (stream, _) = connect_async_tls_with_config(request, None, false, Some(connector))
        .await
        .context("connect LCU websocket")?;

    Ok(stream)
}

async fn subscribe(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Result<()> {
    let payload = Message::Text("[\"subscribe\",\"OnJsonApiEvent\"]".to_string());
    socket
        .send(payload)
        .await
        .context("subscribe OnJsonApiEvent")?;

    let gameflow_payload = Message::Text(format!("[\"subscribe\",\"{GAMEFLOW_URI}\"]"));
    if let Err(error) = socket.send(gameflow_payload).await {
        trace!(?error, "secondary subscription failed");
    }
    Ok(())
}

async fn fetch_current_phase(http: &Client, auth: &LockfileAuth) -> Result<Option<String>> {
    let url = format!("{}/lol-gameflow/v1/gameflow-phase", auth.base_url());
    let response = http
        .get(&url)
        .basic_auth("riot", Some(&auth.password))
        .send()
        .await
        .with_context(|| format!("request failed: GET {url}"))?;

    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }

    if !response.status().is_success() {
        anyhow::bail!("GET {url} -> {}", response.status());
    }

    let body = response.text().await.unwrap_or_default();
    let trimmed = body.trim();

    if trimmed.is_empty() {
        return Ok(None);
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        if let Some(phase) = extract_phase(&value) {
            return Ok(Some(phase));
        }
        if let Some(phase) = value.as_str() {
            return Ok(Some(phase.to_string()));
        }
    }

    Ok(Some(trimmed.trim_matches('"').to_string()))
}

async fn load_lockfile(candidates: &[PathBuf]) -> Result<(PathBuf, LockfileAuth)> {
    for path in candidates {
        match fs::read_to_string(path).await {
            Ok(contents) => match parse_lockfile(path, &contents) {
                Ok(auth) => return Ok((path.clone(), auth)),
                Err(error) => {
                    warn!(path = %path.display(), ?error, "invalid lockfile contents");
                    continue;
                }
            },
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => {
                trace!(path = %path.display(), ?error, "lockfile read failed");
                continue;
            }
        }
    }

    Err(anyhow!("LCU lockfile not found"))
}

fn parse_lockfile(path: &Path, raw: &str) -> Result<LockfileAuth> {
    let line = raw.trim();
    if line.is_empty() {
        anyhow::bail!("{path:?} is empty");
    }

    let parts: Vec<&str> = line.split(':').collect();
    if parts.len() < 5 {
        anyhow::bail!("{path:?} corrupted lockfile");
    }

    let port: u16 = parts[2]
        .parse()
        .with_context(|| format!("parse port from {path:?}"))?;

    Ok(LockfileAuth {
        port,
        password: parts[3].to_string(),
        protocol: parts[4].to_ascii_lowercase(),
    })
}

fn phase_event(phase: &str) -> Event {
    Event {
        kind: EventKind::PhaseChange,
        ts: timestamp_ms(),
        payload: EventPayload::Phase(PhaseEvent {
            phase: phase.to_string(),
        }),
    }
}

fn parse_phase_message(payload: &str) -> Option<String> {
    let value: Value = serde_json::from_str(payload).ok()?;
    extract_phase(&value)
}

fn extract_phase(value: &Value) -> Option<String> {
    match value {
        Value::Array(items) => {
            if items.len() >= 3 {
                if items[0].as_str() == Some("OnJsonApiEvent") {
                    if items[1].as_str() == Some(GAMEFLOW_URI) {
                        return items.get(2)?.as_str().map(|s| s.to_string());
                    }
                }

                if items[1].as_str() == Some("OnJsonApiEvent") {
                    if let Some(candidate) = items.get(2) {
                        if let Some(phase) = extract_phase(candidate) {
                            return Some(phase);
                        }
                    }
                }
            }

            for item in items {
                if let Some(phase) = extract_phase(item) {
                    return Some(phase);
                }
            }
            None
        }
        Value::Object(map) => {
            match map.get("uri").and_then(Value::as_str) {
                Some(GAMEFLOW_URI) => {}
                _ => return None,
            }

            if let Some(data) = map.get("data") {
                if let Some(phase) = data.as_str() {
                    return Some(phase.to_string());
                }
                if let Value::Object(obj) = data {
                    if let Some(phase) = obj
                        .get("phase")
                        .and_then(Value::as_str)
                        .or_else(|| obj.get("gameflowPhase").and_then(Value::as_str))
                    {
                        return Some(phase.to_string());
                    }
                }
            }

            None
        }
        _ => None,
    }
}

fn lockfile_candidates(config: &DaemonConfig) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();

    let mut push = |path: PathBuf| {
        let normalized = normalize_path(path);
        if seen.insert(normalized.clone()) {
            candidates.push(normalized);
        }
    };

    if let Some(path) = config.lcu_lockfile.clone() {
        push(path);
    }

    if let Ok(env_path) = env::var("LEVENTS_LCU_LOCKFILE") {
        if !env_path.trim().is_empty() {
            push(PathBuf::from(env_path));
        }
    }

    for path in default_lockfile_paths() {
        push(path);
    }

    candidates
}

fn normalize_path(path: PathBuf) -> PathBuf {
    if let Some(input) = path.to_str() {
        if input == "~" {
            if let Some(home) = home_dir() {
                return home;
            }
        } else if let Some(stripped) = input.strip_prefix("~/") {
            if let Some(home) = home_dir() {
                return home.join(stripped);
            }
        }
    }
    path
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))
}

fn default_lockfile_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    #[cfg(target_os = "windows")]
    {
        if let Some(local) = env::var_os("LOCALAPPDATA") {
            let base = PathBuf::from(local);
            paths.push(
                base.join("Riot Games")
                    .join("Riot Client")
                    .join("Config")
                    .join("lockfile"),
            );
            paths.push(
                base.join("Riot Games")
                    .join("League of Legends")
                    .join("lockfile"),
            );
        }

        paths.push(PathBuf::from(r"C:\Riot Games\League of Legends\lockfile"));
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = home_dir() {
            paths.push(
                home.join("Library")
                    .join("Application Support")
                    .join("League of Legends")
                    .join("lockfile"),
            );
        }
        paths.push(PathBuf::from(
            "/Applications/League of Legends.app/Contents/LoL/lockfile",
        ));
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(home) = home_dir() {
            paths.push(
                home.join(".config")
                    .join("League of Legends")
                    .join("lockfile"),
            );
            paths.push(
                home.join(".local")
                    .join("share")
                    .join("league-of-legends")
                    .join("lockfile"),
            );
            paths.push(
                home.join("Games")
                    .join("league-of-legends")
                    .join("lockfile"),
            );
        }
    }

    paths
}

fn build_tls_config() -> ClientConfig {
    let roots = RootCertStore::empty();
    let mut config = ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(roots)
        .with_no_client_auth();

    config
        .dangerous()
        .set_certificate_verifier(Arc::new(NoCertificateVerification));
    config
}

struct NoCertificateVerification;

impl ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &Certificate,
        _intermediates: &[Certificate],
        _server_name: &ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: SystemTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        Ok(ServerCertVerified::assertion())
    }
}

fn timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_lockfile_line() {
        let line = "LeagueClient:1234:5678:secret:https";
        let parsed = parse_lockfile(Path::new("/tmp/lockfile"), line).expect("lockfile");
        assert_eq!(parsed.port, 5678);
        assert_eq!(parsed.password, "secret");
        assert_eq!(parsed.protocol, "https");
    }

    #[test]
    fn parse_phase_variants() {
        let variant_a = "[\"OnJsonApiEvent\",\"/lol-gameflow/v1/gameflow-phase\",\"Lobby\"]";
        assert_eq!(parse_phase_message(variant_a), Some("Lobby".to_string()));

        let variant_b = "[8,\"OnJsonApiEvent\",{\"uri\":\"/lol-gameflow/v1/gameflow-phase\",\"eventType\":\"Update\",\"data\":\"ChampSelect\"}]";
        assert_eq!(
            parse_phase_message(variant_b),
            Some("ChampSelect".to_string())
        );

        let variant_c = "[8,\"OnJsonApiEvent\",{\"uri\":\"/lol-gameflow/v1/gameflow-phase\",\"eventType\":\"Update\",\"data\":{\"phase\":\"ReadyCheck\"}}]";
        assert_eq!(
            parse_phase_message(variant_c),
            Some("ReadyCheck".to_string())
        );
    }
}
