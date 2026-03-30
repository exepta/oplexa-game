use self::manager::ManagerPlugin;
use crate::core::config::GlobalConfig;
use crate::core::debug::{BuildInfo, WorldInspectorState};
use crate::core::entities::player::inventory::PlayerInventory;
use crate::core::entities::player::{FpsController, Player};
use crate::core::events::block::block_player_events::{
    BlockBreakByPlayerEvent, BlockPlaceByPlayerEvent,
};
use crate::core::events::chunk_events::SubChunkNeedRemeshEvent;
use crate::core::events::ui_events::{
    ConnectToServerRequest, DisconnectFromServerRequest, DropItemRequest, OpenToLanRequest,
    StopLanHostRequest,
};
use crate::core::multiplayer::MultiplayerConnectionState;
use crate::core::states::states::AppState;
use crate::core::world::block::{
    BlockRegistry, VOXEL_SIZE, build_block_cube_mesh, get_block_world,
};
use crate::core::world::chunk::ChunkMap;
use crate::core::world::chunk_dimension::{Y_MAX, Y_MIN, world_to_chunk_xz, world_y_to_local};
use crate::core::world::fluid::FluidMap;
use crate::core::world::{mark_dirty_block_and_neighbors, world_access_mut};
use crate::generator::chunk::chunk_utils::safe_despawn_entity;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSamplerDescriptor};
use bevy::log::{BoxedLayer, Level, LogPlugin};
use bevy::math::primitives::Capsule3d;
use bevy::mesh::{Mesh3d, VertexAttributeValues};
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
use multiplayer::{
    config::NetworkSettings,
    discovery::{LanDiscoveryClient, LanServerInfo},
    protocols::{
        Auth, ClientBlockBreak, ClientBlockPlace, ClientDropItem, ClientDropPickup, PlayerJoined,
        PlayerLeft, PlayerMove, PlayerSnapshot, ServerBlockBreak, ServerBlockPlace,
        ServerDropPicked, ServerDropSpawn, ServerWelcome, protocol,
    },
    world::{NetworkEntity, NetworkWorld},
};
use naia_client::{
    Client as NaiaClient, ClientConfig, ConnectEvent, DisconnectEvent, ErrorEvent, MessageEvent,
    RejectEvent,
    shared::default_channels::{
        OrderedReliableChannel, UnorderedReliableChannel, UnorderedUnreliableChannel,
    },
    transport::udp,
};
use std::collections::HashMap;
use std::env;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing_subscriber::Layer;
use tracing_subscriber::fmt::writer::BoxMakeWriter;

type NetworkClient = NaiaClient<NetworkEntity>;

#[derive(Component)]
struct RemotePlayerAvatar {
    #[allow(dead_code)]
    player_id: u64,
}

