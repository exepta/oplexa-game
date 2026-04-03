use self::manager::ManagerPlugin;
use crate::core::chat::{ChatLine, ChatLog};
use crate::core::commands::{
    CommandSender, EntitySender, GameModeKind, SystemMessageLevel, SystemSender,
    default_chat_command_registry, parse_chat_command,
};
use crate::core::config::GlobalConfig;
use crate::core::debug::{BuildInfo, WorldInspectorState};
use crate::core::entities::player::inventory::PlayerInventory;
use crate::core::entities::player::{FlightState, FpsController, GameMode, GameModeState, Player};
use crate::core::events::block::block_player_events::{
    BlockBreakByPlayerEvent, BlockPlaceByPlayerEvent,
};
use crate::core::events::chunk_events::SubChunkNeedRemeshEvent;
use crate::core::events::ui_events::{
    ChatSubmitRequest, ConnectToServerRequest, DisconnectFromServerRequest, DropItemRequest,
    OpenToLanRequest, StopLanHostRequest,
};
use crate::core::inventory::items::{ItemRegistry, build_world_item_drop_visual};
use crate::core::multiplayer::{MultiplayerConnectionPhase, MultiplayerConnectionState};
use crate::core::states::states::{AppState, BeforeUiState, LoadingStates};
use crate::core::world::block::{BlockRegistry, VOXEL_SIZE, get_block_world};
use crate::core::world::chunk::{ChunkMap, LoadCenter, SEA_LEVEL};
use crate::core::world::chunk_dimension::{
    CX, CY, CZ, SEC_COUNT, Y_MAX, Y_MIN, world_to_chunk_xz, world_y_to_local,
};
use crate::core::world::fluid::{FluidChunk, FluidMap, WaterMeshIndex};
use crate::core::world::save::RegionCache;
use crate::core::world::{mark_dirty_block_and_neighbors, world_access_mut};
use crate::generator::chunk::chunk_utils::safe_despawn_entity;
use api::core::network::{
    config::{DedicatedServerSettings, NetworkSettings},
    discovery::{LanDiscoveryClient, LanServerInfo},
    protocols::{
        Auth, ClientBlockBreak, ClientBlockPlace, ClientChatMessage, ClientChunkInterest,
        ClientDropItem, ClientDropPickup, ClientKeepAlive, OrderedReliable, PlayerJoined,
        PlayerLeft, PlayerMove, PlayerSnapshot, ProtocolPlugin, ServerAuthRejected,
        ServerBlockBreak, ServerBlockPlace, ServerChatMessage, ServerChunkData, ServerDropPicked,
        ServerDropSpawn, ServerGameModeChanged, ServerWelcome, UnorderedReliable,
        UnorderedUnreliable,
    },
};
use bevy::ecs::event::EntityTrigger;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSamplerDescriptor};
use bevy::log::{BoxedLayer, Level, LogPlugin};
use bevy::math::primitives::Capsule3d;
use bevy::mesh::Mesh3d;
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::render_resource::WgpuFeatures;
use bevy::render::settings::{Backends, RenderCreation, WgpuSettings};
use bevy::window::{PresentMode, WindowMode, WindowResolution};
use bevy_inspector_egui::bevy_egui::EguiPlugin;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use chrono::Utc;
use dotenvy::dotenv;
use lightyear::prelude::client::{
    ClientConfig, ClientPlugins, Connect, Connected, Disconnect, Disconnected, NetcodeClient,
    NetcodeConfig, WebSocketClientIo, WebSocketScheme,
};
use lightyear::prelude::{Authentication, MessageReceiver, MessageSender, PeerAddr};
use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tracing_subscriber::Layer;
use tracing_subscriber::fmt::writer::BoxMakeWriter;

/// Represents remote player avatar used by the `client` module.
#[derive(Component)]
struct RemotePlayerAvatar {
    #[allow(dead_code)]
    player_id: u64,
}

/// Represents remote player snapshot point used by the `client` module.
#[derive(Clone, Copy, Debug)]
struct RemotePlayerSnapshotPoint {
    at_secs: f32,
    translation: Vec3,
    yaw: f32,
}

/// Represents remote player smoothing used by the `client` module.
#[derive(Debug, Default)]
struct RemotePlayerSmoothing {
    snapshots: VecDeque<RemotePlayerSnapshotPoint>,
}

impl RemotePlayerSmoothing {
    /// Runs the `with_initial_snapshot` routine for with initial snapshot in the `client` module.
    fn with_initial_snapshot(at_secs: f32, translation: Vec3, yaw: f32) -> Self {
        let mut snapshots = VecDeque::with_capacity(REMOTE_PLAYER_MAX_SNAPSHOT_POINTS);
        snapshots.push_back(RemotePlayerSnapshotPoint {
            at_secs,
            translation,
            yaw,
        });
        Self { snapshots }
    }

    /// Runs the `reset_snapshot` routine for reset snapshot in the `client` module.
    fn reset_snapshot(&mut self, at_secs: f32, translation: Vec3, yaw: f32) {
        self.snapshots.clear();
        self.snapshots.push_back(RemotePlayerSnapshotPoint {
            at_secs,
            translation,
            yaw,
        });
    }

    /// Runs the `push_snapshot` routine for push snapshot in the `client` module.
    fn push_snapshot(&mut self, at_secs: f32, translation: Vec3, yaw: f32) {
        let next = RemotePlayerSnapshotPoint {
            at_secs,
            translation,
            yaw,
        };

        if let Some(last) = self.snapshots.back_mut() {
            if at_secs <= last.at_secs {
                if (last.at_secs - at_secs).abs() <= 0.0001 {
                    *last = next;
                }
                return;
            }

            if last.translation.distance_squared(translation) <= 0.000001
                && angle_abs_diff(last.yaw, yaw) <= 0.0001
            {
                last.at_secs = at_secs;
                return;
            }
        }

        self.snapshots.push_back(next);
        while self.snapshots.len() > REMOTE_PLAYER_MAX_SNAPSHOT_POINTS {
            self.snapshots.pop_front();
        }
    }
}

/// Represents remote player visuals used by the `client` module.
#[derive(Resource)]
struct RemotePlayerVisuals {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

/// Represents multiplayer dropped item used by the `client` module.
#[derive(Component, Debug)]
struct MultiplayerDroppedItem {
    drop_id: u64,
    item_id: u16,
    pickup_ready_at: f32,
    next_pickup_request_at: f32,
    resting: bool,
    velocity: Vec3,
    angular_velocity: Vec3,
    spin_axis: Vec3,
    spin_speed: f32,
}

/// Represents multiplayer drop index used by the `client` module.
#[derive(Resource, Default)]
struct MultiplayerDropIndex {
    entities: HashMap<u64, Entity>,
}

/// Represents remote chunk stream state used by the `client` module.
#[derive(Resource, Default)]
struct RemoteChunkStreamState {
    last_requested_center: Option<IVec2>,
    last_requested_radius: Option<i32>,
}

/// Represents block id remap used by the `client` module.
#[derive(Resource, Default)]
struct BlockIdRemap {
    server_to_local: Vec<u16>,
    local_to_server: Vec<u16>,
    ready: bool,
}

impl BlockIdRemap {
    /// Runs the `reset` routine for reset in the `client` module.
    fn reset(&mut self) {
        self.server_to_local.clear();
        self.local_to_server.clear();
        self.ready = false;
    }

    /// Runs the `configure_from_server_palette` routine for configure from server palette in the `client` module.
    fn configure_from_server_palette(&mut self, palette: &[String], registry: &BlockRegistry) {
        self.server_to_local = vec![0; palette.len()];
        self.local_to_server = vec![0; registry.defs.len()];
        if !self.local_to_server.is_empty() {
            self.local_to_server[0] = 0;
        }

        let mut unknown_names = 0usize;

        for (server_id, block_name) in palette.iter().enumerate() {
            if server_id > u16::MAX as usize {
                break;
            }

            let server_id_u16 = server_id as u16;
            let local_id = registry.id_opt(block_name).unwrap_or(0);
            self.server_to_local[server_id] = local_id;

            if let Some(slot) = self.local_to_server.get_mut(local_id as usize)
                && (*slot == 0 || local_id == 0)
            {
                *slot = server_id_u16;
            }

            if local_id == 0 && !(server_id == 0 && block_name == "air") {
                unknown_names += 1;
            }
        }

        if unknown_names > 0 {
            warn!(
                "Server announced {} unknown block name(s); unknown IDs will map to air.",
                unknown_names
            );
        }

        self.ready = true;
    }

    /// Runs the `to_local` routine for to local in the `client` module.
    fn to_local(&self, server_id: u16) -> u16 {
        if self.server_to_local.is_empty() {
            return server_id;
        }
        self.server_to_local
            .get(server_id as usize)
            .copied()
            .unwrap_or(0)
    }

    /// Runs the `to_server` routine for to server in the `client` module.
    fn to_server(&self, local_id: u16) -> u16 {
        if self.local_to_server.is_empty() {
            return local_id;
        }
        self.local_to_server
            .get(local_id as usize)
            .copied()
            .unwrap_or(0)
    }

    /// Checks whether ready in the `client` module.
    fn is_ready(&self) -> bool {
        self.ready
    }
}

const MULTIPLAYER_DROP_ITEM_SIZE: f32 = 0.32;
const MULTIPLAYER_DROP_PICKUP_RADIUS: f32 = 1.35;
const MULTIPLAYER_DROP_ATTRACT_RADIUS: f32 = 3.5;
const MULTIPLAYER_DROP_ATTRACT_ACCEL: f32 = 34.0;
const MULTIPLAYER_DROP_ATTRACT_MAX_SPEED: f32 = 12.0;
const MULTIPLAYER_DROP_GRAVITY: f32 = 12.0;
const MULTIPLAYER_DROP_POP_MIN_DIST: f32 = 0.1;
const MULTIPLAYER_DROP_POP_MAX_DIST: f32 = 1.0;
const MULTIPLAYER_DROP_PICKUP_DELAY_SECS: f32 = 0.5;
const REMOTE_PLAYER_INTERP_BACK_TIME_SECS: f32 = 0.10;
const REMOTE_PLAYER_MAX_EXTRAPOLATION_SECS: f32 = 0.08;
const REMOTE_PLAYER_MAX_SNAPSHOT_POINTS: usize = 24;
const REMOTE_PLAYER_SMOOTHING_HZ: f32 = 18.0;
const NETWORK_CONFIG_PATH: &str = "config/network.toml";
const CTRL_C_GRACEFUL_EXIT_DELAY_SECS: f64 = 0.25;
const SERVER_TIMEOUT_ERROR_TEXT: &str = "Server time out!";

static TERMINAL_INTERRUPT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Represents client identity used by the `client` module.
#[derive(Clone, Debug)]
struct ClientIdentity {
    uuid: String,
    player_name: String,
    prod_mode: bool,
    multi_instance: bool,
}

/// Runs the `resolve_client_identity` routine for resolve client identity in the `client` module.
fn resolve_client_identity(settings: &mut NetworkSettings) -> ClientIdentity {
    let multi_instance = is_multi_instance_enabled();
    let prod_mode = settings.client.prod;
    let persistent_identity_enabled = prod_mode && !multi_instance;
    let configured_uuid = settings
        .client
        .client_uuid
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let uuid = if let Some(configured_uuid) = configured_uuid {
        configured_uuid
    } else if persistent_identity_enabled {
        if let Some(existing_uuid) = settings
            .client
            .client_uuid
            .as_deref()
            .filter(|value| parse_uuid_bytes(value).is_some())
        {
            existing_uuid.to_string()
        } else {
            let new_uuid = generate_uuid_v4_string();
            settings.client.client_uuid = Some(new_uuid.clone());
            if let Err(error) = settings.save(NETWORK_CONFIG_PATH) {
                warn!(
                    "Failed to persist client UUID to {}: {}",
                    NETWORK_CONFIG_PATH, error
                );
            }
            new_uuid
        }
    } else {
        generate_uuid_v4_string()
    };

    let player_name = resolve_player_name(settings.client.player_name.as_str());

    ClientIdentity {
        uuid,
        player_name,
        prod_mode,
        multi_instance,
    }
}

/// Checks whether multi instance enabled in the `client` module.
fn is_multi_instance_enabled() -> bool {
    env::var_os("multi_instance").is_some() || env::var_os("MULTI_INSTANCE").is_some()
}

/// Generates uuid v4 string for the `client` module.
fn generate_uuid_v4_string() -> String {
    let mut bytes = rand::random::<[u8; 16]>();
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    let mut uuid = String::with_capacity(36);
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 4 | 6 | 8 | 10) {
            uuid.push('-');
        }
        let _ = write!(&mut uuid, "{:02x}", byte);
    }

    uuid
}

