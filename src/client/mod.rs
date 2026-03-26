use self::manager::ManagerPlugin;
use crate::core::config::GlobalConfig;
use crate::core::debug::{BuildInfo, WorldInspectorState};
use crate::core::entities::player::{FpsController, Player};
use crate::core::events::block::block_player_events::{
    BlockBreakByPlayerEvent, BlockPlaceByPlayerEvent,
};
use crate::core::events::chunk_events::SubChunkNeedRemeshEvent;
use crate::core::states::states::AppState;
use crate::core::world::chunk::ChunkMap;
use crate::core::world::chunk_dimension::{Y_MAX, Y_MIN, world_to_chunk_xz, world_y_to_local};
use crate::core::world::fluid::FluidMap;
use crate::core::world::{mark_dirty_block_and_neighbors, world_access_mut};
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
use multiplayer::{
    config::NetworkSettings,
    discovery::{LanDiscoveryClient, LanServerInfo},
    protocol::{
        Auth, ClientBlockBreak, ClientBlockPlace, PlayerJoined, PlayerLeft, PlayerMove,
        PlayerSnapshot, ServerBlockBreak, ServerBlockPlace, ServerWelcome, protocol,
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
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing_subscriber::Layer;
use tracing_subscriber::fmt::writer::BoxMakeWriter;

type NetworkClient = NaiaClient<NetworkEntity>;

#[derive(Resource, Clone)]
struct MultiplayerSettingsResource(NetworkSettings);

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

struct MultiplayerClientRuntime {
    enabled: bool,
    player_name: String,
    session_url: String,
    auto_connect_lan: bool,
    client: Option<NetworkClient>,
    world: NetworkWorld,
    local_player_id: Option<u64>,
    remote_players: HashMap<u64, Entity>,
    send_timer: Timer,
}

impl MultiplayerClientRuntime {
    fn new(settings: &NetworkSettings) -> Self {
        let auto_connect_lan = settings.client.session_url.eq_ignore_ascii_case("lan:auto");
        let mut runtime = Self {
            enabled: settings.client.enabled,
            player_name: settings.client.player_name.clone(),
            session_url: settings.client.session_url.clone(),
            auto_connect_lan,
            client: None,
            world: NetworkWorld::default(),
            local_player_id: None,
            remote_players: HashMap::new(),
            send_timer: Timer::from_seconds(
                Duration::from_millis(settings.client.transform_send_interval_ms).as_secs_f32(),
                TimerMode::Repeating,
            ),
        };

        if runtime.enabled && settings.client.connect_on_startup && !runtime.auto_connect_lan {
            runtime.connect(runtime.session_url.clone());
        }

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
        .insert_resource(MultiplayerSettingsResource(multiplayer_settings.clone()))
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
    use crate::core::debug::WorldInspectorState;
    use crate::generator::GeneratorModule;
    use crate::graphic::GraphicModule;
    use crate::logic::LogicModule;
    use crate::utils::key_utils::convert;
    use bevy::light::DirectionalLightShadowMap;
    use bevy::prelude::*;
    use bevy_rapier3d::prelude::*;

    pub struct ManagerPlugin;

    impl Plugin for ManagerPlugin {
        fn build(&self, app: &mut App) {
            app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default());
            app.add_plugins((CoreModule, LogicModule, GeneratorModule, GraphicModule));
            app.add_systems(Startup, setup_shadow_map);
            app.add_systems(Update, toggle_world_inspector);
        }
    }

    fn setup_shadow_map(mut commands: Commands) {
        commands.insert_resource(DirectionalLightShadowMap { size: 1024 });
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
}

struct MultiplayerClientPlugin;

impl Plugin for MultiplayerClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_remote_player_visuals)
            .add_systems(
                Update,
                (
                    poll_lan_servers,
                    send_local_block_break_events,
                    send_local_block_place_events,
                    receive_multiplayer_messages,
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

fn poll_lan_servers(
    time: Res<Time>,
    settings: Res<MultiplayerSettingsResource>,
    mut discovery: NonSendMut<LanDiscoveryRuntime>,
    mut runtime: NonSendMut<MultiplayerClientRuntime>,
) {
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

        if runtime.auto_connect_lan
            && settings.0.client.connect_on_startup
            && runtime
                .client
                .as_ref()
                .is_none_or(|client| client.connection_status().is_disconnected())
        {
            runtime.connect(server.session_url.clone());
            runtime.auto_connect_lan = false;
        }
    }
}

fn receive_multiplayer_messages(
    mut commands: Commands,
    visuals: Res<RemotePlayerVisuals>,
    mut chunk_map: ResMut<ChunkMap>,
    mut fluids: ResMut<FluidMap>,
    mut ev_dirty: MessageWriter<SubChunkNeedRemeshEvent>,
    mut runtime: NonSendMut<MultiplayerClientRuntime>,
) {
    let Some(mut client) = runtime.client.take() else {
        return;
    };

    if client.connection_status().is_disconnected() {
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
            commands.entity(entity).despawn();
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

    for error in events.read::<ErrorEvent>() {
        error!("Multiplayer client error: {}", error);
    }

    if disconnect_received {
        for entity in runtime.remote_players.drain().map(|(_, entity)| entity) {
            commands.entity(entity).despawn();
        }
        runtime.local_player_id = None;
    }

    runtime.client = Some(client);
}

fn send_local_block_break_events(
    mut break_events: MessageReader<BlockBreakByPlayerEvent>,
    mut runtime: NonSendMut<MultiplayerClientRuntime>,
) {
    let Some(client) = runtime.client.as_mut() else {
        for _ in break_events.read() {}
        return;
    };

    if !client.connection_status().is_connected() {
        for _ in break_events.read() {}
        return;
    }

    for event in break_events.read() {
        client.send_message::<OrderedReliableChannel, _>(&ClientBlockBreak::new(
            event.location.to_array(),
        ));
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
