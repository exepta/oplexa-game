use crate::state::ServerRuntimeConfig;
use api::core::network::{
    config::DedicatedServerSettings,
    discovery::{LanDiscoveryServer, LanServerInfo},
};
use api::core::world::spawn::ensure_world_spawn_generated;
use bevy::ecs::event::EntityTrigger;
use bevy::prelude::*;
use lightyear::connection::server::Start;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use log::info;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

pub struct BootstrapResult {
    pub discovery: Option<LanDiscoveryServer>,
    pub runtime_config: ServerRuntimeConfig,
    pub world_root: PathBuf,
    pub bind_addr: SocketAddr,
}

pub fn load_bootstrap() -> BootstrapResult {
    let settings_path = DedicatedServerSettings::settings_path("server.settings.toml");
    let server_settings = DedicatedServerSettings::load_or_create(&settings_path);
    let (world_root, world_seed, spawn_translation) = prepare_server_world(&server_settings);

    let bind_addr: SocketAddr = server_settings
        .bind_addr()
        .parse()
        .expect("Invalid bind address in server settings");

    let public_url = server_settings.session_url();

    let discovery = Some(
        LanDiscoveryServer::bind(
            server_settings.discovery_port(),
            LanServerInfo {
                server_name: server_settings.server_name.clone(),
                motd: server_settings.motd.clone(),
                session_url: public_url.clone(),
                current_players: 0,
                max_players: server_settings.max_players,
                observed_addr: None,
            },
        )
        .expect("Failed to start LAN discovery socket"),
    );

    info!(
        "Server will listen on {} (session URL: {}, world: {:?}, seed: {}, dead-entity-check={}s)",
        bind_addr,
        public_url,
        world_root,
        world_seed,
        server_settings.dead_entity_check_interval_secs
    );

    BootstrapResult {
        discovery,
        runtime_config: ServerRuntimeConfig {
            server_name: server_settings.server_name,
            motd: server_settings.motd,
            max_players: server_settings.max_players,
            client_timeout: server_settings.client_timeout,
            world_name: server_settings.world_name,
            world_seed,
            spawn_translation,
            chunk_stream_sends_per_tick_base: server_settings.chunk_stream_sends_per_tick_base,
            chunk_stream_sends_per_tick_per_client: server_settings
                .chunk_stream_sends_per_tick_per_client,
            chunk_stream_sends_per_tick_max: server_settings.chunk_stream_sends_per_tick_max,
            chunk_stream_inflight_per_client: server_settings.chunk_stream_inflight_per_client,
            chunk_flight_timeout_ms: server_settings.chunk_flight_timeout_ms,
            max_stream_radius: server_settings.max_stream_radius,
            dead_entity_check_interval_secs: server_settings.dead_entity_check_interval_secs,
        },
        world_root,
        bind_addr,
    }
}

/// Startup system: spawn the lightyear Server entity and trigger `Start`.
pub fn spawn_server(mut commands: Commands, config: Res<ServerBootstrapConfig>) {
    let server_entity = commands
        .spawn((
            Name::new("NetworkServer"),
            NetcodeServer::new(NetcodeConfig::default()),
            LocalAddr(config.bind_addr),
            WebSocketServerIo {
                config: ServerConfig::builder()
                    .with_bind_address(config.bind_addr)
                    .with_no_encryption(),
            },
        ))
        .id();

    commands.trigger_with(
        Start {
            entity: server_entity,
        },
        EntityTrigger,
    );
    info!("Lightyear server started on {}", config.bind_addr);
}

/// Resource injected before App::run() so `spawn_server` can read the bind address.
#[derive(Resource)]
pub struct ServerBootstrapConfig {
    pub bind_addr: SocketAddr,
}

// ── World preparation helpers ─────────────────────────────────────────────────

fn prepare_server_world(settings: &DedicatedServerSettings) -> (PathBuf, i32, [f32; 3]) {
    let world_root =
        PathBuf::from("worlds").join(normalize_world_name(settings.world_name.as_str()));
    info!("Preparing server world at {:?}", world_root);
    if let Err(error) = fs::create_dir_all(world_root.join("region")) {
        panic!(
            "Failed to create server world at {:?}: {}",
            world_root, error
        );
    }

    let seed_file = world_root.join("seed.txt");
    let world_seed = if let Some(seed) = read_world_seed(&seed_file) {
        info!("Loaded world seed {} from {:?}", seed, seed_file);
        seed
    } else {
        if let Err(error) = fs::write(&seed_file, settings.world_seed.to_string()) {
            panic!("Failed to write world seed file {:?}: {}", seed_file, error);
        }
        info!(
            "Created world seed {} at {:?}",
            settings.world_seed, seed_file
        );
        settings.world_seed
    };

    info!("Ensuring spawn chunks are built...");
    let spawn_translation = ensure_world_spawn_generated(&world_root, world_seed);
    info!(
        "World ready. Spawn translation: [{:.2}, {:.2}, {:.2}]",
        spawn_translation[0], spawn_translation[1], spawn_translation[2]
    );

    (world_root, world_seed, spawn_translation)
}

fn read_world_seed(path: &Path) -> Option<i32> {
    let text = fs::read_to_string(path).ok()?;
    text.trim().parse::<i32>().ok()
}

fn normalize_world_name(raw_name: &str) -> String {
    let normalized = raw_name
        .trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect::<String>();

    if normalized.is_empty() {
        "world".to_string()
    } else {
        normalized
    }
}