/// Parses uuid bytes for the `client` module.
fn parse_uuid_bytes(value: &str) -> Option<[u8; 16]> {
    if value.len() != 36 {
        return None;
    }

    let bytes = value.as_bytes();
    for &separator_index in &[8usize, 13, 18, 23] {
        if bytes.get(separator_index) != Some(&b'-') {
            return None;
        }
    }

    let mut compact = String::with_capacity(32);
    for (index, ch) in value.chars().enumerate() {
        if matches!(index, 8 | 13 | 18 | 23) {
            continue;
        }
        if !ch.is_ascii_hexdigit() {
            return None;
        }
        compact.push(ch);
    }

    if compact.len() != 32 {
        return None;
    }

    let mut decoded = [0u8; 16];
    for (index, slot) in decoded.iter_mut().enumerate() {
        let start = index * 2;
        *slot = u8::from_str_radix(&compact[start..start + 2], 16).ok()?;
    }

    Some(decoded)
}

/// Runs the `resolve_player_name` routine for resolve player name in the `client` module.
fn resolve_player_name(configured_name: &str) -> String {
    let trimmed = configured_name.trim();
    if trimmed.is_empty() || trimmed == "?" {
        generate_random_player_name()
    } else {
        trimmed.to_string()
    }
}

/// Generates random player name for the `client` module.
fn generate_random_player_name() -> String {
    format!("{:08x}", rand::random::<u32>())
}

/// Parses a session URL like "http://127.0.0.1:14191" into a SocketAddr.
fn parse_session_url(url: &str) -> Option<SocketAddr> {
    let stripped = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    stripped.parse().ok()
}

/// Runs the `remap_server_block_id` routine for remap server block id in the `client` module.
fn remap_server_block_id(
    block_remap: &BlockIdRemap,
    registry: &BlockRegistry,
    server_id: u16,
) -> u16 {
    let local_id = block_remap.to_local(server_id);
    if (local_id as usize) < registry.defs.len() {
        local_id
    } else {
        0
    }
}

/// Represents multiplayer client runtime used by the `client` module.
#[derive(Resource)]
struct MultiplayerClientRuntime {
    enabled: bool,
    player_name: String,
    session_url: String,
    client_uuid: String,
    auto_connect_lan: bool,
    connection_entity: Option<Entity>,
    local_player_id: Option<u64>,
    player_names: HashMap<u64, String>,
    remote_players: HashMap<u64, Entity>,
    disconnected_remote_players: HashSet<u64>,
    remote_player_smoothing: HashMap<u64, RemotePlayerSmoothing>,
    next_local_drop_seq: u32,
    disconnect_requested: bool,
    keepalive_timer: Timer,
    send_timer: Timer,
}

/// Represents terminal interrupt exit state used by the `client` module.
#[derive(Resource, Default)]
struct TerminalInterruptExitState {
    started_at: Option<f64>,
}

impl MultiplayerClientRuntime {
    /// Creates a new instance for the `client` module.
    fn new(settings: &NetworkSettings, identity: ClientIdentity) -> Self {
        let auto_connect_lan = settings.client.session_url.eq_ignore_ascii_case("lan:auto");
        info!(
            "Loaded client identity (prod={}, multi_instance={}): uuid={}, player_name={}",
            identity.prod_mode, identity.multi_instance, identity.uuid, identity.player_name
        );
        Self {
            enabled: settings.client.enabled,
            player_name: identity.player_name,
            session_url: settings.client.session_url.clone(),
            client_uuid: identity.uuid,
            auto_connect_lan,
            connection_entity: None,
            local_player_id: None,
            player_names: HashMap::new(),
            remote_players: HashMap::new(),
            disconnected_remote_players: HashSet::new(),
            remote_player_smoothing: HashMap::new(),
            next_local_drop_seq: 1,
            disconnect_requested: false,
            keepalive_timer: Timer::from_seconds(2.0, TimerMode::Repeating),
            send_timer: Timer::from_seconds(
                Duration::from_millis(settings.client.transform_send_interval_ms).as_secs_f32(),
                TimerMode::Repeating,
            ),
        }
    }
}

/// Runs the `do_connect` routine for do connect in the `client` module.
fn do_connect(
    runtime: &mut MultiplayerClientRuntime,
    session_url: String,
    commands: &mut Commands,
) {
    if !runtime.enabled {
        return;
    }

    let Some(server_addr) = parse_session_url(&session_url) else {
        warn!("Cannot parse session URL: {}", session_url);
        return;
    };

    // Keep netcode client IDs ephemeral so duplicate UUID handling happens in
    // our auth layer (with explicit error dialog) instead of netcode timeout.
    let auth = Authentication::Manual {
        server_addr,
        client_id: rand::random::<u64>().max(1),
        private_key: [0u8; 32],
        protocol_id: 0,
    };

    let netcode_client = match NetcodeClient::new(auth, NetcodeConfig::default()) {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to create netcode client: {:?}", e);
            return;
        }
    };

    let entity = commands
        .spawn((
            netcode_client,
            PeerAddr(server_addr),
            WebSocketClientIo::from_addr(
                ClientConfig::builder().with_no_encryption(),
                WebSocketScheme::Plain,
            ),
        ))
        .id();

    commands.trigger_with(Connect { entity }, EntityTrigger);

    info!(
        "Connecting to multiplayer server at {} with player_name={} and client UUID {}",
        session_url, runtime.player_name, runtime.client_uuid
    );

    runtime.connection_entity = Some(entity);
    runtime.session_url = session_url;
    runtime.local_player_id = None;
    runtime.player_names.clear();
    runtime.remote_players.clear();
    runtime.disconnected_remote_players.clear();
    runtime.remote_player_smoothing.clear();
    runtime.next_local_drop_seq = 1;
    runtime.disconnect_requested = false;
    runtime.keepalive_timer.reset();
}

/// Runs the `do_disconnect` routine for do disconnect in the `client` module.
fn do_disconnect(runtime: &mut MultiplayerClientRuntime, commands: &mut Commands) {
    if let Some(entity) = runtime.connection_entity.take() {
        runtime.disconnect_requested = true;
        commands.trigger_with(Disconnect { entity }, EntityTrigger);
    }
}

/// Represents lan discovery runtime used by the `client` module.
struct LanDiscoveryRuntime {
    client: Option<LanDiscoveryClient>,
    known_servers: Vec<LanServerInfo>,
    refresh_timer: Timer,
}

impl LanDiscoveryRuntime {
    /// Creates a new instance for the `client` module.
    fn new(settings: &NetworkSettings) -> Self {
        let client = if settings.client.lan_discovery {
            LanDiscoveryClient::bind(settings.client.lan_discovery_port).ok()
        } else {
            None
        };

        Self {
            client,
            known_servers: Vec::new(),
            refresh_timer: Timer::from_seconds(3.0, TimerMode::Repeating),
        }
    }
}

/// Represents local lan host used by the `client` module.
struct LocalLanHost {
    child: Option<Child>,
    session_url: Option<String>,
    connect_timer: Option<Timer>,
}

impl Default for LocalLanHost {
    /// Runs the `default` routine for default in the `client` module.
    fn default() -> Self {
        Self {
            child: None,
            session_url: None,
            connect_timer: None,
        }
    }
}

impl LocalLanHost {
    /// Refreshes the requested data for the `client` module.
    fn refresh(&mut self) {
        let Some(child) = self.child.as_mut() else {
            return;
        };

        match child.try_wait() {
            Ok(Some(_)) => {
                self.child = None;
                self.session_url = None;
                self.connect_timer = None;
            }
            Ok(None) => {}
            Err(error) => {
                warn!("Failed to poll LAN host process: {}", error);
                self.child = None;
                self.session_url = None;
                self.connect_timer = None;
            }
        }
    }

    /// Runs the `stop` routine for stop in the `client` module.
    fn stop(&mut self) {
        self.refresh();
        let Some(mut child) = self.child.take() else {
            self.session_url = None;
            self.connect_timer = None;
            return;
        };

        if let Err(error) = child.kill() {
            warn!("Failed to stop LAN host process: {}", error);
        }
        let _ = child.wait();
        self.session_url = None;
        self.connect_timer = None;
    }
}

/// Runs the `lan_server_binary_path` routine for lan server binary path in the `client` module.
fn lan_server_binary_path() -> Option<PathBuf> {
    let current = env::current_exe().ok()?;
    let exe_name = format!("oplexa-game-server{}", env::consts::EXE_SUFFIX);
    let direct = current.with_file_name(&exe_name);
    if direct.exists() {
        return Some(direct);
    }

    let parent = current.parent()?;
    let sibling = parent.join(&exe_name);
    if sibling.exists() {
        return Some(sibling);
    }

    let workspace_debug = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("debug")
        .join(&exe_name);
    if workspace_debug.exists() {
        return Some(workspace_debug);
    }

    None
}

/// Spawns lan host process for the `client` module.
fn spawn_lan_host_process() -> std::io::Result<Child> {
    let Some(binary) = lan_server_binary_path() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "oplexa-game-server binary not found",
        ));
    };

    Command::new(binary)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

/// Starts streamed multiplayer world load for the `client` module.
fn start_streamed_multiplayer_world_load(
    spawn_translation: [f32; 3],
    region_cache: &mut RegionCache,
    chunk_map: &mut ChunkMap,
    fluid_map: &mut FluidMap,
    water_mesh_index: &mut WaterMeshIndex,
    commands: &mut Commands,
    next_state: &mut NextState<AppState>,
) {
    let (spawn_chunk, _) = world_to_chunk_xz(
        spawn_translation[0].floor() as i32,
        spawn_translation[2].floor() as i32,
    );
    region_cache.0.clear();
    chunk_map.chunks.clear();
    fluid_map.0.clear();
    water_mesh_index.0.clear();
    commands.insert_resource(LoadCenter {
        world_xz: spawn_chunk,
    });
    next_state.set(AppState::Loading(LoadingStates::BaseGen));
}

/// Runs the main routine for the `client` module.
pub fn run() {
    GlobalConfig::ensure_config_files_exist();
    dotenv().ok();
    install_terminal_interrupt_handler();
    let graphics_config = GlobalConfig::new();
    let mut multiplayer_settings = NetworkSettings::load_or_create(NETWORK_CONFIG_PATH);
    let client_identity = resolve_client_identity(&mut multiplayer_settings);
    let mut app = App::new();
    init_bevy_app(
        &mut app,
        &graphics_config,
        multiplayer_settings,
        client_identity,
    );
}

/// Runs the `install_terminal_interrupt_handler` routine for install terminal interrupt handler in the `client` module.
fn install_terminal_interrupt_handler() {
    static INSTALL_ONCE: Once = Once::new();
    INSTALL_ONCE.call_once(|| {
        if let Err(error) = ctrlc::set_handler(|| {
            TERMINAL_INTERRUPT_REQUESTED.store(true, Ordering::SeqCst);
        }) {
            warn!("Failed to install terminal interrupt handler: {}", error);
        }
    });
}

