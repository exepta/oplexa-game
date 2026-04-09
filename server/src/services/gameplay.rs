use crate::{
    models::HostedDrop,
    state::{ServerRuntimeConfig, ServerState},
};
use api::core::network::protocols::{
    ClientBlockBreak, ClientBlockPlace, ClientChunkInterest, ClientDropItem, ClientDropPickup,
    ClientKeepAlive, OrderedReliable, PlayerMove, PlayerSnapshot, ServerBlockBreak,
    ServerBlockPlace, ServerChunkData, ServerDropPicked, ServerDropSpawn, UnorderedReliable,
    UnorderedUnreliable,
};
use bevy::math::IVec2;
use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use std::collections::{HashSet, VecDeque};
use std::time::Instant;

/// Handles player move messages for the `services::gameplay` module.
pub fn handle_player_move_messages(
    mut q: Query<(Entity, &mut MessageReceiver<PlayerMove>), With<ClientOf>>,
    mut state: ResMut<ServerState>,
    mut multi_sender: ServerMultiMessageSender,
    server: Single<&Server>,
) {
    for (entity, mut receiver) in q.iter_mut() {
        for movement in receiver.receive() {
            if let Some(player) = state.players.get_mut(&entity) {
                player.last_seen = Instant::now();
                player.translation = movement.translation;
                player.yaw = movement.yaw;
                player.pitch = movement.pitch;
                let snapshot = PlayerSnapshot::new(
                    player.player_id,
                    player.translation,
                    player.yaw,
                    player.pitch,
                );
                let _ = multi_sender.send::<_, UnorderedUnreliable>(
                    &snapshot,
                    *server,
                    &NetworkTarget::All,
                );
            }
        }
    }
}

/// Handles keepalive messages for the `services::gameplay` module.
pub fn handle_keepalive_messages(
    mut q: Query<(Entity, &mut MessageReceiver<ClientKeepAlive>), With<ClientOf>>,
    mut state: ResMut<ServerState>,
) {
    for (entity, mut receiver) in q.iter_mut() {
        for _ in receiver.receive() {
            if let Some(player) = state.players.get_mut(&entity) {
                player.last_seen = Instant::now();
            }
        }
    }
}

/// Handles chunk interest messages for the `services::gameplay` module.
pub fn handle_chunk_interest_messages(
    mut q: Query<(Entity, &mut MessageReceiver<ClientChunkInterest>), With<ClientOf>>,
    mut state: ResMut<ServerState>,
    config: Res<ServerRuntimeConfig>,
) {
    let mut pending: Vec<(Entity, IVec2)> = Vec::new();

    for (entity, mut receiver) in q.iter_mut() {
        for message in receiver.receive() {
            let center = IVec2::new(message.center[0], message.center[1]);
            let radius = message.radius.clamp(1, config.max_stream_radius.max(1));
            log::debug!(
                "[CHUNK] ClientChunkInterest from {:?}: center={:?}, radius={}",
                entity,
                [center.x, center.y],
                radius
            );
            let mut desired_chunks = HashSet::new();
            for dz in -radius..=radius {
                for dx in -radius..=radius {
                    desired_chunks.insert(IVec2::new(center.x + dx, center.y + dz));
                }
            }

            let Some(player) = state.players.get_mut(&entity) else {
                continue;
            };
            player.last_seen = Instant::now();
            let mut to_send = desired_chunks
                .difference(&player.streamed_chunks)
                .copied()
                .collect::<Vec<_>>();
            to_send.sort_by_key(|coord| {
                let dx = (coord.x - center.x).abs();
                let dz = (coord.y - center.y).abs();
                dx.max(dz)
            });
            player.streamed_chunks = desired_chunks;

            for coord in to_send {
                pending.push((entity, coord));
            }
        }
    }

    for (entity, coord) in pending {
        state.queue_chunk_for_stream(entity, coord);
    }
}