#[derive(Resource)]
struct RemotePlayerVisuals {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

#[derive(Component, Debug)]
struct MultiplayerDroppedItem {
    drop_id: u64,
    block_id: u16,
    pickup_ready_at: f32,
    next_pickup_request_at: f32,
    resting: bool,
    velocity: Vec3,
    angular_velocity: Vec3,
    spin_axis: Vec3,
    spin_speed: f32,
}

#[derive(Resource, Default)]
struct MultiplayerDropIndex {
    entities: HashMap<u64, Entity>,
}

const MULTIPLAYER_DROP_ITEM_SIZE: f32 = 0.32;
const MULTIPLAYER_DROP_PICKUP_RADIUS: f32 = 1.35;
const MULTIPLAYER_DROP_ATTRACT_RADIUS: f32 = 3.5;
const MULTIPLAYER_DROP_ATTRACT_ACCEL: f32 = 34.0;
const MULTIPLAYER_DROP_ATTRACT_MAX_SPEED: f32 = 12.0;
const MULTIPLAYER_DROP_GRAVITY: f32 = 12.0;
const MULTIPLAYER_DROP_POP_MIN_DIST: f32 = 0.1;
const MULTIPLAYER_DROP_POP_MAX_DIST: f32 = 1.0;
const MULTIPLAYER_DROP_VISUAL_SCALE_X: f32 = 0.85;
const MULTIPLAYER_DROP_VISUAL_SCALE_Y: f32 = 0.72;
const MULTIPLAYER_DROP_VISUAL_SCALE_Z: f32 = 1.14;
const MULTIPLAYER_DROP_PICKUP_DELAY_SECS: f32 = 0.5;

struct MultiplayerClientRuntime {
    enabled: bool,
    player_name: String,
    session_url: String,
    auto_connect_lan: bool,
    client: Option<NetworkClient>,
    world: NetworkWorld,
    local_player_id: Option<u64>,
    remote_players: HashMap<u64, Entity>,
    next_local_drop_seq: u32,
    send_timer: Timer,
}

impl MultiplayerClientRuntime {
    fn new(settings: &NetworkSettings) -> Self {
        let auto_connect_lan = settings.client.session_url.eq_ignore_ascii_case("lan:auto");
        let runtime = Self {
            enabled: settings.client.enabled,
            player_name: settings.client.player_name.clone(),
            session_url: settings.client.session_url.clone(),
            auto_connect_lan,
            client: None,
            world: NetworkWorld::default(),
            local_player_id: None,
            remote_players: HashMap::new(),
            next_local_drop_seq: 1,
            send_timer: Timer::from_seconds(
                Duration::from_millis(settings.client.transform_send_interval_ms).as_secs_f32(),
                TimerMode::Repeating,
            ),
        };

        runtime
    }

    fn connect(&mut self, session_url: String) {
        if !self.enabled {
            return;
        }

        if self
            .client
            .as_ref()
            .is_some_and(|client| !client.connection_status().is_disconnected())
        {
            return;
        }

        let protocol = protocol();
        let socket = udp::Socket::new(&session_url, protocol.socket.link_condition.clone());
        let mut client = NetworkClient::new(ClientConfig::default(), protocol);
        client.auth(Auth::new(self.player_name.clone()));
        client.connect(socket);

        info!("Connecting to multiplayer server at {}", session_url);
        self.client = Some(client);
        self.session_url = session_url;
        self.local_player_id = None;
        self.remote_players.clear();
        self.next_local_drop_seq = 1;
    }

    fn allocate_local_drop_id(&mut self) -> Option<u64> {
        let player_id = self.local_player_id?;
        let drop_id = (player_id << 32) | (self.next_local_drop_seq as u64);
        self.next_local_drop_seq = self.next_local_drop_seq.wrapping_add(1).max(1);
        Some(drop_id)
    }
}

struct LanDiscoveryRuntime {
    client: Option<LanDiscoveryClient>,
    known_servers: Vec<LanServerInfo>,
    refresh_timer: Timer,
}

impl LanDiscoveryRuntime {
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

struct LocalLanHost {
    child: Option<Child>,
    session_url: Option<String>,
    connect_timer: Option<Timer>,
}

impl Default for LocalLanHost {
    fn default() -> Self {
        Self {
            child: None,
            session_url: None,
            connect_timer: None,
        }
    }
}

impl LocalLanHost {
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

pub fn run() {
    GlobalConfig::ensure_config_files_exist();
    let graphics_config = GlobalConfig::new();
    let multiplayer_settings = NetworkSettings::load_or_create("config/network.toml");
    let mut app = App::new();
    init_bevy_app(&mut app, &graphics_config, multiplayer_settings);
}

fn init_bevy_app(app: &mut App, config: &GlobalConfig, multiplayer_settings: NetworkSettings) {
    let build = BuildInfo {
        app_name: "Game Version",
        app_version: env!("CARGO_PKG_VERSION"),
        bevy_version: "0.18.1",
    };

    app.insert_resource(config.clone())
        .insert_resource(build)
        .insert_resource(ClearColor(Color::Srgba(Srgba::rgb_u8(20, 25, 27))))
        .insert_resource(WorldInspectorState(false))
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

    app.insert_non_send_resource(MultiplayerClientRuntime::new(&multiplayer_settings))
        .insert_non_send_resource(LanDiscoveryRuntime::new(&multiplayer_settings))
        .insert_non_send_resource(LocalLanHost::default())
        .init_state::<AppState>()
        .add_plugins(EguiPlugin::default())
        .add_plugins(WorldInspectorPlugin::default().run_if(check_world_inspector_state))
        .add_plugins(ManagerPlugin)
        .add_plugins(MultiplayerClientPlugin)
        .add_systems(
            Update,
            init_app_finish
                .run_if(in_state(AppState::AppInit).and(resource_exists::<GlobalConfig>)),
        )
        .run();
}

fn register_world_inspector_types(app: &mut App) {
    app.register_type::<GizmoConfigStore>()
        .register_type::<bevy::render::view::ColorGradingSection>()
        .register_type::<bevy::render::view::ColorGradingGlobal>()
        .register_type::<AmbientLight>()
        .register_type::<PointLight>()
        .register_type::<DirectionalLight>()
        .register_type::<bevy::camera::Camera3dDepthLoadOp>();
}

fn init_app_finish(mut next_state: ResMut<NextState<AppState>>) {
    info!("Finish initializing app...");
    next_state.set(AppState::Preload);
}

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

fn check_world_inspector_state(world_inspector_state: Res<WorldInspectorState>) -> bool {
    world_inspector_state.0
}

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
    let file_arc = Arc::new(Mutex::new(file));

