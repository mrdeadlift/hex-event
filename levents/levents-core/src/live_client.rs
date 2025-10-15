use super::{
    DaemonConfig, Event, EventBatch, EventKind, EventPayload, GoldEvent, ItemEvent, LevelEvent,
    PhaseEvent, PlayerEvent, PlayerRef, Team,
};
use anyhow::{Context, Result};
use async_stream::try_stream;
use futures_core::Stream;
use reqwest::Client;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;
use tracing::{trace, warn};
use xxhash_rust::xxh3::xxh3_64;

pub(super) fn live_event_stream(
    config: DaemonConfig,
    http: Client,
) -> impl Stream<Item = Result<EventBatch>> + Send {
    try_stream! {
        let mut ctx = PollContext::new(config, http);

        loop {
            let outcome = ctx.poll_once().await?;
            if !outcome.events.is_empty() {
                yield EventBatch { events: outcome.events };
            }
            sleep(outcome.next_delay).await;
        }
    }
}

struct PollContext {
    http: Client,
    config: DaemonConfig,
    digest: DigestState,
    players: PlayerRegistry,
    activity: ActivityState,
}

impl PollContext {
    fn new(config: DaemonConfig, http: Client) -> Self {
        Self {
            http,
            config,
            digest: DigestState::default(),
            players: PlayerRegistry::default(),
            activity: ActivityState::default(),
        }
    }

    async fn poll_once(&mut self) -> Result<PollOutcome> {
        let base = self.config.live_base_url.trim_end_matches('/');
        let active_url = format!("{base}/liveclientdata/activeplayer");
        let players_url = format!("{base}/liveclientdata/playerlist");
        let events_url = format!("{base}/liveclientdata/eventdata");

        // Active player metadata is only used as a lightweight hash to exercise the HTTPS path.
        if let Err(error) = fetch_endpoint(&self.http, &active_url).await.map(|resp| {
            self.digest.active_hash = Some(resp.hash);
        }) {
            trace!(?error, "live client activeplayer probe failed");
        }

        let players_resp = match fetch_endpoint(&self.http, &players_url).await {
            Ok(resp) => resp,
            Err(error) => {
                warn!(?error, "live client playerlist fetch failed");
                let delay = self.activity.on_error(&self.config);
                return Ok(PollOutcome::idle(delay));
            }
        };

        let events_resp = match fetch_endpoint(&self.http, &events_url).await {
            Ok(resp) => resp,
            Err(error) => {
                warn!(?error, "live client eventdata fetch failed");
                let delay = self.activity.on_error(&self.config);
                return Ok(PollOutcome::idle(delay));
            }
        };

        let now_ms = timestamp_ms();
        let mut events = Vec::new();

        if self.digest.players_hash != Some(players_resp.hash) {
            match parse_player_list(&players_resp.body) {
                Ok(list) => {
                    let mut diff_events = self.players.apply(list, now_ms);
                    events.append(&mut diff_events);
                    self.digest.players_hash = Some(players_resp.hash);
                }
                Err(error) => {
                    warn!(?error, "failed to parse playerlist response");
                    let delay = self.activity.on_error(&self.config);
                    return Ok(PollOutcome::idle(delay));
                }
            }
        }

        if self.digest.events_hash != Some(events_resp.hash) {
            match parse_event_list(&events_resp.body) {
                Ok(mut raw_events) => {
                    if self.digest.should_reset(&raw_events) {
                        self.digest.last_event_id = None;
                    }

                    let next_expected = self.digest.next_event_id();
                    let new_events: Vec<RawEvent> = raw_events
                        .drain(..)
                        .filter(|raw| raw.event_id >= next_expected)
                        .collect();

                    if let Some(max_id) = new_events.iter().map(|ev| ev.event_id).max() {
                        self.digest.last_event_id = Some(max_id);
                    }

                    let mut normalized = normalize_events(&new_events, &self.players);
                    events.append(&mut normalized);
                    self.digest.events_hash = Some(events_resp.hash);
                }
                Err(error) => {
                    warn!(?error, "failed to parse eventdata response");
                    let delay = self.activity.on_error(&self.config);
                    return Ok(PollOutcome::idle(delay));
                }
            }
        }

        events.sort_by_key(|event| event.ts);

        let next_delay = self.activity.on_poll(!events.is_empty(), &self.config);
        Ok(PollOutcome { events, next_delay })
    }
}

