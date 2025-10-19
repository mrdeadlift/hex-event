use std::{collections::HashSet, net::SocketAddr, sync::Arc};

use anyhow::{Context, Result};
use futures_util::StreamExt;
use levents_core::LiveDaemon;
use levents_model::{AbilitySlot, Event, EventBatch, EventKind, EventPayload, PlayerRef, Team};
use tokio::sync::broadcast;
use tokio_stream::wrappers::{errors::BroadcastStreamRecvError, BroadcastStream};
use tonic::{transport::Server, Request, Response, Status};
use tracing::{info, trace, warn};

pub mod pb {
    tonic::include_proto!("levents.v1");
}

use pb::control_request::Command as ControlCommand;
use pb::event::Payload as EventPayloadProto;
use pb::event_service_server::{EventService, EventServiceServer};
use pb::{
    ControlRequest, ControlResponse, EmitSyntheticKill, Event as EventProto,
    EventKind as EventKindProto, SubscribeRequest, Team as TeamProto,
};

const BROADCAST_CAPACITY: usize = 256;

#[derive(Clone)]
struct ServerState {
    daemon: LiveDaemon,
    sender: broadcast::Sender<Event>,
}

impl ServerState {
    fn new(daemon: LiveDaemon) -> Self {
        let (sender, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self { daemon, sender }
    }

    fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    fn spawn_sources(self: &Arc<Self>) {
        self.spawn_stream(self.daemon.live_events());
        self.spawn_stream(self.daemon.lcu_events());
    }

    fn spawn_stream<S>(&self, stream: S)
    where
        S: futures_core::Stream<Item = anyhow::Result<EventBatch>> + Send + 'static,
    {
        let sender = self.sender.clone();
        tokio::spawn(async move {
            let mut stream = Box::pin(stream);
            while let Some(result) = stream.next().await {
                match result {
                    Ok(batch) => {
                        for event in batch.events {
                            if sender.send(event).is_err() {
                                trace!("no active subscribers; dropping event");
                                break;
                            }
                        }
                    }
                    Err(error) => {
                        warn!(?error, "event source error");
                    }
                }
            }
        });
    }

    fn emit_batch(&self, batch: EventBatch) {
        for event in batch.events {
            if self.sender.send(event).is_err() {
                trace!("no active subscribers for bootstrap event");
                break;
            }
        }
    }

    fn emit_event(&self, event: Event) {
        if self.sender.send(event).is_err() {
            trace!("no active subscribers for control event");
        }
    }
}

#[derive(Clone)]
struct EventStreamService {
    state: Arc<ServerState>,
}

impl EventStreamService {
    fn new(state: Arc<ServerState>) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl EventService for EventStreamService {
    type SubscribeStream =
        std::pin::Pin<Box<dyn tokio_stream::Stream<Item = Result<EventProto, Status>> + Send>>;

    async fn subscribe(
        &self,
        request: Request<SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        let request = request.into_inner();
        let filter = allowed_kinds(&request);

        let receiver = self.state.subscribe();

        let stream = async_stream::try_stream! {
            let mut stream = BroadcastStream::new(receiver);
            while let Some(item) = stream.next().await {
                match item {
                    Ok(event) => {
                        let proto_kind = map_event_kind(&event.kind);
                        if let Some(ref allowed) = filter {
                            if !allowed.contains(&proto_kind) {
                                continue;
                            }
                        }

                        match convert_event(event) {
                            Ok(proto) => yield proto,
                            Err(error) => {
                                warn!(?error, "failed to convert event to proto");
                            }
                        }
                    }
                    Err(BroadcastStreamRecvError::Lagged(skipped)) => {
                        warn!(skipped, "subscriber lagged; dropping events");
                    }
                }
            }
        };

        Ok(Response::new(Box::pin(stream)))
    }

    async fn control(
        &self,
        request: Request<ControlRequest>,
    ) -> Result<Response<ControlResponse>, Status> {
        let request = request.into_inner();

        let command = request
            .command
            .ok_or_else(|| Status::invalid_argument("missing control command"))?;

        match command {
            ControlCommand::EmitSyntheticKill(EmitSyntheticKill { summoner_name }) => {
                if summoner_name.trim().is_empty() {
                    return Err(Status::invalid_argument("summoner_name is required"));
                }

                let event = self.state.daemon.synthetic_kill(&summoner_name);
                self.state.emit_event(event);
                let response = ControlResponse {
                    accepted: true,
                    message: format!("synthetic kill issued for {summoner_name}"),
                };
                Ok(Response::new(response))
            }
        }
    }
}

pub async fn serve(daemon: LiveDaemon, addr: SocketAddr) -> Result<()> {
    let bootstrap = daemon.bootstrap().await?;
    info!(events = bootstrap.events.len(), "daemon bootstrap complete");

    let state = Arc::new(ServerState::new(daemon));
    state.emit_batch(bootstrap);
    state.spawn_sources();

    info!(%addr, "starting gRPC server");
    Server::builder()
        .add_service(EventServiceServer::new(EventStreamService::new(state)))
        .serve(addr)
        .await
        .context("gRPC server exited")?;

    Ok(())
}

fn allowed_kinds(request: &SubscribeRequest) -> Option<HashSet<EventKindProto>> {
    let kinds: HashSet<_> = request
        .kinds
        .iter()
        .filter_map(|value| EventKindProto::from_i32(*value))
        .filter(|kind| *kind != EventKindProto::Unspecified)
        .collect();

    if kinds.is_empty() {
        None
    } else {
        Some(kinds)
    }
}

fn convert_event(event: Event) -> Result<EventProto, serde_json::Error> {
    let payload = match event.payload {
        EventPayload::Player(inner) => Some(EventPayloadProto::Player(pb::PlayerEvent {
            player: Some(convert_player_ref(inner.player)),
        })),
        EventPayload::PlayerItem(inner) => Some(EventPayloadProto::PlayerItem(pb::ItemEvent {
            player: Some(convert_player_ref(inner.player)),
            item_id: inner.item_id,
            item_name: inner.item_name,
        })),
        EventPayload::PlayerLevel(inner) => Some(EventPayloadProto::PlayerLevel(pb::LevelEvent {
            player: Some(convert_player_ref(inner.player)),
            level: inner.level as u32,
        })),
        EventPayload::PlayerSkillLevel(inner) => Some(EventPayloadProto::PlayerSkillLevel(
            pb::SkillLevelEvent {
                player: Some(convert_player_ref(inner.player)),
                ability: map_ability(inner.ability) as i32,
                level: inner.level as u32,
            },
        )),
        EventPayload::PlayerGold(inner) => Some(EventPayloadProto::PlayerGold(pb::GoldEvent {
            player: Some(convert_player_ref(inner.player)),
            delta: inner.delta,
            total: inner.total,
        })),
        EventPayload::Phase(inner) => Some(EventPayloadProto::Phase(pb::PhaseEvent {
            phase: inner.phase,
        })),
        EventPayload::Heartbeat(inner) => Some(EventPayloadProto::Heartbeat(pb::HeartbeatEvent {
            seq: inner.seq,
        })),
        EventPayload::Custom(inner) => Some(EventPayloadProto::Custom(pb::CustomEvent {
            json: serde_json::to_string(&inner)?,
        })),
    };

    Ok(EventProto {
        kind: map_event_kind(&event.kind) as i32,
        ts: event.ts,
        payload,
    })
}

fn convert_player_ref(reference: PlayerRef) -> pb::PlayerRef {
    pb::PlayerRef {
        summoner_name: reference.summoner_name,
        team: map_team(reference.team) as i32,
        slot: reference.slot as u32,
    }
}

fn map_event_kind(kind: &EventKind) -> EventKindProto {
    match kind {
        EventKind::Kill => EventKindProto::Kill,
        EventKind::Death => EventKindProto::Death,
        EventKind::Assist => EventKindProto::Assist,
        EventKind::LevelUp => EventKindProto::LevelUp,
        EventKind::SkillLevelUp => EventKindProto::SkillLevelUp,
        EventKind::ItemAdded => EventKindProto::ItemAdded,
        EventKind::ItemRemoved => EventKindProto::ItemRemoved,
        EventKind::GoldDelta => EventKindProto::GoldDelta,
        EventKind::Respawn => EventKindProto::Respawn,
        EventKind::PhaseChange => EventKindProto::PhaseChange,
        EventKind::Heartbeat => EventKindProto::Heartbeat,
    }
}

fn map_team(team: Team) -> TeamProto {
    match team {
        Team::Order => TeamProto::Order,
        Team::Chaos => TeamProto::Chaos,
        Team::Neutral => TeamProto::Neutral,
    }
}

fn map_ability(slot: AbilitySlot) -> pb::AbilitySlot {
    match slot {
        AbilitySlot::Q => pb::AbilitySlot::Q,
        AbilitySlot::W => pb::AbilitySlot::W,
        AbilitySlot::E => pb::AbilitySlot::E,
        AbilitySlot::R => pb::AbilitySlot::R,
    }
}
