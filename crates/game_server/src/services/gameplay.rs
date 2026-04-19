use crate::{
    generator::chunk::chunk_utils::{decode_chunk, load_chunk_at_root_sync},
    models::HostedDrop,
    state::{CompletedWorldEdit, ServerRuntimeConfig, ServerState, WorldEditOutcome},
};
use api::core::commands::GameModeKind;
use api::core::events::ui_events::ChestInventorySlotPayload;
use api::core::network::protocols::{
    ClientBlockBreak, ClientBlockPlace, ClientChestInventoryOpen, ClientChestInventoryPersist,
    ClientChunkInterest, ClientDropItem, ClientDropPickup, ClientInventorySync, ClientKeepAlive,
    OrderedReliable, PlayerMove, PlayerSnapshot, ServerBlockBreak, ServerBlockPlace,
    ServerChestInventoryContents, ServerChunkData, ServerDropPicked, ServerDropSpawn,
    ServerTeleport, UnorderedReliable, UnorderedUnreliable,
};
use api::core::world::chunk::ChunkData;
use api::core::world::chunk_dimension::{Y_MAX, Y_MIN, world_to_chunk_xz, world_y_to_local};
use api::core::world::save::StructureRegionInventorySlot;
use bevy::math::IVec2;
use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

const PLAYER_POSITION_SAVE_INTERVAL_SECS: f32 = 1.0;
const PLAYER_COLLIDER_RADIUS: f32 = 0.30;
const PLAYER_COLLIDER_HALF_HEIGHT: f32 = 1.15;
const PLAYER_MAX_STEP_PER_UPDATE: f32 = 4.0;
const PLAYER_CORRECTION_EPSILON: f32 = 0.02;
const PLAYER_PITCH_LIMIT_RAD: f32 = std::f32::consts::FRAC_PI_2 - 0.01;

#[inline]
fn chunk_stream_backlog_per_client(backlog: usize, connected_clients: usize) -> usize {
    if connected_clients == 0 {
        backlog
    } else {
        (backlog + (connected_clients - 1)) / connected_clients
    }
}

#[inline]
fn dynamic_chunk_stream_send_budget(
    config: &ServerRuntimeConfig,
    connected_clients: usize,
    backlog_per_client: usize,
) -> usize {
    let connected = connected_clients.max(1);
    let mut budget = config.chunk_stream_sends_per_tick_base.saturating_add(
        config
            .chunk_stream_sends_per_tick_per_client
            .saturating_mul(connected),
    );

    if backlog_per_client >= 256 {
        budget = budget.saturating_add(
            config
                .chunk_stream_sends_per_tick_per_client
                .saturating_mul(connected)
                .saturating_mul(5),
        );
    } else if backlog_per_client >= 128 {
        budget = budget.saturating_add(
            config
                .chunk_stream_sends_per_tick_per_client
                .saturating_mul(connected)
                .saturating_mul(4),
        );
    } else if backlog_per_client >= 64 {
        budget = budget.saturating_add(
            config
                .chunk_stream_sends_per_tick_per_client
                .saturating_mul(connected)
                .saturating_mul(3),
        );
    } else if backlog_per_client >= 24 {
        budget = budget.saturating_add(
            config
                .chunk_stream_sends_per_tick_per_client
                .saturating_mul(connected)
                .saturating_mul(2),
        );
    }

    budget.clamp(1, config.chunk_stream_sends_per_tick_max.max(1))
}

#[inline]
fn dynamic_chunk_stream_inflight_limit(
    config: &ServerRuntimeConfig,
    backlog_per_client: usize,
) -> usize {
    let base = config.chunk_stream_inflight_per_client.max(1);
    let mut inflight = base;
    if backlog_per_client >= 256 {
        inflight = base.saturating_mul(4);
    } else if backlog_per_client >= 128 {
        inflight = base.saturating_mul(3);
    } else if backlog_per_client >= 64 {
        inflight = base.saturating_mul(2);
    }
    inflight.clamp(base, base.saturating_mul(4).clamp(base, 128))
}