/// Initializes bevy app for the `client` module.
fn init_bevy_app(
    app: &mut App,
    config: &GlobalConfig,
    multiplayer_settings: NetworkSettings,
    client_identity: ClientIdentity,
) {
    let client_uuid = client_identity.uuid.clone();
    let build = BuildInfo {
        app_name: "Game Version",
        app_version: env!("CARGO_PKG_VERSION"),
        bevy_version: "0.18.1",
    };

    app.insert_resource(config.clone())
        .insert_resource(build)
        .insert_resource(ClearColor(Color::Srgba(Srgba::rgb_u8(20, 25, 27))))
        .insert_resource(WorldInspectorState(false))
        .insert_resource(MultiplayerConnectionState::with_client_uuid(client_uuid))
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: String::from("Gear Born"),
                        mode: if config.graphics.fullscreen {
                            WindowMode::BorderlessFullscreen(MonitorSelection::Primary)
                        } else {
                            WindowMode::Windowed
                        },
                        resolution: WindowResolution::new(
                            config.graphics.window_width,
                            config.graphics.window_height,
                        ),
                        present_mode: PresentMode::AutoVsync,
                        ..default()
                    }),
                    ..default()
                })
                .set(RenderPlugin {
                    render_creation: RenderCreation::Automatic(create_gpu_settings(
                        &config.graphics.graphic_backend,
                    )),
                    ..default()
                })
                .set(ImagePlugin {
                    default_sampler: ImageSamplerDescriptor {
                        address_mode_u: ImageAddressMode::Repeat,
                        address_mode_v: ImageAddressMode::Repeat,
                        address_mode_w: ImageAddressMode::Repeat,
                        mag_filter: ImageFilterMode::Linear,
                        min_filter: ImageFilterMode::Linear,
                        mipmap_filter: ImageFilterMode::Linear,
                        anisotropy_clamp: 16,
                        ..default()
                    },
                    ..default()
                })
                .set(LogPlugin {
                    level: Level::DEBUG,
                    filter: load_log_env_filter(),
                    custom_layer: log_file_appender,
                    ..default()
                }),
        );

    register_world_inspector_types(app);

    app.add_plugins(ClientPlugins {
        tick_duration: Duration::from_millis(50),
    })
    .add_plugins(ProtocolPlugin);

    app.insert_resource(MultiplayerClientRuntime::new(
        &multiplayer_settings,
        client_identity,
    ))
    .insert_non_send_resource(LanDiscoveryRuntime::new(&multiplayer_settings))
    .insert_non_send_resource(LocalLanHost::default())
    .init_resource::<RemoteChunkStreamState>()
    .init_state::<AppState>()
    .add_plugins(EguiPlugin::default())
    .add_plugins(WorldInspectorPlugin::default().run_if(check_world_inspector_state))
    .add_plugins(ManagerPlugin)
    .add_plugins(MultiplayerClientPlugin)
    .add_systems(
        Update,
        init_app_finish.run_if(in_state(AppState::AppInit).and(resource_exists::<GlobalConfig>)),
    )
    .run();
}

/// Registers world inspector types for the `client` module.
fn register_world_inspector_types(app: &mut App) {
    app.register_type::<GizmoConfigStore>()
        .register_type::<bevy::render::view::ColorGradingSection>()
        .register_type::<bevy::render::view::ColorGradingGlobal>()
        .register_type::<AmbientLight>()
        .register_type::<PointLight>()
        .register_type::<DirectionalLight>()
        .register_type::<bevy::camera::Camera3dDepthLoadOp>();
}

/// Initializes app finish for the `client` module.
fn init_app_finish(mut next_state: ResMut<NextState<AppState>>) {
    info!("Finish initializing app...");
    next_state.set(AppState::Preload);
}

/// Creates gpu settings for the `client` module.
fn create_gpu_settings(backend_str: &str) -> WgpuSettings {
    let backend = match backend_str {
        "auto" | "AUTO" | "primary" | "PRIMARY" => Some(Backends::PRIMARY),
        "vulkan" | "VULKAN" => Some(Backends::VULKAN),
        "dx12" | "DX12" => Some(Backends::DX12),
        "metal" | "METAL" => Some(Backends::METAL),
        other => {
            eprintln!("Unknown backend '{}', falling back to PRIMARY", other);
            Some(Backends::PRIMARY)
        }
    };

    WgpuSettings {
        features: if cfg!(debug_assertions) {
            WgpuFeatures::POLYGON_MODE_LINE
        } else {
            WgpuFeatures::empty()
        },
        backends: backend,
        ..default()
    }
}

/// Runs the `check_world_inspector_state` routine for check world inspector state in the `client` module.
fn check_world_inspector_state(world_inspector_state: Res<WorldInspectorState>) -> bool {
    world_inspector_state.0
}

/// Runs the `log_file_appender` routine for log file appender in the `client` module.
fn log_file_appender(_app: &mut App) -> Option<BoxedLayer> {
    let log_dir = PathBuf::from("logs");
    if let Err(error) = std::fs::create_dir_all(&log_dir) {
        eprintln!("Failed to create log directory: {}", error);
        return None;
    }

    let timestamp = Utc::now().format("bevy-%d-%m-%Y.log").to_string();
    let log_path = log_dir.join(timestamp);
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .ok()?;
    let file_arc = std::sync::Arc::new(std::sync::Mutex::new(file));

    let _shutdown_logger = StartLogText {
        file: std::sync::Arc::clone(&file_arc),
    };

    let writer = BoxMakeWriter::new(move || {
        let file = file_arc
            .lock()
            .unwrap()
            .try_clone()
            .expect("Failed to clone log file handle");
        Box::new(file) as Box<dyn Write + Send>
    });

    Some(Box::new(
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(writer)
            .boxed(),
    ))
}

/// Loads log env filter for the `client` module.
fn load_log_env_filter() -> String {
    dotenv().ok();
    env::var("LOG_ENV_FILTER").unwrap_or_else(|_| "error".to_string())
}

/// Represents start log text used by the `client` module.
struct StartLogText {
    file: std::sync::Arc<std::sync::Mutex<File>>,
}

impl Drop for StartLogText {
    /// Runs the `drop` routine for drop in the `client` module.
    fn drop(&mut self) {
        let mut file = self.file.lock().unwrap();
        let _ = writeln!(
            file,
            "\n====================================== [ Start ] ======================================\n"
        );
        let _ = file.flush();
    }
}

pub(crate) mod manager {
    use crate::core::CoreModule;
    use crate::core::config::GlobalConfig;
    use crate::core::debug::{ChunkGridGizmos, DebugGridState, WorldInspectorState};
    use crate::core::entities::player::Player;
    use crate::core::world::chunk_dimension::{CX, CZ, Y_MAX, Y_MIN, world_to_chunk_xz};
    use crate::generator::GeneratorModule;
    use crate::graphic::GraphicModule;
    use crate::logic::LogicModule;
    use crate::utils::key_utils::convert;
    use bevy::camera::visibility::RenderLayers;
    use bevy::gizmos::config::{DefaultGizmoConfigGroup, GizmoConfigGroup};
    use bevy::light::DirectionalLightShadowMap;
    use bevy::prelude::*;
    use bevy_rapier3d::prelude::*;

    /// Represents manager plugin used by the `client` module.
    pub struct ManagerPlugin;

    impl Plugin for ManagerPlugin {
        /// Builds this component for the `client` module.
        fn build(&self, app: &mut App) {
            app.init_gizmo_group::<ChunkGridGizmos>();
            app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default());
            app.add_plugins((CoreModule, LogicModule, GeneratorModule, GraphicModule));
            app.add_systems(
                Startup,
                (
                    setup_shadow_map,
                    configure_default_gizmos,
                    configure_chunk_grid_gizmos,
                ),
            );
            app.add_systems(
                Update,
                (
                    toggle_world_inspector,
                    toggle_chunk_grid,
                    draw_chunk_grid_gizmo,
                ),
            );
        }
    }

    /// Runs the `setup_shadow_map` routine for setup shadow map in the `client` module.
    fn setup_shadow_map(mut commands: Commands) {
        commands.insert_resource(DirectionalLightShadowMap { size: 1024 });
    }

    /// Runs the `configure_default_gizmos` routine for configure default gizmos in the `client` module.
    fn configure_default_gizmos(mut gizmo_config_store: ResMut<GizmoConfigStore>) {
        let (config, _) = gizmo_config_store.config_mut::<DefaultGizmoConfigGroup>();
        config.enabled = true;
        config.line.width = 3.0;
        config.depth_bias = -1.0;
        config.render_layers = RenderLayers::from_layers(&[0, 1, 2]);
    }

    /// Runs the `configure_chunk_grid_gizmos` routine for configure chunk grid gizmos in the `client` module.
    fn configure_chunk_grid_gizmos(mut gizmo_config_store: ResMut<GizmoConfigStore>) {
        let (config, _) = gizmo_config_store.config_mut::<ChunkGridGizmos>();
        config.enabled = true;
        config.line.width = 3.0;
        config.depth_bias = -1.0;
        config.render_layers = RenderLayers::from_layers(&[0, 1, 2]);
    }

    /// Runs the `toggle_world_inspector` routine for toggle world inspector in the `client` module.
    fn toggle_world_inspector(
        mut debug_context: ResMut<WorldInspectorState>,
        keyboard: Res<ButtonInput<KeyCode>>,
        game_config: Res<GlobalConfig>,
    ) {
        let key = convert(game_config.input.world_inspector.as_str())
            .expect("Invalid key for world inspector");
        if keyboard.just_pressed(key) {
            debug_context.0 = !debug_context.0;
            info!("World Inspector: {}", debug_context.0);
        }
    }

    /// Runs the `toggle_chunk_grid` routine for toggle chunk grid in the `client` module.
    fn toggle_chunk_grid(
        mut debug_grid: ResMut<DebugGridState>,
        keyboard: Res<ButtonInput<KeyCode>>,
        game_config: Res<GlobalConfig>,
        player_query: Query<&Transform, With<Player>>,
    ) {
        let key =
            convert(game_config.input.chunk_grid.as_str()).expect("Invalid key for chunk grid");

        if keyboard.just_pressed(key) {
            debug_grid.show = !debug_grid.show;
            if let Ok(player_transform) = player_query.single() {
                debug_grid.plane_y = player_transform.translation.y.floor();
            }
            info!("Chunk Grid: {}", debug_grid.show);
        }

        if debug_grid.show
            && let Ok(player_transform) = player_query.single()
        {
            debug_grid.plane_y = player_transform.translation.y.floor();
        }
    }

    /// Draws chunk grid gizmo for the `client` module.
    fn draw_chunk_grid_gizmo(
        debug_grid: Res<DebugGridState>,
        player_query: Query<&Transform, With<Player>>,
        mut gizmos: Gizmos<ChunkGridGizmos>,
        mut fallback_gizmos: Gizmos,
    ) {
        if !debug_grid.show {
            return;
        }

        let Ok(player_transform) = player_query.single() else {
            return;
        };

        let world_x = player_transform.translation.x.floor() as i32;
        let world_z = player_transform.translation.z.floor() as i32;
        let (center_chunk, _) = world_to_chunk_xz(world_x, world_z);
        let plane_y = debug_grid.plane_y + 0.05;
        let y_min = Y_MIN as f32;
        let y_max = (Y_MAX + 1) as f32;
        let range = 2;

        for dz in -range..=range {
            for dx in -range..=range {
                let chunk = center_chunk + IVec2::new(dx, dz);
                draw_chunk_outline(&mut gizmos, chunk, plane_y, y_min, y_max);
                draw_chunk_outline(&mut fallback_gizmos, chunk, plane_y, y_min, y_max);
            }
        }
    }

    /// Draws chunk outline for the `client` module.
    fn draw_chunk_outline<G: GizmoConfigGroup>(
        gizmos: &mut Gizmos<G>,
        chunk: IVec2,
        plane_y: f32,
        y_min: f32,
        y_max: f32,
    ) {
        let x0 = chunk.x as f32 * CX as f32;
        let z0 = chunk.y as f32 * CZ as f32;
        let x1 = x0 + CX as f32;
        let z1 = z0 + CZ as f32;
        let border_color = Color::srgb(0.98, 0.88, 0.16);
        let corner_color = Color::srgb(0.96, 0.54, 0.12);

        gizmos.line(
            Vec3::new(x0, plane_y, z0),
            Vec3::new(x1, plane_y, z0),
            border_color,
        );
        gizmos.line(
            Vec3::new(x1, plane_y, z0),
            Vec3::new(x1, plane_y, z1),
            border_color,
        );
        gizmos.line(
            Vec3::new(x1, plane_y, z1),
            Vec3::new(x0, plane_y, z1),
            border_color,
        );
        gizmos.line(
            Vec3::new(x0, plane_y, z1),
            Vec3::new(x0, plane_y, z0),
            border_color,
        );

        gizmos.line(
            Vec3::new(x0, y_min, z0),
            Vec3::new(x0, y_max, z0),
            corner_color,
        );
        gizmos.line(
            Vec3::new(x1, y_min, z0),
            Vec3::new(x1, y_max, z0),
            corner_color,
        );
        gizmos.line(
            Vec3::new(x1, y_min, z1),
            Vec3::new(x1, y_max, z1),
            corner_color,
        );
        gizmos.line(
            Vec3::new(x0, y_min, z1),
            Vec3::new(x0, y_max, z1),
            corner_color,
        );
    }
}