/// Runs the `flush_chunk_streaming` routine for flush chunk streaming in the `services::gameplay` module.
pub fn flush_chunk_streaming(
    q_clients: Query<Entity, With<ClientOf>>,
    q_remote_id: Query<&RemoteId>,
    mut state: ResMut<ServerState>,
    config: Res<ServerRuntimeConfig>,
    mut multi_sender: ServerMultiMessageSender,
    server: Single<&Server>,
) {
    state.pump_stream_chunk_tasks(&config);
    state.collect_ready_stream_chunks();
    state.pump_stream_chunk_tasks(&config);

    // Expire chunks that have been in-flight long enough to be considered delivered.
    let now = std::time::Instant::now();

    let connected: HashSet<Entity> = q_clients.iter().collect();
    state
        .chunk_send_window
        .retain(|entity, _| connected.contains(entity));
    for window in state.chunk_send_window.values_mut() {
        window.retain(|t| {
            now.duration_since(*t).as_millis() < config.chunk_flight_timeout_ms as u128
        });
    }

    let connected_clients = connected.len();
    let sends_per_tick = config
        .chunk_stream_sends_per_tick_base
        .saturating_add(
            config
                .chunk_stream_sends_per_tick_per_client
                .saturating_mul(connected_clients),
        )
        .min(config.chunk_stream_sends_per_tick_max.max(1));

    let mut sent = 0usize;
    let mut scanned = 0usize;
    let scan_cap = state.pending_chunk_sends.len();

    while sent < sends_per_tick && scanned < scan_cap {
        scanned += 1;
        let Some((entity, coord)) = state.pending_chunk_sends.pop_front() else {
            break;
        };

        if !connected.contains(&entity) {
            continue;
        }

        let in_range = state
            .players
            .get(&entity)
            .map_or(false, |p| p.streamed_chunks.contains(&coord));
        if !in_range {
            continue;
        }

        let Some(encoded_chunk) = state.streamed_chunk_cache.get(&coord).cloned() else {
            state.queue_chunk_for_stream(entity, coord);
            continue;
        };

        let window = state
            .chunk_send_window
            .entry(entity)
            .or_insert_with(VecDeque::new);
        if window.len() >= config.chunk_stream_inflight_per_client.max(1) {
            state.pending_chunk_sends.push_back((entity, coord));
            continue;
        }

        let peer_id = q_remote_id
            .get(entity)
            .map(|r| r.0)
            .unwrap_or(PeerId::Entity(entity.to_bits()));
        log::debug!(
            "[CHUNK] Sending chunk {:?} to {:?} (window: {}/{})",
            [coord.x, coord.y],
            entity,
            window.len(),
            config.chunk_stream_inflight_per_client.max(1)
        );
        let _ = multi_sender.send::<_, UnorderedReliable>(
            &ServerChunkData::new([coord.x, coord.y], encoded_chunk),
            *server,
            &NetworkTarget::Single(peer_id),
        );
        window.push_back(now);
        sent += 1;
    }
}

/// Handles block break messages for the `services::gameplay` module.
pub fn handle_block_break_messages(
    mut q: Query<(Entity, &mut MessageReceiver<ClientBlockBreak>), With<ClientOf>>,
    mut state: ResMut<ServerState>,
    mut multi_sender: ServerMultiMessageSender,
    server: Single<&Server>,
) {
    let mut events: Vec<(Entity, ClientBlockBreak)> = Vec::new();
    for (entity, mut receiver) in q.iter_mut() {
        for message in receiver.receive() {
            events.push((entity, message));
        }
    }

    for (entity, message) in events {
        let Some(player) = state.players.get_mut(&entity) else {
            continue;
        };
        player.last_seen = Instant::now();
        let player_id = player.player_id;

        state.block_overrides.insert(message.location, 0);
        let (chunk_coord, _) = api::core::world::chunk_dimension::world_to_chunk_xz(
            message.location[0],
            message.location[2],
        );
        state.invalidate_streamed_chunk(chunk_coord);
        state.persist_block_overrides();

        let _ = multi_sender.send::<_, OrderedReliable>(
            &ServerBlockBreak::new(player_id, message.location),
            *server,
            &NetworkTarget::All,
        );

        if message.drop_block_id != 0 {
            let drop_id = if message.drop_id != 0 {
                message.drop_id
            } else {
                let generated = state.next_drop_id;
                state.next_drop_id = state.next_drop_id.wrapping_add(1);
                generated
            };

            state.drops.insert(
                drop_id,
                HostedDrop {
                    drop_id,
                    location: message.location,
                    block_id: message.drop_block_id,
                    has_motion: false,
                    spawn_translation: [0.0, 0.0, 0.0],
                    initial_velocity: [0.0, 0.0, 0.0],
                },
            );

            let _ = multi_sender.send::<_, OrderedReliable>(
                &ServerDropSpawn::new(
                    drop_id,
                    message.location,
                    message.drop_block_id,
                    false,
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0],
                ),
                *server,
                &NetworkTarget::All,
            );
        }
    }
}