    let _shutdown_logger = StartLogText {
        file: Arc::clone(&file_arc),
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

fn load_log_env_filter() -> String {
    dotenv().ok();
    env::var("LOG_ENV_FILTER").unwrap_or_else(|_| "error".to_string())
}

struct StartLogText {
    file: Arc<Mutex<File>>,
}

impl Drop for StartLogText {
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

    pub struct ManagerPlugin;

    impl Plugin for ManagerPlugin {
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

    fn setup_shadow_map(mut commands: Commands) {
        commands.insert_resource(DirectionalLightShadowMap { size: 1024 });
    }

    fn configure_default_gizmos(mut gizmo_config_store: ResMut<GizmoConfigStore>) {
        let (config, _) = gizmo_config_store.config_mut::<DefaultGizmoConfigGroup>();
        config.enabled = true;
        config.line.width = 3.0;
        config.depth_bias = -1.0;
        config.render_layers = RenderLayers::from_layers(&[0, 1, 2]);
    }

    fn configure_chunk_grid_gizmos(mut gizmo_config_store: ResMut<GizmoConfigStore>) {
        let (config, _) = gizmo_config_store.config_mut::<ChunkGridGizmos>();
        config.enabled = true;
        config.line.width = 3.0;
        config.depth_bias = -1.0;
        config.render_layers = RenderLayers::from_layers(&[0, 1, 2]);
    }

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

struct MultiplayerClientPlugin;

impl Plugin for MultiplayerClientPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MultiplayerDropIndex>()
            .add_systems(Startup, setup_remote_player_visuals)
            .add_systems(
                Update,
                (
                    poll_lan_servers,
                    connect_to_server_requested,
                    disconnect_from_server_requested,
                    open_to_lan_requested,
                    finish_open_to_lan_connect,
                    stop_lan_host_requested,
                    send_local_block_break_events,
                    send_local_block_place_events,
                    send_local_item_drop_requests,
                    receive_multiplayer_messages,
                    simulate_multiplayer_drop_items,
                    send_local_drop_pickup_requests,
                    send_local_player_pose,
                ),
            );
    }
}

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

fn connect_to_server_requested(
    mut connect_requests: MessageReader<ConnectToServerRequest>,
    discovery: NonSend<LanDiscoveryRuntime>,
    mut runtime: NonSendMut<MultiplayerClientRuntime>,
) {
    if connect_requests.read().next().is_none() {
        return;
    }

    if runtime.session_url.eq_ignore_ascii_case("lan:auto") {
        if let Some(server) = discovery.known_servers.first() {
            runtime.connect(server.session_url.clone());
            runtime.auto_connect_lan = false;
        } else {
            warn!("No LAN server discovered yet. Connect request ignored.");
        }
        return;
    }

    let session_url = runtime.session_url.clone();
    runtime.connect(session_url);
    runtime.auto_connect_lan = false;
}

fn disconnect_from_server_requested(
    mut disconnect_requests: MessageReader<DisconnectFromServerRequest>,
    mut runtime: NonSendMut<MultiplayerClientRuntime>,
) {
    if disconnect_requests.read().next().is_none() {
        return;
    }

    let Some(client) = runtime.client.as_mut() else {
        return;
    };

    let status = client.connection_status();
    if status.is_connected() {
        client.disconnect();
    }
}

fn open_to_lan_requested(
    mut requests: MessageReader<OpenToLanRequest>,
    mut runtime: NonSendMut<MultiplayerClientRuntime>,
    mut local_host: NonSendMut<LocalLanHost>,
) {
    if requests.read().next().is_none() {
        return;
    }

    local_host.refresh();

    if runtime
        .client
        .as_ref()
        .is_some_and(|client| !client.connection_status().is_disconnected())
    {
        warn!("Open to LAN ignored because the client is already connected.");
        return;
    }

    let settings = NetworkSettings::load_or_create("config/network.toml");
    let session_url = settings.server.session_url();

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

fn finish_open_to_lan_connect(
    time: Res<Time>,
    mut runtime: NonSendMut<MultiplayerClientRuntime>,
    mut local_host: NonSendMut<LocalLanHost>,
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

    runtime.connect(session_url);
    local_host.connect_timer = None;
}

fn stop_lan_host_requested(
    mut requests: MessageReader<StopLanHostRequest>,
    mut local_host: NonSendMut<LocalLanHost>,
) {
    if requests.read().next().is_none() {
        return;
    }

    local_host.stop();
}

fn receive_multiplayer_messages(
    mut commands: Commands,
    time: Res<Time>,
    visuals: Res<RemotePlayerVisuals>,
    registry: Option<Res<BlockRegistry>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut inventory: ResMut<PlayerInventory>,
    mut chunk_map: ResMut<ChunkMap>,
    mut fluids: ResMut<FluidMap>,
    mut ev_dirty: MessageWriter<SubChunkNeedRemeshEvent>,
    mut drops: ResMut<MultiplayerDropIndex>,
    mut multiplayer_connection: ResMut<MultiplayerConnectionState>,
    mut runtime: NonSendMut<MultiplayerClientRuntime>,
) {
    let Some(mut client) = runtime.client.take() else {
        multiplayer_connection.connected = false;
        return;
    };

    if client.connection_status().is_disconnected() {
        multiplayer_connection.connected = false;
        clear_multiplayer_drops(&mut commands, &mut drops);
        runtime.client = Some(client);
        return;
    }

    let mut events = client.receive(runtime.world.proxy_mut());

    for server_addr in events.read::<ConnectEvent>() {
        info!("Connected to multiplayer server at {}", server_addr);
    }

    for server_addr in events.read::<RejectEvent>() {
        warn!("Connection rejected by server {}", server_addr);
    }

    let mut disconnect_received = false;
    for server_addr in events.read::<DisconnectEvent>() {
        disconnect_received = true;
        warn!("Disconnected from multiplayer server {}", server_addr);
    }

    for message in events.read::<MessageEvent<UnorderedReliableChannel, ServerWelcome>>() {
        runtime.local_player_id = Some(message.player_id);
        if let Some(entity) = runtime.remote_players.remove(&message.player_id) {
            safe_despawn_entity(&mut commands, entity);
        }
        info!(
            "Server '{}' accepted player id {}",
            message.server_name, message.player_id
        );
    }

    for message in events.read::<MessageEvent<UnorderedReliableChannel, PlayerJoined>>() {
        if Some(message.player_id) == runtime.local_player_id {
            continue;
        }

        ensure_remote_player(
            &mut commands,
            &visuals,
            &mut runtime.remote_players,
            message.player_id,
            Vec3::new(0.0, 180.0, 0.0),
            0.0,
        );
    }

    for message in events.read::<MessageEvent<UnorderedReliableChannel, PlayerLeft>>() {
        if let Some(entity) = runtime.remote_players.remove(&message.player_id) {
            safe_despawn_entity(&mut commands, entity);
        }
    }

    for message in events.read::<MessageEvent<UnorderedUnreliableChannel, PlayerSnapshot>>() {
        if Some(message.player_id) == runtime.local_player_id {
            continue;
        }

        let entity = ensure_remote_player(
            &mut commands,
            &visuals,
            &mut runtime.remote_players,
            message.player_id,
            Vec3::from_array(message.translation),
            message.yaw,
        );

        commands.entity(entity).insert(Transform {
            translation: Vec3::from_array(message.translation),
            rotation: Quat::from_rotation_y(message.yaw),
            scale: Vec3::ONE,
        });
    }

    for message in events.read::<MessageEvent<OrderedReliableChannel, ServerBlockBreak>>() {
        if Some(message.player_id) == runtime.local_player_id {
            continue;
        }

        apply_remote_block_break(message.location, &mut chunk_map, &mut ev_dirty);
    }

    for message in events.read::<MessageEvent<OrderedReliableChannel, ServerBlockPlace>>() {
        if Some(message.player_id) == runtime.local_player_id {
            continue;
        }

        apply_remote_block_place(
            message.location,
            message.block_id,
            &mut chunk_map,
            &mut fluids,
            &mut ev_dirty,
        );
    }

    for message in events.read::<MessageEvent<OrderedReliableChannel, ServerDropSpawn>>() {
        if let Some(registry) = registry.as_ref() {
            spawn_multiplayer_drop(
                &mut commands,
                registry,
                &mut meshes,
                &mut drops,
                message.drop_id,
                message.location,
                message.block_id,
                message.has_motion,
                message.spawn_translation,
                message.initial_velocity,
                time.elapsed_secs(),
            );
        }
    }

    for message in events.read::<MessageEvent<OrderedReliableChannel, ServerDropPicked>>() {
        if let Some(entity) = drops.entities.remove(&message.drop_id) {
            safe_despawn_entity(&mut commands, entity);
        }

        if Some(message.player_id) == runtime.local_player_id {
            let _ = inventory.add_block(message.block_id, 1);
        }
    }

    for error in events.read::<ErrorEvent>() {
        error!("Multiplayer client error: {}", error);
    }

    if disconnect_received {
        for entity in runtime.remote_players.drain().map(|(_, entity)| entity) {
            safe_despawn_entity(&mut commands, entity);
        }
        clear_multiplayer_drops(&mut commands, &mut drops);
        runtime.local_player_id = None;
        multiplayer_connection.connected = false;
    } else {
        multiplayer_connection.connected =
            client.connection_status().is_connected() && runtime.local_player_id.is_some();
    }

    runtime.client = Some(client);
}

fn send_local_block_break_events(
    mut commands: Commands,
    time: Res<Time>,
    registry: Option<Res<BlockRegistry>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut drops: ResMut<MultiplayerDropIndex>,
    mut break_events: MessageReader<BlockBreakByPlayerEvent>,
    mut runtime: NonSendMut<MultiplayerClientRuntime>,
) {
    let is_connected = runtime
        .client
        .as_ref()
        .is_some_and(|client| client.connection_status().is_connected());
    if !is_connected {
        for _ in break_events.read() {}
        return;
    }

    for event in break_events.read() {
        let mut drop_id = 0;
        if event.drops_item {
            if let Some(local_drop_id) = runtime.allocate_local_drop_id() {
                drop_id = local_drop_id;
                if let Some(registry) = registry.as_ref() {
                    spawn_multiplayer_drop(
                        &mut commands,
                        registry,
                        &mut meshes,
                        &mut drops,
                        local_drop_id,
                        event.location.to_array(),
                        event.block_id,
                        false,
                        [0.0, 0.0, 0.0],
                        [0.0, 0.0, 0.0],
                        time.elapsed_secs(),
                    );
                }
            }
        }

        if let Some(client) = runtime.client.as_mut() {
            client.send_message::<OrderedReliableChannel, _>(&ClientBlockBreak::new(
                event.location.to_array(),
                if event.drops_item { event.block_id } else { 0 },
                drop_id,
            ));
        }
    }
}

fn send_local_block_place_events(
    mut place_events: MessageReader<BlockPlaceByPlayerEvent>,
    mut runtime: NonSendMut<MultiplayerClientRuntime>,
) {
    let Some(client) = runtime.client.as_mut() else {
        for _ in place_events.read() {}
        return;
    };

    if !client.connection_status().is_connected() {
        for _ in place_events.read() {}
        return;
    }

    for event in place_events.read() {
        client.send_message::<OrderedReliableChannel, _>(&ClientBlockPlace::new(
            event.location.to_array(),
            event.block_id,
        ));
    }
}

fn send_local_item_drop_requests(
    mut drop_requests: MessageReader<DropItemRequest>,
    mut runtime: NonSendMut<MultiplayerClientRuntime>,
) {
    let Some(client) = runtime.client.as_mut() else {
        for _ in drop_requests.read() {}
        return;
    };

    if !client.connection_status().is_connected() {
        for _ in drop_requests.read() {}
        return;
    }

    for request in drop_requests.read() {
        if request.block_id == 0 || request.amount == 0 {
            continue;
        }

        client.send_message::<OrderedReliableChannel, _>(&ClientDropItem::new(
            request.location,
            request.block_id,
            request.amount,
            request.spawn_translation,
            request.initial_velocity,
        ));
    }
}

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

fn send_local_drop_pickup_requests(
    time: Res<Time>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
    inventory: Res<PlayerInventory>,
    player: Query<&Transform, With<Player>>,
    mut drops: Query<(&Transform, &mut MultiplayerDroppedItem), With<MultiplayerDroppedItem>>,
    mut runtime: NonSendMut<MultiplayerClientRuntime>,
) {
    if !multiplayer_connection.connected {
        return;
    }

    let Some(client) = runtime.client.as_mut() else {
        return;
    };

    if !client.connection_status().is_connected() {
        return;
    }

    let Ok(player_transform) = player.single() else {
        return;
    };

    let radius_sq = MULTIPLAYER_DROP_PICKUP_RADIUS * MULTIPLAYER_DROP_PICKUP_RADIUS;
    let player_pos = player_transform.translation;
    let now = time.elapsed_secs();

    for (transform, mut drop) in &mut drops {
        if now < drop.pickup_ready_at {
            continue;
        }

        if now < drop.next_pickup_request_at {
            continue;
        }

        if !inventory_can_add_block(&inventory, drop.block_id) {
            continue;
        }

        if player_pos.distance_squared(transform.translation) > radius_sq {
            continue;
        }

        client.send_message::<OrderedReliableChannel, _>(&ClientDropPickup::new(drop.drop_id));
        drop.next_pickup_request_at = now + 0.25;
    }
}

fn send_local_player_pose(
    time: Res<Time>,
    q_player: Query<(&Transform, &FpsController), With<Player>>,
    mut runtime: NonSendMut<MultiplayerClientRuntime>,
) {
    runtime.send_timer.tick(time.delta());
    if !runtime.send_timer.just_finished() {
        return;
    }

    let Some(client) = runtime.client.as_mut() else {
        return;
    };

    if !client.connection_status().is_connected() {
        return;
    }

    let Ok((transform, controller)) = q_player.single() else {
        return;
    };

    client.send_message::<UnorderedUnreliableChannel, _>(&PlayerMove::new(
        transform.translation.to_array(),
        controller.yaw,
        controller.pitch,
    ));
}

fn apply_remote_block_break(
    location: [i32; 3],
    chunk_map: &mut ChunkMap,
    ev_dirty: &mut MessageWriter<SubChunkNeedRemeshEvent>,
) {
    let world_pos = IVec3::from_array(location);

    if let Some(mut access) = world_access_mut(chunk_map, world_pos) {
        if access.get() == 0 {
            return;
        }
        access.set(0);
        mark_dirty_block_and_neighbors(chunk_map, world_pos, ev_dirty);
    }
}

fn apply_remote_block_place(
    location: [i32; 3],
    block_id: u16,
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

    if let Some(mut access) = world_access_mut(chunk_map, world_pos) {
        access.set(block_id);
        mark_dirty_block_and_neighbors(chunk_map, world_pos, ev_dirty);
    }

    if let Some(fluid_chunk) = fluids.0.get_mut(&chunk_coord) {
        fluid_chunk.set(lx, ly, lz, false);
    }
}

fn spawn_multiplayer_drop(
    commands: &mut Commands,
    registry: &BlockRegistry,
    meshes: &mut Assets<Mesh>,
    drops: &mut MultiplayerDropIndex,
    drop_id: u64,
    location: [i32; 3],
    block_id: u16,
    has_motion: bool,
    spawn_translation: [f32; 3],
    initial_velocity: [f32; 3],
    spawn_now: f32,
) {
    if block_id == 0 {
        return;
    }

    if drops.entities.contains_key(&drop_id) {
        return;
    }

    let mut mesh = build_block_cube_mesh(registry, block_id, MULTIPLAYER_DROP_ITEM_SIZE);
    center_mesh_vertices(&mut mesh, MULTIPLAYER_DROP_ITEM_SIZE * 0.5);

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
                block_id,
                pickup_ready_at: spawn_now + MULTIPLAYER_DROP_PICKUP_DELAY_SECS,
                next_pickup_request_at: 0.0,
                resting: false,
                velocity: pop_velocity,
                angular_velocity,
                spin_axis,
                spin_speed,
            },
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(registry.material(block_id)),
            Transform {
                translation: center,
                rotation: initial_rotation,
                scale: Vec3::new(
                    MULTIPLAYER_DROP_VISUAL_SCALE_X,
                    MULTIPLAYER_DROP_VISUAL_SCALE_Y,
                    MULTIPLAYER_DROP_VISUAL_SCALE_Z,
                ),
            },
            Visibility::default(),
            Name::new(format!("MultiplayerDrop#{drop_id}")),
        ))
        .id();