/// Represents multiplayer client plugin used by the `client` module.
struct MultiplayerClientPlugin;

impl Plugin for MultiplayerClientPlugin {
    /// Builds this component for the `client` module.
    fn build(&self, app: &mut App) {
        app.init_resource::<MultiplayerDropIndex>()
            .init_resource::<RemoteChunkStreamState>()
            .init_resource::<BlockIdRemap>()
            .init_resource::<TerminalInterruptExitState>()
            .add_systems(Startup, setup_remote_player_visuals)
            .add_observer(on_server_connected)
            .add_observer(on_server_disconnected)
            .add_systems(Update, handle_terminal_interrupt_exit)
            .add_systems(
                Update,
                (
                    poll_lan_servers,
                    connect_to_server_requested,
                    disconnect_from_server_requested,
                    open_to_lan_requested,
                    finish_open_to_lan_connect,
                    stop_lan_host_requested,
                    handle_chat_submit_requests,
                    send_local_block_break_events,
                    send_local_block_place_events,
                    send_local_item_drop_requests,
                    send_client_keepalive,
                    receive_player_messages,
                    receive_chat_messages,
                    receive_world_messages,
                    receive_drop_messages,
                    update_connection_state,
                    simulate_multiplayer_drop_items,
                    send_local_drop_pickup_requests,
                    send_chunk_interest_updates,
                    send_local_player_pose,
                ),
            )
            .add_systems(Update, smooth_remote_players);
    }
}

/// Handles terminal interrupt exit for the `client` module.
fn handle_terminal_interrupt_exit(
    time: Res<Time>,
    mut interrupt_state: ResMut<TerminalInterruptExitState>,
    mut runtime: ResMut<MultiplayerClientRuntime>,
    mut block_remap: ResMut<BlockIdRemap>,
    mut multiplayer_connection: ResMut<MultiplayerConnectionState>,
    mut chunk_stream: ResMut<RemoteChunkStreamState>,
    mut commands: Commands,
    mut app_exit: MessageWriter<AppExit>,
) {
    if TERMINAL_INTERRUPT_REQUESTED.load(Ordering::SeqCst) && interrupt_state.started_at.is_none() {
        info!("Ctrl+C detected. Sending disconnect before shutdown...");
        do_disconnect(&mut runtime, &mut commands);
        runtime.local_player_id = None;
        runtime.player_names.clear();
        runtime.remote_players.clear();
        runtime.disconnected_remote_players.clear();
        runtime.remote_player_smoothing.clear();
        block_remap.reset();
        chunk_stream.last_requested_center = None;
        chunk_stream.last_requested_radius = None;
        multiplayer_connection.clear_session();
        interrupt_state.started_at = Some(time.elapsed_secs_f64());
        return;
    }

    let Some(started_at) = interrupt_state.started_at else {
        return;
    };

    if time.elapsed_secs_f64() - started_at >= CTRL_C_GRACEFUL_EXIT_DELAY_SECS {
        app_exit.write(AppExit::Success);
    }
}

/// Runs the `setup_remote_player_visuals` routine for setup remote player visuals in the `client` module.
fn setup_remote_player_visuals(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(RemotePlayerVisuals {
        mesh: meshes.add(Mesh::from(Capsule3d::new(0.35, 1.2))),
        material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.85, 0.35, 0.25),
            perceptual_roughness: 0.9,
            ..default()
        }),
    });
}

/// Observer: fires when the netcode handshake completes and `Connected` is added to our entity.
fn on_server_connected(
    trigger: On<Add, Connected>,
    runtime: Res<MultiplayerClientRuntime>,
    mut q_auth: Query<&mut MessageSender<Auth>>,
    mut multiplayer_connection: ResMut<MultiplayerConnectionState>,
) {
    if Some(trigger.entity) != runtime.connection_entity {
        return;
    }

    multiplayer_connection.phase = MultiplayerConnectionPhase::Connecting;
    multiplayer_connection.last_error = None;

    if let Ok(mut sender) = q_auth.get_mut(trigger.entity) {
        sender.send::<UnorderedReliable>(Auth::new(
            runtime.player_name.clone(),
            runtime.client_uuid.clone(),
        ));
        info!(
            "Connected to server, sent Auth as '{}' with UUID {}",
            runtime.player_name, runtime.client_uuid
        );
    }
}

/// Observer: fires when the connection drops and `Disconnected` is added to our entity.
fn on_server_disconnected(
    trigger: On<Add, Disconnected>,
    q_disconnected: Query<&Disconnected>,
    app_state: Res<State<AppState>>,
    mut runtime: ResMut<MultiplayerClientRuntime>,
    mut block_remap: ResMut<BlockIdRemap>,
    mut multiplayer_connection: ResMut<MultiplayerConnectionState>,
    mut chunk_stream: ResMut<RemoteChunkStreamState>,
    mut chat_log: ResMut<ChatLog>,
    mut drops: ResMut<MultiplayerDropIndex>,
    mut next_state: ResMut<NextState<AppState>>,
    mut chunk_map: ResMut<ChunkMap>,
    mut commands: Commands,
) {
    if Some(trigger.entity) != runtime.connection_entity {
        return;
    }

    // NetcodeClient has #[require(Disconnected)], so Disconnected is added on spawn
    // with reason: None. Real disconnects always have reason: Some(...). Skip the
    // initial spawn-time Disconnected so we don't immediately despawn the entity.
    if let Ok(disconnected) = q_disconnected.get(trigger.entity) {
        if disconnected.reason.is_none() {
            return;
        }
    }

    for entity in runtime.remote_players.drain().map(|(_, e)| e) {
        safe_despawn_entity(&mut commands, entity);
    }
    chat_log.clear();
    runtime.remote_player_smoothing.clear();
    clear_multiplayer_drops(&mut commands, &mut drops);
    runtime.local_player_id = None;
    runtime.player_names.clear();
    runtime.disconnected_remote_players.clear();
    block_remap.reset();
    chunk_stream.last_requested_center = None;
    chunk_stream.last_requested_radius = None;

    // If we disconnect while the world is loading or in-game, reset to the menu.
    // Without this, check_base_gen_world_ready would see uses_local_save_data()=true
    // (session URL cleared below) and send the client to WaterGen/local generation,
    // which crashes because no local world resources are set up.
    match app_state.get() {
        AppState::Loading(_) | AppState::InGame(_) | AppState::PostLoad => {
            chunk_map.chunks.clear();
            next_state.set(AppState::Screen(BeforeUiState::MultiPlayer));
        }
        _ => {}
    }

    let existing_error = multiplayer_connection.last_error.clone();
    let disconnected_by_request = runtime.disconnect_requested;
    runtime.disconnect_requested = false;
    multiplayer_connection.clear_session();
    multiplayer_connection.last_error = if disconnected_by_request {
        existing_error
    } else {
        existing_error.or_else(|| Some(SERVER_TIMEOUT_ERROR_TEXT.to_string()))
    };

    runtime.connection_entity = None;
}

/// Runs the `poll_lan_servers` routine for poll lan servers in the `client` module.
fn poll_lan_servers(time: Res<Time>, mut discovery: NonSendMut<LanDiscoveryRuntime>) {
    if discovery.client.is_none() {
        return;
    }

    discovery.refresh_timer.tick(time.delta());
    if discovery.refresh_timer.just_finished() {
        if let Err(error) = discovery
            .client
            .as_ref()
            .expect("LAN discovery client vanished")
            .broadcast_query()
        {
            warn!("LAN discovery broadcast failed: {}", error);
        }
    }

    let Ok(found_servers) = discovery
        .client
        .as_ref()
        .expect("LAN discovery client vanished")
        .poll()
    else {
        return;
    };

    for server in found_servers {
        let already_known = discovery
            .known_servers
            .iter()
            .any(|known| known.session_url == server.session_url);

        if !already_known {
            info!(
                "Discovered LAN server '{}' at {}",
                server.server_name, server.session_url
            );
            discovery.known_servers.push(server.clone());
        }
    }
}

/// Runs the `connect_to_server_requested` routine for connect to server requested in the `client` module.
fn connect_to_server_requested(
    mut connect_requests: MessageReader<ConnectToServerRequest>,
    mut multiplayer_connection: ResMut<MultiplayerConnectionState>,
    mut chunk_stream: ResMut<RemoteChunkStreamState>,
    mut block_remap: ResMut<BlockIdRemap>,
    q_active: Query<
        (),
        Or<(
            With<Connected>,
            With<lightyear::prelude::client::Connecting>,
        )>,
    >,
    mut runtime: ResMut<MultiplayerClientRuntime>,
    mut commands: Commands,
) {
    // Drain all pending requests and use only the last one. Processing multiple
    // requests per frame would spawn an entity, immediately despawn it (because
    // the deferred spawn hasn't been applied yet and the q_active check fails),
    // then spawn another — leaving dangling deferred hook commands that target
    // the already-despawned entity, which causes a panic.
    let request = match connect_requests.read().last() {
        Some(r) => r.clone(),
        None => return,
    };

    let session_url = request.session_url.trim();
    if session_url.is_empty() {
        warn!("Connect request ignored because no session URL was provided.");
        return;
    }

    // Don't reconnect if already connected or connecting
    if let Some(entity) = runtime.connection_entity {
        if q_active.get(entity).is_ok() {
            return;
        }
        // Existing disconnected entity – drop the runtime handle. Lightyear may
        // still have deferred cleanup commands for this entity.
        runtime.connection_entity = None;
    }

    block_remap.reset();
    do_connect(&mut runtime, session_url.to_string(), &mut commands);
    runtime.auto_connect_lan = false;
    multiplayer_connection.connected = false;
    multiplayer_connection.phase = MultiplayerConnectionPhase::Connecting;
    multiplayer_connection.set_world_data_mode_remote();
    multiplayer_connection.active_session_url = Some(session_url.to_string());
    multiplayer_connection.server_name = if request.server_name.trim().is_empty() {
        None
    } else {
        Some(request.server_name.trim().to_string())
    };
    multiplayer_connection.world_name = None;
    multiplayer_connection.world_seed = None;
    multiplayer_connection.spawn_translation = None;
    multiplayer_connection.known_player_names.clear();
    multiplayer_connection.last_error = None;
    chunk_stream.last_requested_center = None;
    chunk_stream.last_requested_radius = None;
}

/// Runs the `disconnect_from_server_requested` routine for disconnect from server requested in the `client` module.
fn disconnect_from_server_requested(
    mut disconnect_requests: MessageReader<DisconnectFromServerRequest>,
    mut multiplayer_connection: ResMut<MultiplayerConnectionState>,
    mut chunk_stream: ResMut<RemoteChunkStreamState>,
    mut block_remap: ResMut<BlockIdRemap>,
    mut runtime: ResMut<MultiplayerClientRuntime>,
    mut commands: Commands,
) {
    if disconnect_requests.read().next().is_none() {
        return;
    }

    do_disconnect(&mut runtime, &mut commands);
    block_remap.reset();
    multiplayer_connection.clear_session();
    chunk_stream.last_requested_center = None;
    chunk_stream.last_requested_radius = None;
}