#[inline]
fn dynamic_chunk_stream_timeout_ms(config: &ServerRuntimeConfig, backlog_per_client: usize) -> u64 {
    let base = config.chunk_flight_timeout_ms.clamp(70, 140);
    if backlog_per_client >= 256 {
        (base / 3).max(70)
    } else if backlog_per_client >= 128 {
        (base / 2).max(80)
    } else if backlog_per_client >= 64 {
        (base.saturating_mul(2) / 3).max(90)
    } else {
        base
    }
}

/// Handles player move messages for the `services::gameplay` module.
pub fn handle_player_move_messages(
    mut q: Query<(Entity, &mut MessageReceiver<PlayerMove>), With<ClientOf>>,
    q_remote_id: Query<&RemoteId>,
    mut state: ResMut<ServerState>,
    mut multi_sender: ServerMultiMessageSender,
    server: Single<&Server>,
) {
    for (entity, mut receiver) in q.iter_mut() {
        for movement in receiver.receive() {
            let Some((player_id, previous_translation, game_mode, previous_yaw, previous_pitch)) =
                state.players.get(&entity).map(|player| {
                    (
                        player.player_id,
                        player.translation,
                        player.game_mode,
                        player.yaw,
                        player.pitch,
                    )
                })
            else {
                continue;
            };

            let requested_translation =
                sanitize_client_position(previous_translation, movement.translation);
            let mut chunk_cache: HashMap<IVec2, ChunkData> = HashMap::new();
            let resolved_translation = resolve_player_world_collision(
                &mut state,
                &mut chunk_cache,
                previous_translation,
                requested_translation,
                game_mode,
            );
            let requested_pitch = sanitize_client_pitch(previous_pitch, movement.pitch);
            let requested_yaw = if movement.yaw.is_finite() {
                movement.yaw
            } else {
                previous_yaw
            };
            let corrected = distance_squared3(resolved_translation, movement.translation)
                > PLAYER_CORRECTION_EPSILON * PLAYER_CORRECTION_EPSILON;

            if let Some(player) = state.players.get_mut(&entity) {
                player.last_seen = Instant::now();
                player.translation = resolved_translation;
                player.yaw = requested_yaw;
                player.pitch = requested_pitch;
            }

            let snapshot = PlayerSnapshot::new(
                player_id,
                resolved_translation,
                requested_yaw,
                requested_pitch,
            );
            let _ = multi_sender.send::<_, UnorderedUnreliable>(
                &snapshot,
                *server,
                &NetworkTarget::All,
            );

            if corrected {
                let peer_id = q_remote_id
                    .get(entity)
                    .map(|remote_id| remote_id.0)
                    .unwrap_or(PeerId::Entity(entity.to_bits()));
                let _ = multi_sender.send::<_, UnorderedReliable>(
                    &ServerTeleport::new(player_id, resolved_translation),
                    *server,
                    &NetworkTarget::Single(peer_id),
                );
            }
        }
    }
}

#[inline]
fn sanitize_client_position(previous: [f32; 3], requested: [f32; 3]) -> [f32; 3] {
    if !requested.iter().all(|value| value.is_finite()) {
        return previous;
    }

    let mut sanitized = requested;
    let delta = [
        sanitized[0] - previous[0],
        sanitized[1] - previous[1],
        sanitized[2] - previous[2],
    ];
    let distance = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
    if distance > PLAYER_MAX_STEP_PER_UPDATE {
        let scale = PLAYER_MAX_STEP_PER_UPDATE / distance;
        sanitized = [
            previous[0] + delta[0] * scale,
            previous[1] + delta[1] * scale,
            previous[2] + delta[2] * scale,
        ];
    }

    sanitized[1] = sanitized[1].clamp(Y_MIN as f32 - 4.0, Y_MAX as f32 + 8.0);
    sanitized
}

#[inline]
fn sanitize_client_pitch(previous: f32, requested: f32) -> f32 {
    if requested.is_finite() {
        requested.clamp(-PLAYER_PITCH_LIMIT_RAD, PLAYER_PITCH_LIMIT_RAD)
    } else {
        previous
    }
}