#[derive(Default)]
struct DigestState {
    active_hash: Option<u64>,
    players_hash: Option<u64>,
    events_hash: Option<u64>,
    last_event_id: Option<u64>,
}

impl DigestState {
    fn next_event_id(&self) -> u64 {
        self.last_event_id.map(|value| value + 1).unwrap_or(0)
    }

    fn should_reset(&self, events: &[RawEvent]) -> bool {
        if self.last_event_id.is_none() {
            return false;
        }

        events
            .iter()
            .any(|event| event.event_id == 0 && event.event_time < 5.0)
    }
}

#[derive(Default)]
struct PlayerRegistry {
    players: HashMap<String, PlayerSnapshot>,
}

impl PlayerRegistry {
    fn apply(&mut self, entries: Vec<PlayerListEntry>, ts_ms: u64) -> Vec<Event> {
        let mut new_players = HashMap::with_capacity(entries.len());
        let mut events = Vec::new();
        let mut used_slots: HashSet<u8> = self
            .players
            .values()
            .map(|snapshot| snapshot.reference.slot)
            .collect();

        for entry in entries {
            let team = parse_team(&entry.team);
            let name = entry.summoner_name.clone();
            let previous = self.players.get(&name);

            let slot = previous
                .map(|snapshot| snapshot.reference.slot)
                .unwrap_or_else(|| allocate_slot(team, &mut used_slots));

            used_slots.insert(slot);

            let snapshot = PlayerSnapshot::from_entry(entry, team, slot);
            if let Some(prev) = previous {
                let mut diff = snapshot.diff(prev, ts_ms);
                events.append(&mut diff);
            }

            new_players.insert(name, snapshot);
        }

        self.players = new_players;
        events
    }

    fn player_ref(&self, name: &str) -> Option<PlayerRef> {
        self.players
            .get(name)
            .map(|snapshot| snapshot.reference.clone())
    }
}

#[derive(Clone)]
struct PlayerSnapshot {
    reference: PlayerRef,
    level: u8,
    current_gold: i32,
    is_dead: bool,
    items: HashMap<u32, ItemEntry>,
}

impl PlayerSnapshot {
    fn from_entry(entry: PlayerListEntry, team: Team, slot: u8) -> Self {
        let reference = PlayerRef {
            summoner_name: entry.summoner_name.clone(),
            team,
            slot,
        };

        let current_gold = entry
            .current_gold
            .unwrap_or(0.0)
            .round()
            .clamp(i32::MIN as f64, i32::MAX as f64) as i32;

        Self {
            reference,
            level: entry.level.min(u8::MAX as u32) as u8,
            current_gold,
            is_dead: entry.is_dead,
            items: fold_items(entry.items),
        }
    }

    fn diff(&self, previous: &PlayerSnapshot, ts_ms: u64) -> Vec<Event> {
        let mut events = Vec::new();

        if self.level > previous.level {
            events.push(Event {
                kind: EventKind::LevelUp,
                ts: ts_ms,
                payload: EventPayload::PlayerLevel(LevelEvent {
                    player: self.reference.clone(),
                    level: self.level,
                }),
            });
        }

        let gold_delta = self.current_gold - previous.current_gold;
        if gold_delta != 0 {
            events.push(Event {
                kind: EventKind::GoldDelta,
                ts: ts_ms,
                payload: EventPayload::PlayerGold(GoldEvent {
                    player: self.reference.clone(),
                    delta: gold_delta,
                    total: self.current_gold,
                }),
            });
        }

        if previous.is_dead && !self.is_dead {
            events.push(Event {
                kind: EventKind::Respawn,
                ts: ts_ms,
                payload: EventPayload::Player(PlayerEvent {
                    player: self.reference.clone(),
                }),
            });
        }

        events.extend(self.diff_items(previous, ts_ms));

        events
    }

