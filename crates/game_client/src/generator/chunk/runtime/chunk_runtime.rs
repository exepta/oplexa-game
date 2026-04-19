use crate::core::config::GlobalConfig;
use crate::core::events::block::block_player_events::{
    BlockBreakByPlayerEvent, BlockPlaceByPlayerEvent,
};
use crate::core::entities::player::Player;
use crate::core::events::chunk_events::{
    ChunkUnloadEvent, SubChunkNeedColliderRefreshEvent, SubChunkNeedRemeshEvent,
};
use crate::core::multiplayer::MultiplayerConnectionState;
use crate::core::shader::terrain_shader::{TerrainChunkMatIndex, TerrainChunkMaterial};
use crate::core::shader::water_shader::{WaterMatHandle, WaterMaterial};
use crate::core::states::states::{AppState, InGameStates, LoadingStates};
use crate::core::world::block::*;
use crate::core::world::chunk::*;
use crate::core::world::chunk_dimension::*;
use crate::generator::chunk::chunk_meshing::*;
use crate::generator::chunk::chunk_runtime_types::*;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, ComputeTaskPool};
use bevy::tasks::futures_lite::future;
use bevy_rapier3d::prelude::{Collider, ColliderDisabled, RigidBody, TriMeshFlags};
use std::cmp::Reverse;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Instant;

/// Represents collider backlog used by the `generator::chunk::chunk_builder` module.
#[derive(Default, Resource)]
pub struct ColliderBacklog(HashMap<(IVec2, u8), ColliderTodo>);

impl ColliderBacklog {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn chunk_coords(&self) -> impl Iterator<Item = IVec2> + '_ {
        self.0.keys().map(|(coord, _)| *coord)
    }
}

/// Represents collider todo used by the `generator::chunk::chunk_builder` module.
struct ColliderTodo {
    coord: IVec2,
    sub: u8,
    version: u64,
    origin: Vec3,
    positions: Vec<[f32; 3]>,
    indices: Vec<u32>,
}

/// Represents collider build used by the `generator::chunk::chunk_builder` module.
struct ColliderBuild {
    version: u64,
    origin: Vec3,
    collider: Option<Collider>,
}

/// Represents chunk collider index used by the `generator::chunk::chunk_builder` module.
#[derive(Resource, Default)]
struct ChunkColliderIndex(pub HashMap<(IVec2, u8), Entity>);

#[derive(Component, Clone, Copy)]
struct ChunkColliderProxy {
    coord: IVec2,
}

/// Represents pending collider build used by the `generator::chunk::chunk_builder` module.
#[derive(Resource, Default)]
pub struct PendingColliderBuild(
    HashMap<(IVec2, u8), bevy::tasks::Task<((IVec2, u8), ColliderBuild)>>,
);

impl PendingColliderBuild {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn chunk_coords(&self) -> impl Iterator<Item = IVec2> + '_ {
        self.0.keys().map(|(coord, _)| *coord)
    }
}

#[derive(Resource, Default)]
struct ColliderReadyQueue(VecDeque<((IVec2, u8), ColliderBuild)>);

/// Represents kick queue used by the `generator::chunk::chunk_builder` module.
#[derive(Resource, Default)]
struct KickQueue(Vec<KickItem>);

/// Represents kick item used by the `generator::chunk::chunk_builder` module.
#[derive(Clone, Copy, Debug)]
struct KickItem {
    coord: IVec2,
    sub: u8,
    frames_left: u8,
    tries_left: u8,
}

/// Represents kicked once used by the `generator::chunk::chunk_builder` module.
#[derive(Resource, Default)]
struct KickedOnce(HashSet<(IVec2, u8)>);

/// Represents queued once used by the `generator::chunk::chunk_builder` module.
#[derive(Resource, Default)]
struct QueuedOnce(HashSet<(IVec2, u8)>);

const HIGH_RANGE_PRELOAD_THRESHOLD: i32 = 10;
const HIDDEN_PRELOAD_RING: i32 = 2;

#[derive(Resource, Default)]
struct MeshBacklogSet(HashSet<(IVec2, usize)>);

struct ReadyMeshItem {
    key: (IVec2, usize),
    version: u64,
    builds: Vec<(BlockId, MeshBuild)>,
    immediate: bool,
}

#[derive(Resource, Default)]
struct ImmediateMeshReadyQueue(VecDeque<ReadyMeshItem>);

#[derive(Resource, Default)]
struct LocalEditMeshPriorityQueue(VecDeque<(IVec2, usize)>);

#[derive(Resource, Default)]
struct ChunkReadySet(HashSet<IVec2>);

#[derive(Resource, Default)]
struct MeshUpdateState {
    desired_mesh_versions: HashMap<(IVec2, usize), u64>,
    pending_mesh_versions: HashMap<(IVec2, usize), u64>,
    immediate_mesh_keys: HashSet<(IVec2, usize)>,
    desired_collider_versions: HashMap<(IVec2, u8), u64>,
    pending_collider_versions: HashMap<(IVec2, u8), u64>,
    last_mesh_fingerprint: HashMap<(IVec2, usize), u64>,
    last_collider_fingerprint: HashMap<(IVec2, u8), u64>,
}

#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct ChunkStageTelemetry {
    pub stage_gen_collect_ms: f32,
    pub stage_mesh_apply_ms: f32,
    pub stage_collider_schedule_ms: f32,
    pub stage_collider_apply_ms: f32,
    pub chunk_ready_latency_ms: f32,
    pub chunk_ready_latency_p95_ms: f32,
}

#[derive(Resource, Default)]
struct ChunkReadyLatencyState {
    requested_at: HashMap<IVec2, f64>,
    recent_samples_ms: VecDeque<f32>,
}

/// Represents chunk unload state used by the `generator::chunk::chunk_builder` module.
#[derive(SystemParam)]
struct ChunkUnloadState<'w, 's> {
    pending_gen: ResMut<'w, PendingGen>,
    pending_mesh: ResMut<'w, PendingMesh>,
    backlog: ResMut<'w, MeshBacklog>,
    backlog_set: ResMut<'w, MeshBacklogSet>,
    pending_collider: ResMut<'w, PendingColliderBuild>,
    collider_ready: ResMut<'w, ColliderReadyQueue>,
    coll_backlog: ResMut<'w, ColliderBacklog>,
    ready_set: ResMut<'w, ChunkReadySet>,
    mesh_update: ResMut<'w, MeshUpdateState>,
    _marker: std::marker::PhantomData<&'s ()>,
}

/// Represents chunk cleanup state used by the `generator::chunk::chunk_builder` module.
#[derive(SystemParam)]
struct ChunkCleanupState<'w, 's> {
    pending_gen: ResMut<'w, PendingGen>,
    pending_mesh: ResMut<'w, PendingMesh>,
    backlog: ResMut<'w, MeshBacklog>,
    backlog_set: ResMut<'w, MeshBacklogSet>,
    pending_collider: ResMut<'w, PendingColliderBuild>,
    collider_ready: ResMut<'w, ColliderReadyQueue>,
    coll_backlog: ResMut<'w, ColliderBacklog>,
    kick_queue: ResMut<'w, KickQueue>,
    kicked: ResMut<'w, KickedOnce>,
    queued: ResMut<'w, QueuedOnce>,
    ready_set: ResMut<'w, ChunkReadySet>,
    mesh_update: ResMut<'w, MeshUpdateState>,
    _marker: std::marker::PhantomData<&'s ()>,
}