fn resolve_player_world_collision(
    state: &mut ServerState,
    chunk_cache: &mut HashMap<IVec2, ChunkData>,
    current: [f32; 3],
    requested: [f32; 3],
    game_mode: GameModeKind,
) -> [f32; 3] {
    if game_mode == GameModeKind::Spectator {
        return requested;
    }

    let mut resolved = current;
    for axis in 0..3 {
        let mut candidate = resolved;
        candidate[axis] = requested[axis];
        if !player_collides_with_world(state, chunk_cache, candidate) {
            resolved = candidate;
        }
    }

    if player_collides_with_world(state, chunk_cache, resolved) {
        current
    } else {
        resolved
    }
}

fn player_collides_with_world(
    state: &mut ServerState,
    chunk_cache: &mut HashMap<IVec2, ChunkData>,
    player_translation: [f32; 3],
) -> bool {
    let player_min = [
        player_translation[0] - PLAYER_COLLIDER_RADIUS,
        player_translation[1] - PLAYER_COLLIDER_HALF_HEIGHT,
        player_translation[2] - PLAYER_COLLIDER_RADIUS,
    ];
    let player_max = [
        player_translation[0] + PLAYER_COLLIDER_RADIUS,
        player_translation[1] + PLAYER_COLLIDER_HALF_HEIGHT,
        player_translation[2] + PLAYER_COLLIDER_RADIUS,
    ];

    if player_min[1] < Y_MIN as f32 || player_max[1] > Y_MAX as f32 {
        return true;
    }

    let x0 = player_min[0].floor() as i32;
    let x1 = player_max[0].floor() as i32;
    let y0 = player_min[1].floor() as i32;
    let y1 = player_max[1].floor() as i32;
    let z0 = player_min[2].floor() as i32;
    let z1 = player_max[2].floor() as i32;

    for wy in y0..=y1 {
        for wz in z0..=z1 {
            for wx in x0..=x1 {
                let Some(block_id) = world_block_id(state, chunk_cache, wx, wy, wz) else {
                    // Missing chunk data is treated as solid to keep the authoritative player
                    // from entering unknown/unloaded terrain.
                    return true;
                };
                if block_id == 0 || !state.block_registry.is_solid_for_collision(block_id) {
                    continue;
                }

                let Some((block_min, block_max)) =
                    block_collision_bounds(state.block_registry.as_ref(), block_id, wx, wy, wz)
                else {
                    continue;
                };
                if aabb_intersects(player_min, player_max, block_min, block_max) {
                    return true;
                }
            }
        }
    }

    false
}

fn world_block_id(
    state: &mut ServerState,
    chunk_cache: &mut HashMap<IVec2, ChunkData>,
    wx: i32,
    wy: i32,
    wz: i32,
) -> Option<u16> {
    if !(Y_MIN..=Y_MAX).contains(&wy) {
        return None;
    }

    let (coord, local) = world_to_chunk_xz(wx, wz);
    if !chunk_cache.contains_key(&coord) {
        let chunk = if let Some(encoded) = state.streamed_chunk_cache.get(&coord) {
            decode_chunk(encoded).ok()?
        } else if let Some(chunk) = load_chunk_at_root_sync(state.world_root.as_path(), coord) {
            chunk
        } else {
            return None;
        };
        chunk_cache.insert(coord, chunk);
    }

    let chunk = chunk_cache.get(&coord)?;
    let ly = world_y_to_local(wy);
    Some(chunk.get(local.x as usize, ly, local.y as usize))
}

fn block_collision_bounds(
    block_registry: &api::core::world::block::BlockRegistry,
    block_id: u16,
    wx: i32,
    wy: i32,
    wz: i32,
) -> Option<([f32; 3], [f32; 3])> {
    let (size_m, offset_m) = if let Some(bounds) = block_registry.collision_box(block_id) {
        bounds
    } else if block_registry.collision_uses_render_mesh(block_id) {
        ([1.0, 1.0, 1.0], [0.0, 0.0, 0.0])
    } else {
        return None;
    };

    let center = [
        wx as f32 + 0.5 + offset_m[0],
        wy as f32 + 0.5 + offset_m[1],
        wz as f32 + 0.5 + offset_m[2],
    ];
    let half = [size_m[0] * 0.5, size_m[1] * 0.5, size_m[2] * 0.5];
    Some((
        [
            center[0] - half[0],
            center[1] - half[1],
            center[2] - half[2],
        ],
        [
            center[0] + half[0],
            center[1] + half[1],
            center[2] + half[2],
        ],
    ))
}

