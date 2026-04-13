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
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use sysinfo::System;

const WORLD_META_FILE: &str = "world.meta.json";
const LEGACY_SEED_FILE: &str = "seed.txt";
const LEGACY_SPAWN_FILE: &str = "spawn.txt";

/// Represents bootstrap result used by the `bootstrap` module.
pub struct BootstrapResult {
    pub discovery: Option<LanDiscoveryServer>,
    pub runtime_config: ServerRuntimeConfig,
    pub world_root: PathBuf,
    pub bind_addr: SocketAddr,
}

/// Loads bootstrap for the `bootstrap` module.
pub fn load_bootstrap() -> BootstrapResult {
    let settings_path = DedicatedServerSettings::settings_path("server.settings.toml");
    let server_settings = DedicatedServerSettings::load_or_create(&settings_path);
    log_system_profile();
    let (world_root, world_seed, spawn_translation) = prepare_server_world(&server_settings);

    let bind_addr: SocketAddr = server_settings
        .bind_addr()
        .parse()
        .expect("Invalid bind address in server settings");
    info!(
        "Network binding configured: host={}, port={}",
        bind_addr.ip(),
        bind_addr.port()
    );

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
        "Server will listen on {} (session URL: {}, world: {:?}, seed: {}, dead-entity-check={}s, chunk-stream: base={}, per-client={}, max={}, inflight/client={}, timeout={}ms, gen-max-inflight={})",
        bind_addr,
        public_url,
        world_root,
        world_seed,
        server_settings.dead_entity_check_interval_secs,
        server_settings.chunk_stream_sends_per_tick_base,
        server_settings.chunk_stream_sends_per_tick_per_client,
        server_settings.chunk_stream_sends_per_tick_max,
        server_settings.chunk_stream_inflight_per_client,
        server_settings.chunk_flight_timeout_ms,
        server_settings.chunk_stream_gen_max_inflight
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
            chunk_stream_gen_max_inflight: server_settings.chunk_stream_gen_max_inflight,
            max_stream_radius: server_settings.max_stream_radius,
            locate_search_radius: server_settings.locate_search_radius,
            dead_entity_check_interval_secs: server_settings.dead_entity_check_interval_secs,
        },
        world_root,
        bind_addr,
    }
}

fn log_system_profile() {
    let logical_cores = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1);
    let total_ram_bytes = System::new_all().total_memory();
    let total_ram_gib = total_ram_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    info!(
        "System profile: logical_cores={}, total_ram={:.2} GiB",
        logical_cores, total_ram_gib
    );
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldMeta {
    seed: i32,
    #[serde(default)]
    spawn_translation: Option<[f32; 3]>,
}

// ── World preparation helpers ─────────────────────────────────────────────────

/// Runs the `prepare_server_world` routine for prepare server world in the `bootstrap` module.
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

    let meta_path = world_root.join(WORLD_META_FILE);
    let seed_file = world_root.join(LEGACY_SEED_FILE);
    let spawn_file = world_root.join(LEGACY_SPAWN_FILE);

    let world_meta = read_world_meta(&meta_path);
    let world_seed = match world_meta.as_ref() {
        Some(meta) => {
            info!("Loaded world seed {} from {:?}", meta.seed, meta_path);
            meta.seed
        }
        None => {
            if let Some(seed) = read_legacy_world_seed(&seed_file) {
                info!("Loaded legacy world seed {} from {:?}", seed, seed_file);
                seed
            } else {
                info!(
                    "Using world seed {} from server settings",
                    settings.world_seed
                );
                settings.world_seed
            }
        }
    };

    let existing_spawn = world_meta
        .as_ref()
        .and_then(|meta| meta.spawn_translation)
        .or_else(|| read_legacy_spawn_translation(&spawn_file));

    if let Some(spawn_translation) = existing_spawn {
        info!(
            "Loaded world spawn [{:.2}, {:.2}, {:.2}]",
            spawn_translation[0], spawn_translation[1], spawn_translation[2]
        );
    }

    info!("Ensuring spawn chunks are built...");
    let generated_spawn = ensure_world_spawn_generated(&world_root, world_seed);
    let spawn_translation = existing_spawn.unwrap_or(generated_spawn);

    let final_meta = WorldMeta {
        seed: world_seed,
        spawn_translation: Some(spawn_translation),
    };
    if let Err(error) = write_world_meta(&meta_path, &final_meta) {
        warn!("Failed to persist world meta {:?}: {}", meta_path, error);
    }

    info!(
        "World ready. Spawn translation: [{:.2}, {:.2}, {:.2}]",
        spawn_translation[0], spawn_translation[1], spawn_translation[2]
    );

    (world_root, world_seed, spawn_translation)
}

/// Reads legacy world seed for the `bootstrap` module.
fn read_legacy_world_seed(path: &Path) -> Option<i32> {
    let text = fs::read_to_string(path).ok()?;
    text.trim().parse::<i32>().ok()
}

fn read_legacy_spawn_translation(path: &Path) -> Option<[f32; 3]> {
    let text = fs::read_to_string(path).ok()?;
    let mut parts = text.split_whitespace();
    let x = parts.next()?.parse::<f32>().ok()?;
    let y = parts.next()?.parse::<f32>().ok()?;
    let z = parts.next()?.parse::<f32>().ok()?;
    Some([x, y, z])
}

fn read_world_meta(path: &Path) -> Option<WorldMeta> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str::<WorldMeta>(&text).ok()
}

fn write_world_meta(path: &Path, world_meta: &WorldMeta) -> io::Result<()> {
    let text = serde_json::to_string_pretty(world_meta)
        .map_err(|error| io::Error::other(error.to_string()))?;
    fs::write(path, text)
}

/// Runs the `normalize_world_name` routine for normalize world name in the `bootstrap` module.
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
