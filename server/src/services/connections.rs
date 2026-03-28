use crate::{
    models::{HostedPlayer, PLAYER_STALE_TIMEOUT},
    state::ServerState,
    types::Server,
};
use log::{info, warn};
use multiplayer::protocol::{
    PlayerJoined, PlayerLeft, PlayerSnapshot, ServerDropSpawn, ServerWelcome,
};
use naia_server::{
    UserKey,
    shared::default_channels::{
        OrderedReliableChannel, UnorderedReliableChannel, UnorderedUnreliableChannel,
    },
};
use std::time::Instant;

pub fn purge_stale_players(server: &mut Server, state: &mut ServerState) {
    let now = Instant::now();
    let stale_user_keys: Vec<UserKey> = state
        .players
        .iter()
        .filter_map(|(user_key, player)| {
            (now.duration_since(player.last_seen) > PLAYER_STALE_TIMEOUT).then_some(*user_key)
        })
        .collect();

    for user_key in stale_user_keys {
        if server.user_exists(&user_key) {
            server.user_mut(&user_key).disconnect();
        }

        handle_player_disconnect(
            server,
            state,
            user_key,
            format!("timeout ({:?})", PLAYER_STALE_TIMEOUT),
        );
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
) {
    let username = state
        .pending_auth
        .remove(&user_key)
        .unwrap_or_else(|| format!("Player{}", state.next_player_id));

    let player = HostedPlayer {
        player_id: state.next_player_id,
        username: username.clone(),
        translation: [0.0, 180.0, 0.0],
        yaw: 0.0,
        pitch: 0.0,
        last_seen: Instant::now(),
    };
    state.next_player_id = state.next_player_id.wrapping_add(1);

    server.send_message::<UnorderedReliableChannel, _>(
        &user_key,
        &ServerWelcome::new(player.player_id, server_name.to_string(), motd.to_string()),
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