#[derive(SystemParam)]
struct ChunkMeshApplyState<'w, 's> {
    pending_mesh: ResMut<'w, PendingMesh>,
    mesh_index: ResMut<'w, ChunkMeshIndex>,
    collider_index: ResMut<'w, ChunkColliderIndex>,
    pending_collider: ResMut<'w, PendingColliderBuild>,
    collider_ready: ResMut<'w, ColliderReadyQueue>,
    meshes: ResMut<'w, Assets<Mesh>>,
    chunk_map: ResMut<'w, ChunkMap>,
    ready_set: ResMut<'w, ChunkReadySet>,
    coll_backlog: ResMut<'w, ColliderBacklog>,
    ready_latency: ResMut<'w, ChunkReadyLatencyState>,
    stage_telemetry: ResMut<'w, ChunkStageTelemetry>,
    mesh_update: ResMut<'w, MeshUpdateState>,
    _marker: std::marker::PhantomData<&'s ()>,
}

/// Coordinates streamed chunk meshing, collider builds, and unloads on the client.
pub struct ChunkRuntimePlugin;

impl Plugin for ChunkRuntimePlugin {
    /// Builds this component for the `generator::chunk::chunk_builder` module.
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkMeshIndex>()
            .init_resource::<MeshBacklog>()
            .init_resource::<PendingGen>()
            .init_resource::<PendingMesh>()
            .init_resource::<ChunkColliderIndex>()
            .init_resource::<ColliderBacklog>()
            .init_resource::<PendingColliderBuild>()
            .init_resource::<ColliderReadyQueue>()
            .init_resource::<KickQueue>()
            .init_resource::<KickedOnce>()
            .init_resource::<QueuedOnce>()
            .init_resource::<MeshBacklogSet>()
            .init_resource::<ImmediateMeshReadyQueue>()
            .init_resource::<LocalEditMeshPriorityQueue>()
            .init_resource::<ChunkReadySet>()
            .init_resource::<MeshUpdateState>()
            .init_resource::<ChunkStageTelemetry>()
            .init_resource::<ChunkReadyLatencyState>()
            // --- Generation, Meshing, Kick etc. ---
            .add_systems(
                Update,
                (
                    collect_generated_chunks.run_if(
                        in_state(AppState::Loading(LoadingStates::BaseGen))
                            .or(in_state(AppState::InGame(InGameStates::Game))),
                    ),
                    emit_local_collider_refresh_requests
                        .run_if(in_state(AppState::InGame(InGameStates::Game))),
                    emit_local_mesh_priority_requests
                        .run_if(in_state(AppState::InGame(InGameStates::Game))),
                    apply_immediate_collider_refresh_requests
                        .run_if(in_state(AppState::InGame(InGameStates::Game))),
                    schedule_remesh_tasks_from_events
                        .in_set(VoxelStage::Meshing)
                        .run_if(
                            in_state(AppState::Loading(LoadingStates::BaseGen))
                                .or(in_state(AppState::InGame(InGameStates::Game))),
                        ),
                    (
                        collect_meshed_subchunks,
                        enqueue_kick_for_new_subchunks,
                        process_kick_queue,
                    )
                        .chain()
                        .run_if(
                            in_state(AppState::Loading(LoadingStates::BaseGen))
                                .or(in_state(AppState::InGame(InGameStates::Game))),
                        ),
                    drain_mesh_backlog.run_if(
                        in_state(AppState::Loading(LoadingStates::BaseGen))
                            .or(in_state(AppState::InGame(InGameStates::Game))),
                    ),
                    (
                        schedule_collider_build_tasks,
                        collect_finished_collider_builds,
                    )
                        .chain()
                        .run_if(
                            in_state(AppState::Loading(LoadingStates::BaseGen))
                                .or(in_state(AppState::InGame(InGameStates::Game))),
                        ),
                    update_chunk_collider_activation
                        .run_if(in_state(AppState::InGame(InGameStates::Game))),
                    unload_far_chunks.run_if(
                        in_state(AppState::Loading(LoadingStates::BaseGen))
                            .or(in_state(AppState::InGame(InGameStates::Game))),
                    ),
                    cleanup_kick_flags_on_unload
                        .after(unload_far_chunks)
                        .run_if(
                            in_state(AppState::Loading(LoadingStates::BaseGen))
                                .or(in_state(AppState::InGame(InGameStates::Game))),
                        ),
                )
                    .chain(),
            )
            .add_systems(
                Update,
                check_base_gen_world_ready
                    .run_if(in_state(AppState::Loading(LoadingStates::BaseGen))),
            )
            .add_systems(
                Update,
                sync_chunk_mesh_visibility.run_if(
                    in_state(AppState::Loading(LoadingStates::BaseGen))
                        .or(in_state(AppState::InGame(InGameStates::Game))),
                ),
            )
            .add_systems(
                OnExit(AppState::InGame(InGameStates::Game)),
                cleanup_chunk_runtime_on_exit,
            );
    }
}

// ================================================
//                    Sub Update
// ================================================

/// Runs the `enqueue_kick_for_new_subchunks` routine for enqueue kick for new subchunks in the `generator::chunk::chunk_builder` module.
fn enqueue_kick_for_new_subchunks(
    q_new_meshes: Query<&SubchunkMesh, Added<SubchunkMesh>>,
    mut queue: ResMut<KickQueue>,
    kicked: Res<KickedOnce>,
    mut queued: ResMut<QueuedOnce>,
) {
    let mut seen: HashSet<(IVec2, u8)> = HashSet::new();

    for m in q_new_meshes.iter() {
        let key = (m.coord, m.sub);

        if kicked.0.contains(&key) {
            continue;
        }

        if !seen.insert(key) {
            continue;
        }

        if queued.0.contains(&key) {
            continue;
        }

        queue.0.push(KickItem {
            coord: m.coord,
            sub: m.sub,
            frames_left: 3,
            tries_left: 8,
        });
        queued.0.insert(key);
    }
}

#[inline]
fn enqueue_mesh_fast(
    backlog: &mut MeshBacklog,
    backlog_set: &mut MeshBacklogSet,
    pending: &PendingMesh,
    key: (IVec2, usize),
) {
    if pending.0.contains_key(&key) || backlog_set.0.contains(&key) {
        return;
    }
    backlog.0.push_back(key);
    backlog_set.0.insert(key);
}

