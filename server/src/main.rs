mod bootstrap;
mod models;
mod services;
mod state;
mod types;

use crate::{
    bootstrap::bootstrap_server,
    services::{
        flush_chunk_streaming, handle_auth, handle_block_break, handle_block_place,
        handle_chunk_interest, handle_connect, handle_drop_item, handle_drop_pickup,
        handle_player_disconnect, handle_player_move, purge_stale_players,
    },
    state::ServerState,
};
use api::core::network::protocols::{
    Auth, ClientBlockBreak, ClientBlockPlace, ClientChunkInterest, ClientDropItem,
    ClientDropPickup, PlayerMove,
};
use log::{error, warn};
use naia_server::{
    AuthEvent, ConnectEvent, DisconnectEvent, ErrorEvent, MessageEvent,
    shared::default_channels::{OrderedReliableChannel, UnorderedUnreliableChannel},
};
use simple_logger::SimpleLogger;
use std::{thread::sleep, time::Duration};

fn main() {
    SimpleLogger::new()
        .init()
        .expect("Logger initialization failed");

    let bootstrap = bootstrap_server();
    let mut server = bootstrap.server;
    let discovery = bootstrap.discovery;
    let runtime_config = bootstrap.runtime_config;
    let mut state = ServerState::new(bootstrap.world_root, runtime_config.world_seed);

    loop {
        if let Some(discovery) = &discovery {
            if let Err(error) = discovery.poll() {
                warn!("LAN discovery error: {}", error);
            }
        }

        let mut events = server.receive(state.world.proxy_mut());
        let mut busy = false;

        purge_stale_players(&mut server, &mut state, runtime_config.client_timeout);

        for (user_key, auth) in events.read::<AuthEvent<Auth>>() {
            busy = true;
            handle_auth(
                &mut server,
                &mut state,
                user_key,
                auth.username.to_string(),
                runtime_config.max_players,
            );
        }

        for user_key in events.read::<ConnectEvent>() {
            busy = true;
            handle_connect(
                &mut server,
                &mut state,
                user_key,
                &runtime_config.server_name,
                &runtime_config.motd,
                &runtime_config.world_name,
                runtime_config.world_seed,
                runtime_config.spawn_translation,
            );
        }

        for (user_key, movement) in
            events.read::<MessageEvent<UnorderedUnreliableChannel, PlayerMove>>()
        {
            busy = true;
            handle_player_move(&mut server, &mut state, user_key, &movement);
        }

        for (user_key, message) in
            events.read::<MessageEvent<OrderedReliableChannel, ClientChunkInterest>>()
        {
            busy = true;
            handle_chunk_interest(&mut server, &mut state, user_key, &message);
        }

        for (user_key, message) in
            events.read::<MessageEvent<OrderedReliableChannel, ClientBlockBreak>>()
        {
            busy = true;
            handle_block_break(&mut server, &mut state, user_key, &message);
        }

        for (user_key, message) in
            events.read::<MessageEvent<OrderedReliableChannel, ClientBlockPlace>>()
        {
            busy = true;
            handle_block_place(&mut server, &mut state, user_key, &message);
        }

        for (user_key, message) in
            events.read::<MessageEvent<OrderedReliableChannel, ClientDropItem>>()
        {
            busy = true;
            handle_drop_item(&mut server, &mut state, user_key, &message);
        }

        for (user_key, message) in
            events.read::<MessageEvent<OrderedReliableChannel, ClientDropPickup>>()
        {
            busy = true;
            handle_drop_pickup(&mut server, &mut state, user_key, &message);
        }

        for (user_key, user) in events.read::<DisconnectEvent>() {
            busy = true;
            handle_player_disconnect(
                &mut server,
                &mut state,
                user_key,
                format!("network disconnect ({})", user.address()),
            );
        }

        for error in events.read::<ErrorEvent>() {
            busy = true;
            error!("Naia server error: {}", error);
        }

        flush_chunk_streaming(&mut server, &mut state);

        server.send_all_updates(state.world.proxy());

        if !busy {
            sleep(Duration::from_millis(3));
        }
    }
}
