use crate::{
    LanDiscoveryResource,
    models::HostedPlayer,
    state::{ServerRuntimeConfig, ServerState},
};
use api::core::commands::{CommandSender, GameModeKind, SystemMessageLevel, SystemSender};
use api::core::network::protocols::{
    Auth, ClientInventorySync, InventorySlotState, OrderedReliable, PlayerJoined, PlayerLeft,
    PlayerSnapshot, ServerAuthRejected, ServerChatMessage, ServerDropSpawn, ServerWelcome,
    UnorderedReliable, UnorderedUnreliable,
};
use bevy::ecs::event::EntityTrigger;
use bevy::prelude::*;
use lightyear::connection::client::{Connected, Disconnect, Disconnected};
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use log::{info, warn};
use std::collections::HashSet;
use std::time::Instant;

const DUPLICATE_UUID_REJECT_MESSAGE: &str =
    "Error 405 Client with the same UUID is already connected!";

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
        MessageReceiver::<ClientInventorySync>::default(),
        MessageReceiver::<ClientChatMessage>::default(),
    ));
    // Import message types used above (re-exported from protocols)
    use api::core::network::protocols::{
        ClientBlockBreak, ClientBlockPlace, ClientChatMessage, ClientChunkInterest, ClientDropItem,
        ClientDropPickup, ClientKeepAlive, PlayerMove,
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
        state.persist_player_data(
            player.client_uuid.as_str(),
            player.translation,
            player.yaw,
            player.pitch,
            &player.inventory_slots,
        );
        info!(
            "{} disconnected (uuid={})",
            player.username, player.client_uuid
        );
        let msg = PlayerLeft::new(player.player_id);
        let _ = multi_sender.send::<_, UnorderedReliable>(&msg, *server, &NetworkTarget::All);
        let _ = multi_sender.send::<_, UnorderedReliable>(
            &ServerChatMessage::new(
                CommandSender::System(SystemSender::Server {
                    level: SystemMessageLevel::Info,
                }),
                format!("Player {} disconnected.", player.username),
            ),
            *server,
            &NetworkTarget::All,
        );
    }

    // Remove any pending chunk work for this connection
    for waiters in state.pending_stream_chunk_waiters.values_mut() {
        waiters.remove(&entity);
    }
    state.pending_chunk_sends.retain(|(e, _)| *e != entity);
    state.chunk_send_window.remove(&entity);
}

/// Fallback cleanup: remove players whose connection entity no longer has `ClientOf`.
/// This covers edge-cases where a disconnect event is missed but the link entity vanished.
pub fn cleanup_orphaned_players(
    q_clients: Query<(Entity, Has<Disconnected>), With<ClientOf>>,
    config: Res<ServerRuntimeConfig>,
    mut last_scan: Local<Option<Instant>>,
    mut state: ResMut<ServerState>,
    mut multi_sender: ServerMultiMessageSender,
    server: Single<&Server>,
    mut commands: Commands,
) {
    let now = Instant::now();
    let interval = std::time::Duration::from_secs(config.dead_entity_check_interval_secs.max(1));
    if let Some(last) = *last_scan
        && now.duration_since(last) < interval
    {
        return;
    }
    *last_scan = Some(now);

    let mut link_state = std::collections::HashMap::<Entity, bool>::new();
    for (entity, disconnected) in q_clients.iter() {
        link_state.insert(entity, disconnected);
    }

    let stale: Vec<(Entity, bool)> = state
        .players
        .keys()
        .copied()
        .filter_map(|entity| match link_state.get(&entity) {
            None => Some((entity, false)),
            Some(true) => Some((entity, true)),
            Some(false) => None,
        })
        .collect();

    if stale.is_empty() {
        return;
    }

    for (entity, disconnected_link) in stale {
        if let Some(player) = state.players.remove(&entity) {
            state.persist_player_data(
                player.client_uuid.as_str(),
                player.translation,
                player.yaw,
                player.pitch,
                &player.inventory_slots,
            );
            if disconnected_link {
                warn!(
                    "Cleaned up disconnected player link {:?} (username={}, uuid={})",
                    entity, player.username, player.client_uuid
                );
            } else {
                warn!(
                    "Cleaned up orphaned player {:?} (username={}, uuid={})",
                    entity, player.username, player.client_uuid
                );
            }
            let _ = multi_sender.send::<_, UnorderedReliable>(
                &PlayerLeft::new(player.player_id),
                *server,
                &NetworkTarget::All,
            );
            let _ = multi_sender.send::<_, UnorderedReliable>(
                &ServerChatMessage::new(
                    CommandSender::System(SystemSender::Server {
                        level: SystemMessageLevel::Info,
                    }),
                    format!("Player {} disconnected.", player.username),
                ),
                *server,
                &NetworkTarget::All,
            );
        }

        state.pending_auth.remove(&entity);
        for waiters in state.pending_stream_chunk_waiters.values_mut() {
            waiters.remove(&entity);
        }
        state.pending_chunk_sends.retain(|(e, _)| *e != entity);
        state.chunk_send_window.remove(&entity);

        if disconnected_link {
            commands.queue(move |world: &mut World| {
                if let Ok(entity_mut) = world.get_entity_mut(entity) {
                    entity_mut.despawn();
                }
            });
        }
    }
}

// ── Auth message handler ──────────────────────────────────────────────────────

