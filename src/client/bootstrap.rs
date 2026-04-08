use super::*;

/// Initializes bevy app for the `client` module.
pub(super) fn init_bevy_app(
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
                        title: String::from("Oplexa"),
                        mode: if config.graphics.fullscreen {
                            WindowMode::BorderlessFullscreen(MonitorSelection::Primary)
                        } else {
                            WindowMode::Windowed
                        },
                        resolution: WindowResolution::new(
                            config.graphics.window_width,
                            config.graphics.window_height,
                        ),
                        present_mode: if config.graphics.vsync {
                            PresentMode::AutoVsync
                        } else {
                            PresentMode::Mailbox
                        },
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
    .add_plugins(manager::ManagerPlugin)
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

/// Runs the `log_file_appender` routine for oak_log file appender in the `client` module.
fn log_file_appender(_app: &mut App) -> Option<BoxedLayer> {
    let log_dir = PathBuf::from("logs");
    if let Err(error) = std::fs::create_dir_all(&log_dir) {
        eprintln!("Failed to create oak_log directory: {}", error);
        return None;
    }

    let timestamp = Utc::now().format("bevy-%d-%m-%Y.oak_log").to_string();
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
            .expect("Failed to clone oak_log file handle");
        Box::new(file) as Box<dyn Write + Send>
    });

    Some(Box::new(
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(writer)
            .boxed(),
    ))
}

/// Loads oak_log env filter for the `client` module.
fn load_log_env_filter() -> String {
    dotenv().ok();
    let base = env::var("LOG_ENV_FILTER").unwrap_or_else(|_| "error".to_string());
    if base.contains("calloop::loop_logic") {
        base
    } else {
        format!("{base},calloop::loop_logic=error")
    }
}

/// Represents start oak_log text used by the `client` module.
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