    fn diff_items(&self, previous: &PlayerSnapshot, ts_ms: u64) -> Vec<Event> {
        let mut events = Vec::new();
        let mut remaining = previous.items.clone();

        for (item_id, new_entry) in &self.items {
            match remaining.remove(item_id) {
                Some(old_entry) => {
                    if new_entry.count > old_entry.count {
                        push_item_events(
                            &mut events,
                            ts_ms,
                            &self.reference,
                            EventKind::ItemAdded,
                            *item_id,
                            new_entry.name.clone().or(old_entry.name.clone()),
                            new_entry.count - old_entry.count,
                        );
                    } else if new_entry.count < old_entry.count {
                        push_item_events(
                            &mut events,
                            ts_ms,
                            &self.reference,
                            EventKind::ItemRemoved,
                            *item_id,
                            old_entry.name.clone().or(new_entry.name.clone()),
                            old_entry.count - new_entry.count,
                        );
                    }
                }
                None => {
                    push_item_events(
                        &mut events,
                        ts_ms,
                        &self.reference,
                        EventKind::ItemAdded,
                        *item_id,
                        new_entry.name.clone(),
                        new_entry.count,
                    );
                }
            }
        }

        for (item_id, old_entry) in remaining {
            push_item_events(
                &mut events,
                ts_ms,
                &self.reference,
                EventKind::ItemRemoved,
                item_id,
                old_entry.name,
                old_entry.count,
            );
        }

        events
    }
}

#[derive(Clone)]
struct ItemEntry {
    count: u32,
    name: Option<String>,
}

struct ActivityState {
    level: PollActivity,
    last_activity: Instant,
}

impl ActivityState {
    fn on_poll(&mut self, had_activity: bool, config: &DaemonConfig) -> Duration {
        let now = Instant::now();

        if had_activity {
            self.level = PollActivity::Combat;
            self.last_activity = now;
        } else {
            match self.level {
                PollActivity::Combat => {
                    if now.duration_since(self.last_activity) >= config.combat_cooldown {
                        self.level = PollActivity::Normal;
                        self.last_activity = now;
                    }
                }
                PollActivity::Normal => {
                    if now.duration_since(self.last_activity) >= config.idle_cooldown {
                        self.level = PollActivity::Idle;
                        self.last_activity = now;
                    }
                }
                PollActivity::Idle => {}
            }
        }

        match self.level {
            PollActivity::Combat => config.poll_interval_combat,
            PollActivity::Normal => config.poll_interval_normal,
            PollActivity::Idle => config.poll_interval_idle,
        }
    }

    fn on_error(&mut self, config: &DaemonConfig) -> Duration {
        self.level = PollActivity::Idle;
        self.last_activity = Instant::now();
        config.error_backoff
    }
}

impl Default for ActivityState {
    fn default() -> Self {
        Self {
            level: PollActivity::Idle,
            last_activity: Instant::now(),
        }
    }
}

#[derive(Default)]
enum PollActivity {
    Combat,
    Normal,
    #[default]
    Idle,
}

struct PollOutcome {
    events: Vec<Event>,
    next_delay: Duration,
}

