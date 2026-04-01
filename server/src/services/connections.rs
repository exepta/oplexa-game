use crate::{
    LanDiscoveryResource,
    models::HostedPlayer,
    state::{ServerRuntimeConfig, ServerState},
};
use api::core::network::protocols::{
    Auth, OrderedReliable, PlayerJoined, PlayerLeft, PlayerSnapshot, ServerBlockBreak,
    ServerBlockPlace, ServerDropSpawn, ServerWelcome, UnorderedReliable, UnorderedUnreliable,
};
use bevy::ecs::event::EntityTrigger;
use bevy::prelude::*;
use lightyear::connection::client::{Connected, Disconnect, Disconnected};
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use log::{info, warn};
use std::collections::HashSet;
use std::time::Instant;

// ── Connection lifecycle observers ───────────────────────────────────────────

/// Fired when a new `LinkOf` component is added – i.e. a new connection entity is created.
pub fn handle_new_client(trigger: On<Add, ClientOf>, mut commands: Commands) {
    // Add receivers for all messages the server expects from this client.
    commands.entity(trigger.entity).insert((
        Name::new("ClientLink"),
        MessageReceiver::<Auth>::default(),
        MessageReceiver::<PlayerMove>::default(),
        MessageReceiver::<ClientKeepAlive>::default(),
        MessageReceiver::<ClientChunkInterest>::default(),
        MessageReceiver::<ClientBlockBreak>::default(),
        MessageReceiver::<ClientBlockPlace>::default(),
        MessageReceiver::<ClientDropItem>::default(),
        MessageReceiver::<ClientDropPickup>::default(),
    ));
    // Import message types used above (re-exported from protocols)
    use api::core::network::protocols::{
        ClientBlockBreak, ClientBlockPlace, ClientChunkInterest, ClientDropItem, ClientDropPickup,
        ClientKeepAlive, PlayerMove,
    };
}

/// Fired when the connection is fully established (`Connected` is added).
pub fn handle_client_connected(
    trigger: On<Add, Connected>,
    q_client: Query<Entity, With<ClientOf>>,
    // Nothing extra to do here – Auth message arrives separately
) {
    if q_client.contains(trigger.entity) {
        info!(
            "Client {:?} fully connected, waiting for Auth",
            trigger.entity
        );
    }
}

/// Fired when `Disconnected` is added (connection lost or closed).
pub fn handle_client_disconnected(
    trigger: On<Add, Disconnected>,
    mut state: ResMut<ServerState>,
    mut multi_sender: ServerMultiMessageSender,
    server: Single<&Server>,
) {
    let entity = trigger.entity;
    state.pending_auth.remove(&entity);

    if let Some(player) = state.players.remove(&entity) {
        info!("{} disconnected", player.username);
        let msg = PlayerLeft::new(player.player_id);
        let _ = multi_sender.send::<_, UnorderedReliable>(&msg, *server, &NetworkTarget::All);
    }

    // Remove any pending chunk work for this connection
    for waiters in state.pending_stream_chunk_waiters.values_mut() {
        waiters.remove(&entity);
    }
    state.pending_chunk_sends.retain(|(e, _)| *e != entity);
    state.chunk_send_window.remove(&entity);
}

// ── Auth message handler ──────────────────────────────────────────────────────

