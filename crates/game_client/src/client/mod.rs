use crate::core::chat::{ChatLine, ChatLog};
use crate::core::commands::{
    CommandSender, EntitySender, GameModeKind, SystemMessageLevel, SystemSender,
    default_chat_command_registry, parse_chat_command,
};
use crate::core::config::{GlobalConfig, WorldGenConfig};
use crate::core::debug::{BuildInfo, ChunkDebugStats, WorldInspectorState};
use crate::core::entities::player::inventory::{PLAYER_INVENTORY_SLOTS, PlayerInventory};
use crate::core::entities::player::{FlightState, FpsController, GameMode, GameModeState, Player};
use crate::core::events::block::block_player_events::{
    BlockBreakByPlayerEvent, BlockBreakObservedEvent, BlockPlaceByPlayerEvent,
    BlockPlaceObservedEvent,
};
use crate::core::events::chunk_events::{
    SubChunkNeedColliderRefreshEvent, SubChunkNeedRemeshEvent,
};
use crate::core::events::ui_events::{
    ChatSubmitRequest, ChestInventoryContentsSync, ChestInventoryPersistRequest,
    ChestInventorySnapshotRequest, ChestInventoryUiOpened, ConnectToServerRequest,
    DisconnectFromServerRequest, DropItemRequest,
};
use crate::core::inventory::items::{ItemRegistry, build_world_item_drop_visual};
use crate::core::multiplayer::{MultiplayerConnectionPhase, MultiplayerConnectionState};
use crate::core::states::states::{AppState, BeforeUiState, LoadingStates};
use crate::core::world::biome::func::locate_biome_chunk_by_localized_name;
use crate::core::world::biome::registry::BiomeRegistry;
use crate::core::world::block::{BlockRegistry, VOXEL_SIZE, get_block_world};
use crate::core::world::chunk::{ChunkData, ChunkMap, LoadCenter, SEA_LEVEL, VoxelStage};
use crate::core::world::chunk_dimension::{
    CX, CZ, SEC_COUNT, SEC_H, Y_MAX, Y_MIN, world_to_chunk_xz, world_y_to_local,
};
use crate::core::world::fluid::{FluidChunk, FluidMap, WaterMeshIndex};
use crate::core::world::save::RegionCache;
use crate::core::world::{mark_dirty_block_and_neighbors, world_access_mut};
use crate::generator::chunk::chunk_meshing::{decode_chunk, safe_despawn_entity};
use crate::generator::chunk::chunk_runtime_types::{MeshBacklog, PendingMesh};
use crate::integrated_server::IntegratedServerSession;
use crate::logic::events::block_event_handler::{
    MultiplayerStructureReconcileQueue, StructureRuntimeState,
};
use api::core::network::{
    config::NetworkSettings,
    discovery::{LanDiscoveryClient, LanServerInfo},
    protocols::{
        Auth, ClientBlockBreak, ClientBlockPlace, ClientChatMessage, ClientChestInventoryOpen,
        ClientChestInventoryPersist, ClientChunkInterest, ClientDropItem, ClientDropPickup,
        ClientInventorySync, ClientKeepAlive, OrderedReliable, PlayerJoined, PlayerLeft,
        PlayerMove, PlayerSnapshot, ProtocolPlugin, ServerAuthRejected, ServerBlockBreak,
        ServerBlockPlace, ServerChatMessage, ServerChestInventoryContents, ServerChunkData,
        ServerDropPicked, ServerDropSpawn, ServerGameModeChanged, ServerTeleport, ServerWelcome,
        UnorderedReliable, UnorderedUnreliable,
    },
};
use bevy::ecs::event::EntityTrigger;
use bevy::ecs::system::SystemParam;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSamplerDescriptor};
use bevy::log::{BoxedLayer, Level, LogPlugin};
use bevy::math::primitives::Capsule3d;
use bevy::mesh::Mesh3d;
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::{Backends, RenderCreation, WgpuSettings};
use bevy::tasks::futures_lite::future;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use bevy::window::{PresentMode, WindowCloseRequested, WindowMode, WindowResolution};
use bevy_inspector_egui::bevy_egui::EguiPlugin;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use chrono::Utc;
use dotenvy::dotenv;
use lightyear::prelude::client::{
    ClientConfig, ClientPlugins, Connect, Connected, Disconnect, Disconnected, NetcodeClient,
    NetcodeConfig, WebSocketClientIo, WebSocketScheme,
};
use lightyear::prelude::{Authentication, MessageReceiver, MessageSender, PeerAddr, Unlink};
use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tracing_subscriber::Layer;
use tracing_subscriber::fmt::writer::BoxMakeWriter;

mod bootstrap;
mod chunk_debug_grid;
pub(crate) mod manager;
mod runtime;

use bootstrap::init_bevy_app;
use runtime::*;

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
    block_visual: bool,
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
    progressive_radius: Option<i32>,
    next_radius_step_at: f32,
}

impl RemoteChunkStreamState {
    /// Runs the `reset` routine for reset in the `client` module.
    fn reset(&mut self) {
        self.last_requested_center = None;
        self.last_requested_radius = None;
        self.progressive_radius = None;
        self.next_radius_step_at = 0.0;
    }
}

/// Represents remote chunk decode queue used by the `client` module.
#[derive(Resource, Default)]
struct RemoteChunkDecodeQueue {
    queued_order: VecDeque<[i32; 2]>,
    queued_by_coord: HashMap<[i32; 2], ServerChunkData>,
}

impl RemoteChunkDecodeQueue {
    /// Runs the `reset` routine for reset in the `client` module.
    fn reset(&mut self) {
        self.queued_order.clear();
        self.queued_by_coord.clear();
    }

    #[inline]
    fn enqueue(&mut self, message: ServerChunkData) {
        let coord = message.coord;
        if !self.queued_by_coord.contains_key(&coord) {
            self.queued_order.push_back(coord);
        }
        self.queued_by_coord.insert(coord, message);
    }

    #[inline]
    fn pop_front(&mut self) -> Option<ServerChunkData> {
        while let Some(coord) = self.queued_order.pop_front() {
            if let Some(message) = self.queued_by_coord.remove(&coord) {
                return Some(message);
            }
        }
        None
    }

    #[inline]
    fn len(&self) -> usize {
        self.queued_by_coord.len()
    }
}

/// Represents decoded chunk remesh queue used by the `client` module.
#[derive(Resource, Default)]
struct RemoteChunkRemeshQueue {
    queued: VecDeque<IVec2>,
    queued_set: HashSet<IVec2>,
}

impl RemoteChunkRemeshQueue {
    /// Runs the `reset` routine for reset in the `client` module.
    fn reset(&mut self) {
        self.queued.clear();
        self.queued_set.clear();
    }

    #[inline]
    fn enqueue(&mut self, coord: IVec2) {
        if self.queued_set.insert(coord) {
            self.queued.push_back(coord);
        }
    }

    #[inline]
    fn enqueue_front(&mut self, coord: IVec2) {
        if self.queued_set.insert(coord) {
            self.queued.push_front(coord);
            return;
        }

        if let Some(index) = self.queued.iter().position(|queued| *queued == coord) {
            self.queued.remove(index);
            self.queued.push_front(coord);
        }
    }

    #[inline]
    fn pop_front(&mut self) -> Option<IVec2> {
        let coord = self.queued.pop_front()?;
        self.queued_set.remove(&coord);
        Some(coord)
    }

