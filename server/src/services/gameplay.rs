use crate::{models::HostedDrop, state::ServerState, types::Server};
use api::core::network::protocols::{
    ClientBlockBreak, ClientBlockPlace, ClientChunkInterest, ClientDropItem, ClientDropPickup,
    ClientKeepAlive, PlayerMove, PlayerSnapshot, ServerBlockBreak, ServerBlockPlace,
    ServerChunkData,
    ServerDropPicked, ServerDropSpawn,
};
use bevy_math::IVec2;
use naia_server::{
    UserKey,
    shared::default_channels::{
        OrderedReliableChannel, UnorderedReliableChannel, UnorderedUnreliableChannel,
    },
};
use std::collections::HashSet;
use std::time::Instant;

const MAX_STREAM_RADIUS: i32 = 12;
const MAX_STREAM_SENDS_PER_TICK: usize = 24;

pub fn handle_player_move(
    server: &mut Server,
    state: &mut ServerState,
    user_key: UserKey,
    movement: &PlayerMove,
) {
    if let Some(player) = state.players.get_mut(&user_key) {
        player.last_seen = Instant::now();
        player.translation = movement.translation;
        player.yaw = movement.yaw;
        player.pitch = movement.pitch;

        server.broadcast_message::<UnorderedUnreliableChannel, _>(&PlayerSnapshot::new(
            player.player_id,
            player.translation,
            player.yaw,
            player.pitch,
        ));
    }
}

pub fn handle_keepalive(
    _server: &mut Server,
    state: &mut ServerState,
    user_key: UserKey,
    _message: &ClientKeepAlive,
) {
    if let Some(player) = state.players.get_mut(&user_key) {
        player.last_seen = Instant::now();
    }
}

pub fn handle_chunk_interest(
    _server: &mut Server,
    state: &mut ServerState,
    user_key: UserKey,
    message: &ClientChunkInterest,
) {
    let Some(player) = state.players.get_mut(&user_key) else {
        return;
    };

    player.last_seen = Instant::now();

    let center = IVec2::new(message.center[0], message.center[1]);
    let radius = message.radius.clamp(1, MAX_STREAM_RADIUS);
    let mut desired_chunks = HashSet::new();
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            desired_chunks.insert(IVec2::new(center.x + dx, center.y + dz));
        }
    }

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

    let _ = player;

    for coord in to_send {
        state.queue_chunk_for_stream(user_key, coord);
    }
}

pub fn flush_chunk_streaming(server: &mut Server, state: &mut ServerState) {
    state.collect_ready_stream_chunks();

    let mut sent = 0usize;
    while sent < MAX_STREAM_SENDS_PER_TICK {
        let Some((user_key, coord)) = state.pending_chunk_sends.pop_front() else {
            break;
        };

        if !server.user_exists(&user_key) {
            continue;
        }

        let Some(player) = state.players.get(&user_key) else {
            continue;
        };
        if !player.streamed_chunks.contains(&coord) {
            continue;
        }

        let Some(encoded_chunk) = state.streamed_chunk_cache.get(&coord) else {
            state.queue_chunk_for_stream(user_key, coord);
            continue;
        };

        server.send_message::<UnorderedReliableChannel, _>(
            &user_key,
            &ServerChunkData::new([coord.x, coord.y], encoded_chunk.clone()),
        );
        sent += 1;
    }
}

pub fn handle_block_break(
    server: &mut Server,
    state: &mut ServerState,
    user_key: UserKey,
    message: &ClientBlockBreak,
) {
    if let Some(player) = state.players.get_mut(&user_key) {
        player.last_seen = Instant::now();
        let player_id = player.player_id;
        let _ = player;
        state.block_overrides.insert(message.location, 0);
        let (chunk_coord, _) = api::core::world::chunk_dimension::world_to_chunk_xz(
            message.location[0],
            message.location[2],
        );
        state.invalidate_streamed_chunk(chunk_coord);
        state.persist_block_overrides();
        server.broadcast_message::<OrderedReliableChannel, _>(&ServerBlockBreak::new(
            player_id,
            message.location,
        ));

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

            server.broadcast_message::<OrderedReliableChannel, _>(&ServerDropSpawn::new(
                drop_id,
                message.location,
                message.drop_block_id,
                false,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
            ));
        }
    }
}

pub fn handle_block_place(
    server: &mut Server,
    state: &mut ServerState,
    user_key: UserKey,
    message: &ClientBlockPlace,
) {
    if let Some(player) = state.players.get_mut(&user_key) {
        player.last_seen = Instant::now();
        let player_id = player.player_id;
        let _ = player;
        state
            .block_overrides
            .insert(message.location, message.block_id);
        let (chunk_coord, _) = api::core::world::chunk_dimension::world_to_chunk_xz(
            message.location[0],
            message.location[2],
        );
        state.invalidate_streamed_chunk(chunk_coord);
        state.persist_block_overrides();
        server.broadcast_message::<OrderedReliableChannel, _>(&ServerBlockPlace::new(
            player_id,
            message.location,
            message.block_id,
        ));
    }
}

pub fn handle_drop_item(
    server: &mut Server,
    state: &mut ServerState,
    user_key: UserKey,
    message: &ClientDropItem,
) {
    let Some(player) = state.players.get_mut(&user_key) else {
        return;
    };
    player.last_seen = Instant::now();

    if message.block_id == 0 || message.amount == 0 {
        return;
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

        server.broadcast_message::<OrderedReliableChannel, _>(&ServerDropSpawn::new(
            drop_id,
            message.location,
            message.block_id,
            true,
            spawn_translation,
            message.initial_velocity,
        ));
    }
}

pub fn handle_drop_pickup(
    server: &mut Server,
    state: &mut ServerState,
    user_key: UserKey,
    message: &ClientDropPickup,
) {
    let Some(player) = state.players.get_mut(&user_key) else {
        return;
    };
    player.last_seen = Instant::now();

    let Some(drop) = state.drops.remove(&message.drop_id) else {
        return;
    };

    server.broadcast_message::<OrderedReliableChannel, _>(&ServerDropPicked::new(
        drop.drop_id,
        player.player_id,
        drop.block_id,
    ));
}