/// Runs the `open_to_lan_requested` routine for open to lan requested in the `client` module.
fn open_to_lan_requested(
    mut requests: MessageReader<OpenToLanRequest>,
    q_active: Query<
        (),
        Or<(
            With<Connected>,
            With<lightyear::prelude::client::Connecting>,
        )>,
    >,
    mut runtime: ResMut<MultiplayerClientRuntime>,
    mut local_host: NonSendMut<LocalLanHost>,
) {
    if requests.read().next().is_none() {
        return;
    }

    local_host.refresh();

    if let Some(entity) = runtime.connection_entity {
        if q_active.get(entity).is_ok() {
            warn!("Open to LAN ignored because the client is already connected.");
            return;
        }
    }

    let settings = DedicatedServerSettings::load_or_create("server.settings.toml");
    let session_url = settings.session_url();

    if local_host.child.is_none() {
        match spawn_lan_host_process() {
            Ok(child) => {
                info!("Started LAN host at {}", session_url);
                local_host.child = Some(child);
            }
            Err(error) => {
                warn!("Failed to start LAN host: {}", error);
                return;
            }
        }
    }

    local_host.session_url = Some(session_url.clone());
    local_host.connect_timer = Some(Timer::from_seconds(0.75, TimerMode::Once));
    runtime.auto_connect_lan = false;
}

/// Runs the `finish_open_to_lan_connect` routine for finish open to lan connect in the `client` module.
fn finish_open_to_lan_connect(
    time: Res<Time>,
    mut runtime: ResMut<MultiplayerClientRuntime>,
    mut local_host: NonSendMut<LocalLanHost>,
    mut multiplayer_connection: ResMut<MultiplayerConnectionState>,
    mut chunk_stream: ResMut<RemoteChunkStreamState>,
    mut block_remap: ResMut<BlockIdRemap>,
    mut commands: Commands,
) {
    local_host.refresh();

    let Some(timer) = local_host.connect_timer.as_mut() else {
        return;
    };
    timer.tick(time.delta());
    if !timer.is_finished() {
        return;
    }

    let Some(session_url) = local_host.session_url.clone() else {
        local_host.connect_timer = None;
        return;
    };

    block_remap.reset();
    do_connect(&mut runtime, session_url.clone(), &mut commands);
    multiplayer_connection.connected = false;
    multiplayer_connection.phase = MultiplayerConnectionPhase::Connecting;
    multiplayer_connection.set_world_data_mode_remote();
    multiplayer_connection.active_session_url = Some(session_url);
    multiplayer_connection.server_name = None;
    multiplayer_connection.world_name = None;
    multiplayer_connection.world_seed = None;
    multiplayer_connection.spawn_translation = None;
    multiplayer_connection.known_player_names.clear();
    multiplayer_connection.last_error = None;
    chunk_stream.last_requested_center = None;
    chunk_stream.last_requested_radius = None;
    local_host.connect_timer = None;
}

/// Runs the `stop_lan_host_requested` routine for stop lan host requested in the `client` module.
fn stop_lan_host_requested(
    mut requests: MessageReader<StopLanHostRequest>,
    mut local_host: NonSendMut<LocalLanHost>,
) {
    if requests.read().next().is_none() {
        return;
    }

    local_host.stop();
}

/// Polls connection state and updates `MultiplayerConnectionState`.
fn update_connection_state(
    runtime: Res<MultiplayerClientRuntime>,
    q_connected: Query<Has<Connected>>,
    mut multiplayer_connection: ResMut<MultiplayerConnectionState>,
) {
    let Some(entity) = runtime.connection_entity else {
        return;
    };

    let is_connected = q_connected.get(entity).unwrap_or(false);
    multiplayer_connection.connected = is_connected && runtime.local_player_id.is_some();
    multiplayer_connection.phase = if multiplayer_connection.connected {
        MultiplayerConnectionPhase::Idle
    } else if is_connected {
        MultiplayerConnectionPhase::Connecting
    } else {
        MultiplayerConnectionPhase::Idle
    };
}

/// Handles ServerWelcome / ServerAuthRejected and player sync messages.
#[allow(clippy::too_many_arguments)]
fn receive_player_messages(
    mut commands: Commands,
    time: Res<Time>,
    app_state: Res<State<AppState>>,
    visuals: Res<RemotePlayerVisuals>,
    registry: Option<Res<BlockRegistry>>,
    mut region_cache: ResMut<RegionCache>,
    mut chunk_map: ResMut<ChunkMap>,
    mut fluids: ResMut<FluidMap>,
    mut water_mesh_index: ResMut<WaterMeshIndex>,
    mut next_state: ResMut<NextState<AppState>>,
    mut multiplayer_connection: ResMut<MultiplayerConnectionState>,
    mut chunk_stream: ResMut<RemoteChunkStreamState>,
    mut block_remap: ResMut<BlockIdRemap>,
    mut runtime: ResMut<MultiplayerClientRuntime>,
    mut q: Query<(
        &mut MessageReceiver<ServerWelcome>,
        &mut MessageReceiver<ServerAuthRejected>,
        &mut MessageReceiver<PlayerJoined>,
        &mut MessageReceiver<PlayerLeft>,
        &mut MessageReceiver<PlayerSnapshot>,
    )>,
) {
    let Some(entity) = runtime.connection_entity else {
        return;
    };

    let Ok((
        mut recv_welcome,
        mut recv_auth_rejected,
        mut recv_joined,
        mut recv_left,
        mut recv_snapshot,
    )) = q.get_mut(entity)
    else {
        return;
    };

    let now = time.elapsed_secs();

    for message in recv_auth_rejected.receive() {
        let reason = if message.reason.trim().is_empty() {
            "Disconnected from multiplayer server.".to_string()
        } else {
            message.reason.clone()
        };
        warn!("Server rejected multiplayer auth: {}", reason);

        runtime.local_player_id = None;
        runtime.player_names.clear();
        runtime.disconnected_remote_players.clear();
        block_remap.reset();
        chunk_stream.last_requested_center = None;
        chunk_stream.last_requested_radius = None;

        if matches!(
            app_state.get(),
            AppState::Loading(_) | AppState::InGame(_) | AppState::PostLoad
        ) {
            chunk_map.chunks.clear();
        }
        next_state.set(AppState::Screen(BeforeUiState::MultiPlayer));

        multiplayer_connection.clear_session();
        multiplayer_connection.last_error = Some(reason);

        if let Some(connection_entity) = runtime.connection_entity {
            commands.trigger_with(
                Disconnect {
                    entity: connection_entity,
                },
                EntityTrigger,
            );
        }
    }

    for message in recv_welcome.receive() {
        runtime.local_player_id = Some(message.player_id);
        let local_player_name = runtime.player_name.clone();
        runtime
            .player_names
            .insert(message.player_id, local_player_name);
        runtime.disconnected_remote_players.clear();
        if let Some(existing) = runtime.remote_players.remove(&message.player_id) {
            safe_despawn_entity(&mut commands, existing);
        }

        if let Some(registry) = registry.as_ref() {
            block_remap.configure_from_server_palette(&message.block_palette, registry);
        } else {
            block_remap.reset();
        }

        info!(
            "Server '{}' accepted player id {}",
            message.server_name, message.player_id
        );
        multiplayer_connection.server_name = Some(message.server_name.clone());
        multiplayer_connection.world_name = Some(message.world_name.clone());
        multiplayer_connection.world_seed = Some(message.world_seed);
        multiplayer_connection.last_error = None;
        let spawn_translation = message.spawn_translation;
        start_streamed_multiplayer_world_load(
            spawn_translation,
            &mut region_cache,
            &mut chunk_map,
            &mut fluids,
            &mut water_mesh_index,
            &mut commands,
            &mut next_state,
        );
        multiplayer_connection.spawn_translation = Some(spawn_translation);
        chunk_stream.last_requested_center = None;
        chunk_stream.last_requested_radius = None;
    }

    for message in recv_joined.receive() {
        runtime
            .player_names
            .insert(message.player_id, message.username.clone());
        if Some(message.player_id) == runtime.local_player_id {
            continue;
        }
        runtime
            .disconnected_remote_players
            .remove(&message.player_id);

        let translation = multiplayer_connection
            .spawn_translation
            .map(Vec3::from_array)
            .unwrap_or(Vec3::new(0.0, 180.0, 0.0));
        ensure_remote_player(
            &mut commands,
            &visuals,
            &mut runtime.remote_players,
            message.player_id,
            translation,
            0.0,
        );

        runtime
            .remote_player_smoothing
            .entry(message.player_id)
            .or_default()
            .reset_snapshot(now, translation, 0.0);
    }

    for message in recv_left.receive() {
        runtime
            .disconnected_remote_players
            .insert(message.player_id);
        runtime.player_names.remove(&message.player_id);
        runtime.remote_player_smoothing.remove(&message.player_id);
        if let Some(ent) = runtime.remote_players.remove(&message.player_id) {
            safe_despawn_entity(&mut commands, ent);
        }
    }

    for message in recv_snapshot.receive() {
        if Some(message.player_id) == runtime.local_player_id {
            continue;
        }
        if runtime
            .disconnected_remote_players
            .contains(&message.player_id)
        {
            continue;
        }

        ensure_remote_player(
            &mut commands,
            &visuals,
            &mut runtime.remote_players,
            message.player_id,
            Vec3::from_array(message.translation),
            message.yaw,
        );

        let translation = Vec3::from_array(message.translation);
        let smoothing = runtime
            .remote_player_smoothing
            .entry(message.player_id)
            .or_insert_with(|| {
                RemotePlayerSmoothing::with_initial_snapshot(now, translation, message.yaw)
            });
        smoothing.push_snapshot(now, translation, message.yaw);
    }

    sync_known_player_names(&runtime, &mut multiplayer_connection);
}

/// Handles server chat lines and game mode synchronization.
fn receive_chat_messages(
    runtime: Res<MultiplayerClientRuntime>,
    mut chat_log: ResMut<ChatLog>,
    mut game_mode: ResMut<GameModeState>,
    mut flight_state: Query<&mut FlightState>,
    mut q: Query<(
        &mut MessageReceiver<ServerChatMessage>,
        &mut MessageReceiver<ServerGameModeChanged>,
    )>,
) {
    let Some(entity) = runtime.connection_entity else {
        return;
    };

    let Ok((mut recv_chat, mut recv_game_mode)) = q.get_mut(entity) else {
        return;
    };

    for message in recv_chat.receive() {
        chat_log.push(ChatLine::new(message.sender, message.message));
    }

    for message in recv_game_mode.receive() {
        if Some(message.player_id) != runtime.local_player_id {
            continue;
        }
        apply_local_game_mode(message.mode, &mut game_mode, &mut flight_state);
    }
}