    #[inline]
    fn len(&self) -> usize {
        self.queued.len()
    }
}

type RemoteDecodedChunk = (IVec2, ChunkData);

/// Represents remote chunk decode tasks used by the `client` module.
#[derive(Resource, Default)]
struct RemoteChunkDecodeTasks {
    tasks: HashMap<[i32; 2], Task<Option<RemoteDecodedChunk>>>,
}

impl RemoteChunkDecodeTasks {
    #[inline]
    fn reset(&mut self) {
        self.tasks.clear();
    }

    #[inline]
    fn len(&self) -> usize {
        self.tasks.len()
    }
}

/// Represents decoded chunk ready queue used by the `client` module.
#[derive(Resource, Default)]
struct RemoteChunkDecodedQueue {
    queued_order: VecDeque<[i32; 2]>,
    queued_by_coord: HashMap<[i32; 2], ChunkData>,
}

impl RemoteChunkDecodedQueue {
    #[inline]
    fn reset(&mut self) {
        self.queued_order.clear();
        self.queued_by_coord.clear();
    }

    #[inline]
    fn enqueue(&mut self, coord: IVec2, chunk: ChunkData) {
        let key = [coord.x, coord.y];
        if !self.queued_by_coord.contains_key(&key) {
            self.queued_order.push_back(key);
        }
        self.queued_by_coord.insert(key, chunk);
    }

    #[inline]
    fn pop_front(&mut self) -> Option<RemoteDecodedChunk> {
        while let Some(coord) = self.queued_order.pop_front() {
            if let Some(chunk) = self.queued_by_coord.remove(&coord) {
                return Some((IVec2::new(coord[0], coord[1]), chunk));
            }
        }
        None
    }

    #[inline]
    fn len(&self) -> usize {
        self.queued_by_coord.len()
    }
}

#[derive(SystemParam)]
struct RemoteChunkFlowState<'w> {
    chunk_stream: ResMut<'w, RemoteChunkStreamState>,
    chunk_decode_queue: ResMut<'w, RemoteChunkDecodeQueue>,
    chunk_decode_tasks: ResMut<'w, RemoteChunkDecodeTasks>,
    chunk_decoded_queue: ResMut<'w, RemoteChunkDecodedQueue>,
    chunk_remesh_queue: ResMut<'w, RemoteChunkRemeshQueue>,
}

#[derive(SystemParam)]
struct ObservedWaterFlowEventWriters<'w> {
    break_events: MessageWriter<'w, BlockBreakObservedEvent>,
    place_events: MessageWriter<'w, BlockPlaceObservedEvent>,
    collider_refresh: MessageWriter<'w, SubChunkNeedColliderRefreshEvent>,
}

impl<'w> RemoteChunkFlowState<'w> {
    #[inline]
    fn reset(&mut self) {
        self.chunk_stream.reset();
        self.chunk_decode_queue.reset();
        self.chunk_decode_tasks.reset();
        self.chunk_decoded_queue.reset();
        self.chunk_remesh_queue.reset();
    }
}