/// Processes kick queue for the `generator::chunk::chunk_builder` module.
fn process_kick_queue(
    mut queue: ResMut<KickQueue>,
    mut kicked: ResMut<KickedOnce>,
    mut queued: ResMut<QueuedOnce>,
    chunk_map: Res<ChunkMap>,
    mut ev_dirty: MessageWriter<SubChunkNeedRemeshEvent>,
) {
    let mut i = 0;
    while i < queue.0.len() {
        let item = &mut queue.0[i];

        if item.frames_left > 0 {
            item.frames_left -= 1;
            i += 1;
            continue;
        }

        if !chunk_map.chunks.contains_key(&item.coord) {
            queued.0.remove(&(item.coord, item.sub));
            queue.0.swap_remove(i);
            continue;
        }

        if neighbors_ready(&chunk_map, item.coord) {
            ev_dirty.write(SubChunkNeedRemeshEvent {
                coord: item.coord,
                sub: item.sub as usize,
            });
            kicked.0.insert((item.coord, item.sub));
            queued.0.remove(&(item.coord, item.sub));
            queue.0.swap_remove(i);
            continue;
        }

        if item.tries_left > 0 {
            item.frames_left = 3;
            item.tries_left -= 1;
            i += 1;
        } else {
            // Keep a low-frequency retry alive until neighbors actually exist.
            item.frames_left = 20;
            item.tries_left = 3;
            i += 1;
        }
    }
}
// ================================================
//                    Main
// ================================================

/// Runs the `check_base_gen_world_ready` routine for check base gen world ready in the `generator::chunk::chunk_builder` module.
fn check_base_gen_world_ready(
    game_config: Res<GlobalConfig>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
    load_center: Option<Res<LoadCenter>>,
    chunk_map: Res<ChunkMap>,
    pending_gen: Res<PendingGen>,
    pending_mesh: Res<PendingMesh>,
    backlog: Res<MeshBacklog>,
    mut next: ResMut<NextState<AppState>>,
    mut commands: Commands,
) {
    let initial_radius = visible_radius(game_config.graphics.chunk_range);
    let center = load_center
        .as_ref()
        .map(|lc| lc.world_xz)
        .unwrap_or(IVec2::ZERO);

    let ready = if multiplayer_connection.is_remote_session() {
        let remote_ready_radius = 1;
        area_chunks_in_map(center, remote_ready_radius, &chunk_map)
            && area_ready(
                center,
                remote_ready_radius,
                &chunk_map,
                &pending_gen,
                &pending_mesh,
                &backlog,
            )
    } else {
        let local_ready_radius = (initial_radius / 2).clamp(2, 8).min(initial_radius).max(1);
        area_chunks_in_map(center, initial_radius, &chunk_map)
            && area_ready(
                center,
                local_ready_radius,
                &chunk_map,
                &pending_gen,
                &pending_mesh,
                &backlog,
            )
    };

    if ready {
        commands.remove_resource::<LoadCenter>();
        next.set(AppState::InGame(InGameStates::Game));
    }
}

fn emit_local_collider_refresh_requests(
    mut breaks: MessageReader<BlockBreakByPlayerEvent>,
    mut places: MessageReader<BlockPlaceByPlayerEvent>,
    mut writer: MessageWriter<SubChunkNeedColliderRefreshEvent>,
) {
    for event in breaks.read() {
        let (coord, _) = world_to_chunk_xz(event.location.x, event.location.z);
        let sub = (world_y_to_local(event.location.y) / SEC_H) as u8;
        writer.write(SubChunkNeedColliderRefreshEvent { coord, sub });
    }

    for event in places.read() {
        let (coord, _) = world_to_chunk_xz(event.location.x, event.location.z);
        let sub = (world_y_to_local(event.location.y) / SEC_H) as u8;
        writer.write(SubChunkNeedColliderRefreshEvent { coord, sub });
    }
}

fn emit_local_mesh_priority_requests(
    mut breaks: MessageReader<BlockBreakByPlayerEvent>,
    mut places: MessageReader<BlockPlaceByPlayerEvent>,
    mut queue: ResMut<LocalEditMeshPriorityQueue>,
) {
    for event in breaks.read() {
        let sub = (world_y_to_local(event.location.y) / SEC_H).min(SEC_COUNT - 1);
        queue.0.push_back((event.chunk_coord, sub));
    }

    for event in places.read() {
        let coord = world_to_chunk_xz(event.location.x, event.location.z).0;
        let sub = (world_y_to_local(event.location.y) / SEC_H).min(SEC_COUNT - 1);
        queue.0.push_back((coord, sub));
    }
}

fn apply_immediate_collider_refresh_requests(
    mut commands: Commands,
    chunk_map: Res<ChunkMap>,
    reg: Res<BlockRegistry>,
    mut collider_index: ResMut<ChunkColliderIndex>,
    mut refresh_events: MessageReader<SubChunkNeedColliderRefreshEvent>,
) {
    let mut hard_refresh = HashSet::new();
    for event in refresh_events.read().copied() {
        hard_refresh.insert((event.coord, event.sub));
    }

    for (coord, sub) in hard_refresh {
        let Some(chunk) = chunk_map.chunks.get(&coord) else {
            // Do not touch existing collider state when chunk data is temporarily absent.
            // The regular unload/reconcile path is responsible for teardown.
            continue;
        };

        let placeholder = build_surface_placeholder_collider(chunk, &reg, sub as usize);
        let origin = Vec3::new(coord.x as f32 * CX as f32, 0.0, coord.y as f32 * CZ as f32)
            * VOXEL_SIZE;

        match (collider_index.0.get(&(coord, sub)).copied(), placeholder) {
            // Keep the previous collider stable during rapid local edits.
            // The normal async remesh/collider path will replace it with the updated result.
            (Some(_entity), Some((_collider, _local_offset))) => {}
            (Some(_), None) => {}
            (None, Some((collider, local_offset))) => {
                let entity = commands
                    .spawn((
                        RigidBody::Fixed,
                        collider,
                        Transform::from_translation(origin + local_offset),
                        ChunkColliderProxy { coord },
                        Name::new(format!("collider chunk({},{}) sub{}", coord.x, coord.y, sub)),
                    ))
                    .id();
                collider_index.0.insert((coord, sub), entity);
            }
            (None, None) => {}
        }
    }
}

