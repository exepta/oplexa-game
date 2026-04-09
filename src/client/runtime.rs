use super::*;

/// Represents client identity used by the `client` module.
#[derive(Clone, Debug)]
pub(super) struct ClientIdentity {
    pub(super) uuid: String,
    pub(super) player_name: String,
    pub(super) prod_mode: bool,
    pub(super) multi_instance: bool,
}

/// Runs the `resolve_client_identity` routine for resolve client identity in the `client` module.
pub(super) fn resolve_client_identity(settings: &mut NetworkSettings) -> ClientIdentity {
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
pub(super) fn parse_session_url(url: &str) -> Option<SocketAddr> {
    let stripped = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    stripped.parse().ok()
}

/// Runs the `remap_server_block_id` routine for remap server block id in the `client` module.
pub(super) fn remap_server_block_id(
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
pub(super) struct MultiplayerClientRuntime {
    pub(super) enabled: bool,
    pub(super) player_name: String,
    pub(super) session_url: String,
    pub(super) client_uuid: String,
    pub(super) auto_connect_lan: bool,
    pub(super) connection_entity: Option<Entity>,
    pub(super) local_player_id: Option<u64>,
    pub(super) player_names: HashMap<u64, String>,
    pub(super) remote_players: HashMap<u64, Entity>,
    pub(super) disconnected_remote_players: HashSet<u64>,
    pub(super) remote_player_smoothing: HashMap<u64, RemotePlayerSmoothing>,
    pub(super) next_local_drop_seq: u32,
    pub(super) disconnect_requested: bool,
    pub(super) keepalive_timer: Timer,
    pub(super) send_timer: Timer,
}

/// Represents terminal interrupt exit state used by the `client` module.
#[derive(Resource, Default)]
pub(super) struct TerminalInterruptExitState {
    pub(super) started_at: Option<f64>,
}

impl MultiplayerClientRuntime {
    /// Creates a new instance for the `client` module.
    pub(super) fn new(settings: &NetworkSettings, identity: ClientIdentity) -> Self {
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
pub(super) fn do_connect(
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
pub(super) fn do_disconnect(runtime: &mut MultiplayerClientRuntime, commands: &mut Commands) {
    if let Some(entity) = runtime.connection_entity.take() {
        runtime.disconnect_requested = true;
        commands.trigger_with(Disconnect { entity }, EntityTrigger);
    }
}

/// Represents lan discovery runtime used by the `client` module.
pub(super) struct LanDiscoveryRuntime {
    pub(super) client: Option<LanDiscoveryClient>,
    pub(super) known_servers: Vec<LanServerInfo>,
    pub(super) refresh_timer: Timer,
}

impl LanDiscoveryRuntime {
    /// Creates a new instance for the `client` module.
    pub(super) fn new(settings: &NetworkSettings) -> Self {
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

/// Starts streamed multiplayer world load for the `client` module.
pub(super) fn start_streamed_multiplayer_world_load(
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