/// Handles locally submitted chat input and forwards it to multiplayer or local command handling.
fn handle_chat_submit_requests(
    mut submit_requests: MessageReader<ChatSubmitRequest>,
    runtime: Res<MultiplayerClientRuntime>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
    q_connected: Query<Has<Connected>>,
    mut q_sender: Query<&mut MessageSender<ClientChatMessage>>,
    mut chat_log: ResMut<ChatLog>,
    mut game_mode: ResMut<GameModeState>,
    mut flight_state: Query<&mut FlightState>,
) {
    for request in submit_requests.read() {
        let text = request.text.trim();
        if text.is_empty() {
            continue;
        }

        let connected = runtime.connection_entity.is_some_and(|entity| {
            q_connected.get(entity).unwrap_or(false) && multiplayer_connection.connected
        });

        if connected {
            if let Some(entity) = runtime.connection_entity
                && let Ok(mut sender) = q_sender.get_mut(entity)
            {
                sender.send::<OrderedReliable>(ClientChatMessage::new(text.to_string()));
            }
            continue;
        }

        if let Some(command) = parse_chat_command(text) {
            let registry = default_chat_command_registry();
            let Some(descriptor) = registry.find(command.name.as_str()) else {
                push_system_chat(
                    &mut chat_log,
                    SystemMessageLevel::Warn,
                    format!("Unknown command '/{}'. Use /help.", command.name),
                );
                continue;
            };

            match descriptor.name.as_str() {
                "help" => {
                    let names = registry
                        .sorted_descriptors()
                        .into_iter()
                        .map(|entry| format!("/{}", entry.name))
                        .collect::<Vec<_>>()
                        .join(", ");
                    push_system_chat(
                        &mut chat_log,
                        SystemMessageLevel::Info,
                        format!("Available commands: {}", names),
                    );
                }
                "gamemode" => {
                    let Some(raw_mode) = command.args.first() else {
                        push_system_chat(
                            &mut chat_log,
                            SystemMessageLevel::Warn,
                            "Usage: /gamemode <survival|creative|spectator>".to_string(),
                        );
                        continue;
                    };

                    let Some(mode) = GameModeKind::parse(raw_mode) else {
                        push_system_chat(
                            &mut chat_log,
                            SystemMessageLevel::Warn,
                            format!(
                                "Unknown game mode '{}'. Use survival, creative, or spectator.",
                                raw_mode
                            ),
                        );
                        continue;
                    };

                    apply_local_game_mode(mode, &mut game_mode, &mut flight_state);
                    push_system_chat(
                        &mut chat_log,
                        SystemMessageLevel::Info,
                        format!("Game mode set to {}.", mode.as_str()),
                    );
                }
                _ => {
                    push_system_chat(
                        &mut chat_log,
                        SystemMessageLevel::Warn,
                        format!("Command '/{}' is not executable yet.", descriptor.name),
                    );
                }
            }
            continue;
        }

        chat_log.push(ChatLine::new(
            CommandSender::Entity(EntitySender::Player {
                player_id: runtime.local_player_id.unwrap_or(0),
                player_name: runtime.player_name.clone(),
            }),
            text.to_string(),
        ));
    }
}

fn push_system_chat(chat_log: &mut ChatLog, level: SystemMessageLevel, message: String) {
    chat_log.push(ChatLine::new(
        CommandSender::System(SystemSender::Server { level }),
        message,
    ));
}

fn apply_local_game_mode(
    mode: GameModeKind,
    game_mode: &mut ResMut<GameModeState>,
    flight_state: &mut Query<&mut FlightState>,
) {
    game_mode.0 = match mode {
        GameModeKind::Survival => GameMode::Survival,
        GameModeKind::Creative => GameMode::Creative,
        GameModeKind::Spectator => GameMode::Spectator,
    };

    if let Ok(mut state) = flight_state.single_mut() {
        state.flying = matches!(mode, GameModeKind::Creative | GameModeKind::Spectator);
    }
}

fn sync_known_player_names(
    runtime: &MultiplayerClientRuntime,
    multiplayer_connection: &mut MultiplayerConnectionState,
) {
    let mut names = runtime.player_names.values().cloned().collect::<Vec<_>>();
    names.sort_by_key(|name| name.to_ascii_lowercase());
    names.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    multiplayer_connection.known_player_names = names;
}

/// Handles ServerChunkData, ServerBlockBreak, ServerBlockPlace messages.
#[allow(clippy::too_many_arguments)]
fn receive_world_messages(
    registry: Option<Res<BlockRegistry>>,
    block_remap: Res<BlockIdRemap>,
    mut chunk_map: ResMut<ChunkMap>,
    mut fluids: ResMut<FluidMap>,
    mut ev_dirty: MessageWriter<SubChunkNeedRemeshEvent>,
    runtime: Res<MultiplayerClientRuntime>,
    mut q: Query<(
        &mut MessageReceiver<ServerChunkData>,
        &mut MessageReceiver<ServerBlockBreak>,
        &mut MessageReceiver<ServerBlockPlace>,
    )>,
) {
    let Some(entity) = runtime.connection_entity else {
        return;
    };
    if !block_remap.is_ready() {
        return;
    }

    let Ok((mut recv_chunk, mut recv_block_break, mut recv_block_place)) = q.get_mut(entity) else {
        return;
    };

    for message in recv_chunk.receive() {
        let Some(registry) = registry.as_ref() else {
            continue;
        };

        let water_local_id = registry
            .id_opt("water_block")
            .or_else(|| registry.id_opt("water"))
            .unwrap_or(0);
        let coord = IVec2::new(message.coord[0], message.coord[1]);
        let Ok(mut chunk) = crate::generator::chunk::chunk_utils::decode_chunk(&message.blocks)
        else {
            warn!("Failed to decode streamed chunk {},{}", coord.x, coord.y);
            continue;
        };

        let mut fluid_chunk = FluidChunk::new(SEA_LEVEL);
        for y in 0..CY {
            for z in 0..CZ {
                for x in 0..CX {
                    let id = chunk.get(x, y, z);
                    let local_id = remap_server_block_id(&block_remap, registry, id);
                    if water_local_id != 0 && local_id == water_local_id {
                        chunk.set(x, y, z, 0);
                        fluid_chunk.set(x, y, z, true);
                    } else {
                        chunk.set(x, y, z, local_id);
                    }
                }
            }
        }

        debug!(
            "[MP] Received ServerChunkData for chunk {:?} (total: {})",
            coord,
            chunk_map.chunks.len() + 1
        );
        chunk.mark_all_dirty();
        chunk_map.chunks.insert(coord, chunk);
        fluids.0.insert(coord, fluid_chunk);

        for sub in 0..SEC_COUNT {
            ev_dirty.write(SubChunkNeedRemeshEvent { coord, sub });
        }

        for neighbor in [
            IVec2::new(coord.x + 1, coord.y),
            IVec2::new(coord.x - 1, coord.y),
            IVec2::new(coord.x, coord.y + 1),
            IVec2::new(coord.x, coord.y - 1),
        ] {
            if chunk_map.chunks.contains_key(&neighbor) {
                for sub in 0..SEC_COUNT {
                    ev_dirty.write(SubChunkNeedRemeshEvent {
                        coord: neighbor,
                        sub,
                    });
                }
            }
        }
    }

    for message in recv_block_break.receive() {
        if Some(message.player_id) == runtime.local_player_id {
            continue;
        }
        apply_remote_block_break(message.location, &mut chunk_map, &mut fluids, &mut ev_dirty);
    }

    for message in recv_block_place.receive() {
        if Some(message.player_id) == runtime.local_player_id {
            continue;
        }
        let Some(registry) = registry.as_ref() else {
            continue;
        };
        apply_remote_block_place(
            message.location,
            remap_server_block_id(&block_remap, registry, message.block_id),
            registry,
            &mut chunk_map,
            &mut fluids,
            &mut ev_dirty,
        );
    }
}

/// Handles ServerDropSpawn and ServerDropPicked messages.
#[allow(clippy::too_many_arguments)]
fn receive_drop_messages(
    mut commands: Commands,
    time: Res<Time>,
    registry: Option<Res<BlockRegistry>>,
    item_registry: Option<Res<ItemRegistry>>,
    block_remap: Res<BlockIdRemap>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut inventory: ResMut<PlayerInventory>,
    mut drops: ResMut<MultiplayerDropIndex>,
    runtime: Res<MultiplayerClientRuntime>,
    mut q: Query<(
        &mut MessageReceiver<ServerDropSpawn>,
        &mut MessageReceiver<ServerDropPicked>,
    )>,
) {
    let Some(entity) = runtime.connection_entity else {
        return;
    };
    if !block_remap.is_ready() {
        return;
    }

    let Ok((mut recv_spawn, mut recv_picked)) = q.get_mut(entity) else {
        return;
    };

    for message in recv_spawn.receive() {
        if let (Some(registry), Some(item_registry)) = (registry.as_ref(), item_registry.as_ref()) {
            let local_block_id = remap_server_block_id(&block_remap, registry, message.block_id);
            let local_item_id = item_registry.item_for_block(local_block_id).unwrap_or(0);
            spawn_multiplayer_drop(
                &mut commands,
                registry,
                item_registry,
                &mut meshes,
                &mut drops,
                message.drop_id,
                message.location,
                local_item_id,
                message.has_motion,
                message.spawn_translation,
                message.initial_velocity,
                time.elapsed_secs(),
            );
        }
    }

    for message in recv_picked.receive() {
        if let Some(ent) = drops.entities.remove(&message.drop_id) {
            safe_despawn_entity(&mut commands, ent);
        }

        if Some(message.player_id) == runtime.local_player_id {
            let local_item_id = if let (Some(registry), Some(item_registry)) =
                (registry.as_ref(), item_registry.as_ref())
            {
                let local_block_id =
                    remap_server_block_id(&block_remap, registry, message.block_id);
                item_registry.item_for_block(local_block_id).unwrap_or(0)
            } else {
                0
            };

            if local_item_id != 0
                && let Some(item_registry) = item_registry.as_ref()
            {
                let _ = inventory.add_item(local_item_id, 1, item_registry);
            }
        }
    }
}

/// Runs the `send_chunk_interest_updates` routine for send chunk interest updates in the `client` module.
fn send_chunk_interest_updates(
    game_config: Res<GlobalConfig>,
    q_player: Query<&Transform, With<Player>>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
    q_connected: Query<Has<Connected>>,
    mut chunk_stream: ResMut<RemoteChunkStreamState>,
    runtime: Res<MultiplayerClientRuntime>,
    mut q_sender: Query<&mut MessageSender<ClientChunkInterest>>,
) {
    let Some(entity) = runtime.connection_entity else {
        chunk_stream.last_requested_center = None;
        chunk_stream.last_requested_radius = None;
        return;
    };

    if !q_connected.get(entity).unwrap_or(false)
        || multiplayer_connection.active_session_url.is_none()
    {
        return;
    }

    let radius = game_config.graphics.chunk_range.max(1);
    let center = if let Ok(transform) = q_player.single() {
        world_to_chunk_xz(
            (transform.translation.x / VOXEL_SIZE).floor() as i32,
            (transform.translation.z / VOXEL_SIZE).floor() as i32,
        )
        .0
    } else if let Some(spawn_translation) = multiplayer_connection.spawn_translation {
        world_to_chunk_xz(
            spawn_translation[0].floor() as i32,
            spawn_translation[2].floor() as i32,
        )
        .0
    } else {
        return;
    };

    if chunk_stream.last_requested_center == Some(center)
        && chunk_stream.last_requested_radius == Some(radius)
    {
        return;
    }

    if let Ok(mut sender) = q_sender.get_mut(entity) {
        debug!(
            "[MP] Sending ClientChunkInterest: center={:?}, radius={}",
            [center.x, center.y],
            radius
        );
        sender.send::<OrderedReliable>(ClientChunkInterest::new([center.x, center.y], radius));
        chunk_stream.last_requested_center = Some(center);
        chunk_stream.last_requested_radius = Some(radius);
    }
}