pub fn handle_auth_messages(
    mut q: Query<(Entity, &mut MessageReceiver<Auth>), With<ClientOf>>,
    q_remote_id: Query<&RemoteId>,
    mut state: ResMut<ServerState>,
    config: Res<ServerRuntimeConfig>,
    mut multi_sender: ServerMultiMessageSender,
    server: Single<&Server>,
    mut commands: Commands,
) {
    for (entity, mut receiver) in q.iter_mut() {
        for auth in receiver.receive() {
            let username = auth.username.trim().to_string();

            if username.is_empty() {
                warn!("Empty username from {:?}, disconnecting", entity);
                commands.trigger_with(Disconnect { entity }, EntityTrigger);
                continue;
            }

            if state.players.len() >= config.max_players {
                warn!("Server full, disconnecting {:?}", entity);
                commands.trigger_with(Disconnect { entity }, EntityTrigger);
                continue;
            }

            // Already fully authed → ignore duplicate Auth
            if state.players.contains_key(&entity) {
                continue;
            }

            let player = HostedPlayer {
                player_id: state.next_player_id,
                username: username.clone(),
                translation: config.spawn_translation,
                yaw: 0.0,
                pitch: 0.0,
                last_seen: Instant::now(),
                streamed_chunks: HashSet::new(),
            };
            state.next_player_id = state.next_player_id.wrapping_add(1);

            // Send welcome
            let block_palette = state
                .block_registry
                .defs
                .iter()
                .map(|def| def.name.clone())
                .collect::<Vec<_>>();
            let welcome = ServerWelcome {
                player_id: player.player_id,
                server_name: config.server_name.clone(),
                motd: config.motd.clone(),
                world_name: config.world_name.clone(),
                world_seed: config.world_seed,
                spawn_translation: config.spawn_translation,
                block_palette,
            };
            let _ = multi_sender.send::<_, UnorderedReliable>(
                &welcome,
                *server,
                &NetworkTarget::Single(entity_to_peer_id(entity, &q_remote_id)),
            );

            // Sync existing players
            for other in state.players.values() {
                let _ = multi_sender.send::<_, UnorderedReliable>(
                    &PlayerJoined::new(other.player_id, other.username.clone()),
                    *server,
                    &NetworkTarget::Single(entity_to_peer_id(entity, &q_remote_id)),
                );
                let _ = multi_sender.send::<_, UnorderedUnreliable>(
                    &PlayerSnapshot::new(
                        other.player_id,
                        other.translation,
                        other.yaw,
                        other.pitch,
                    ),
                    *server,
                    &NetworkTarget::Single(entity_to_peer_id(entity, &q_remote_id)),
                );
            }

            // Sync block overrides
            for (location, block_id) in &state.block_overrides {
                if *block_id == 0 {
                    let _ = multi_sender.send::<_, OrderedReliable>(
                        &ServerBlockBreak::new(0, *location),
                        *server,
                        &NetworkTarget::Single(entity_to_peer_id(entity, &q_remote_id)),
                    );
                } else {
                    let _ = multi_sender.send::<_, OrderedReliable>(
                        &ServerBlockPlace::new(0, *location, *block_id),
                        *server,
                        &NetworkTarget::Single(entity_to_peer_id(entity, &q_remote_id)),
                    );
                }
            }

            // Sync existing drops
            for drop in state.drops.values() {
                let _ = multi_sender.send::<_, OrderedReliable>(
                    &ServerDropSpawn::new(
                        drop.drop_id,
                        drop.location,
                        drop.block_id,
                        drop.has_motion,
                        drop.spawn_translation,
                        drop.initial_velocity,
                    ),
                    *server,
                    &NetworkTarget::Single(entity_to_peer_id(entity, &q_remote_id)),
                );
            }

            let player_id = player.player_id;
            let uname = player.username.clone();
            state.players.insert(entity, player);

            info!("{} joined as id {}", uname, player_id);

            // Broadcast join to all (including new player)
            let _ = multi_sender.send::<_, UnorderedReliable>(
                &PlayerJoined::new(player_id, uname),
                *server,
                &NetworkTarget::All,
            );
            let _ = multi_sender.send::<_, UnorderedUnreliable>(
                &PlayerSnapshot::new(player_id, config.spawn_translation, 0.0, 0.0),
                *server,
                &NetworkTarget::All,
            );
        }
    }
}

// ── Stale player cleanup ──────────────────────────────────────────────────────

pub fn purge_stale_players(
    mut state: ResMut<ServerState>,
    config: Res<ServerRuntimeConfig>,
    mut multi_sender: ServerMultiMessageSender,
    server: Single<&Server>,
    mut commands: Commands,
) {
    let now = Instant::now();
    let timeout = std::time::Duration::from_secs(config.client_timeout.max(1));

    let stale: Vec<Entity> = state
        .players
        .iter()
        .filter_map(|(entity, player)| {
            (now.duration_since(player.last_seen) > timeout).then_some(*entity)
        })
        .collect();

    for entity in stale {
        info!("Disconnecting stale player {:?}", entity);
        commands.trigger_with(Disconnect { entity }, EntityTrigger);

        if let Some(player) = state.players.remove(&entity) {
            let _ = multi_sender.send::<_, UnorderedReliable>(
                &PlayerLeft::new(player.player_id),
                *server,
                &NetworkTarget::All,
            );
        }
        state.pending_auth.remove(&entity);
    }
}

// ── LAN discovery poll ────────────────────────────────────────────────────────

pub fn poll_lan_discovery(mut discovery: ResMut<LanDiscoveryResource>) {
    if let Some(d) = discovery.0.as_mut() {
        if let Err(e) = d.poll() {
            warn!("LAN discovery error: {}", e);
        }
    }
}

// ── Helper ────────────────────────────────────────────────────────────────────

/// Look up the lightyear `PeerId` for a client entity via its `RemoteId` component.
fn entity_to_peer_id(entity: Entity, q_remote_id: &Query<&RemoteId>) -> PeerId {
    q_remote_id
        .get(entity)
        .map(|r| r.0)
        .unwrap_or(PeerId::Entity(entity.to_bits()))
}