//System
/// Runs the `drain_mesh_backlog` routine for drain mesh backlog in the `generator::chunk::chunk_builder` module.
fn drain_mesh_backlog(
    mut backlog: ResMut<MeshBacklog>,
    mut backlog_set: ResMut<MeshBacklogSet>,
    mut pending_mesh: ResMut<PendingMesh>,
    chunk_map: Res<ChunkMap>,
    reg: Res<BlockRegistry>,
    game_config: Res<GlobalConfig>,
    time: Res<Time>,
    app_state: Res<State<AppState>>,
    q_cam: Query<&GlobalTransform, With<Camera3d>>,
    load_center: Option<Res<LoadCenter>>,
    mut mesh_update: ResMut<MeshUpdateState>,
) {
    if chunk_map.chunks.is_empty() {
        backlog.0.clear();
        backlog_set.0.clear();
        return;
    }

    let waiting = is_waiting(&app_state);
    let in_game = matches!(app_state.get(), AppState::InGame(InGameStates::Game));
    let frame_ms = time.delta_secs() * 1000.0;
    let dynamic_divisor = frame_pressure_divisor(frame_ms);
    let waiting_mesh_cap = (AsyncComputeTaskPool::get().thread_num().max(1) * 8).clamp(32, 256);
    let max_inflight_mesh = if waiting {
        waiting_mesh_cap
    } else if in_game {
        (game_config.graphics.chunk_mesh_max_inflight.max(1) / dynamic_divisor).clamp(4, 16)
    } else {
        game_config.graphics.chunk_mesh_max_inflight.max(1)
    };
    let pull_budget = if waiting {
        usize::MAX
    } else if frame_ms > 34.0 {
        1
    } else if frame_ms > 24.0 {
        2
    } else if frame_ms > 18.0 {
        3
    } else {
        5
    };

    let reg_lite = RegLite::from_reg(&reg);
    let pool = AsyncComputeTaskPool::get();
    let center_c = if let Ok(t) = q_cam.single() {
        let (c, _) = world_to_chunk_xz(
            (t.translation().x / VOXEL_SIZE).floor() as i32,
            (t.translation().z / VOXEL_SIZE).floor() as i32,
        );
        c
    } else if let Some(lc) = load_center {
        lc.world_xz
    } else {
        IVec2::ZERO
    };

    let mut pulled = 0usize;
    while pending_mesh.0.len() < max_inflight_mesh && pulled < pull_budget {
        let next = if waiting {
            backlog.0.pop_front()
        } else {
            let best = backlog
                .0
                .iter()
                .take(768)
                .enumerate()
                .min_by_key(|(_, (coord, sub))| {
                    let dx = coord.x - center_c.x;
                    let dz = coord.y - center_c.y;
                    (dx * dx + dz * dz, *sub)
                })
                .map(|(idx, _)| idx);
            best.and_then(|idx| backlog.0.remove(idx))
                .or_else(|| backlog.0.pop_front())
        };

        let Some((coord, sub)) = next else {
            break;
        };
        pulled += 1;
        backlog_set.0.remove(&(coord, sub));
        if pending_mesh.0.contains_key(&(coord, sub)) {
            continue;
        }

        let mut subs = vec![sub];
        if !waiting && frame_ms < 20.0 && pending_mesh.0.len() + 1 < max_inflight_mesh {
            let max_take = (max_inflight_mesh - pending_mesh.0.len() - 1).min(SEC_COUNT);
            let mut i = 0usize;
            while i < backlog.0.len() && subs.len() <= max_take {
                if backlog.0[i].0 == coord {
                    if let Some((_, s2)) = backlog.0.remove(i) {
                        backlog_set.0.remove(&(coord, s2));
                        if !subs.contains(&s2) {
                            subs.push(s2);
                        }
                    }
                } else {
                    i += 1;
                }
            }
        }

        let Some(chunk) = chunk_map.chunks.get(&coord) else {
            continue;
        };
        let chunk_copy = Arc::new(chunk.clone());
        let reg_copy = reg_lite.clone();
        for sub in subs {
            if pending_mesh.0.contains_key(&(coord, sub)) {
                continue;
            }
            let y0 = sub * SEC_H;
            let y1 = (y0 + SEC_H).min(CY);
            let borders = snapshot_borders(&chunk_map, coord, y0, y1);

            let key = (coord, sub);
            let chunk_for_task = Arc::clone(&chunk_copy);
            let reg_for_task = reg_copy.clone();
            let t = pool.spawn(async move {
                let builds = mesh_subchunk_async(
                    &chunk_for_task,
                    &reg_for_task,
                    sub,
                    VOXEL_SIZE,
                    Some(borders),
                )
                .await;
                (key, builds)
            });
            let desired = mesh_update
                .desired_mesh_versions
                .get(&key)
                .copied()
                .unwrap_or(0);
            mesh_update.pending_mesh_versions.insert(key, desired);
            pending_mesh.0.insert(key, t);
            if pending_mesh.0.len() >= max_inflight_mesh {
                break;
            }
        }
    }
}

//System
/// Runs the `collect_generated_chunks` routine for collect generated chunks in the `generator::chunk::chunk_builder` module.
fn collect_generated_chunks(
    mut pending_gen: ResMut<PendingGen>,
    mut pending_mesh: ResMut<PendingMesh>,
    mut backlog: ResMut<MeshBacklog>,
    mut backlog_set: ResMut<MeshBacklogSet>,
    mut ready_set: ResMut<ChunkReadySet>,
    mut chunk_map: ResMut<ChunkMap>,
    reg: Res<BlockRegistry>,
    game_config: Res<GlobalConfig>,
    app_state: Res<State<AppState>>,
    time: Res<Time>,
    mut ready_latency: ResMut<ChunkReadyLatencyState>,
    mut stage_telemetry: ResMut<ChunkStageTelemetry>,
    mut mesh_update: ResMut<MeshUpdateState>,
) {
    let stage_start = Instant::now();
    let waiting = is_waiting(&app_state);
    let in_game = matches!(app_state.get(), AppState::InGame(InGameStates::Game));
    let frame_ms = time.delta_secs() * 1000.0;
    let dynamic_divisor = frame_pressure_divisor(frame_ms);
    let waiting_mesh_cap = (AsyncComputeTaskPool::get().thread_num().max(1) * 8).clamp(32, 256);
    let max_inflight_mesh = if waiting {
        waiting_mesh_cap
    } else if in_game {
        (game_config.graphics.chunk_mesh_max_inflight.max(1) / dynamic_divisor).clamp(8, 24)
    } else {
        game_config.graphics.chunk_mesh_max_inflight.max(1)
    };
    let mesh_pressure = pending_mesh.0.len() + backlog.0.len();
    let gen_apply_cap = if waiting {
        BIG
    } else if in_game {
        if frame_ms > 24.0 {
            1
        } else {
            (2usize / dynamic_divisor).max(1)
        }
    } else if mesh_pressure > 4_000 {
        1
    } else if mesh_pressure > 2_500 {
        2
    } else if mesh_pressure > 1_500 {
        3
    } else {
        6
    };
    let allow_neighbor_enqueue = waiting || (!in_game && mesh_pressure < 1_200);

    let reg_lite = RegLite::from_reg(&reg);
    let mut finished = Vec::new();
    let mut applied_gen = 0usize;

    for (coord, task) in pending_gen.0.iter_mut() {
        if applied_gen >= gen_apply_cap {
            break;
        }
        if let Some((c, mut data)) = future::block_on(future::poll_once(task)) {
            clear_air_only_subchunks_dirty(&mut data);
            let chunk_shared = Arc::new(data);
            chunk_map.chunks.insert(c, (*chunk_shared).clone());
            ready_set.0.remove(&c);
            if chunk_shared.dirty_mask == 0 {
                ready_set.0.insert(c);
                telemetry_mark_chunk_ready(
                    c,
                    time.elapsed_secs_f64(),
                    &mut ready_latency,
                    &mut stage_telemetry,
                );
            }

            let pool = AsyncComputeTaskPool::get();
            let order = sub_priority_order(&chunk_shared);
            let max_spawn_per_chunk = if in_game {
                if frame_ms <= 16.0 {
                    4usize
                } else if frame_ms <= 22.0 {
                    3usize
                } else {
                    2usize
                }
            } else {
                usize::MAX
            };
            let mut spawned_for_chunk = 0usize;
            for sub in order {
                if !chunk_shared.is_dirty(sub) {
                    continue;
                }
                let key = (c, sub);
                let should_spawn_now = pending_mesh.0.len() < max_inflight_mesh
                    && spawned_for_chunk < max_spawn_per_chunk;
                if should_spawn_now {
                    let y0 = sub * SEC_H;
                    let y1 = (y0 + SEC_H).min(CY);
                    let borders = snapshot_borders(&chunk_map, c, y0, y1);
                    let chunk_copy = Arc::clone(&chunk_shared);
                    let reg_copy = reg_lite.clone();
                    let t = pool.spawn(async move {
                        let builds = mesh_subchunk_async(
                            &chunk_copy,
                            &reg_copy,
                            sub,
                            VOXEL_SIZE,
                            Some(borders),
                        )
                        .await;
                        ((c, sub), builds)
                    });
                    let desired = mesh_update
                        .desired_mesh_versions
                        .get(&key)
                        .copied()
                        .unwrap_or(0);
                    mesh_update.pending_mesh_versions.insert(key, desired);
                    pending_mesh.0.insert(key, t);
                    spawned_for_chunk += 1;
                } else {
                    enqueue_mesh_fast(&mut backlog, &mut backlog_set, &pending_mesh, key);
                }
            }

            if allow_neighbor_enqueue {
                for n_coord in neighbors4_iter(c) {
                    if let Some(n_chunk) = chunk_map.chunks.get(&n_coord) {
                        let neighbor_shared = Arc::new(n_chunk.clone());
                        let order_n = sub_priority_order(n_chunk);
                        for sub in order_n {
                            if !n_chunk.is_dirty(sub) {
                                continue;
                            }
                            let key = (n_coord, sub);
                            if pending_mesh.0.contains_key(&key) {
                                continue;
                            }
                            if pending_mesh.0.len() < max_inflight_mesh {
                                let y0 = sub * SEC_H;
                                let y1 = (y0 + SEC_H).min(CY);
                                let borders = snapshot_borders(&chunk_map, n_coord, y0, y1);
                                let pool = AsyncComputeTaskPool::get();
                                let reg_copy = reg_lite.clone();
                                let chunk_copy = Arc::clone(&neighbor_shared);
                                let t = pool.spawn(async move {
                                    let builds = mesh_subchunk_async(
                                        &chunk_copy,
                                        &reg_copy,
                                        sub,
                                        VOXEL_SIZE,
                                        Some(borders),
                                    )
                                    .await;
                                    ((n_coord, sub), builds)
                                });
                                let desired = mesh_update
                                    .desired_mesh_versions
                                    .get(&key)
                                    .copied()
                                    .unwrap_or(0);
                                mesh_update.pending_mesh_versions.insert(key, desired);
                                pending_mesh.0.insert(key, t);
                            } else {
                                enqueue_mesh_fast(
                                    &mut backlog,
                                    &mut backlog_set,
                                    &pending_mesh,
                                    key,
                                );
                            }
                        }
                    }
                }
            }

            finished.push(*coord);
            applied_gen += 1;
        }
    }

    for c in finished {
        pending_gen.0.remove(&c);
    }

    stage_telemetry.stage_gen_collect_ms = smooth_stage_ms(
        stage_telemetry.stage_gen_collect_ms,
        stage_start.elapsed().as_secs_f32() * 1000.0,
    );
}

