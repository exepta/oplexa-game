use crate::{models::HostedDrop, state::ServerState, types::Server};
use multiplayer::protocol::{
    ClientBlockBreak, ClientBlockPlace, ClientDropItem, ClientDropPickup, PlayerMove,
    PlayerSnapshot, ServerBlockBreak, ServerBlockPlace, ServerDropPicked, ServerDropSpawn,
};
use naia_server::{
    UserKey,
    shared::default_channels::{OrderedReliableChannel, UnorderedUnreliableChannel},
};
use std::time::Instant;

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

pub fn handle_block_break(
    server: &mut Server,
    state: &mut ServerState,
    user_key: UserKey,
    message: &ClientBlockBreak,
) {
    if let Some(player) = state.players.get_mut(&user_key) {
        player.last_seen = Instant::now();
        server.broadcast_message::<OrderedReliableChannel, _>(&ServerBlockBreak::new(
            player.player_id,
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
        server.broadcast_message::<OrderedReliableChannel, _>(&ServerBlockPlace::new(
            player.player_id,
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