#[inline]
fn aabb_intersects(a_min: [f32; 3], a_max: [f32; 3], b_min: [f32; 3], b_max: [f32; 3]) -> bool {
    a_min[0] < b_max[0]
        && a_max[0] > b_min[0]
        && a_min[1] < b_max[1]
        && a_max[1] > b_min[1]
        && a_min[2] < b_max[2]
        && a_max[2] > b_min[2]
}

#[inline]
fn distance_squared3(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}

/// Persists online player positions for the `services::gameplay` module.
pub fn persist_online_player_positions(
    time: Res<Time>,
    state: Res<ServerState>,
    mut save_timer: Local<Option<Timer>>,
) {
    let timer = save_timer.get_or_insert_with(|| {
        Timer::from_seconds(PLAYER_POSITION_SAVE_INTERVAL_SECS, TimerMode::Repeating)
    });

    if !timer.tick(time.delta()).just_finished() {
        return;
    }

    for player in state.players.values() {
        state.persist_player_data(
            player.client_uuid.as_str(),
            player.translation,
            player.yaw,
            player.pitch,
            &player.inventory_slots,
        );
    }
}

/// Handles inventory sync messages for the `services::gameplay` module.
pub fn handle_inventory_sync_messages(
    mut q: Query<(Entity, &mut MessageReceiver<ClientInventorySync>), With<ClientOf>>,
    mut state: ResMut<ServerState>,
) {
    for (entity, mut receiver) in q.iter_mut() {
        for message in receiver.receive() {
            let Some((client_uuid, translation, yaw, pitch, inventory_slots)) = (|| {
                let player = state.players.get_mut(&entity)?;
                player.last_seen = Instant::now();
                player.inventory_slots = message.to_slots();
                Some((
                    player.client_uuid.clone(),
                    player.translation,
                    player.yaw,
                    player.pitch,
                    player.inventory_slots,
                ))
            })() else {
                continue;
            };

            state.persist_player_data(
                client_uuid.as_str(),
                translation,
                yaw,
                pitch,
                &inventory_slots,
            );
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
    let connected: HashSet<Entity> = q_clients.iter().collect();
    let connected_clients = connected.len();
    let backlog_per_client =
        chunk_stream_backlog_per_client(state.pending_chunk_sends.len(), connected_clients);
    let inflight_per_client = dynamic_chunk_stream_inflight_limit(&config, backlog_per_client);
    let flight_timeout_ms = dynamic_chunk_stream_timeout_ms(&config, backlog_per_client);

    state.pump_stream_chunk_tasks(&config, connected_clients);
    state.collect_ready_stream_chunks();
    state.pump_stream_chunk_tasks(&config, connected_clients);

    // Expire chunks that have been in-flight long enough to be considered delivered.
    let now = std::time::Instant::now();

    state
        .chunk_send_window
        .retain(|entity, _| connected.contains(entity));
    for window in state.chunk_send_window.values_mut() {
        window.retain(|t| now.duration_since(*t).as_millis() < flight_timeout_ms as u128);
    }

    let sends_per_tick =
        dynamic_chunk_stream_send_budget(&config, connected_clients, backlog_per_client);

    let mut sent = 0usize;
    let mut scanned = 0usize;
    let scan_cap = state
        .pending_chunk_sends
        .len()
        .min(sends_per_tick.saturating_mul(48).max(512));

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

        let inflight_full = state
            .chunk_send_window
            .get(&entity)
            .is_some_and(|window| window.len() >= inflight_per_client);
        if inflight_full {
            state.pending_chunk_sends.push_back((entity, coord));
            continue;
        }

        let peer_id = q_remote_id
            .get(entity)
            .map(|r| r.0)
            .unwrap_or(PeerId::Entity(entity.to_bits()));
        let structures = state.load_structure_records_for_chunk(coord);
        let _ = multi_sender.send::<_, UnorderedReliable>(
            &ServerChunkData::new([coord.x, coord.y], encoded_chunk, structures),
            *server,
            &NetworkTarget::Single(peer_id),
        );
        state
            .chunk_send_window
            .entry(entity)
            .or_insert_with(VecDeque::new)
            .push_back(now);
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

        match state.set_block_persisted(
            player_id,
            message.location,
            message.drop_block_id,
            message.drop_id,
        ) {
            WorldEditOutcome::Rejected => continue,
            WorldEditOutcome::Queued => continue,
            WorldEditOutcome::Applied => {}
        }
        if let Err(error) = state.clear_chest_inventory_slots(message.location) {
            log::warn!(
                "Failed clearing chest inventory at {:?} after block break: {}",
                message.location,
                error
            );
        }

        let _ = multi_sender.send::<_, UnorderedReliable>(
            &ServerBlockBreak::new(player_id, message.location),
            *server,
            &NetworkTarget::All,
        );

        if message.drop_block_id != 0 {
            let drop_item_id = state
                .item_registry
                .item_for_block(message.drop_block_id)
                .unwrap_or(0);
            if drop_item_id == 0 {
                continue;
            }
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
                    item_id: drop_item_id,
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
                    drop_item_id,
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

        match state.set_block_persisted_with_stacked(
            player_id,
            message.location,
            message.block_id,
            message.stacked_block_id,
            0,
        ) {
            WorldEditOutcome::Rejected => continue,
            WorldEditOutcome::Queued => continue,
            WorldEditOutcome::Applied => {}
        }
        if let Err(error) = state.clear_chest_inventory_slots(message.location) {
            log::warn!(
                "Failed clearing chest inventory at {:?} after block place: {}",
                message.location,
                error
            );
        }
        if let Err(error) = state.upsert_structure_record_for_block(
            message.location,
            message.block_id,
            message.stacked_block_id,
        ) {
            log::warn!(
                "Failed persisting structure record at {:?} after block place: {}",
                message.location,
                error
            );
        }

        let _ = multi_sender.send::<_, UnorderedReliable>(
            &ServerBlockPlace::new(
                player_id,
                message.location,
                message.block_id,
                message.stacked_block_id,
            ),
            *server,
            &NetworkTarget::All,
        );
    }
}

pub fn flush_deferred_world_edit_messages(
    mut state: ResMut<ServerState>,
    mut multi_sender: ServerMultiMessageSender,
    server: Single<&Server>,
) {
    for event in state.drain_completed_world_edits() {
        match event {
            CompletedWorldEdit::Break {
                player_id,
                location,
                drop,
            } => {
                let _ = multi_sender.send::<_, UnorderedReliable>(
                    &ServerBlockBreak::new(player_id, location),
                    *server,
                    &NetworkTarget::All,
                );
                if let Some(drop) = drop {
                    let _ = multi_sender.send::<_, OrderedReliable>(
                        &ServerDropSpawn::new(
                            drop.drop_id,
                            drop.location,
                            drop.item_id,
                            drop.block_id,
                            drop.has_motion,
                            drop.spawn_translation,
                            drop.initial_velocity,
                        ),
                        *server,
                        &NetworkTarget::All,
                    );
                }
            }
            CompletedWorldEdit::Place {
                player_id,
                location,
                block_id,
                stacked_block_id,
            } => {
                let _ = multi_sender.send::<_, UnorderedReliable>(
                    &ServerBlockPlace::new(player_id, location, block_id, stacked_block_id),
                    *server,
                    &NetworkTarget::All,
                );
            }
        }
    }
}

/// Handles chest-open requests and returns the authoritative inventory contents.
pub fn handle_chest_inventory_open_messages(
    mut q: Query<(Entity, &mut MessageReceiver<ClientChestInventoryOpen>), With<ClientOf>>,
    q_remote_id: Query<&RemoteId>,
    mut state: ResMut<ServerState>,
    mut multi_sender: ServerMultiMessageSender,
    server: Single<&Server>,
) {
    let mut requests = Vec::new();
    for (entity, mut receiver) in q.iter_mut() {
        for message in receiver.receive() {
            requests.push((entity, message));
        }
    }

    for (entity, message) in requests {
        if let Some(player) = state.players.get_mut(&entity) {
            player.last_seen = Instant::now();
        }

        let slots = chest_payload_slots_from_region_slots(
            state
                .load_chest_inventory_slots(message.world_pos)
                .as_slice(),
        );
        let peer_id = q_remote_id
            .get(entity)
            .map(|remote_id| remote_id.0)
            .unwrap_or(PeerId::Entity(entity.to_bits()));
        let _ = multi_sender.send::<_, UnorderedReliable>(
            &ServerChestInventoryContents::new(message.world_pos, slots),
            *server,
            &NetworkTarget::Single(peer_id),
        );
    }
}

/// Handles authoritative chest inventory persistence requests.
pub fn handle_chest_inventory_persist_messages(
    mut q: Query<(Entity, &mut MessageReceiver<ClientChestInventoryPersist>), With<ClientOf>>,
    mut state: ResMut<ServerState>,
) {
    let mut requests = Vec::new();
    for (entity, mut receiver) in q.iter_mut() {
        for message in receiver.receive() {
            requests.push((entity, message));
        }
    }

    for (entity, message) in requests {
        if let Some(player) = state.players.get_mut(&entity) {
            player.last_seen = Instant::now();
        }

        let saved_slots = chest_region_inventory_slots_from_payload(
            message.slots.as_slice(),
            state.item_registry.as_ref(),
        );
        if let Err(error) = state.persist_chest_inventory_slots(message.world_pos, saved_slots) {
            log::warn!(
                "Failed persisting chest inventory at {:?}: {}",
                message.world_pos,
                error
            );
        }
    }
}

/// Flushes deferred chunk saves after the authoritative runtime state was already updated.
pub fn flush_pending_chunk_saves(mut state: ResMut<ServerState>) {
    state.flush_pending_chunk_saves(32);
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

        if message.amount == 0 {
            continue;
        }

        let item_id =
            if message.item_id != 0 && state.item_registry.def_opt(message.item_id).is_some() {
                message.item_id
            } else if message.block_id != 0 {
                state
                    .item_registry
                    .item_for_block(message.block_id)
                    .unwrap_or(0)
            } else {
                0
            };
        if item_id == 0 {
            continue;
        }
        let block_id = state
            .item_registry
            .block_for_item(item_id)
            .unwrap_or(message.block_id);

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
                    item_id,
                    block_id,
                    has_motion: true,
                    spawn_translation,
                    initial_velocity: message.initial_velocity,
                },
            );

            let _ = multi_sender.send::<_, OrderedReliable>(
                &ServerDropSpawn::new(
                    drop_id,
                    message.location,
                    item_id,
                    block_id,
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
            &ServerDropPicked::new(drop.drop_id, player_id, drop.item_id, drop.block_id),
            *server,
            &NetworkTarget::All,
        );
    }
}

