use crate::{models::HostedPlayer, state::ServerState, types::Server};
use api::core::network::protocols::{
    PlayerJoined, PlayerLeft, PlayerSnapshot, ServerBlockBreak, ServerBlockPlace, ServerDropSpawn,
    ServerWelcome,
};
use log::{info, warn};
use naia_server::{
    UserKey,
    shared::default_channels::{
        OrderedReliableChannel, UnorderedReliableChannel, UnorderedUnreliableChannel,
    },
};
use std::collections::HashSet;
use std::time::Instant;

pub fn purge_stale_players(server: &mut Server, state: &mut ServerState, timeout_secs: u64) {
    let now = Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs.max(1));
    let stale_user_keys: Vec<UserKey> = state
        .players
        .iter()
        .filter_map(|(user_key, player)| {
            (now.duration_since(player.last_seen) > timeout).then_some(*user_key)
        })
        .collect();

    for user_key in stale_user_keys {
        if server.user_exists(&user_key) {
            server.user_mut(&user_key).disconnect();
        }

        handle_player_disconnect(server, state, user_key, format!("timeout ({:?})", timeout));
    }
}

pub fn handle_auth(
    server: &mut Server,
    state: &mut ServerState,
    user_key: UserKey,
    username: String,
    max_players: usize,
) {
    if username.trim().is_empty() {
        warn!("Rejected empty username for {:?}", user_key);
        server.reject_connection(&user_key);
        return;
    }

    if state.players.len() >= max_players {
        warn!("Server full, rejecting {:?}", user_key);
        server.reject_connection(&user_key);
        return;
    }

    state.pending_auth.insert(user_key, username);
    server.accept_connection(&user_key);
}

pub fn handle_connect(
    server: &mut Server,
    state: &mut ServerState,
    user_key: UserKey,
    server_name: &str,
    motd: &str,
    world_name: &str,
    world_seed: i32,
    spawn_translation: [f32; 3],
) {
    let username = state
        .pending_auth
        .remove(&user_key)
        .unwrap_or_else(|| format!("Player{}", state.next_player_id));

    let player = HostedPlayer {
        player_id: state.next_player_id,
        username: username.clone(),
        translation: spawn_translation,
        yaw: 0.0,
        pitch: 0.0,
        last_seen: Instant::now(),
        streamed_chunks: HashSet::new(),
    };
    state.next_player_id = state.next_player_id.wrapping_add(1);

    server.send_message::<UnorderedReliableChannel, _>(
        &user_key,
        &ServerWelcome {
            player_id: player.player_id,
            server_name: server_name.to_string(),
            motd: motd.to_string(),
            world_name: world_name.to_string(),
            world_seed,
            spawn_translation,
        },
    );

    for other in state.players.values() {
        server.send_message::<UnorderedReliableChannel, _>(
            &user_key,
            &PlayerJoined::new(other.player_id, other.username.clone()),
        );
        server.send_message::<UnorderedUnreliableChannel, _>(
            &user_key,
            &PlayerSnapshot::new(other.player_id, other.translation, other.yaw, other.pitch),
        );
    }

    for (location, block_id) in &state.block_overrides {
        if *block_id == 0 {
            server.send_message::<OrderedReliableChannel, _>(
                &user_key,
                &ServerBlockBreak::new(0, *location),
            );
        } else {
            server.send_message::<OrderedReliableChannel, _>(
                &user_key,
                &ServerBlockPlace::new(0, *location, *block_id),
            );
        }
    }

    for drop in state.drops.values() {
        server.send_message::<OrderedReliableChannel, _>(
            &user_key,
            &ServerDropSpawn::new(
                drop.drop_id,
                drop.location,
                drop.block_id,
                drop.has_motion,
                drop.spawn_translation,
                drop.initial_velocity,
            ),
        );
    }

    let player_id = player.player_id;
    let username = player.username.clone();
    state.players.insert(user_key, player);

    info!("{} joined as id {}", username, player_id);
    server
        .broadcast_message::<UnorderedReliableChannel, _>(&PlayerJoined::new(player_id, username));
    server.broadcast_message::<UnorderedUnreliableChannel, _>(&PlayerSnapshot::new(
        player_id,
        spawn_translation,
        0.0,
        0.0,
    ));
}

pub fn handle_player_disconnect(
    server: &mut Server,
    state: &mut ServerState,
    user_key: UserKey,
    reason: String,
) {
    state.pending_auth.remove(&user_key);

    if let Some(player) = state.players.remove(&user_key) {
        info!("{} disconnected: {}", player.username, reason);
        server.broadcast_message::<UnorderedReliableChannel, _>(&PlayerLeft::new(player.player_id));
    }
}
