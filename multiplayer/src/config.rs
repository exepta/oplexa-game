use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::net::{IpAddr, UdpSocket};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSettings {
    pub client: ClientNetworkSettings,
    pub server: ServerNetworkSettings,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            client: ClientNetworkSettings::default(),
            server: ServerNetworkSettings::default(),
        }
    }
}

impl NetworkSettings {
    pub fn load_or_create(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();

        if path.exists() {
            let contents =
                fs::read_to_string(path).expect("Failed to read multiplayer config file");
            toml::from_str(&contents).expect("Failed to parse multiplayer config file")
        } else {
            let settings = Self::default();
            settings.save(path).expect("Failed to create multiplayer config file");
            settings
        }
    }

    pub fn save(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let contents = toml::to_string_pretty(self).expect("Failed to serialize network config");
        fs::write(path, contents)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientNetworkSettings {
    pub enabled: bool,
    pub connect_on_startup: bool,
    pub session_url: String,
    pub player_name: String,
    pub lan_discovery: bool,
    pub lan_discovery_port: u16,
    pub transform_send_interval_ms: u64,
}

impl Default for ClientNetworkSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            connect_on_startup: false,
            session_url: "http://127.0.0.1:14191".to_string(),
            player_name: "Player".to_string(),
            lan_discovery: true,
            lan_discovery_port: 14192,
            transform_send_interval_ms: 50,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerNetworkSettings {
    pub bind_ip: String,
    pub game_port: u16,
    pub advertise_host: String,
    pub server_name: String,
    pub motd: String,
    pub max_players: usize,
    pub lan_discovery: bool,
    pub lan_discovery_port: u16,
}

impl Default for ServerNetworkSettings {
    fn default() -> Self {
        Self {
            bind_ip: "0.0.0.0".to_string(),
            game_port: 14191,
            advertise_host: "auto".to_string(),
            server_name: "Oplexa LAN".to_string(),
            motd: "Open to LAN".to_string(),
            max_players: 8,
            lan_discovery: true,
            lan_discovery_port: 14192,
        }
    }
}

impl ServerNetworkSettings {
    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.bind_ip, self.game_port)
    }

    pub fn advertised_host(&self) -> String {
        if self.advertise_host.eq_ignore_ascii_case("auto") {
            resolve_local_ip()
                .map(|ip| ip.to_string())
                .unwrap_or_else(|| "127.0.0.1".to_string())
        } else {
            self.advertise_host.clone()
        }
    }

    pub fn session_url(&self) -> String {
        format!("http://{}:{}", self.advertised_host(), self.game_port)
    }
}

fn resolve_local_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip())
}
