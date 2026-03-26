use log::{error, info, warn};
use multiplayer::{
    config::NetworkSettings,
    discovery::{LanDiscoveryServer, LanServerInfo},
    protocol::{
        Auth, ClientBlockBreak, ClientBlockPlace, PlayerJoined, PlayerLeft, PlayerMove,
        PlayerSnapshot, ServerBlockBreak, ServerBlockPlace, ServerWelcome, protocol,
    },
    world::{NetworkEntity, NetworkWorld},
};
use naia_server::{
    AuthEvent, ConnectEvent, DisconnectEvent, ErrorEvent, MessageEvent, Server as NaiaServer,
    ServerConfig, UserKey,
    shared::default_channels::{
        OrderedReliableChannel, UnorderedReliableChannel, UnorderedUnreliableChannel,
    },
    transport::udp,
};
use simple_logger::SimpleLogger;
use std::{collections::HashMap, thread::sleep, time::Duration};

type Server = NaiaServer<NetworkEntity>;

struct HostedPlayer {
    player_id: u64,
    username: String,
    translation: [f32; 3],
    yaw: f32,
    pitch: f32,
}

fn main() {
    SimpleLogger::new()
        .init()
        .expect("Logger initialization failed");

    let settings = NetworkSettings::load_or_create("config/network.toml");
    let server_settings = settings.server.clone();
    let bind_addr = server_settings
        .bind_addr()
        .parse()
        .expect("Invalid bind address in config/network.toml");
    let public_url = server_settings.session_url();

    let server_addrs = udp::ServerAddrs::new(bind_addr, bind_addr, &public_url);
    let protocol = protocol();
    let socket = udp::Socket::new(&server_addrs, protocol.socket.link_condition.clone());

    let mut server = Server::new(ServerConfig::default(), protocol);
    server.listen(socket);

    let discovery = if server_settings.lan_discovery {
        Some(
            LanDiscoveryServer::bind(
                server_settings.lan_discovery_port,
                LanServerInfo {
                    server_name: server_settings.server_name.clone(),
                    motd: server_settings.motd.clone(),
                    session_url: public_url.clone(),
                    observed_addr: None,
                },
            )
            .expect("Failed to start LAN discovery socket"),
        )
    } else {
        None
    };

    info!(
        "Server listening on {} (session URL: {})",
        bind_addr, public_url
    );

    let mut world = NetworkWorld::default();
    let mut next_player_id = 1_u64;
    let mut pending_auth = HashMap::<UserKey, String>::new();
    let mut players = HashMap::<UserKey, HostedPlayer>::new();

    loop {
        if let Some(discovery) = &discovery {
            if let Err(error) = discovery.poll() {
                warn!("LAN discovery error: {}", error);
            }
        }

        let mut events = server.receive(world.proxy_mut());
        let mut busy = false;

        for (user_key, auth) in events.read::<AuthEvent<Auth>>() {
            busy = true;

            if auth.username.trim().is_empty() {
                warn!("Rejected empty username for {:?}", user_key);
                server.reject_connection(&user_key);
                continue;
            }

            if players.len() >= server_settings.max_players {
                warn!("Server full, rejecting {:?}", user_key);
                server.reject_connection(&user_key);
                continue;
            }

            pending_auth.insert(user_key, auth.username.to_string());
            server.accept_connection(&user_key);
        }

        for user_key in events.read::<ConnectEvent>() {
            busy = true;

            let username = pending_auth
                .remove(&user_key)
                .unwrap_or_else(|| format!("Player{}", next_player_id));

            let player = HostedPlayer {
                player_id: next_player_id,
                username: username.clone(),
                translation: [0.0, 180.0, 0.0],
                yaw: 0.0,
                pitch: 0.0,
            };
            next_player_id = next_player_id.wrapping_add(1);

            server.send_message::<UnorderedReliableChannel, _>(
                &user_key,
                &ServerWelcome::new(
                    player.player_id,
                    server_settings.server_name.clone(),
                    server_settings.motd.clone(),
                ),
            );

            for other in players.values() {
                server.send_message::<UnorderedReliableChannel, _>(
                    &user_key,
                    &PlayerJoined::new(other.player_id, other.username.clone()),
                );
                server.send_message::<UnorderedUnreliableChannel, _>(
                    &user_key,
                    &PlayerSnapshot::new(
                        other.player_id,
                        other.translation,
                        other.yaw,
                        other.pitch,
                    ),
                );
            }

            players.insert(user_key, player);
            let joined = players
                .get(&user_key)
                .expect("Player must exist after insert");

            info!("{} joined as id {}", joined.username, joined.player_id);
            server.broadcast_message::<UnorderedReliableChannel, _>(&PlayerJoined::new(
                joined.player_id,
                joined.username.clone(),
            ));
        }

        for (user_key, movement) in
            events.read::<MessageEvent<UnorderedUnreliableChannel, PlayerMove>>()
        {
            busy = true;

            if let Some(player) = players.get_mut(&user_key) {
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

        for (user_key, message) in
            events.read::<MessageEvent<OrderedReliableChannel, ClientBlockBreak>>()
        {
            busy = true;

            if let Some(player) = players.get(&user_key) {
                server.broadcast_message::<OrderedReliableChannel, _>(&ServerBlockBreak::new(
                    player.player_id,
                    message.location,
                ));
            }
        }

        for (user_key, message) in
            events.read::<MessageEvent<OrderedReliableChannel, ClientBlockPlace>>()
        {
            busy = true;

            if let Some(player) = players.get(&user_key) {
                server.broadcast_message::<OrderedReliableChannel, _>(&ServerBlockPlace::new(
                    player.player_id,
                    message.location,
                    message.block_id,
                ));
            }
        }

        for (user_key, user) in events.read::<DisconnectEvent>() {
            busy = true;
            pending_auth.remove(&user_key);

            if let Some(player) = players.remove(&user_key) {
                info!("{} disconnected from {}", player.username, user.address());
                server.broadcast_message::<UnorderedReliableChannel, _>(&PlayerLeft::new(
                    player.player_id,
                ));
            }
        }

        for error in events.read::<ErrorEvent>() {
            busy = true;
            error!("Naia server error: {}", error);
        }

        server.send_all_updates(world.proxy());

        if !busy {
            sleep(Duration::from_millis(3));
        }
    }
}