/// Handles auth messages for the `services::connections` module.
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
            let client_uuid = auth.client_uuid.trim().to_ascii_lowercase();
            info!(
                "Auth request from {:?}: username='{}', uuid={}",
                entity, username, client_uuid
            );

            if username.is_empty() {
                warn!("Empty username from {:?}, disconnecting", entity);
                commands.trigger_with(Disconnect { entity }, EntityTrigger);
                continue;
            }

            if client_uuid.is_empty() {
                warn!("Empty client UUID from {:?}, disconnecting", entity);
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

            let duplicate_uuid_online = state.players.values().any(|player| {
                player
                    .client_uuid
                    .eq_ignore_ascii_case(client_uuid.as_str())
            });
            if duplicate_uuid_online {
                warn!(
                    "Rejected duplicate UUID login for {:?} (uuid={})",
                    entity, client_uuid
                );
                let _ = multi_sender.send::<_, UnorderedReliable>(
                    &ServerAuthRejected::new(DUPLICATE_UUID_REJECT_MESSAGE),
                    *server,
                    &NetworkTarget::Single(entity_to_peer_id(entity, &q_remote_id)),
                );
                continue;
            }

            let loaded = state.load_player_data(client_uuid.as_str());
            let spawn_translation = loaded
                .as_ref()
                .map(|data| data.translation)
                .unwrap_or(config.spawn_translation);
            let spawn_yaw = loaded.as_ref().map(|data| data.yaw).unwrap_or(0.0);
            let spawn_pitch = loaded.as_ref().map(|data| data.pitch).unwrap_or(0.0);
            let inventory_slots = loaded
                .map(|data| data.inventory_slots)
                .unwrap_or_default();
            let player = HostedPlayer {
                player_id: state.next_player_id,
                username: username.clone(),
                client_uuid,
                game_mode: GameModeKind::Creative,
                translation: spawn_translation,
                yaw: spawn_yaw,
                pitch: spawn_pitch,
                inventory_slots,
                last_seen: Instant::now(),
                streamed_chunks: HashSet::new(),
            };
            state.next_player_id = state.next_player_id.wrapping_add(1);

            // Send welcome
            let block_palette = state
                .block_registry
                .defs
                .iter()
                .map(|def| def.localized_name.clone())
                .collect::<Vec<_>>();
            let welcome = ServerWelcome {
                player_id: player.player_id,
                server_name: config.server_name.clone(),
                motd: config.motd.clone(),
                world_name: config.world_name.clone(),
                world_seed: config.world_seed,
                spawn_translation,
                inventory_slots: player
                    .inventory_slots
                    .iter()
                    .copied()
                    .map(InventorySlotState::from)
                    .collect(),
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
            let uuid = player.client_uuid.clone();
            state.players.insert(entity, player);

            info!("{} joined as id {} (uuid={})", uname, player_id, uuid);

            // Broadcast join to all (including new player)
            let _ = multi_sender.send::<_, UnorderedReliable>(
                &PlayerJoined::new(player_id, uname),
                *server,
                &NetworkTarget::All,
            );
            let _ = multi_sender.send::<_, UnorderedReliable>(
                &ServerChatMessage::new(
                    CommandSender::System(SystemSender::Server {
                        level: SystemMessageLevel::Info,
                    }),
                    format!("Player {} joined the game.", username),
                ),
                *server,
                &NetworkTarget::All,
            );
            let _ = multi_sender.send::<_, UnorderedUnreliable>(
                &PlayerSnapshot::new(player_id, spawn_translation, spawn_yaw, spawn_pitch),
                *server,
                &NetworkTarget::All,
            );
        }
    }
}

// ── Stale player cleanup ──────────────────────────────────────────────────────

/// Runs the `purge_stale_players` routine for purge stale players in the `services::connections` module.
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
        if let Some(player) = state.players.get(&entity) {
            info!(
                "Disconnecting stale player {:?} (username={}, uuid={})",
                entity, player.username, player.client_uuid
            );
        } else {
            info!("Disconnecting stale player {:?}", entity);
        }
        commands.trigger_with(Disconnect { entity }, EntityTrigger);

        if let Some(player) = state.players.remove(&entity) {
            state.persist_player_data(
                player.client_uuid.as_str(),
                player.translation,
                player.yaw,
                player.pitch,
                &player.inventory_slots,
            );
            let _ = multi_sender.send::<_, UnorderedReliable>(
                &PlayerLeft::new(player.player_id),
                *server,
                &NetworkTarget::All,
            );
            let _ = multi_sender.send::<_, UnorderedReliable>(
                &ServerChatMessage::new(
                    CommandSender::System(SystemSender::Server {
                        level: SystemMessageLevel::Info,
                    }),
                    format!("Player {} disconnected.", player.username),
                ),
                *server,
                &NetworkTarget::All,
            );
        }
        state.pending_auth.remove(&entity);
    }
}

// ── LAN discovery poll ────────────────────────────────────────────────────────

/// Runs the `poll_lan_discovery` routine for poll lan discovery in the `services::connections` module.
pub fn poll_lan_discovery(
    mut discovery: ResMut<LanDiscoveryResource>,
    state: Res<ServerState>,
    config: Res<ServerRuntimeConfig>,
) {
    if let Some(d) = discovery.0.as_mut() {
        if let Err(e) = d.update_player_counts(state.players.len(), config.max_players) {
            warn!("LAN discovery payload update failed: {}", e);
        }
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