impl PollOutcome {
    fn idle(delay: Duration) -> Self {
        Self {
            events: Vec::new(),
            next_delay: delay,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
struct PlayerListEntry {
    #[serde(rename = "summonerName")]
    summoner_name: String,
    #[serde(rename = "team")]
    team: String,
    #[serde(rename = "level")]
    level: u32,
    #[serde(rename = "currentGold")]
    current_gold: Option<f64>,
    #[serde(rename = "isDead")]
    is_dead: bool,
    #[serde(default)]
    items: Vec<PlayerItemEntry>,
}

#[derive(Debug, Deserialize, Clone)]
struct PlayerItemEntry {
    #[serde(rename = "itemID")]
    item_id: u32,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EventListResponse {
    #[serde(rename = "Events")]
    events: Vec<RawEvent>,
}

#[derive(Debug, Deserialize)]
struct RawEvent {
    #[serde(rename = "EventID")]
    event_id: u64,
    #[serde(rename = "EventName")]
    event_name: String,
    #[serde(rename = "EventTime")]
    event_time: f64,
    #[serde(rename = "KillerName")]
    killer_name: Option<String>,
    #[serde(rename = "VictimName")]
    victim_name: Option<String>,
    #[serde(rename = "Assisters", default)]
    assisters: Vec<String>,
    #[serde(rename = "SummonerName")]
    summoner_name: Option<String>,
    #[serde(rename = "Level")]
    level: Option<u32>,
    #[serde(rename = "ItemID")]
    item_id: Option<u32>,
    #[serde(rename = "ItemName")]
    item_name: Option<String>,
}

#[derive(Debug)]
struct FetchResponse {
    hash: u64,
    body: Vec<u8>,
}

async fn fetch_endpoint(client: &Client, url: &str) -> Result<FetchResponse> {
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("request failed: GET {url}"))?;

    if !response.status().is_success() {
        anyhow::bail!("GET {url} -> {}", response.status());
    }

    let body = response.bytes().await?.to_vec();
    let hash = xxh3_64(&body);
    Ok(FetchResponse { hash, body })
}

fn parse_player_list(bytes: &[u8]) -> Result<Vec<PlayerListEntry>> {
    serde_json::from_slice(bytes).context("deserialize playerlist response")
}

fn parse_event_list(bytes: &[u8]) -> Result<Vec<RawEvent>> {
    let response: EventListResponse =
        serde_json::from_slice(bytes).context("deserialize eventdata response")?;
    Ok(response.events)
}

fn parse_team(team: &str) -> Team {
    match team {
        "ORDER" => Team::Order,
        "CHAOS" => Team::Chaos,
        _ => Team::Neutral,
    }
}

fn allocate_slot(team: Team, used: &mut HashSet<u8>) -> u8 {
    let (start, end) = match team {
        Team::Order => (0u8, 5u8),
        Team::Chaos => (5u8, 10u8),
        Team::Neutral => (0u8, 10u8),
    };

    for slot in start..end {
        if used.insert(slot) {
            return slot;
        }
    }

    // Fallback: reuse the first slot for the team if all are taken.
    start
}

fn fold_items(items: Vec<PlayerItemEntry>) -> HashMap<u32, ItemEntry> {
    let mut map = HashMap::new();
    for item in items {
        if item.item_id == 0 {
            continue;
        }

        let entry = map.entry(item.item_id).or_insert(ItemEntry {
            count: 0,
            name: item.display_name.clone(),
        });
        entry.count += 1;
        if entry.name.is_none() {
            entry.name = item.display_name.clone();
        }
    }
    map
}

fn normalize_events(raw_events: &[RawEvent], registry: &PlayerRegistry) -> Vec<Event> {
    let mut events = Vec::new();

    for raw in raw_events {
        let timestamp = seconds_to_millis(raw.event_time);
        match raw.event_name.as_str() {
            "ChampionKill" | "ChampionSpecialKill" => {
                if let Some(name) = raw.killer_name.as_ref() {
                    let reference = resolve_player(registry, name);
                    events.push(Event {
                        kind: EventKind::Kill,
                        ts: timestamp,
                        payload: EventPayload::Player(PlayerEvent { player: reference }),
                    });
                }
                if let Some(name) = raw.victim_name.as_ref() {
                    let reference = resolve_player(registry, name);
                    events.push(Event {
                        kind: EventKind::Death,
                        ts: timestamp,
                        payload: EventPayload::Player(PlayerEvent { player: reference }),
                    });
                }
                for assister in &raw.assisters {
                    if assister.is_empty() {
                        continue;
                    }
                    let reference = resolve_player(registry, assister);
                    events.push(Event {
                        kind: EventKind::Assist,
                        ts: timestamp,
                        payload: EventPayload::Player(PlayerEvent { player: reference }),
                    });
                }
            }
            "LevelUp" | "ItemPurchased" | "ItemDestroyed" | "ItemSold" | "ItemUndo" => {
                // These are covered by player diffing; skip duplicates.
            }
            "Respawn" => {
                if let Some(name) = raw.summoner_name.as_ref() {
                    let reference = resolve_player(registry, name);
                    events.push(Event {
                        kind: EventKind::Respawn,
                        ts: timestamp,
                        payload: EventPayload::Player(PlayerEvent { player: reference }),
                    });
                }
            }
            phase if is_phase_change(phase) => {
                events.push(Event {
                    kind: EventKind::PhaseChange,
                    ts: timestamp,
                    payload: EventPayload::Phase(PhaseEvent {
                        phase: phase.to_string(),
                    }),
                });
            }
            _ => {
                trace!(name = %raw.event_name, "unhandled live event");
            }
        }
    }

    events
}

fn push_item_events(
    events: &mut Vec<Event>,
    ts_ms: u64,
    player: &PlayerRef,
    kind: EventKind,
    item_id: u32,
    item_name: Option<String>,
    count: u32,
) {
    for _ in 0..count {
        events.push(Event {
            kind,
            ts: ts_ms,
            payload: EventPayload::PlayerItem(ItemEvent {
                player: player.clone(),
                item_id,
                item_name: item_name.clone(),
            }),
        });
    }
}

fn resolve_player(registry: &PlayerRegistry, name: &str) -> PlayerRef {
    registry
        .player_ref(name)
        .unwrap_or_else(|| neutral_player(name))
}

fn neutral_player(name: &str) -> PlayerRef {
    PlayerRef {
        summoner_name: name.to_string(),
        team: Team::Neutral,
        slot: 0,
    }
}

fn seconds_to_millis(value: f64) -> u64 {
    if !value.is_finite() || value < 0.0 {
        return 0;
    }
    (value * 1000.0).round() as u64
}

fn timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn is_phase_change(name: &str) -> bool {
    matches!(
        name,
        "GameStart"
            | "MinionsSpawning"
            | "FirstBrick"
            | "FirstBlood"
            | "TurretKilled"
            | "InhibKilled"
            | "InhibRespawningSoon"
            | "InhibRespawned"
            | "DragonKill"
            | "HeraldKill"
            | "BaronKill"
            | "GameEnd"
            | "Ace"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_player_entry(
        name: &str,
        team: &str,
        level: u32,
        gold: f64,
        is_dead: bool,
        items: Vec<PlayerItemEntry>,
    ) -> PlayerListEntry {
        PlayerListEntry {
            summoner_name: name.to_string(),
            team: team.to_string(),
            level,
            current_gold: Some(gold),
            is_dead,
            items,
        }
    }

    fn make_item(id: u32, name: &str) -> PlayerItemEntry {
        PlayerItemEntry {
            item_id: id,
            display_name: Some(name.to_string()),
        }
    }

    #[test]
    fn registry_emits_level_gold_and_item_events() {
        let mut registry = PlayerRegistry::default();
        let baseline = vec![
            make_player_entry("Alpha", "ORDER", 1, 500.0, false, vec![]),
            make_player_entry("Bravo", "CHAOS", 1, 300.0, false, vec![]),
        ];

        assert!(registry.apply(baseline.clone(), 1_000).is_empty());

        let updated = vec![
            make_player_entry(
                "Alpha",
                "ORDER",
                2,
                650.0,
                false,
                vec![make_item(1055, "Doran's Blade")],
            ),
            make_player_entry("Bravo", "CHAOS", 1, 300.0, false, vec![]),
        ];

        let events = registry.apply(updated, 2_000);
        assert_eq!(events.len(), 3);
        assert!(events
            .iter()
            .any(|event| matches!(event.kind, EventKind::LevelUp)));
        assert!(events
            .iter()
            .any(|event| matches!(event.kind, EventKind::GoldDelta)));
        assert!(events
            .iter()
            .any(|event| matches!(event.kind, EventKind::ItemAdded)));
    }

    #[test]
    fn normalize_events_emits_combat_events() {
        let mut registry = PlayerRegistry::default();
        let baseline = vec![
            make_player_entry("Alpha", "ORDER", 1, 500.0, false, vec![]),
            make_player_entry("Bravo", "CHAOS", 1, 300.0, false, vec![]),
            make_player_entry("Charlie", "ORDER", 1, 200.0, false, vec![]),
        ];
        registry.apply(baseline, 1_000);

        let raw = RawEvent {
            event_id: 1,
            event_name: "ChampionKill".to_string(),
            event_time: 12.5,
            killer_name: Some("Alpha".to_string()),
            victim_name: Some("Bravo".to_string()),
            assisters: vec!["Charlie".to_string()],
            summoner_name: None,
            level: None,
            item_id: None,
            item_name: None,
        };

        let events = normalize_events(&[raw], &registry);
        assert_eq!(events.len(), 3);
        assert!(events.iter().any(|event| event.kind == EventKind::Kill));
        assert!(events.iter().any(|event| event.kind == EventKind::Death));
        assert!(events.iter().any(|event| event.kind == EventKind::Assist));
    }

    #[test]
    fn activity_state_scales_intervals() {
        let config = DaemonConfig::default();
        let mut state = ActivityState::default();

        let combat = state.on_poll(true, &config);
        assert_eq!(combat, config.poll_interval_combat);

        state.last_activity = state
            .last_activity
            .checked_sub(config.combat_cooldown)
            .unwrap();
        let normal = state.on_poll(false, &config);
        assert_eq!(normal, config.poll_interval_normal);

        state.last_activity = state
            .last_activity
            .checked_sub(config.idle_cooldown)
            .unwrap();
        let idle = state.on_poll(false, &config);
        assert_eq!(idle, config.poll_interval_idle);

        let backoff = state.on_error(&config);
        assert_eq!(backoff, config.error_backoff);
    }
}
