use crate::{state::ServerRuntimeConfig, types::Server};
use log::info;
use multiplayer::{
    config::NetworkSettings,
    discovery::{LanDiscoveryServer, LanServerInfo},
    protocol::protocol,
};
use naia_server::{ServerConfig, transport::udp};

pub struct BootstrapResult {
    pub server: Server,
    pub discovery: Option<LanDiscoveryServer>,
    pub runtime_config: ServerRuntimeConfig,
}

pub fn bootstrap_server() -> BootstrapResult {
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

    BootstrapResult {
        server,
        discovery,
        runtime_config: ServerRuntimeConfig {
            server_name: server_settings.server_name,
            motd: server_settings.motd,
            max_players: server_settings.max_players,
        },
    }
}