/// Handles block place messages for the `services::gameplay` module.
pub fn handle_block_place_messages(
    mut q: Query<(Entity, &mut MessageReceiver<ClientBlockPlace>), With<ClientOf>>,
    mut state: ResMut<ServerState>,
    mut multi_sender: ServerMultiMessageSender,
    server: Single<&Server>,
) {
    let mut events: Vec<(Entity, ClientBlockPlace)> = Vec::new();
    for (entity, mut receiver) in q.iter_mut() {
        for message in receiver.receive() {
            events.push((entity, message));
        }
    }

    for (entity, message) in events {
        let Some(player) = state.players.get_mut(&entity) else {
            continue;
        };
        player.last_seen = Instant::now();
        let player_id = player.player_id;

        state
            .block_overrides
            .insert(message.location, message.block_id);
        let (chunk_coord, _) = api::core::world::chunk_dimension::world_to_chunk_xz(
            message.location[0],
            message.location[2],
        );
        state.invalidate_streamed_chunk(chunk_coord);
        state.persist_block_overrides();

        let _ = multi_sender.send::<_, OrderedReliable>(
            &ServerBlockPlace::new(player_id, message.location, message.block_id),
            *server,
            &NetworkTarget::All,
        );
    }
}

/// Handles drop item messages for the `services::gameplay` module.
pub fn handle_drop_item_messages(
    mut q: Query<(Entity, &mut MessageReceiver<ClientDropItem>), With<ClientOf>>,
    mut state: ResMut<ServerState>,
    mut multi_sender: ServerMultiMessageSender,
    server: Single<&Server>,
) {
    let mut events: Vec<(Entity, ClientDropItem)> = Vec::new();
    for (entity, mut receiver) in q.iter_mut() {
        for message in receiver.receive() {
            events.push((entity, message));
        }
    }

    for (entity, message) in events {
        let Some(player) = state.players.get_mut(&entity) else {
            continue;
        };
        player.last_seen = Instant::now();

        if message.block_id == 0 || message.amount == 0 {
            continue;
        }

        let amount = message.amount.min(128);
        for i in 0..amount {
            let spawn_translation = [
                message.spawn_translation[0],
                message.spawn_translation[1] + i as f32 * 0.015,
                message.spawn_translation[2],
            ];
            let drop_id = state.next_drop_id;
            state.next_drop_id = state.next_drop_id.wrapping_add(1).max(1);

            state.drops.insert(
                drop_id,
                HostedDrop {
                    drop_id,
                    location: message.location,
                    block_id: message.block_id,
                    has_motion: true,
                    spawn_translation,
                    initial_velocity: message.initial_velocity,
                },
            );

            let _ = multi_sender.send::<_, OrderedReliable>(
                &ServerDropSpawn::new(
                    drop_id,
                    message.location,
                    message.block_id,
                    true,
                    spawn_translation,
                    message.initial_velocity,
                ),
                *server,
                &NetworkTarget::All,
            );
        }
    }
}

/// Handles drop pickup messages for the `services::gameplay` module.
pub fn handle_drop_pickup_messages(
    mut q: Query<(Entity, &mut MessageReceiver<ClientDropPickup>), With<ClientOf>>,
    mut state: ResMut<ServerState>,
    mut multi_sender: ServerMultiMessageSender,
    server: Single<&Server>,
) {
    let mut events: Vec<(Entity, ClientDropPickup)> = Vec::new();
    for (entity, mut receiver) in q.iter_mut() {
        for message in receiver.receive() {
            events.push((entity, message));
        }
    }

    for (entity, message) in events {
        let Some(player) = state.players.get_mut(&entity) else {
            continue;
        };
        player.last_seen = Instant::now();
        let player_id = player.player_id;

        let Some(drop) = state.drops.remove(&message.drop_id) else {
            continue;
        };

        let _ = multi_sender.send::<_, OrderedReliable>(
            &ServerDropPicked::new(drop.drop_id, player_id, drop.block_id),
            *server,
            &NetworkTarget::All,
        );
    }
}