/// Runs the `smooth_remote_players` routine for smooth remote players in the `client` module.
fn smooth_remote_players(
    time: Res<Time>,
    mut remote_players: Query<(&RemotePlayerAvatar, &mut Transform)>,
    mut runtime: ResMut<MultiplayerClientRuntime>,
) {
    let now = time.elapsed_secs();
    let render_at = (now - REMOTE_PLAYER_INTERP_BACK_TIME_SECS).max(0.0);
    let alpha = (1.0 - (-REMOTE_PLAYER_SMOOTHING_HZ * time.delta_secs()).exp()).clamp(0.0, 1.0);

    for (avatar, mut transform) in &mut remote_players {
        let Some(smoothing) = runtime.remote_player_smoothing.get_mut(&avatar.player_id) else {
            continue;
        };
        let Some(front) = smoothing.snapshots.front().copied() else {
            continue;
        };

        while smoothing.snapshots.len() >= 2 {
            let next = smoothing.snapshots.get(1).copied();
            if match next {
                Some(snapshot) => snapshot.at_secs > render_at,
                None => true,
            } {
                break;
            }
            smoothing.snapshots.pop_front();
        }

        let (target_translation, target_yaw) = if let Some(next) = smoothing.snapshots.get(1) {
            let from = smoothing.snapshots[0];
            let to = *next;
            let span = (to.at_secs - from.at_secs).max(0.0001);
            let t = ((render_at - from.at_secs) / span).clamp(0.0, 1.0);
            (
                from.translation.lerp(to.translation, t),
                lerp_angle_radians(from.yaw, to.yaw, t),
            )
        } else {
            let latest = smoothing.snapshots.back().copied().unwrap_or(front);
            let extrapolated =
                if let Some(previous) = smoothing.snapshots.iter().rev().nth(1).copied() {
                    let dt = (latest.at_secs - previous.at_secs).max(0.0001);
                    let velocity = (latest.translation - previous.translation) / dt;
                    let ahead = (render_at - latest.at_secs)
                        .clamp(0.0, REMOTE_PLAYER_MAX_EXTRAPOLATION_SECS);
                    latest.translation + velocity * ahead
                } else {
                    latest.translation
                };
            (extrapolated, latest.yaw)
        };

        transform.translation = transform.translation.lerp(target_translation, alpha);
        let current_yaw = transform.rotation.to_euler(EulerRot::YXZ).0;
        let smoothed_yaw = lerp_angle_radians(current_yaw, target_yaw, alpha);
        transform.rotation = Quat::from_rotation_y(smoothed_yaw);
    }
}

/// Runs the `send_local_block_break_events` routine for send local block break events in the `client` module.
fn send_local_block_break_events(
    mut break_events: MessageReader<BlockBreakByPlayerEvent>,
    item_registry: Option<Res<ItemRegistry>>,
    block_remap: Res<BlockIdRemap>,
    runtime: Res<MultiplayerClientRuntime>,
    q_connected: Query<Has<Connected>>,
    mut q_sender: Query<&mut MessageSender<ClientBlockBreak>>,
) {
    let Some(entity) = runtime.connection_entity else {
        for _ in break_events.read() {}
        return;
    };

    if !q_connected.get(entity).unwrap_or(false) {
        for _ in break_events.read() {}
        return;
    }

    let Ok(mut sender) = q_sender.get_mut(entity) else {
        for _ in break_events.read() {}
        return;
    };

    for event in break_events.read() {
        let drop_block_id = if event.drops_item {
            item_registry
                .as_ref()
                .and_then(|items| items.block_for_item(event.drop_item_id))
                .map(|local_block_id| block_remap.to_server(local_block_id))
                .unwrap_or(0)
        } else {
            0
        };
        sender.send::<OrderedReliable>(ClientBlockBreak::new(
            event.location.to_array(),
            drop_block_id,
            0,
        ));
    }
}

/// Runs the `send_local_block_place_events` routine for send local block place events in the `client` module.
fn send_local_block_place_events(
    mut place_events: MessageReader<BlockPlaceByPlayerEvent>,
    block_remap: Res<BlockIdRemap>,
    runtime: Res<MultiplayerClientRuntime>,
    q_connected: Query<Has<Connected>>,
    mut q_sender: Query<&mut MessageSender<ClientBlockPlace>>,
) {
    let Some(entity) = runtime.connection_entity else {
        for _ in place_events.read() {}
        return;
    };

    if !q_connected.get(entity).unwrap_or(false) {
        for _ in place_events.read() {}
        return;
    }

    let Ok(mut sender) = q_sender.get_mut(entity) else {
        for _ in place_events.read() {}
        return;
    };

    for event in place_events.read() {
        sender.send::<OrderedReliable>(ClientBlockPlace::new(
            event.location.to_array(),
            block_remap.to_server(event.block_id),
        ));
    }
}

/// Runs the `send_local_item_drop_requests` routine for send local item drop requests in the `client` module.
fn send_local_item_drop_requests(
    mut drop_requests: MessageReader<DropItemRequest>,
    item_registry: Option<Res<ItemRegistry>>,
    block_remap: Res<BlockIdRemap>,
    runtime: Res<MultiplayerClientRuntime>,
    q_connected: Query<Has<Connected>>,
    mut q_sender: Query<&mut MessageSender<ClientDropItem>>,
) {
    let Some(entity) = runtime.connection_entity else {
        for _ in drop_requests.read() {}
        return;
    };

    if !q_connected.get(entity).unwrap_or(false) {
        for _ in drop_requests.read() {}
        return;
    }

    let Ok(mut sender) = q_sender.get_mut(entity) else {
        for _ in drop_requests.read() {}
        return;
    };

    for request in drop_requests.read() {
        if request.item_id == 0 || request.amount == 0 {
            continue;
        }
        let Some(local_block_id) = item_registry
            .as_ref()
            .and_then(|items| items.block_for_item(request.item_id))
        else {
            continue;
        };

        sender.send::<OrderedReliable>(ClientDropItem::new(
            request.location,
            block_remap.to_server(local_block_id),
            request.amount,
            request.spawn_translation,
            request.initial_velocity,
        ));
    }
}

/// Runs the `simulate_multiplayer_drop_items` routine for simulate multiplayer drop items in the `client` module.
fn simulate_multiplayer_drop_items(
    time: Res<Time>,
    chunk_map: Res<ChunkMap>,
    player: Query<&Transform, (With<Player>, Without<MultiplayerDroppedItem>)>,
    mut drops: Query<(&mut MultiplayerDroppedItem, &mut Transform), With<MultiplayerDroppedItem>>,
) {
    let delta = time.delta_secs();
    let now = time.elapsed_secs();
    let player_pos = player.single().ok().map(|t| t.translation);

    for (mut drop, mut transform) in &mut drops {
        drop.velocity.y -= MULTIPLAYER_DROP_GRAVITY * delta;
        let vx = drop.velocity.x;
        let vz = drop.velocity.z;
        drop.angular_velocity += Vec3::new(vz, 0.0, -vx) * (1.25 * delta);
        let max_spin = 36.0;
        let spin_len = drop.angular_velocity.length();
        if spin_len > max_spin {
            drop.angular_velocity = drop.angular_velocity / spin_len * max_spin;
        }
        let mut spin = Quat::IDENTITY;
        if drop.angular_velocity.length_squared() > 0.000_001 {
            spin = Quat::from_scaled_axis(drop.angular_velocity * delta) * spin;
        }
        if !drop.resting
            && drop.spin_axis.length_squared() > 0.000_001
            && drop.spin_speed.abs() > 0.001
        {
            spin = Quat::from_axis_angle(drop.spin_axis, drop.spin_speed * delta) * spin;
        }
        if spin != Quat::IDENTITY {
            transform.rotation = (spin * transform.rotation).normalize();
        }

        let half = MULTIPLAYER_DROP_ITEM_SIZE * 0.5;
        let support_probe = transform.translation - Vec3::Y * (half + 0.06);
        let support_x = support_probe.x.floor() as i32;
        let support_y = support_probe.y.floor() as i32;
        let support_z = support_probe.z.floor() as i32;
        let has_support =
            get_block_world(&chunk_map, IVec3::new(support_x, support_y, support_z)) != 0;

        if now >= drop.pickup_ready_at {
            if let Some(player_pos) = player_pos {
                let to_player = player_pos - transform.translation;
                let dist_sq = to_player.length_squared();
                if dist_sq <= MULTIPLAYER_DROP_ATTRACT_RADIUS * MULTIPLAYER_DROP_ATTRACT_RADIUS
                    && dist_sq > 0.000_001
                {
                    let dist = dist_sq.sqrt();
                    let dir = to_player / dist;
                    let t = 1.0 - (dist / MULTIPLAYER_DROP_ATTRACT_RADIUS).clamp(0.0, 1.0);
                    let accel = MULTIPLAYER_DROP_ATTRACT_ACCEL * (0.35 + t * 1.65);
                    drop.velocity += dir * (accel * delta);
                    let speed = drop.velocity.length();
                    if speed > MULTIPLAYER_DROP_ATTRACT_MAX_SPEED {
                        drop.velocity = drop.velocity / speed * MULTIPLAYER_DROP_ATTRACT_MAX_SPEED;
                    }
                    drop.resting = false;
                }
            }
        }

        if drop.resting {
            if has_support {
                drop.velocity = Vec3::ZERO;
                let drag = (1.0 - 4.0 * delta).clamp(0.0, 1.0);
                drop.angular_velocity *= drag;
                drop.spin_speed *= drag;
                if drop.angular_velocity.length_squared() < 0.000_1 {
                    drop.angular_velocity = Vec3::ZERO;
                }
                if drop.spin_speed.abs() < 0.01 {
                    drop.spin_speed = 0.0;
                }
                continue;
            }

            drop.resting = false;
            drop.velocity = Vec3::new(0.0, drop.velocity.y.min(-0.1), 0.0);
        }

        transform.translation += drop.velocity * delta;

        let foot = transform.translation - Vec3::Y * (half + 0.03);
        let wx = foot.x.floor() as i32;
        let wy = foot.y.floor() as i32;
        let wz = foot.z.floor() as i32;

        let below_is_solid = get_block_world(&chunk_map, IVec3::new(wx, wy, wz)) != 0;
        if !below_is_solid || drop.velocity.y > 0.0 {
            continue;
        }

        let ground_top = wy as f32 + 1.0;
        if transform.translation.y - half > ground_top {
            continue;
        }

        transform.translation.y = ground_top + half;
        drop.velocity = Vec3::ZERO;
        drop.resting = true;
        drop.angular_velocity *= 0.4;
        drop.spin_speed *= 0.5;
    }
}

/// Runs the `send_local_drop_pickup_requests` routine for send local drop pickup requests in the `client` module.
fn send_local_drop_pickup_requests(
    time: Res<Time>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
    inventory: Res<PlayerInventory>,
    item_registry: Option<Res<ItemRegistry>>,
    player: Query<&Transform, With<Player>>,
    mut drops: Query<(&Transform, &mut MultiplayerDroppedItem), With<MultiplayerDroppedItem>>,
    runtime: Res<MultiplayerClientRuntime>,
    q_connected: Query<Has<Connected>>,
    mut q_sender: Query<&mut MessageSender<ClientDropPickup>>,
) {
    if !multiplayer_connection.connected {
        return;
    }

    let Some(entity) = runtime.connection_entity else {
        return;
    };

    if !q_connected.get(entity).unwrap_or(false) {
        return;
    }

    let Ok(mut sender) = q_sender.get_mut(entity) else {
        return;
    };

    let Ok(player_transform) = player.single() else {
        return;
    };

    let radius_sq = MULTIPLAYER_DROP_PICKUP_RADIUS * MULTIPLAYER_DROP_PICKUP_RADIUS;
    let player_pos = player_transform.translation;
    let now = time.elapsed_secs();
    let Some(item_registry) = item_registry.as_ref() else {
        return;
    };

    for (transform, mut drop) in &mut drops {
        if now < drop.pickup_ready_at {
            continue;
        }

        if now < drop.next_pickup_request_at {
            continue;
        }

        if !inventory_can_add_item(&inventory, drop.item_id, item_registry) {
            continue;
        }

        if player_pos.distance_squared(transform.translation) > radius_sq {
            continue;
        }

        sender.send::<OrderedReliable>(ClientDropPickup::new(drop.drop_id));
        drop.next_pickup_request_at = now + 0.25;
    }
}

/// Runs the `send_local_player_pose` routine for send local player pose in the `client` module.
fn send_local_player_pose(
    time: Res<Time>,
    q_player: Query<(&Transform, &FpsController), With<Player>>,
    mut runtime: ResMut<MultiplayerClientRuntime>,
    q_connected: Query<Has<Connected>>,
    mut q_sender: Query<&mut MessageSender<PlayerMove>>,
) {
    runtime.send_timer.tick(time.delta());
    if !runtime.send_timer.just_finished() {
        return;
    }

    let Some(entity) = runtime.connection_entity else {
        return;
    };

    if !q_connected.get(entity).unwrap_or(false) {
        return;
    }

    let Ok(mut sender) = q_sender.get_mut(entity) else {
        return;
    };

    let Ok((transform, controller)) = q_player.single() else {
        return;
    };

    sender.send::<UnorderedUnreliable>(PlayerMove::new(
        transform.translation.to_array(),
        controller.yaw,
        controller.pitch,
    ));
}