//System
/// Runs the `collect_meshed_subchunks` routine for collect meshed subchunks in the `generator::chunk::chunk_builder` module.
fn collect_meshed_subchunks(
    mut commands: Commands,
    mut apply_state: ChunkMeshApplyState,
    mut backlog: ResMut<MeshBacklog>,
    mut backlog_set: ResMut<MeshBacklogSet>,
    mut immediate_ready: ResMut<ImmediateMeshReadyQueue>,
    reg: Res<BlockRegistry>,
    terrain_mats: Res<TerrainChunkMatIndex>,
    water_mat: Option<Res<WaterMatHandle>>,
    q_mesh: Query<&Mesh3d>,
    q_cam: Query<&GlobalTransform, With<Camera3d>>,
    load_center: Option<Res<LoadCenter>>,
    game_config: Res<GlobalConfig>,
    app_state: Res<State<AppState>>,
    time: Res<Time>,
) {
    if terrain_mats.0.len() < reg.defs.len().saturating_sub(1) {
        return;
    }

    let stage_start = Instant::now();
    let waiting = is_waiting(&app_state);
    let in_game = matches!(app_state.get(), AppState::InGame(InGameStates::Game));
    let frame_ms = time.delta_secs() * 1000.0;
    let dynamic_divisor = frame_pressure_divisor(frame_ms);
    let ingame_apply_cap = game_config.graphics.chunk_mesh_apply_per_frame.max(1);
    let ingame_apply_cap = if in_game {
        (ingame_apply_cap / dynamic_divisor).clamp(1, 10)
    } else {
        ingame_apply_cap
    };
    let waiting_mesh_apply_cap =
        (AsyncComputeTaskPool::get().thread_num().max(1) * 6).clamp(24, 160);
    let apply_cap = if waiting {
        waiting_mesh_apply_cap
    } else {
        ingame_apply_cap
    };
    let center_c = if let Ok(t) = q_cam.single() {
        let (c, _) = world_to_chunk_xz(
            (t.translation().x / VOXEL_SIZE).floor() as i32,
            (t.translation().z / VOXEL_SIZE).floor() as i32,
        );
        c
    } else if let Some(lc) = load_center {
        lc.world_xz
    } else {
        IVec2::ZERO
    };
    let poll_scan_limit = if waiting {
        1024usize
    } else {
        (apply_cap.saturating_mul(3)).clamp(12, 64)
    };
    let mut polled_done_keys: Vec<(IVec2, usize)> = Vec::new();
    let mut prioritized_polled: HashSet<(IVec2, usize)> = HashSet::new();
    if !waiting {
        let immediate_poll_budget = apply_state
            .mesh_update
            .immediate_mesh_keys
            .len()
            .clamp(0, 8);
        let immediate_keys = apply_state
            .mesh_update
            .immediate_mesh_keys
            .iter()
            .copied()
            .take(immediate_poll_budget)
            .collect::<Vec<_>>();
        for key in immediate_keys {
            prioritized_polled.insert(key);
            let Some(task) = apply_state.pending_mesh.0.get_mut(&key) else {
                continue;
            };
            if let Some((ready_key, builds)) = future::block_on(future::poll_once(task)) {
                polled_done_keys.push(key);
                let version = apply_state
                    .mesh_update
                    .pending_mesh_versions
                    .remove(&key)
                    .unwrap_or(0);
                let immediate = apply_state.mesh_update.immediate_mesh_keys.remove(&key);
                immediate_ready.0.retain(|item| item.key != ready_key);
                immediate_ready.0.push_back(ReadyMeshItem {
                    key: ready_key,
                    version,
                    builds,
                    immediate,
                });
            }
        }
    }
    let mut scanned = 0usize;
    for (key, task) in apply_state.pending_mesh.0.iter_mut() {
        if scanned >= poll_scan_limit {
            break;
        }
        if prioritized_polled.contains(key) {
            continue;
        }
        scanned += 1;
        if let Some((ready_key, builds)) = future::block_on(future::poll_once(task)) {
            polled_done_keys.push(*key);
            let version = apply_state
                .mesh_update
                .pending_mesh_versions
                .remove(key)
                .unwrap_or(0);
            let immediate = apply_state.mesh_update.immediate_mesh_keys.remove(key);
            immediate_ready.0.retain(|item| item.key != ready_key);
            immediate_ready.0.push_back(ReadyMeshItem {
                key: ready_key,
                version,
                builds,
                immediate,
            });
        }
    }
    for key in polled_done_keys {
        apply_state.pending_mesh.0.remove(&key);
    }

    let mut ready_results: Vec<ReadyMeshItem> = Vec::new();
    while ready_results.len() < apply_cap {
        if let Some(idx) = immediate_ready.0.iter().position(|item| item.immediate) {
            if let Some(item) = immediate_ready.0.remove(idx) {
                ready_results.push(item);
                continue;
            }
        }
        let next = if waiting {
            immediate_ready.0.pop_front()
        } else {
            let best = immediate_ready
                .0
                .iter()
                .take(512)
                .enumerate()
                .min_by_key(|(_, item)| {
                    let dx = item.key.0.x - center_c.x;
                    let dz = item.key.0.y - center_c.y;
                    (dx * dx + dz * dz, item.key.1)
                })
                .map(|(idx, _)| idx);
            best.and_then(|idx| immediate_ready.0.remove(idx))
                .or_else(|| immediate_ready.0.pop_front())
        };
        let Some(item) = next else {
            break;
        };
        ready_results.push(item);
    }

    let apply_budget_ms = if waiting {
        10.0
    } else if in_game {
        if frame_ms > 34.0 {
            1.0
        } else if frame_ms > 26.0 {
            1.4
        } else if frame_ms > 20.0 {
            2.0
        } else {
            2.6
        }
    } else {
        6.0
    };
    let mut applied_count = 0usize;
    let mut immediate_applied = 0usize;
    let immediate_apply_budget = if waiting { usize::MAX } else { 2usize };
    let mut ready_iter = ready_results.into_iter();
    while let Some(item) = ready_iter.next() {
        let bypass_budget = item.immediate && immediate_applied < immediate_apply_budget;
        if !bypass_budget
            && applied_count > 0
            && stage_start.elapsed().as_secs_f32() * 1000.0 >= apply_budget_ms
        {
            immediate_ready.0.push_front(item);
            for queued in ready_iter {
                immediate_ready.0.push_back(queued);
            }
            break;
        }
        let ((coord, sub), version, builds, immediate) =
            (item.key, item.version, item.builds, item.immediate);
        let desired_version = apply_state
            .mesh_update
            .desired_mesh_versions
            .get(&(coord, sub))
            .copied()
            .unwrap_or(0);
        if version < desired_version {
            enqueue_mesh_fast(
                &mut backlog,
                &mut backlog_set,
                &apply_state.pending_mesh,
                (coord, sub),
            );
            continue;
        }

        let s = VOXEL_SIZE;
        let origin = Vec3::new(
            (coord.x * CX as i32) as f32 * s,
            (Y_MIN as f32) * s,
            (coord.y * CZ as i32) as f32 * s,
        );

        // Build, render meshes, collect physics arrays.
        let mut phys_positions: Vec<[f32; 3]> = Vec::new();
        let mut phys_indices: Vec<u32> = Vec::new();
        let mut combined_builds: Vec<(BlockId, MeshBuild)> = Vec::with_capacity(builds.len());
        let mut merged_fluid: Option<(BlockId, MeshBuild)> = None;
        for (bid, mb) in builds {
            if reg.is_fluid(bid) {
                if let Some((_, fluid_build)) = merged_fluid.as_mut() {
                    let base = fluid_build.pos.len() as u32;
                    let mut mb = mb;
                    fluid_build.pos.append(&mut mb.pos);
                    fluid_build.nrm.append(&mut mb.nrm);
                    fluid_build.uv.append(&mut mb.uv);
                    fluid_build.ctm.append(&mut mb.ctm);
                    fluid_build.tile_rect.append(&mut mb.tile_rect);
                    fluid_build.idx.extend(mb.idx.into_iter().map(|i| base + i));
                } else {
                    merged_fluid = Some((bid, mb));
                }
            } else {
                combined_builds.push((bid, mb));
            }
        }
        if let Some(fluid_build) = merged_fluid.take() {
            combined_builds.push(fluid_build);
        }

        let mesh_key = (coord, sub);
        let mesh_fingerprint = fingerprint_mesh_builds(&combined_builds);
        let mesh_changed = apply_state
            .mesh_update
            .last_mesh_fingerprint
            .get(&mesh_key)
            .copied()
            != Some(mesh_fingerprint);
        if mesh_changed {
            apply_state
                .mesh_update
                .last_mesh_fingerprint
                .insert(mesh_key, mesh_fingerprint);
        }

        for (bid, mb) in &combined_builds {
            if reg.collision_uses_render_mesh(*bid) {
                let base = phys_positions.len() as u32;
                phys_positions.extend_from_slice(&mb.pos);
                phys_indices.extend(mb.idx.iter().map(|i| base + *i));
            }
        }

        if mesh_changed {
            // Reuse existing render entities per (coord,sub,block) where possible.
            // This avoids heavy entity churn during frequent remesh updates.
            let old_keys: Vec<_> = apply_state
                .mesh_index
                .map
                .keys()
                .cloned()
                .filter(|(c, s, _)| c == &coord && *s as usize == sub)
                .collect();
            let mut reusable_mesh_entities: HashMap<BlockId, Entity> = HashMap::new();
            for key in old_keys {
                if let Some(ent) = apply_state.mesh_index.map.remove(&key) {
                    reusable_mesh_entities.insert(key.2, ent);
                }
            }

            for (bid, mb) in combined_builds {
                if mb.pos.is_empty() {
                    continue;
                }

                let mesh = mb.into_mesh();

                let mesh_handle = apply_state.meshes.add(mesh);
                let ent = if let Some(ent) = reusable_mesh_entities.remove(&bid) {
                    if let Ok(Mesh3d(old_handle)) = q_mesh.get(ent) {
                        apply_state.meshes.remove(old_handle.id());
                    }

                    if reg.is_fluid(bid) {
                        if let Some(water_mat) = water_mat.as_ref() {
                            commands
                                .entity(ent)
                                .remove::<MeshMaterial3d<TerrainChunkMaterial>>();
                            commands.entity(ent).insert((
                                Mesh3d(mesh_handle),
                                MeshMaterial3d::<WaterMaterial>(water_mat.0.clone()),
                                Transform::from_translation(origin),
                                SubchunkMesh {
                                    coord,
                                    sub: sub as u8,
                                    block: bid,
                                },
                                Name::new(format!(
                                    "chunk({},{}) sub{} water{}",
                                    coord.x, coord.y, sub, bid
                                )),
                            ));
                        } else {
                            let Some(handle) = terrain_mats.0.get(&bid).cloned() else {
                                continue;
                            };
                            commands
                                .entity(ent)
                                .remove::<MeshMaterial3d<WaterMaterial>>();
                            commands.entity(ent).insert((
                                Mesh3d(mesh_handle),
                                MeshMaterial3d::<TerrainChunkMaterial>(handle),
                                Transform::from_translation(origin),
                                SubchunkMesh {
                                    coord,
                                    sub: sub as u8,
                                    block: bid,
                                },
                                Name::new(format!(
                                    "chunk({},{}) sub{} block{}",
                                    coord.x, coord.y, sub, bid
                                )),
                            ));
                        }
                    } else {
                        let Some(handle) = terrain_mats.0.get(&bid).cloned() else {
                            continue;
                        };
                        commands
                            .entity(ent)
                            .remove::<MeshMaterial3d<WaterMaterial>>();
                        commands.entity(ent).insert((
                            Mesh3d(mesh_handle),
                            MeshMaterial3d::<TerrainChunkMaterial>(handle),
                            Transform::from_translation(origin),
                            SubchunkMesh {
                                coord,
                                sub: sub as u8,
                                block: bid,
                            },
                            Name::new(format!(
                                "chunk({},{}) sub{} block{}",
                                coord.x, coord.y, sub, bid
                            )),
                        ));
                    }
                    ent
                } else if reg.is_fluid(bid) {
                    if let Some(water_mat) = water_mat.as_ref() {
                        commands
                            .spawn((
                                Mesh3d(mesh_handle),
                                MeshMaterial3d::<WaterMaterial>(water_mat.0.clone()),
                                Transform::from_translation(origin),
                                SubchunkMesh {
                                    coord,
                                    sub: sub as u8,
                                    block: bid,
                                },
                                Name::new(format!(
                                    "chunk({},{}) sub{} water{}",
                                    coord.x, coord.y, sub, bid
                                )),
                            ))
                            .id()
                    } else {
                        let Some(handle) = terrain_mats.0.get(&bid).cloned() else {
                            continue;
                        };
                        commands
                            .spawn((
                                Mesh3d(mesh_handle),
                                MeshMaterial3d::<TerrainChunkMaterial>(handle),
                                Transform::from_translation(origin),
                                SubchunkMesh {
                                    coord,
                                    sub: sub as u8,
                                    block: bid,
                                },
                                Name::new(format!(
                                    "chunk({},{}) sub{} block{}",
                                    coord.x, coord.y, sub, bid
                                )),
                            ))
                            .id()
                    }
                } else {
                    let Some(handle) = terrain_mats.0.get(&bid).cloned() else {
                        continue;
                    };
                    commands
                        .spawn((
                            Mesh3d(mesh_handle),
                            MeshMaterial3d::<TerrainChunkMaterial>(handle),
                            Transform::from_translation(origin),
                            SubchunkMesh {
                                coord,
                                sub: sub as u8,
                                block: bid,
                            },
                            Name::new(format!(
                                "chunk({},{}) sub{} block{}",
                                coord.x, coord.y, sub, bid
                            )),
                        ))
                        .id()
                };
                apply_state
                    .mesh_index
                    .map
                    .insert((coord, sub as u8, bid), ent);
            }

            for (_, ent) in reusable_mesh_entities {
                if let Ok(Mesh3d(old_handle)) = q_mesh.get(ent) {
                    apply_state.meshes.remove(old_handle.id());
                }
                safe_despawn_entity(&mut commands, ent);
            }
        }

        if let Some(chunk) = apply_state.chunk_map.chunks.get(&coord) {
            append_custom_box_colliders_for_subchunk(
                chunk,
                &reg,
                sub,
                VOXEL_SIZE,
                &mut phys_positions,
                &mut phys_indices,
            );
        }

        // ----- Physics collider handling -----
        let need_collider = !phys_positions.is_empty();
        let collider_key = (coord, sub as u8);
        let has_collider_state = apply_state.collider_index.0.contains_key(&collider_key)
            || apply_state.pending_collider.0.contains_key(&collider_key)
            || apply_state.coll_backlog.0.contains_key(&collider_key);

        if need_collider {
            let collider_fingerprint =
                fingerprint_collider_geometry(&phys_positions, &phys_indices);
            let collider_changed = apply_state
                .mesh_update
                .last_collider_fingerprint
                .get(&collider_key)
                .copied()
                != Some(collider_fingerprint);
            if collider_changed || !has_collider_state {
                let desired_collider_version = apply_state
                    .mesh_update
                    .desired_collider_versions
                    .entry(collider_key)
                    .or_insert(0);
                *desired_collider_version = desired_collider_version.saturating_add(1);
                let collider_version = *desired_collider_version;
                apply_state
                    .mesh_update
                    .last_collider_fingerprint
                    .insert(collider_key, collider_fingerprint);
                apply_state
                    .collider_ready
                    .0
                    .retain(|(key, build)| *key != collider_key || build.version >= collider_version);
                if immediate {
                    // For local player edits: update physics immediately in the same frame.
                    apply_state.coll_backlog.0.remove(&collider_key);
                    apply_state.pending_collider.0.remove(&collider_key);
                    apply_state
                        .mesh_update
                        .pending_collider_versions
                        .remove(&collider_key);
                    let existing_collider_entity =
                        apply_state.collider_index.0.get(&collider_key).copied();

                    // Use a cheap placeholder immediately and build exact collider async.
                    let placeholder = apply_state
                        .chunk_map
                        .chunks
                        .get(&coord)
                        .and_then(|chunk| build_surface_placeholder_collider(chunk, &reg, sub))
                        .or_else(|| build_bounds_collider(&phys_positions));
                    if let Some((collider, local_offset)) = placeholder {
                        let ent = if let Some(existing) = existing_collider_entity {
                            commands.entity(existing).insert((
                                RigidBody::Fixed,
                                collider,
                                Transform::from_translation(origin + local_offset),
                                ChunkColliderProxy { coord },
                                Name::new(format!(
                                    "collider chunk({},{}) sub{}",
                                    coord.x, coord.y, sub
                                )),
                            ));
                            existing
                        } else {
                            commands
                                .spawn((
                                    RigidBody::Fixed,
                                    collider,
                                    Transform::from_translation(origin + local_offset),
                                    ChunkColliderProxy { coord },
                                    Name::new(format!(
                                        "collider chunk({},{}) sub{}",
                                        coord.x, coord.y, sub
                                    )),
                                ))
                                .id()
                        };
                        apply_state.collider_index.0.insert(collider_key, ent);
                    }

                    apply_state.coll_backlog.0.insert(
                        collider_key,
                        ColliderTodo {
                            coord,
                            sub: sub as u8,
                            version: collider_version,
                            origin,
                            positions: phys_positions,
                            indices: phys_indices,
                        },
                    );
                } else {
                    let has_existing_collider =
                        apply_state.collider_index.0.contains_key(&collider_key);
                    // Keep old collider until the new async collider is ready.
                    // Replacing an existing collider with a coarse placeholder can open temporary holes.
                    let should_place_placeholder_now = !has_existing_collider;

                    if should_place_placeholder_now {
                        apply_state.pending_collider.0.remove(&collider_key);
                        apply_state
                            .mesh_update
                            .pending_collider_versions
                            .remove(&collider_key);
                        if let Some(ent) = apply_state.collider_index.0.remove(&collider_key) {
                            safe_despawn_entity(&mut commands, ent);
                        }

                        // Keep gameplay stable: never leave freshly generated chunks without collider.
                        let placeholder = apply_state
                            .chunk_map
                            .chunks
                            .get(&coord)
                            .and_then(|chunk| build_surface_placeholder_collider(chunk, &reg, sub))
                            .or_else(|| build_bounds_collider(&phys_positions));
                        if let Some((collider, local_offset)) = placeholder {
                            let ent = commands
                                .spawn((
                                    RigidBody::Fixed,
                                    collider,
                                    Transform::from_translation(origin + local_offset),
                                    ChunkColliderProxy { coord },
                                    Name::new(format!(
                                        "collider chunk({},{}) sub{}",
                                        coord.x, coord.y, sub
                                    )),
                                ))
                                .id();
                            apply_state.collider_index.0.insert(collider_key, ent);
                        }
                    }

                    apply_state.coll_backlog.0.insert(
                        collider_key,
                        ColliderTodo {
                            coord,
                            sub: sub as u8,
                            version: collider_version,
                            origin,
                            positions: phys_positions,
                            indices: phys_indices,
                        },
                    );
                }
            }
        } else {
            // No render-mesh collision geometry does not necessarily mean "no collision":
            // the subchunk may still contain solid blocks that are currently hidden from meshing.
            let placeholder = apply_state
                .chunk_map
                .chunks
                .get(&coord)
                .and_then(|chunk| build_surface_placeholder_collider(chunk, &reg, sub));

            apply_state.coll_backlog.0.remove(&collider_key);
            apply_state.pending_collider.0.remove(&collider_key);
            apply_state
                .mesh_update
                .pending_collider_versions
                .remove(&collider_key);
            let desired_collider_version = apply_state
                .mesh_update
                .desired_collider_versions
                .entry(collider_key)
                .or_insert(0);
            *desired_collider_version = desired_collider_version.saturating_add(1);
            let collider_version = *desired_collider_version;
            apply_state
                .collider_ready
                .0
                .retain(|(key, build)| *key != collider_key || build.version >= collider_version);
            if let Some((collider, local_offset)) = placeholder {
                let ent = if let Some(existing) = apply_state.collider_index.0.get(&collider_key).copied()
                {
                    commands.entity(existing).insert((
                        RigidBody::Fixed,
                        collider,
                        Transform::from_translation(origin + local_offset),
                        ChunkColliderProxy { coord },
                        Name::new(format!(
                            "collider chunk({},{}) sub{}",
                            coord.x, coord.y, sub
                        )),
                    ));
                    existing
                } else {
                    commands
                        .spawn((
                            RigidBody::Fixed,
                            collider,
                            Transform::from_translation(origin + local_offset),
                            ChunkColliderProxy { coord },
                            Name::new(format!(
                                "collider chunk({},{}) sub{}",
                                coord.x, coord.y, sub
                            )),
                        ))
                        .id()
                };
                apply_state.collider_index.0.insert(collider_key, ent);
            } else if let Some(ent) = apply_state.collider_index.0.remove(&collider_key) {
                safe_despawn_entity(&mut commands, ent);
            }
            apply_state
                .mesh_update
                .last_collider_fingerprint
                .remove(&collider_key);
        }

        if let Some(chunk) = apply_state.chunk_map.chunks.get_mut(&coord) {
            chunk.clear_dirty(sub);
            if chunk.dirty_mask == 0 {
                apply_state.ready_set.0.insert(coord);
                telemetry_mark_chunk_ready(
                    coord,
                    time.elapsed_secs_f64(),
                    &mut apply_state.ready_latency,
                    &mut apply_state.stage_telemetry,
                );
            }
        }
        applied_count += 1;
        if immediate {
            immediate_applied += 1;
        }
    }

    apply_state.stage_telemetry.stage_mesh_apply_ms = smooth_stage_ms(
        apply_state.stage_telemetry.stage_mesh_apply_ms,
        stage_start.elapsed().as_secs_f32() * 1000.0,
    );
}