fn chest_payload_slots_from_region_slots(
    slots: &[StructureRegionInventorySlot],
) -> Vec<ChestInventorySlotPayload> {
    let mut payload = Vec::new();
    let mut occupied = HashSet::new();
    for slot in slots {
        if slot.count == 0 || !occupied.insert(slot.slot) {
            continue;
        }
        if slot.item.trim().is_empty() {
            continue;
        }
        payload.push(ChestInventorySlotPayload {
            slot: slot.slot,
            item: slot.item.clone(),
            count: slot.count.max(1),
        });
    }
    payload.sort_by_key(|slot| slot.slot);
    payload
}

fn chest_region_inventory_slots_from_payload(
    slots: &[ChestInventorySlotPayload],
    item_registry: &api::core::inventory::items::ItemRegistry,
) -> Vec<StructureRegionInventorySlot> {
    let mut saved_slots = Vec::new();
    let mut occupied = HashSet::new();
    for slot in slots {
        if slot.count == 0 || !occupied.insert(slot.slot) {
            continue;
        }
        let item_name = slot.item.trim();
        if item_name.is_empty() {
            continue;
        }
        let Some(item_id) = item_registry.id_opt(item_name) else {
            continue;
        };
        let Some(item_def) = item_registry.def_opt(item_id) else {
            continue;
        };
        saved_slots.push(StructureRegionInventorySlot {
            slot: slot.slot,
            item: item_def.localized_name.clone(),
            count: slot.count.max(1),
        });
    }
    saved_slots.sort_by_key(|slot| slot.slot);
    saved_slots
}