    drops.entities.insert(drop_id, entity);
}

fn clear_multiplayer_drops(commands: &mut Commands, drops: &mut MultiplayerDropIndex) {
    for entity in drops.entities.drain().map(|(_, entity)| entity) {
        safe_despawn_entity(commands, entity);
    }
}

fn inventory_can_add_block(inventory: &PlayerInventory, block_id: u16) -> bool {
    if block_id == 0 {
        return false;
    }

    inventory.slots.iter().any(|slot| {
        slot.is_empty()
            || (slot.block_id == block_id
                && slot.count
                    < crate::core::entities::player::inventory::PLAYER_INVENTORY_STACK_MAX)
    })
}

fn center_mesh_vertices(mesh: &mut Mesh, half_extent: f32) {
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    else {
        return;
    };

    for position in positions.iter_mut() {
        position[0] -= half_extent;
        position[1] -= half_extent;
        position[2] -= half_extent;
    }
}

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

fn compute_multiplayer_drop_angular_velocity(world_loc: IVec3, drop_id: u64) -> Vec3 {
    let seed_base = seed_from_world_loc(world_loc) ^ drop_id ^ 0x5EED;

    Vec3::new(
        -8.0 + hash01_u64(seed_base ^ 0x51) * 16.0,
        -10.0 + hash01_u64(seed_base ^ 0x52) * 20.0,
        -8.0 + hash01_u64(seed_base ^ 0x53) * 16.0,
    )
}

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

fn seed_from_world_loc(world_loc: IVec3) -> u64 {
    (world_loc.x as i64 as u64).wrapping_mul(0x9E37_79B1_85EB_CA87)
        ^ (world_loc.y as i64 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ (world_loc.z as i64 as u64).wrapping_mul(0x1656_67B1_9E37_79F9)
}

fn hash01_u64(mut x: u64) -> f32 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;

    (x as f64 / u64::MAX as f64) as f32
}

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