/// Runs the `schedule_collider_build_tasks` routine for schedule collider build tasks in the `generator::chunk::chunk_builder` module.
fn schedule_collider_build_tasks(
    mut backlog: ResMut<ColliderBacklog>,
    mut pending: ResMut<PendingColliderBuild>,
    mut mesh_update: ResMut<MeshUpdateState>,
    game_config: Res<GlobalConfig>,
    app_state: Res<State<AppState>>,
    mut stage_telemetry: ResMut<ChunkStageTelemetry>,
    q_cam: Query<&GlobalTransform, With<Camera3d>>,
    load_center: Option<Res<LoadCenter>>,
) {
    let stage_start = Instant::now();
    let waiting = is_waiting(&app_state);
    let waiting_collider_cap = (AsyncComputeTaskPool::get().thread_num().max(1) * 4).clamp(16, 96);
    let worker_hint = AsyncComputeTaskPool::get().thread_num().max(1);
    let max_inflight = if waiting {
        waiting_collider_cap
    } else {
        game_config
            .graphics
            .chunk_collider_max_inflight
            .max((worker_hint / 2).clamp(6, 12))
            .clamp(6, 12)
    };
    let pool = AsyncComputeTaskPool::get();
    let center_blocks = if let Ok(t) = q_cam.single() {
        IVec2::new(
            (t.translation().x / VOXEL_SIZE).floor() as i32,
            (t.translation().z / VOXEL_SIZE).floor() as i32,
        )
    } else if let Some(lc) = load_center {
        IVec2::new(
            lc.world_xz.x * CX as i32 + (CX as i32 / 2),
            lc.world_xz.y * CZ as i32 + (CZ as i32 / 2),
        )
    } else {
        IVec2::ZERO
    };

    let collider_activation_blocks = game_config
        .graphics
        .chunk_collider_activation_radius_blocks
        .max(1);
    let radius_sq = i64::from(collider_activation_blocks) * i64::from(collider_activation_blocks);

    while pending.0.len() < max_inflight {
        let Some(key) = backlog
            .0
            .keys()
            .take(1024)
            .copied()
            .filter(|(coord, _)| {
                waiting || chunk_min_distance_sq_blocks(*coord, center_blocks) <= radius_sq
            })
            .min_by_key(|(coord, sub)| (chunk_min_distance_sq_blocks(*coord, center_blocks), *sub))
        else {
            break;
        };
        let Some(todo) = backlog.0.remove(&key) else {
            continue;
        };

        let task = pool.spawn(async move {
            // Keep collisions robust on open/non-manifold terrain meshes.
            let flags = TriMeshFlags::FIX_INTERNAL_EDGES
                | TriMeshFlags::MERGE_DUPLICATE_VERTICES
                | TriMeshFlags::DELETE_DEGENERATE_TRIANGLES;
            let (collider, local_offset) =
                build_collider_with_fallback(todo.positions, todo.indices, flags);
            (
                (todo.coord, todo.sub),
                ColliderBuild {
                    version: todo.version,
                    origin: todo.origin + local_offset,
                    collider,
                },
            )
        });
        mesh_update.pending_collider_versions.insert(key, todo.version);
        pending.0.insert(key, task);
    }

    stage_telemetry.stage_collider_schedule_ms = smooth_stage_ms(
        stage_telemetry.stage_collider_schedule_ms,
        stage_start.elapsed().as_secs_f32() * 1000.0,
    );
}

// Extracted tail systems/helpers to keep this module focused and below 2000 lines.
include!("chunk_runtime_tail.rs");