/// Runs the `send_client_keepalive` routine for send client keepalive in the `client` module.
fn send_client_keepalive(
    time: Res<Time>,
    mut runtime: ResMut<MultiplayerClientRuntime>,
    q_connected: Query<Has<Connected>>,
    mut q_sender: Query<&mut MessageSender<ClientKeepAlive>>,
) {
    runtime.keepalive_timer.tick(time.delta());
    if !runtime.keepalive_timer.just_finished() {
        return;
    }

    let Some(entity) = runtime.connection_entity else {
        return;
    };

    if !q_connected.get(entity).unwrap_or(false) {
        return;
    }

    let Ok(mut sender) = q_sender.get_mut(entity) else {
        return;
    };

    let stamp_ms = (time.elapsed_secs_f64() * 1000.0) as u32;
    sender.send::<UnorderedReliable>(ClientKeepAlive::new(stamp_ms));
}

/// Applies remote block break for the `client` module.
fn apply_remote_block_break(
    location: [i32; 3],
    chunk_map: &mut ChunkMap,
    fluids: &mut FluidMap,
    ev_dirty: &mut MessageWriter<SubChunkNeedRemeshEvent>,
) {
    let world_pos = IVec3::from_array(location);
    if world_pos.y < Y_MIN || world_pos.y > Y_MAX {
        return;
    }

    let mut changed = false;
    if let Some(mut access) = world_access_mut(chunk_map, world_pos) {
        if access.get() != 0 {
            access.set(0);
            changed = true;
        }
    }

    let (chunk_coord, local) = world_to_chunk_xz(world_pos.x, world_pos.z);
    let lx = local.x as usize;
    let lz = local.y as usize;
    let ly = world_y_to_local(world_pos.y);
    if let Some(fluid_chunk) = fluids.0.get_mut(&chunk_coord) {
        if fluid_chunk.get(lx, ly, lz) {
            fluid_chunk.set(lx, ly, lz, false);
            changed = true;
        }
    }

    if changed {
        mark_dirty_block_and_neighbors(chunk_map, world_pos, ev_dirty);
    }
}

/// Applies remote block place for the `client` module.
fn apply_remote_block_place(
    location: [i32; 3],
    block_id: u16,
    registry: &BlockRegistry,
    chunk_map: &mut ChunkMap,
    fluids: &mut FluidMap,
    ev_dirty: &mut MessageWriter<SubChunkNeedRemeshEvent>,
) {
    if block_id == 0 {
        return;
    }

    let world_pos = IVec3::from_array(location);
    if world_pos.y < Y_MIN || world_pos.y > Y_MAX {
        return;
    }

    let (chunk_coord, local) = world_to_chunk_xz(world_pos.x, world_pos.z);
    let lx = local.x as usize;
    let lz = local.y as usize;
    let ly = world_y_to_local(world_pos.y);
    let is_fluid = registry.is_fluid(block_id);

    if let Some(mut access) = world_access_mut(chunk_map, world_pos) {
        access.set(if is_fluid { 0 } else { block_id });
        mark_dirty_block_and_neighbors(chunk_map, world_pos, ev_dirty);
    }

    let fluid_chunk = fluids
        .0
        .entry(chunk_coord)
        .or_insert_with(|| FluidChunk::new(SEA_LEVEL));
    if is_fluid {
        fluid_chunk.set(lx, ly, lz, true);
    } else {
        fluid_chunk.set(lx, ly, lz, false);
    }
}

/// Spawns multiplayer drop for the `client` module.
#[allow(clippy::too_many_arguments)]
fn spawn_multiplayer_drop(
    commands: &mut Commands,
    registry: &BlockRegistry,
    item_registry: &ItemRegistry,
    meshes: &mut Assets<Mesh>,
    drops: &mut MultiplayerDropIndex,
    drop_id: u64,
    location: [i32; 3],
    item_id: u16,
    has_motion: bool,
    spawn_translation: [f32; 3],
    initial_velocity: [f32; 3],
    spawn_now: f32,
) {
    if item_id == 0 {
        return;
    }

    if drops.entities.contains_key(&drop_id) {
        return;
    }

    let Some((mesh, material, visual_scale)) =
        build_world_item_drop_visual(registry, item_registry, item_id, MULTIPLAYER_DROP_ITEM_SIZE)
    else {
        return;
    };

    let world_loc = IVec3::from_array(location);
    let pop_velocity = if has_motion {
        Vec3::from_array(initial_velocity)
    } else {
        compute_multiplayer_drop_pop_velocity(world_loc, drop_id)
    };
    let angular_velocity = compute_multiplayer_drop_angular_velocity(world_loc, drop_id);
    let spin_axis = compute_multiplayer_drop_spin_axis(world_loc, drop_id);
    let spin_speed = compute_multiplayer_drop_spin_speed(world_loc, drop_id);
    let initial_rotation = Quat::from_euler(
        EulerRot::XYZ,
        hash01_u64(seed_from_world_loc(world_loc) ^ drop_id ^ 0xA11CE) * std::f32::consts::TAU,
        hash01_u64(seed_from_world_loc(world_loc) ^ drop_id ^ 0xB00B5) * std::f32::consts::TAU,
        hash01_u64(seed_from_world_loc(world_loc) ^ drop_id ^ 0xC0FFEE) * std::f32::consts::TAU,
    );
    let center = if has_motion {
        Vec3::from_array(spawn_translation)
    } else {
        Vec3::new(
            (world_loc.x as f32 + 0.5) * VOXEL_SIZE,
            (world_loc.y as f32 + 0.5) * VOXEL_SIZE + 0.28,
            (world_loc.z as f32 + 0.5) * VOXEL_SIZE,
        )
    };

    let entity = commands
        .spawn((
            MultiplayerDroppedItem {
                drop_id,
                item_id,
                pickup_ready_at: spawn_now + MULTIPLAYER_DROP_PICKUP_DELAY_SECS,
                next_pickup_request_at: 0.0,
                resting: false,
                velocity: pop_velocity,
                angular_velocity,
                spin_axis,
                spin_speed,
            },
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(material),
            Transform {
                translation: center,
                rotation: initial_rotation,
                scale: visual_scale,
            },
            Visibility::default(),
            Name::new(format!("MultiplayerDrop#{drop_id}")),
        ))
        .id();

    drops.entities.insert(drop_id, entity);
}

/// Clears multiplayer drops for the `client` module.
fn clear_multiplayer_drops(commands: &mut Commands, drops: &mut MultiplayerDropIndex) {
    for entity in drops.entities.drain().map(|(_, entity)| entity) {
        safe_despawn_entity(commands, entity);
    }
}

/// Runs the `inventory_can_add_item` routine for inventory can add item in the `client` module.
fn inventory_can_add_item(
    inventory: &PlayerInventory,
    item_id: u16,
    item_registry: &ItemRegistry,
) -> bool {
    if item_id == 0 {
        return false;
    }

    let stack_limit = item_registry.stack_limit(item_id);
    inventory
        .slots
        .iter()
        .any(|slot| slot.is_empty() || (slot.item_id == item_id && slot.count < stack_limit))
}

/// Computes multiplayer drop pop velocity for the `client` module.
fn compute_multiplayer_drop_pop_velocity(world_loc: IVec3, drop_id: u64) -> Vec3 {
    let seed_base = seed_from_world_loc(world_loc) ^ drop_id;
    let angle = hash01_u64(seed_base ^ 0x10) * std::f32::consts::TAU;
    let distance = MULTIPLAYER_DROP_POP_MIN_DIST
        + (MULTIPLAYER_DROP_POP_MAX_DIST - MULTIPLAYER_DROP_POP_MIN_DIST)
            * hash01_u64(seed_base ^ 0x20);
    let flight_time = 0.35 + hash01_u64(seed_base ^ 0x30) * 0.25;
    let horizontal_speed = (distance / flight_time).max(0.2);

    Vec3::new(
        angle.cos() * horizontal_speed,
        2.8 + hash01_u64(seed_base ^ 0x40) * 1.2,
        angle.sin() * horizontal_speed,
    )
}

/// Computes multiplayer drop angular velocity for the `client` module.
fn compute_multiplayer_drop_angular_velocity(world_loc: IVec3, drop_id: u64) -> Vec3 {
    let seed_base = seed_from_world_loc(world_loc) ^ drop_id ^ 0x5EED;

    Vec3::new(
        -8.0 + hash01_u64(seed_base ^ 0x51) * 16.0,
        -10.0 + hash01_u64(seed_base ^ 0x52) * 20.0,
        -8.0 + hash01_u64(seed_base ^ 0x53) * 16.0,
    )
}

/// Computes multiplayer drop spin axis for the `client` module.
fn compute_multiplayer_drop_spin_axis(world_loc: IVec3, drop_id: u64) -> Vec3 {
    let seed_base = seed_from_world_loc(world_loc) ^ drop_id ^ 0x7A51_5EED;
    let axis = Vec3::new(
        -1.0 + hash01_u64(seed_base ^ 0x71) * 2.0,
        0.35 + hash01_u64(seed_base ^ 0x72) * 1.3,
        -1.0 + hash01_u64(seed_base ^ 0x73) * 2.0,
    );
    let axis = axis.normalize_or_zero();
    if axis.length_squared() > 0.000_001 {
        axis
    } else {
        Vec3::new(0.78, 0.44, 0.44).normalize()
    }
}

/// Computes multiplayer drop spin speed for the `client` module.
fn compute_multiplayer_drop_spin_speed(world_loc: IVec3, drop_id: u64) -> f32 {
    let seed_base = seed_from_world_loc(world_loc) ^ drop_id ^ 0x8BAD_F00D;
    let magnitude = 18.0 + hash01_u64(seed_base ^ 0x81) * 14.0;
    let sign = if hash01_u64(seed_base ^ 0x82) < 0.5 {
        -1.0
    } else {
        1.0
    };
    sign * magnitude
}

/// Runs the `angle_abs_diff` routine for angle abs diff in the `client` module.
#[inline]
fn angle_abs_diff(from: f32, to: f32) -> f32 {
    let wrapped =
        (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    wrapped.abs()
}

/// Runs the `lerp_angle_radians` routine for lerp angle radians in the `client` module.
#[inline]
fn lerp_angle_radians(from: f32, to: f32, t: f32) -> f32 {
    let wrapped =
        (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    from + wrapped * t.clamp(0.0, 1.0)
}

/// Runs the `seed_from_world_loc` routine for seed from world loc in the `client` module.
fn seed_from_world_loc(world_loc: IVec3) -> u64 {
    (world_loc.x as i64 as u64).wrapping_mul(0x9E37_79B1_85EB_CA87)
        ^ (world_loc.y as i64 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ (world_loc.z as i64 as u64).wrapping_mul(0x1656_67B1_9E37_79F9)
}

/// Runs the `hash01_u64` routine for hash01 u64 in the `client` module.
fn hash01_u64(mut x: u64) -> f32 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;

    (x as f64 / u64::MAX as f64) as f32
}

/// Runs the `ensure_remote_player` routine for ensure remote player in the `client` module.
fn ensure_remote_player(
    commands: &mut Commands,
    visuals: &RemotePlayerVisuals,
    remote_players: &mut HashMap<u64, Entity>,
    player_id: u64,
    translation: Vec3,
    yaw: f32,
) -> Entity {
    if let Some(entity) = remote_players.get(&player_id) {
        return *entity;
    }

    let entity = commands
        .spawn((
            RemotePlayerAvatar { player_id },
            Name::new(format!("RemotePlayer#{player_id}")),
            Mesh3d(visuals.mesh.clone()),
            MeshMaterial3d(visuals.material.clone()),
            Transform {
                translation,
                rotation: Quat::from_rotation_y(yaw),
                scale: Vec3::ONE,
            },
            Visibility::default(),
            InheritedVisibility::default(),
            ViewVisibility::default(),
        ))
        .id();

    remote_players.insert(player_id, entity);
    entity
}
