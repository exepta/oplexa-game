use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::net::{IpAddr, UdpSocket};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSettings {
    pub client: ClientNetworkSettings,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            client: ClientNetworkSettings::default(),
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
            settings
                .save(path)
                .expect("Failed to create multiplayer config file");
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
#[serde(rename_all = "kebab-case")]
pub struct DedicatedServerSettings {
    pub ip: String,
    pub port: u16,
    pub world_name: String,
    pub world_seed: i32,
    pub server_name: String,
    pub motd: String,
    #[serde(default = "default_max_players")]
    pub max_players: usize,
    #[serde(default = "default_client_timeout")]
    pub client_timeout: u64,
    #[serde(default = "default_chunk_stream_sends_per_tick_base")]
    pub chunk_stream_sends_per_tick_base: usize,
    #[serde(default = "default_chunk_stream_sends_per_tick_per_client")]
    pub chunk_stream_sends_per_tick_per_client: usize,
    #[serde(default = "default_chunk_stream_sends_per_tick_max")]
    pub chunk_stream_sends_per_tick_max: usize,
    #[serde(default = "default_chunk_stream_inflight_per_client")]
    pub chunk_stream_inflight_per_client: usize,
    #[serde(default = "default_chunk_flight_timeout_ms")]
    pub chunk_flight_timeout_ms: u64,
    #[serde(default = "default_max_stream_radius")]
    pub max_stream_radius: i32,
}

impl Default for DedicatedServerSettings {
    fn default() -> Self {
        Self {
            ip: "auto".to_string(),
            port: 14191,
            world_name: "world".to_string(),
            world_seed: 1337,
            server_name: "Oplexa Server".to_string(),
            motd: "Welcome to Oplexa".to_string(),
            max_players: default_max_players(),
            client_timeout: default_client_timeout(),
            chunk_stream_sends_per_tick_base: default_chunk_stream_sends_per_tick_base(),
            chunk_stream_sends_per_tick_per_client: default_chunk_stream_sends_per_tick_per_client(
            ),
            chunk_stream_sends_per_tick_max: default_chunk_stream_sends_per_tick_max(),
            chunk_stream_inflight_per_client: default_chunk_stream_inflight_per_client(),
            chunk_flight_timeout_ms: default_chunk_flight_timeout_ms(),
            max_stream_radius: default_max_stream_radius(),
        }
    }
}

impl DedicatedServerSettings {
    pub fn load_or_create(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();

        if path.exists() {
            let contents =
                fs::read_to_string(path).expect("Failed to read dedicated server settings");
            toml::from_str(&contents).expect("Failed to parse dedicated server settings")
        } else {
            let settings = Self::default();
            settings
                .save(path)
                .expect("Failed to create dedicated server settings");
            settings
        }
    }

    pub fn save(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let contents =
            toml::to_string_pretty(self).expect("Failed to serialize dedicated server settings");
        fs::write(path, contents)
    }

    pub fn settings_path(default_path: &str) -> PathBuf {
        if let Ok(path) = std::env::var("OPLEXA_SERVER_SETTINGS")
            && !path.trim().is_empty()
        {
            return PathBuf::from(path);
        }

        PathBuf::from(default_path)
    }

    pub fn bind_addr(&self) -> String {
        format!("0.0.0.0:{}", self.port)
    }

    pub fn advertised_host(&self) -> String {
        if self.ip.trim().is_empty() || self.ip.eq_ignore_ascii_case("auto") {
            resolve_local_ip()
                .map(|ip| ip.to_string())
                .unwrap_or_else(|| "127.0.0.1".to_string())
        } else {
            self.ip.clone()
        }
    }

    pub fn session_url(&self) -> String {
        format!("http://{}:{}", self.advertised_host(), self.port)
    }

    pub fn discovery_port(&self) -> u16 {
        self.port.saturating_add(1)
    }
}

fn default_max_players() -> usize {
    10
}

fn default_client_timeout() -> u64 {
    60
}

fn default_chunk_stream_sends_per_tick_base() -> usize {
    24
}

fn default_chunk_stream_sends_per_tick_per_client() -> usize {
    6
}

fn default_chunk_stream_sends_per_tick_max() -> usize {
    256
}

fn default_chunk_stream_inflight_per_client() -> usize {
    24
}

fn default_chunk_flight_timeout_ms() -> u64 {
    500
}

fn default_max_stream_radius() -> i32 {
    12
}

fn resolve_local_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip())
}