/// Represents block id remap used by the `client` module.
#[derive(Resource, Default)]
struct BlockIdRemap {
    server_to_local: Vec<u16>,
    local_to_server: Vec<u16>,
    ready: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct PendingWorldAck {
    kind: PendingWorldAckKind,
    location: [i32; 3],
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum PendingWorldAckKind {
    Break,
    Place,
}

#[derive(Resource, Default)]
struct PendingWorldAckState {
    entries: HashSet<PendingWorldAck>,
}

#[derive(Resource, Default)]
struct DeferredDisconnectState {
    active: bool,
    deadline_secs: f32,
}

#[derive(Clone, Copy, Debug)]
enum LocalWorldEditKind {
    Break,
    Place {
        block_id: u16,
        stacked_block_id: u16,
    },
}

#[derive(Clone, Copy, Debug)]
struct LocalWorldEditOverlay {
    location: [i32; 3],
    kind: LocalWorldEditKind,
    expires_at_secs: f32,
}

#[derive(Resource, Default)]
struct LocalWorldEditOverlayState {
    entries: Vec<LocalWorldEditOverlay>,
}

#[derive(Resource, Default)]
struct RecentAuthoritativeEditGuards {
    by_chunk_until_secs: HashMap<IVec2, f32>,
}

#[derive(SystemParam)]
struct MultiplayerWorldReceiveState<'w> {
    runtime: Res<'w, MultiplayerClientRuntime>,
    pending_world_acks: ResMut<'w, PendingWorldAckState>,
    structure_runtime: Option<ResMut<'w, StructureRuntimeState>>,
    structure_reconcile_queue: Option<ResMut<'w, MultiplayerStructureReconcileQueue>>,
    local_world_edits: ResMut<'w, LocalWorldEditOverlayState>,
    recent_edit_guards: ResMut<'w, RecentAuthoritativeEditGuards>,
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
            let local_id = resolve_local_block_id(registry, block_name).unwrap_or(0);
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

fn resolve_local_block_id(registry: &BlockRegistry, server_block_name: &str) -> Option<u16> {
    if let Some(id) = registry.id_opt(server_block_name) {
        return Some(id);
    }

    registry
        .defs
        .iter()
        .position(|def| {
            def.localized_name.eq_ignore_ascii_case(server_block_name)
                || def.name.eq_ignore_ascii_case(server_block_name)
        })
        .map(|index| index as u16)
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
const MULTIPLAYER_CHUNK_INTEREST_BOOTSTRAP_RADIUS: i32 = 64;
const MULTIPLAYER_CHUNK_INTEREST_STEP_INTERVAL_SECS: f32 = 0.01;
const LOCAL_WORLD_EDIT_OVERLAY_TTL_SECS: f32 = 8.0;
const RECENT_EDIT_CHUNK_GUARD_SECS: f32 = 1.25;
const MULTIPLAYER_CHUNK_DECODES_PER_FRAME_BASE: usize = 3;
const MULTIPLAYER_CHUNK_DECODES_PER_FRAME_MAX: usize = 16;
const MULTIPLAYER_CHUNK_DECODE_TASKS_INFLIGHT_MAX: usize = 64;
const MULTIPLAYER_CHUNK_APPLY_PER_FRAME_BASE: usize = 3;
const MULTIPLAYER_CHUNK_APPLY_PER_FRAME_MAX: usize = 12;
const MULTIPLAYER_CHUNK_REMESH_COORDS_PER_FRAME_BASE: usize = 2;
const MULTIPLAYER_CHUNK_REMESH_COORDS_PER_FRAME_MAX: usize = 6;
const LOCATE_MAX_RADIUS_BLOCKS_CAP: i32 = 1000;
const NETWORK_CONFIG_PATH: &str = "config/network.toml";
const SERVER_TIMEOUT_ERROR_TEXT: &str = "Server time out!";

static TERMINAL_INTERRUPT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Converts locate radius from blocks into chunks and clamps it to safe bounds.
fn locate_radius_chunks_from_blocks(radius_blocks: i32) -> i32 {
    let clamped_blocks = radius_blocks.clamp(1, LOCATE_MAX_RADIUS_BLOCKS_CAP);
    let chunk_span = (CX as i32).max(CZ as i32);
    (clamped_blocks + (chunk_span - 1)) / chunk_span
}

/// Dynamic decode budget for streamed multiplayer chunks.
fn multiplayer_chunk_decode_budget(
    backlog_len: usize,
    mesh_pressure: usize,
    frame_secs: f32,
) -> usize {
    let mut budget = MULTIPLAYER_CHUNK_DECODES_PER_FRAME_BASE;
    if backlog_len >= 32 {
        budget = 13;
    }
    if backlog_len >= 96 {
        budget = 18;
    }
    if backlog_len >= 192 {
        budget = 26;
    }
    if backlog_len >= 384 {
        budget = MULTIPLAYER_CHUNK_DECODES_PER_FRAME_MAX;
    }

    if frame_secs > 0.060 {
        budget = budget.min(8);
    } else if frame_secs > 0.045 {
        budget = budget.min(13);
    } else if frame_secs > 0.034 {
        budget = budget.min(18);
    }

    if mesh_pressure >= 1024 {
        budget = budget.min(8);
    } else if mesh_pressure >= 768 {
        budget = budget.min(10);
    } else if mesh_pressure >= 512 {
        budget = budget.min(13);
    }

    budget.clamp(
        MULTIPLAYER_CHUNK_DECODES_PER_FRAME_BASE,
        MULTIPLAYER_CHUNK_DECODES_PER_FRAME_MAX,
    )
}

/// Dynamic budget for how many decoded chunk coordinates are remeshed per frame.
fn multiplayer_chunk_remesh_coord_budget(
    backlog_len: usize,
    mesh_pressure: usize,
    frame_secs: f32,
) -> usize {
    let mut budget = MULTIPLAYER_CHUNK_REMESH_COORDS_PER_FRAME_BASE;
    if backlog_len >= 16 {
        budget = 5;
    }
    if backlog_len >= 48 {
        budget = 8;
    }
    if backlog_len >= 96 {
        budget = 12;
    }
    if backlog_len >= 192 {
        budget = MULTIPLAYER_CHUNK_REMESH_COORDS_PER_FRAME_MAX;
    }

    if frame_secs > 0.060 {
        budget = budget.min(4);
    } else if frame_secs > 0.045 {
        budget = budget.min(5);
    } else if frame_secs > 0.034 {
        budget = budget.min(8);
    }

    if mesh_pressure >= 1024 {
        budget = budget.min(4);
    } else if mesh_pressure >= 768 {
        budget = budget.min(5);
    } else if mesh_pressure >= 512 {
        budget = budget.min(8);
    }

    budget.clamp(
        MULTIPLAYER_CHUNK_REMESH_COORDS_PER_FRAME_BASE,
        MULTIPLAYER_CHUNK_REMESH_COORDS_PER_FRAME_MAX,
    )
}

/// Dynamic budget for how many already-decoded chunks are applied per frame.
fn multiplayer_chunk_apply_budget(
    backlog_len: usize,
    mesh_pressure: usize,
    frame_secs: f32,
) -> usize {
    let mut budget = MULTIPLAYER_CHUNK_APPLY_PER_FRAME_BASE;
    if backlog_len >= 16 {
        budget = 13;
    }
    if backlog_len >= 48 {
        budget = 18;
    }
    if backlog_len >= 96 {
        budget = 26;
    }
    if backlog_len >= 192 {
        budget = MULTIPLAYER_CHUNK_APPLY_PER_FRAME_MAX;
    }

    if frame_secs > 0.060 {
        budget = budget.min(8);
    } else if frame_secs > 0.045 {
        budget = budget.min(13);
    } else if frame_secs > 0.034 {
        budget = budget.min(18);
    }

    if mesh_pressure >= 1024 {
        budget = budget.min(8);
    } else if mesh_pressure >= 768 {
        budget = budget.min(10);
    } else if mesh_pressure >= 512 {
        budget = budget.min(13);
    }

    budget.clamp(
        MULTIPLAYER_CHUNK_APPLY_PER_FRAME_BASE,
        MULTIPLAYER_CHUNK_APPLY_PER_FRAME_MAX,
    )
}

/// Dynamic cap for concurrently running chunk decode tasks.
fn multiplayer_chunk_decode_task_inflight_cap(mesh_pressure: usize, frame_secs: f32) -> usize {
    let mut cap = MULTIPLAYER_CHUNK_DECODE_TASKS_INFLIGHT_MAX;

    if frame_secs > 0.060 {
        cap = cap.min(20);
    } else if frame_secs > 0.045 {
        cap = cap.min(30);
    } else if frame_secs > 0.034 {
        cap = cap.min(42);
    }

    if mesh_pressure >= 1024 {
        cap = cap.min(20);
    } else if mesh_pressure >= 768 {
        cap = cap.min(26);
    } else if mesh_pressure >= 512 {
        cap = cap.min(36);
    }

    cap.clamp(12, MULTIPLAYER_CHUNK_DECODE_TASKS_INFLIGHT_MAX)
}

fn remap_server_chunk_payload_raw(
    chunk: &mut crate::core::world::chunk::ChunkData,
    server_to_local: &[u16],
    local_block_registry_len: usize,
) {
    let remap = |server_id: u16| -> u16 {
        let local_id = if server_to_local.is_empty() {
            server_id
        } else {
            server_to_local
                .get(server_id as usize)
                .copied()
                .unwrap_or(0)
        };
        if (local_id as usize) < local_block_registry_len {
            local_id
        } else {
            0
        }
    };

    for server_id in &mut chunk.blocks {
        let local_id = remap(*server_id);
        *server_id = local_id;
    }

    for server_id in &mut chunk.stacked_blocks {
        *server_id = remap(*server_id);
    }
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

/// Represents multiplayer client plugin used by the `client` module.
struct MultiplayerClientPlugin;

impl Plugin for MultiplayerClientPlugin {
    /// Builds this component for the `client` module.
    fn build(&self, app: &mut App) {
        app.init_resource::<MultiplayerDropIndex>()
            .init_resource::<RemoteChunkStreamState>()
            .init_resource::<RemoteChunkDecodeQueue>()
            .init_resource::<RemoteChunkDecodeTasks>()
            .init_resource::<RemoteChunkDecodedQueue>()
            .init_resource::<RemoteChunkRemeshQueue>()
            .init_resource::<BlockIdRemap>()
            .init_resource::<PendingWorldAckState>()
            .init_resource::<DeferredDisconnectState>()
            .init_resource::<LocalWorldEditOverlayState>()
            .init_resource::<RecentAuthoritativeEditGuards>()
            .init_resource::<TerminalInterruptExitState>()
            .add_systems(First, handle_window_close_disconnect)
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
                    handle_chat_submit_requests,
                    send_local_block_break_events,
                    send_local_block_place_events,
                    send_chest_inventory_snapshot_requests,
                    send_open_chest_inventory_requests,
                    send_persist_chest_inventory_requests,
                    send_local_item_drop_requests,
                    send_client_keepalive,
                    process_deferred_disconnect_requests,
                    cleanup_stale_client_link_entities,
                ),
            )
            .add_systems(
                Update,
                (
                    receive_player_messages,
                    receive_chat_messages,
                    receive_drop_messages,
                    receive_chest_inventory_messages,
                    update_connection_state,
                    simulate_multiplayer_drop_items,
                    send_local_drop_pickup_requests,
                    send_chunk_interest_updates,
                    send_local_player_pose,
                ),
            )
            .add_systems(
                Update,
                (receive_world_messages, flush_remote_chunk_remesh_queue)
                    .chain()
                    .in_set(VoxelStage::WorldEdit),
            )
            .add_systems(Update, send_local_inventory_sync)
            .add_systems(Update, smooth_remote_players);
    }
}

/// Handles terminal interrupt exit for the `client` module.
fn handle_terminal_interrupt_exit(
    mut interrupt_state: ResMut<TerminalInterruptExitState>,
    mut runtime: ResMut<MultiplayerClientRuntime>,
    mut integrated_server: ResMut<IntegratedServerSession>,
    mut block_remap: ResMut<BlockIdRemap>,
    mut multiplayer_connection: ResMut<MultiplayerConnectionState>,
    mut chunk_stream: ResMut<RemoteChunkStreamState>,
    mut chunk_decode_queue: ResMut<RemoteChunkDecodeQueue>,
    mut chunk_decode_tasks: ResMut<RemoteChunkDecodeTasks>,
    mut chunk_decoded_queue: ResMut<RemoteChunkDecodedQueue>,
    mut chunk_remesh_queue: ResMut<RemoteChunkRemeshQueue>,
    mut commands: Commands,
    mut app_exit: MessageWriter<AppExit>,
) {
    if TERMINAL_INTERRUPT_REQUESTED.load(Ordering::SeqCst) && !interrupt_state.handled {
        integrated_server.shutdown_blocking();

        let connection_entity = runtime.connection_entity;
        let should_send_disconnect = connection_entity.is_some();
        if should_send_disconnect {
            info!("Ctrl+C detected. Sending multiplayer disconnect before shutdown...");
            do_disconnect(&mut runtime, &mut commands);
            if let Some(entity) = connection_entity {
                commands.trigger_with(
                    Unlink {
                        entity,
                        reason: "Client shutdown".to_string(),
                    },
                    EntityTrigger,
                );
                safe_despawn_entity(&mut commands, entity);
            }
        } else {
            info!("Ctrl+C detected. Shutting down local session...");
        }
        runtime.local_player_id = None;
        runtime.player_names.clear();
        runtime.remote_players.clear();
        runtime.disconnected_remote_players.clear();
        runtime.remote_player_smoothing.clear();
        block_remap.reset();
        chunk_stream.reset();
        chunk_decode_queue.reset();
        chunk_decode_tasks.reset();
        chunk_decoded_queue.reset();
        chunk_remesh_queue.reset();
        multiplayer_connection.clear_session();
        interrupt_state.handled = true;
        app_exit.write(AppExit::Success);
    }
}

/// Handles OS window close requests early so network links are gone before lightyear send runs.
fn handle_window_close_disconnect(
    mut close_requests: MessageReader<WindowCloseRequested>,
    mut runtime: ResMut<MultiplayerClientRuntime>,
    mut integrated_server: ResMut<IntegratedServerSession>,
    q_links: Query<Entity, With<NetcodeClient>>,
    mut commands: Commands,
) {
    if close_requests.read().next().is_none() {
        return;
    }

    integrated_server.shutdown_blocking();

    if runtime.connection_entity.is_some() {
        do_disconnect(&mut runtime, &mut commands);
    }

    for entity in q_links.iter() {
        commands.trigger_with(
            Unlink {
                entity,
                reason: "Window close requested".to_string(),
            },
            EntityTrigger,
        );
        safe_despawn_entity(&mut commands, entity);
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
    mut integrated_server: ResMut<IntegratedServerSession>,
    mut runtime: ResMut<MultiplayerClientRuntime>,
    mut block_remap: ResMut<BlockIdRemap>,
    mut multiplayer_connection: ResMut<MultiplayerConnectionState>,
    mut chunk_stream: ResMut<RemoteChunkStreamState>,
    mut chunk_decode_queue: ResMut<RemoteChunkDecodeQueue>,
    mut chunk_decode_tasks: ResMut<RemoteChunkDecodeTasks>,
    mut chunk_decoded_queue: ResMut<RemoteChunkDecodedQueue>,
    mut chunk_remesh_queue: ResMut<RemoteChunkRemeshQueue>,
    mut chat_log: ResMut<ChatLog>,
    mut drops: ResMut<MultiplayerDropIndex>,
    mut next_state: ResMut<NextState<AppState>>,
    mut chunk_map: ResMut<ChunkMap>,
    mut commands: Commands,
) {
    if Some(trigger.entity) != runtime.connection_entity {
        return;
    }

    let integrated_session_active = integrated_server.is_active();
    // NetcodeClient has #[require(Disconnected)], so Disconnected is added on spawn
    // with reason: None. Real disconnects always have reason: Some(...). Skip the
    // initial spawn-time Disconnected so we don't immediately despawn the entity.
    let disconnect_reason = q_disconnected
        .get(trigger.entity)
        .ok()
        .and_then(|disconnected| {
            if disconnected.reason.is_none() {
                return None;
            }
            disconnected.reason.clone()
        });
    if disconnect_reason.is_none() {
        return;
    }

    // Ensure the underlying IO link is closed immediately. Without this, an integrated
    // Crossbeam link can stay in `Linked` state for a short time and spam "channel is
    // disconnected" logs every frame.
    commands.trigger_with(
        Unlink {
            entity: trigger.entity,
            reason: "Client disconnected".to_string(),
        },
        EntityTrigger,
    );

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
    chunk_stream.reset();
    chunk_decode_queue.reset();
    chunk_decode_tasks.reset();
    chunk_decoded_queue.reset();
    chunk_remesh_queue.reset();
    integrated_server.shutdown();

    // If we disconnect while the world is loading or in-game, reset to the menu.
    // Without this, check_base_gen_world_ready would see uses_local_save_data()=true
    // (session URL cleared below) and send the client to WaterGen/local generation,
    // which crashes because no local world resources are set up.
    let disconnected_by_request = runtime.disconnect_requested;
    match app_state.get() {
        AppState::Loading(_) | AppState::InGame(_) | AppState::PostLoad
            if !disconnected_by_request =>
        {
            chunk_map.chunks.clear();
            next_state.set(if integrated_session_active {
                AppState::Screen(BeforeUiState::Menu)
            } else {
                AppState::Screen(BeforeUiState::MultiPlayer)
            });
        }
        _ => {}
    }

    let existing_error = multiplayer_connection.last_error.clone();
    runtime.disconnect_requested = false;
    multiplayer_connection.clear_session();
    multiplayer_connection.last_error = if disconnected_by_request {
        existing_error
    } else {
        existing_error
            .or(disconnect_reason)
            .or_else(|| Some(SERVER_TIMEOUT_ERROR_TEXT.to_string()))
    };

    runtime.connection_entity = None;
}

/// Cleans up stale client link entities that no longer belong to the active runtime session.
fn cleanup_stale_client_link_entities(
    runtime: Res<MultiplayerClientRuntime>,
    q_links: Query<Entity, With<NetcodeClient>>,
    mut commands: Commands,
) {
    for entity in q_links.iter() {
        if Some(entity) == runtime.connection_entity {
            continue;
        }

        commands.trigger_with(
            Unlink {
                entity,
                reason: "Cleanup stale client link".to_string(),
            },
            EntityTrigger,
        );
        safe_despawn_entity(&mut commands, entity);
    }
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
    mut chunk_decode_queue: ResMut<RemoteChunkDecodeQueue>,
    mut chunk_decode_tasks: ResMut<RemoteChunkDecodeTasks>,
    mut chunk_decoded_queue: ResMut<RemoteChunkDecodedQueue>,
    mut chunk_remesh_queue: ResMut<RemoteChunkRemeshQueue>,
    mut block_remap: ResMut<BlockIdRemap>,
    q_active: Query<
        (),
        Or<(
            With<Connected>,
            With<lightyear::prelude::client::Connecting>,
        )>,
    >,
    #[cfg(feature = "integrated")] mut integrated_server: ResMut<IntegratedServerSession>,
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
    let session_url = session_url.to_string();
    #[cfg(feature = "integrated")]
    if session_url == oplexa_game_server::INTEGRATED_SESSION_URL {
        let Some(integrated_client_io) = integrated_server.take_client_io() else {
            warn!(
                "Connect request for integrated session ignored because no in-memory channel was prepared."
            );
            return;
        };
        do_connect_integrated(
            &mut runtime,
            session_url.clone(),
            integrated_client_io,
            &mut commands,
        );
    } else {
        do_connect(&mut runtime, session_url.clone(), &mut commands);
    }

    #[cfg(not(feature = "integrated"))]
    do_connect(&mut runtime, session_url.clone(), &mut commands);

    runtime.auto_connect_lan = false;
    multiplayer_connection.connected = false;
    multiplayer_connection.phase = MultiplayerConnectionPhase::Connecting;
    multiplayer_connection.set_world_data_mode_remote();
    multiplayer_connection.active_session_url = Some(session_url.clone());
    multiplayer_connection.server_name = if request.server_name.trim().is_empty() {
        None
    } else {
        Some(request.server_name.trim().to_string())
    };
    multiplayer_connection.world_name = None;
    multiplayer_connection.world_seed = None;
    multiplayer_connection.spawn_translation = None;
    multiplayer_connection.spawn_yaw_pitch = None;
    multiplayer_connection.known_player_names.clear();
    multiplayer_connection.last_error = None;
    chunk_stream.reset();
    chunk_decode_queue.reset();
    chunk_decode_tasks.reset();
    chunk_decoded_queue.reset();
    chunk_remesh_queue.reset();
}

/// Runs the `disconnect_from_server_requested` routine for disconnect from server requested in the `client` module.
fn disconnect_from_server_requested(
    time: Res<Time>,
    mut disconnect_requests: MessageReader<DisconnectFromServerRequest>,
    mut deferred_disconnect: ResMut<DeferredDisconnectState>,
    mut pending_world_acks: ResMut<PendingWorldAckState>,
    mut local_world_edits: ResMut<LocalWorldEditOverlayState>,
    mut multiplayer_connection: ResMut<MultiplayerConnectionState>,
    mut chunk_stream: ResMut<RemoteChunkStreamState>,
    mut chunk_decode_queue: ResMut<RemoteChunkDecodeQueue>,
    mut chunk_decode_tasks: ResMut<RemoteChunkDecodeTasks>,
    mut chunk_decoded_queue: ResMut<RemoteChunkDecodedQueue>,
    mut chunk_remesh_queue: ResMut<RemoteChunkRemeshQueue>,
    mut block_remap: ResMut<BlockIdRemap>,
    mut runtime: ResMut<MultiplayerClientRuntime>,
    mut integrated_server: ResMut<IntegratedServerSession>,
    mut commands: Commands,
) {
    if disconnect_requests.read().next().is_none() {
        return;
    }

    let should_wait_for_acks = runtime.connection_entity.is_some()
        && multiplayer_connection.connected
        && !pending_world_acks.entries.is_empty();
    if should_wait_for_acks {
        deferred_disconnect.active = true;
        deferred_disconnect.deadline_secs = time.elapsed_secs() + 0.75;
        return;
    }

    finalize_disconnect_cleanup(
        &mut runtime,
        &mut integrated_server,
        &mut pending_world_acks,
        &mut local_world_edits,
        &mut deferred_disconnect,
        &mut block_remap,
        &mut multiplayer_connection,
        &mut chunk_stream,
        &mut chunk_decode_queue,
        &mut chunk_decode_tasks,
        &mut chunk_decoded_queue,
        &mut chunk_remesh_queue,
        &mut commands,
    );
}

fn process_deferred_disconnect_requests(
    time: Res<Time>,
    mut deferred_disconnect: ResMut<DeferredDisconnectState>,
    mut pending_world_acks: ResMut<PendingWorldAckState>,
    mut local_world_edits: ResMut<LocalWorldEditOverlayState>,
    mut multiplayer_connection: ResMut<MultiplayerConnectionState>,
    mut chunk_stream: ResMut<RemoteChunkStreamState>,
    mut chunk_decode_queue: ResMut<RemoteChunkDecodeQueue>,
    mut chunk_decode_tasks: ResMut<RemoteChunkDecodeTasks>,
    mut chunk_decoded_queue: ResMut<RemoteChunkDecodedQueue>,
    mut chunk_remesh_queue: ResMut<RemoteChunkRemeshQueue>,
    mut block_remap: ResMut<BlockIdRemap>,
    mut runtime: ResMut<MultiplayerClientRuntime>,
    mut integrated_server: ResMut<IntegratedServerSession>,
    mut commands: Commands,
) {
    if !deferred_disconnect.active {
        return;
    }
    if !pending_world_acks.entries.is_empty()
        && time.elapsed_secs() < deferred_disconnect.deadline_secs
    {
        return;
    }

    deferred_disconnect.active = false;
    finalize_disconnect_cleanup(
        &mut runtime,
        &mut integrated_server,
        &mut pending_world_acks,
        &mut local_world_edits,
        &mut deferred_disconnect,
        &mut block_remap,
        &mut multiplayer_connection,
        &mut chunk_stream,
        &mut chunk_decode_queue,
        &mut chunk_decode_tasks,
        &mut chunk_decoded_queue,
        &mut chunk_remesh_queue,
        &mut commands,
    );
}

fn finalize_disconnect_cleanup(
    runtime: &mut MultiplayerClientRuntime,
    integrated_server: &mut IntegratedServerSession,
    pending_world_acks: &mut PendingWorldAckState,
    local_world_edits: &mut LocalWorldEditOverlayState,
    deferred_disconnect: &mut DeferredDisconnectState,
    block_remap: &mut BlockIdRemap,
    multiplayer_connection: &mut MultiplayerConnectionState,
    chunk_stream: &mut RemoteChunkStreamState,
    chunk_decode_queue: &mut RemoteChunkDecodeQueue,
    chunk_decode_tasks: &mut RemoteChunkDecodeTasks,
    chunk_decoded_queue: &mut RemoteChunkDecodedQueue,
    chunk_remesh_queue: &mut RemoteChunkRemeshQueue,
    commands: &mut Commands,
) {
    if let Some(entity) = runtime.connection_entity {
        do_disconnect(runtime, commands);
        commands.trigger_with(
            Unlink {
                entity,
                reason: "Disconnect requested".to_string(),
            },
            EntityTrigger,
        );
        safe_despawn_entity(commands, entity);
        runtime.connection_entity = None;
    }

    integrated_server.shutdown();

    pending_world_acks.entries.clear();
    local_world_edits.entries.clear();
    deferred_disconnect.active = false;
    deferred_disconnect.deadline_secs = 0.0;
    block_remap.reset();
    multiplayer_connection.clear_session();
    chunk_stream.reset();
    chunk_decode_queue.reset();
    chunk_decode_tasks.reset();
    chunk_decoded_queue.reset();
    chunk_remesh_queue.reset();
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
    visuals: Res<RemotePlayerVisuals>,
    registry: Option<Res<BlockRegistry>>,
    mut region_cache: ResMut<RegionCache>,
    mut chunk_map: ResMut<ChunkMap>,
    mut fluids: ResMut<FluidMap>,
    mut water_mesh_index: ResMut<WaterMeshIndex>,
    mut next_state: ResMut<NextState<AppState>>,
    mut multiplayer_connection: ResMut<MultiplayerConnectionState>,
    mut inventory: ResMut<PlayerInventory>,
    mut chunk_flow: RemoteChunkFlowState,
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
        chunk_flow.reset();

        chunk_map.chunks.clear();
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
        multiplayer_connection.spawn_yaw_pitch = Some(message.spawn_yaw_pitch);
        apply_welcome_inventory(&message, &mut inventory);
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
        chunk_flow.reset();
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

#[inline]
fn apply_welcome_inventory(message: &ServerWelcome, inventory: &mut PlayerInventory) {
    for slot in &mut inventory.slots {
        *slot = Default::default();
    }

    for (index, slot) in message
        .inventory_slots
        .iter()
        .copied()
        .take(PLAYER_INVENTORY_SLOTS)
        .enumerate()
    {
        inventory.slots[index] = slot.into();
    }
}

/// Handles server chat lines and game mode synchronization.
fn receive_chat_messages(
    runtime: Res<MultiplayerClientRuntime>,
    mut chat_log: ResMut<ChatLog>,
    mut game_mode: ResMut<GameModeState>,
    mut flight_state: Query<&mut FlightState>,
    mut q_player: Query<&mut Transform, With<Player>>,
    mut q: Query<(
        &mut MessageReceiver<ServerChatMessage>,
        &mut MessageReceiver<ServerGameModeChanged>,
        &mut MessageReceiver<ServerTeleport>,
    )>,
) {
    let Some(entity) = runtime.connection_entity else {
        return;
    };

    let Ok((mut recv_chat, mut recv_game_mode, mut recv_teleport)) = q.get_mut(entity) else {
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

    for message in recv_teleport.receive() {
        if Some(message.player_id) != runtime.local_player_id {
            continue;
        }
        if let Ok(mut player_transform) = q_player.single_mut() {
            player_transform.translation = Vec3::from_array(message.translation);
        }
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
    global_config: Res<GlobalConfig>,
    world_gen_config: Res<WorldGenConfig>,
    biomes: Res<BiomeRegistry>,
    mut q_player: Query<&mut Transform, With<Player>>,
) {
    let locate_max_radius_chunks =
        locate_radius_chunks_from_blocks(global_config.interface.locate_search_radius);

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
                "locate" => {
                    let Some(raw_type) = command.args.first() else {
                        push_system_chat(
                            &mut chat_log,
                            SystemMessageLevel::Warn,
                            "Usage: /locate <biome> <name:key>".to_string(),
                        );
                        continue;
                    };
                    let Some(raw_target) = command.args.get(1) else {
                        push_system_chat(
                            &mut chat_log,
                            SystemMessageLevel::Warn,
                            "Usage: /locate <biome> <name:key>".to_string(),
                        );
                        continue;
                    };

                    if !raw_type.eq_ignore_ascii_case("biome") {
                        push_system_chat(
                            &mut chat_log,
                            SystemMessageLevel::Warn,
                            "Only type 'biome' is supported right now.".to_string(),
                        );
                        continue;
                    }

                    let target = raw_target.trim();
                    if !target.contains(':') {
                        push_system_chat(
                            &mut chat_log,
                            SystemMessageLevel::Warn,
                            "Biome key must be in format 'name:key'.".to_string(),
                        );
                        continue;
                    }

                    if biomes.get_by_localized_name(target).is_none() {
                        push_system_chat(
                            &mut chat_log,
                            SystemMessageLevel::Warn,
                            format!("Unknown biome '{}'.", target),
                        );
                        continue;
                    }

                    let Ok(player_transform) = q_player.single() else {
                        push_system_chat(
                            &mut chat_log,
                            SystemMessageLevel::Warn,
                            "Could not resolve player position for locate.".to_string(),
                        );
                        continue;
                    };

                    let world_x = player_transform.translation.x.floor() as i32;
                    let world_z = player_transform.translation.z.floor() as i32;
                    let (origin_chunk, _) = world_to_chunk_xz(world_x, world_z);

                    let locate_result =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            locate_biome_chunk_by_localized_name(
                                &biomes,
                                world_gen_config.seed,
                                origin_chunk,
                                target,
                                locate_max_radius_chunks,
                            )
                        }));
                    let Some(found_chunk) = (match locate_result {
                        Ok(found) => found,
                        Err(_) => {
                            push_system_chat(
                                &mut chat_log,
                                SystemMessageLevel::Warn,
                                "Locate failed due to an internal error.".to_string(),
                            );
                            continue;
                        }
                    }) else {
                        push_system_chat(
                            &mut chat_log,
                            SystemMessageLevel::Warn,
                            format!("Biome '{}' not found nearby.", target),
                        );
                        continue;
                    };

                    let found_x = found_chunk.x * CX as i32 + (CX as i32 / 2);
                    let found_z = found_chunk.y * CZ as i32 + (CZ as i32 / 2);
                    push_system_chat(
                        &mut chat_log,
                        SystemMessageLevel::Info,
                        format!("found: [{}, {}]", found_x, found_z),
                    );
                }
                "tp" => {
                    let args = command.args.as_slice();
                    if args.is_empty() {
                        push_system_chat(
                            &mut chat_log,
                            SystemMessageLevel::Warn,
                            "Usage: /tp <player>|<x y z>|<player player>|<player x y z>"
                                .to_string(),
                        );
                        continue;
                    }

                    match args.len() {
                        3 => {
                            let Ok(target) = parse_tp_xyz(&args[0], &args[1], &args[2]) else {
                                push_system_chat(
                                    &mut chat_log,
                                    SystemMessageLevel::Warn,
                                    "Usage: /tp <x> <y> <z>".to_string(),
                                );
                                continue;
                            };
                            let Ok(mut player_transform) = q_player.single_mut() else {
                                push_system_chat(
                                    &mut chat_log,
                                    SystemMessageLevel::Warn,
                                    "Could not resolve player for teleport.".to_string(),
                                );
                                continue;
                            };
                            player_transform.translation =
                                Vec3::new(target[0], target[1], target[2]);
                            push_system_chat(
                                &mut chat_log,
                                SystemMessageLevel::Info,
                                format!(
                                    "Teleported to [{:.2}, {:.2}, {:.2}].",
                                    target[0], target[1], target[2]
                                ),
                            );
                        }
                        _ => {
                            push_system_chat(
                                &mut chat_log,
                                SystemMessageLevel::Warn,
                                "Local /tp supports only coordinates: /tp <x> <y> <z>. \
In multiplayer all /tp variants are available."
                                    .to_string(),
                            );
                        }
                    }
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

fn parse_tp_xyz(x: &str, y: &str, z: &str) -> Result<[f32; 3], ()> {
    let x = x.trim().parse::<f32>().map_err(|_| ())?;
    let y = y.trim().parse::<f32>().map_err(|_| ())?;
    let z = z.trim().parse::<f32>().map_err(|_| ())?;
    Ok([x, y, z])
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
    time: Res<Time>,
    registry: Option<Res<BlockRegistry>>,
    block_remap: Res<BlockIdRemap>,
    pending_mesh: Option<Res<PendingMesh>>,
    mesh_backlog: Option<Res<MeshBacklog>>,
    mut chunk_map: ResMut<ChunkMap>,
    mut fluids: ResMut<FluidMap>,
    mut ev_dirty: MessageWriter<SubChunkNeedRemeshEvent>,
    mut chunk_decode_queue: ResMut<RemoteChunkDecodeQueue>,
    mut chunk_decode_tasks: ResMut<RemoteChunkDecodeTasks>,
    mut chunk_decoded_queue: ResMut<RemoteChunkDecodedQueue>,
    mut chunk_remesh_queue: ResMut<RemoteChunkRemeshQueue>,
    mut chunk_debug: Option<ResMut<ChunkDebugStats>>,
    mut observed_flow_events: ObservedWaterFlowEventWriters,
    mut multiplayer_world: MultiplayerWorldReceiveState,
    mut q: Query<(
        &mut MessageReceiver<ServerChunkData>,
        &mut MessageReceiver<ServerBlockBreak>,
        &mut MessageReceiver<ServerBlockPlace>,
    )>,
) {
    let now_secs = time.elapsed_secs();
    multiplayer_world
        .recent_edit_guards
        .by_chunk_until_secs
        .retain(|_, until| *until > now_secs);
    multiplayer_world
        .local_world_edits
        .entries
        .retain(|entry| entry.expires_at_secs > now_secs);

    let Some(entity) = multiplayer_world.runtime.connection_entity else {
        chunk_decode_queue.reset();
        chunk_decode_tasks.reset();
        chunk_decoded_queue.reset();
        chunk_remesh_queue.reset();
        if let Some(stats) = chunk_debug.as_deref_mut() {
            stats.remote_decode_queue = 0;
            stats.remote_remesh_queue = 0;
            stats.remote_decode_queue_peak = 0;
            stats.remote_remesh_queue_peak = 0;
        }
        return;
    };
    if !block_remap.is_ready() {
        chunk_decode_queue.reset();
        chunk_decode_tasks.reset();
        chunk_decoded_queue.reset();
        chunk_remesh_queue.reset();
        if let Some(stats) = chunk_debug.as_deref_mut() {
            stats.remote_decode_queue = 0;
            stats.remote_remesh_queue = 0;
            stats.remote_decode_queue_peak = 0;
            stats.remote_remesh_queue_peak = 0;
        }
        return;
    }

    let Ok((mut recv_chunk, mut recv_block_break, mut recv_block_place)) = q.get_mut(entity) else {
        chunk_decode_queue.reset();
        chunk_decode_tasks.reset();
        chunk_decoded_queue.reset();
        chunk_remesh_queue.reset();
        if let Some(stats) = chunk_debug.as_deref_mut() {
            stats.remote_decode_queue = 0;
            stats.remote_remesh_queue = 0;
            stats.remote_decode_queue_peak = 0;
            stats.remote_remesh_queue_peak = 0;
        }
        return;
    };

    for message in recv_chunk.receive() {
        let coord = IVec2::new(message.coord[0], message.coord[1]);
        if multiplayer_world
            .recent_edit_guards
            .by_chunk_until_secs
            .get(&coord)
            .is_some_and(|until| *until > now_secs)
        {
            continue;
        }
        if let Some(runtime) = multiplayer_world.structure_runtime.as_deref_mut() {
            runtime
                .records_by_chunk
                .insert(coord, message.structures.clone());
        }
        if let Some(queue) = multiplayer_world.structure_reconcile_queue.as_deref_mut() {
            queue.pending_chunks.insert(coord);
        }
        chunk_decode_queue.enqueue(message);
    }

    let mesh_pressure = pending_mesh.as_ref().map_or(0, |pending| pending.0.len())
        + mesh_backlog.as_ref().map_or(0, |backlog| backlog.0.len());
    let decode_spawn_budget =
        multiplayer_chunk_decode_budget(chunk_decode_queue.len(), mesh_pressure, time.delta_secs());
    let decode_apply_budget =
        multiplayer_chunk_apply_budget(chunk_decoded_queue.len(), mesh_pressure, time.delta_secs());
    let decode_task_inflight_cap =
        multiplayer_chunk_decode_task_inflight_cap(mesh_pressure, time.delta_secs());

    if let Some(registry) = registry.as_ref() {
        let local_block_registry_len = registry.defs.len();
        if local_block_registry_len > 0 {
            let pool = AsyncComputeTaskPool::get();
            let remap = std::sync::Arc::new(block_remap.server_to_local.clone());
            let mut scanned = 0usize;
            let scan_cap = chunk_decode_queue.len().max(decode_spawn_budget);
            let mut started = 0usize;

            while started < decode_spawn_budget
                && chunk_decode_tasks.len() < decode_task_inflight_cap
                && scanned < scan_cap
            {
                scanned += 1;
                let Some(message) = chunk_decode_queue.pop_front() else {
                    break;
                };
                if chunk_decode_tasks.tasks.contains_key(&message.coord) {
                    // A task for this coord is already running; keep only the latest payload queued.
                    chunk_decode_queue.enqueue(message);
                    continue;
                }

                let coord_arr = message.coord;
                let blocks = message.blocks;
                let remap_for_task = std::sync::Arc::clone(&remap);
                let task = pool.spawn(async move {
                    let coord = IVec2::new(coord_arr[0], coord_arr[1]);
                    let Ok(mut chunk) = decode_chunk(&blocks) else {
                        return None;
                    };
                    remap_server_chunk_payload_raw(
                        &mut chunk,
                        remap_for_task.as_slice(),
                        local_block_registry_len,
                    );
                    Some((coord, chunk))
                });

                chunk_decode_tasks.tasks.insert(coord_arr, task);
                started += 1;
            }
        }
    }

    let mut finished_tasks: Vec<[i32; 2]> = Vec::new();
    let poll_scan_limit = decode_task_inflight_cap.saturating_mul(2).clamp(8, 48);
    let mut scanned_tasks = 0usize;
    for (coord, task) in chunk_decode_tasks.tasks.iter_mut() {
        if scanned_tasks >= poll_scan_limit {
            break;
        }
        scanned_tasks += 1;
        if let Some(result) = future::block_on(future::poll_once(task)) {
            finished_tasks.push(*coord);
            if let Some((decoded_coord, decoded_chunk)) = result {
                chunk_decoded_queue.enqueue(decoded_coord, decoded_chunk);
            }
        }
    }
    for coord in finished_tasks {
        chunk_decode_tasks.tasks.remove(&coord);
    }

    for _ in 0..decode_apply_budget {
        let Some((coord, mut chunk)) = chunk_decoded_queue.pop_front() else {
            break;
        };
        if registry.is_some() {
            chunk.mark_all_dirty();
            chunk_map.chunks.insert(coord, chunk);
            fluids.0.remove(&coord);
            reapply_local_world_edit_overlays_for_chunk(
                coord,
                now_secs,
                registry.as_deref(),
                &mut multiplayer_world.local_world_edits,
                &mut chunk_map,
                &mut fluids,
                &mut ev_dirty,
            );
            chunk_remesh_queue.enqueue_front(coord);

            for neighbor in [
                IVec2::new(coord.x + 1, coord.y),
                IVec2::new(coord.x - 1, coord.y),
                IVec2::new(coord.x, coord.y + 1),
                IVec2::new(coord.x, coord.y - 1),
            ] {
                if chunk_map.chunks.contains_key(&neighbor) {
                    chunk_remesh_queue.enqueue(neighbor);
                }
            }
        }
    }

    if let Some(stats) = chunk_debug.as_deref_mut() {
        stats.remote_decode_queue =
            chunk_decode_queue.len() + chunk_decode_tasks.len() + chunk_decoded_queue.len();
        stats.remote_remesh_queue = chunk_remesh_queue.len();
        stats.remote_decode_queue_peak = stats
            .remote_decode_queue_peak
            .max(stats.remote_decode_queue);
        stats.remote_remesh_queue_peak = stats
            .remote_remesh_queue_peak
            .max(stats.remote_remesh_queue);
    }

    for message in recv_block_break.receive() {
        if Some(message.player_id) == multiplayer_world.runtime.local_player_id {
            multiplayer_world
                .pending_world_acks
                .entries
                .remove(&PendingWorldAck {
                    kind: PendingWorldAckKind::Break,
                    location: message.location,
                });
        }
        let location = IVec3::new(
            message.location[0],
            message.location[1],
            message.location[2],
        );
        apply_remote_block_break(
            message.location,
            registry.as_deref(),
            &mut chunk_map,
            &mut fluids,
            &mut ev_dirty,
        );
        let (coord, _) = world_to_chunk_xz(message.location[0], message.location[2]);
        let sub = (world_y_to_local(message.location[1]) / SEC_H).min(SEC_COUNT - 1) as u8;
        multiplayer_world
            .recent_edit_guards
            .by_chunk_until_secs
            .insert(coord, now_secs + RECENT_EDIT_CHUNK_GUARD_SECS);
        observed_flow_events
            .collider_refresh
            .write(SubChunkNeedColliderRefreshEvent { coord, sub });
        observed_flow_events
            .break_events
            .write(BlockBreakObservedEvent { location });
    }

    for message in recv_block_place.receive() {
        if Some(message.player_id) == multiplayer_world.runtime.local_player_id {
            multiplayer_world
                .pending_world_acks
                .entries
                .remove(&PendingWorldAck {
                    kind: PendingWorldAckKind::Place,
                    location: message.location,
                });
        }
        let Some(registry) = registry.as_ref() else {
            continue;
        };
        let local_block_id = remap_server_block_id(&block_remap, registry, message.block_id);
        let local_stacked_block_id =
            remap_server_block_id(&block_remap, registry, message.stacked_block_id);
        apply_remote_block_place(
            message.location,
            local_block_id,
            local_stacked_block_id,
            registry,
            &mut chunk_map,
            &mut fluids,
            &mut ev_dirty,
        );
        let (coord, _) = world_to_chunk_xz(message.location[0], message.location[2]);
        let sub = (world_y_to_local(message.location[1]) / SEC_H).min(SEC_COUNT - 1) as u8;
        multiplayer_world
            .recent_edit_guards
            .by_chunk_until_secs
            .insert(coord, now_secs + RECENT_EDIT_CHUNK_GUARD_SECS);
        observed_flow_events
            .collider_refresh
            .write(SubChunkNeedColliderRefreshEvent { coord, sub });
        observed_flow_events
            .place_events
            .write(BlockPlaceObservedEvent {
                location: IVec3::new(
                    message.location[0],
                    message.location[1],
                    message.location[2],
                ),
                block_id: local_block_id,
            });
    }
}

fn reapply_local_world_edit_overlays_for_chunk(
    coord: IVec2,
    now_secs: f32,
    registry: Option<&BlockRegistry>,
    overlays: &mut LocalWorldEditOverlayState,
    chunk_map: &mut ChunkMap,
    fluids: &mut FluidMap,
    ev_dirty: &mut MessageWriter<SubChunkNeedRemeshEvent>,
) {
    let matching = overlays
        .entries
        .iter()
        .copied()
        .filter(|entry| {
            entry.expires_at_secs > now_secs
                && world_to_chunk_xz(entry.location[0], entry.location[2]).0 == coord
        })
        .collect::<Vec<_>>();

    for entry in matching {
        match entry.kind {
            LocalWorldEditKind::Break => {
                apply_remote_block_break(entry.location, registry, chunk_map, fluids, ev_dirty);
            }
            LocalWorldEditKind::Place {
                block_id,
                stacked_block_id,
            } => {
                let Some(registry) = registry else {
                    continue;
                };
                apply_remote_block_place(
                    entry.location,
                    block_id,
                    stacked_block_id,
                    registry,
                    chunk_map,
                    fluids,
                    ev_dirty,
                );
            }
        }
    }
}

/// Spreads streamed chunk remesh triggers over multiple frames to avoid burst spikes.
fn flush_remote_chunk_remesh_queue(
    time: Res<Time>,
    pending_mesh: Option<Res<PendingMesh>>,
    mesh_backlog: Option<Res<MeshBacklog>>,
    chunk_map: Res<ChunkMap>,
    mut chunk_remesh_queue: ResMut<RemoteChunkRemeshQueue>,
    mut chunk_debug: Option<ResMut<ChunkDebugStats>>,
    mut ev_dirty: MessageWriter<SubChunkNeedRemeshEvent>,
) {
    let mesh_pressure = pending_mesh.as_ref().map_or(0, |pending| pending.0.len())
        + mesh_backlog.as_ref().map_or(0, |backlog| backlog.0.len());
    let budget = multiplayer_chunk_remesh_coord_budget(
        chunk_remesh_queue.len(),
        mesh_pressure,
        time.delta_secs(),
    );

    for _ in 0..budget {
        let Some(coord) = chunk_remesh_queue.pop_front() else {
            break;
        };
        if !chunk_map.chunks.contains_key(&coord) {
            continue;
        }
        for sub in 0..SEC_COUNT {
            ev_dirty.write(SubChunkNeedRemeshEvent { coord, sub });
        }
    }

    if let Some(stats) = chunk_debug.as_deref_mut() {
        stats.remote_remesh_queue = chunk_remesh_queue.len();
        stats.remote_remesh_queue_peak = stats
            .remote_remesh_queue_peak
            .max(stats.remote_remesh_queue);
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
            let local_item_id =
                if message.item_id != 0 && item_registry.def_opt(message.item_id).is_some() {
                    message.item_id
                } else {
                    let local_block_id =
                        remap_server_block_id(&block_remap, registry, message.block_id);
                    item_registry.item_for_block(local_block_id).unwrap_or(0)
                };
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
                if message.item_id != 0 && item_registry.def_opt(message.item_id).is_some() {
                    message.item_id
                } else {
                    let local_block_id =
                        remap_server_block_id(&block_remap, registry, message.block_id);
                    item_registry.item_for_block(local_block_id).unwrap_or(0)
                }
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

// Extracted multiplayer runtime tail to keep this file below 2000 lines.
include!("mod_multiplayer_tail.rs");
