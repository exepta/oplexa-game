use crate::core::config::{GlobalConfig, WorldGenConfig};
use crate::core::entities::player::Player;
use crate::core::events::chunk_events::{ChunkUnloadEvent, SubChunkNeedRemeshEvent};
use crate::core::multiplayer::MultiplayerConnectionState;
use crate::core::shader::terrain_shader::{TerrainChunkMatIndex, TerrainChunkMaterial};
use crate::core::states::states::{AppState, InGameStates, LoadingStates};
use crate::core::world::biome::registry::BiomeRegistry;
use crate::core::world::block::*;
use crate::core::world::chunk::*;
use crate::core::world::chunk_dimension::*;
use crate::core::world::save::WorldSave;
use crate::generator::chunk::chunk_struct::*;
use crate::generator::chunk::chunk_utils::*;
use crate::generator::chunk::trees::registry::TreeRegistry;
use crate::generator::chunk::water_builder::WaterReadySet;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;
use bevy::tasks::futures_lite::future;
use bevy_rapier3d::prelude::{Collider, ColliderDisabled, RigidBody, TriMeshFlags};
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet, VecDeque};
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
    origin: Vec3,
    positions: Vec<[f32; 3]>,
    indices: Vec<u32>,
}

/// Represents collider build used by the `generator::chunk::chunk_builder` module.
struct ColliderBuild {
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

/// Represents pending chunk save used by the `generator::chunk::chunk_builder` module.
#[derive(Resource, Default)]
struct PendingChunkSave(pub HashMap<IVec2, bevy::tasks::Task<IVec2>>);

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
const HIDDEN_PRELOAD_RING: i32 = 4;

#[derive(Resource, Default)]
struct StreamLookaheadState {
    last_cam_xz: Option<Vec2>,
    smoothed_dir: Vec2,
}

#[derive(Resource, Default)]
struct RingDeadlineState {
    visible_miss_frames: u32,
    preload_miss_frames: u32,
}

#[derive(Resource, Default)]
struct MeshBacklogSet(HashSet<(IVec2, usize)>);

struct ReadyMeshItem {
    key: (IVec2, usize),
    builds: Vec<(BlockId, MeshBuild)>,
    immediate: bool,
}

#[derive(Resource, Default)]
struct ImmediateMeshReadyQueue(VecDeque<ReadyMeshItem>);

#[derive(Resource, Default)]
struct ChunkReadySet(HashSet<IVec2>);

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
    pending_save: ResMut<'w, PendingChunkSave>,
    coll_backlog: ResMut<'w, ColliderBacklog>,
    ready_set: ResMut<'w, ChunkReadySet>,
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
    pending_save: ResMut<'w, PendingChunkSave>,
    coll_backlog: ResMut<'w, ColliderBacklog>,
    kick_queue: ResMut<'w, KickQueue>,
    kicked: ResMut<'w, KickedOnce>,
    queued: ResMut<'w, QueuedOnce>,
    ready_set: ResMut<'w, ChunkReadySet>,
    _marker: std::marker::PhantomData<&'s ()>,
}

#[derive(SystemParam)]
struct ChunkScheduleState<'w, 's> {
    stream_lookahead: ResMut<'w, StreamLookaheadState>,
    ring_deadlines: ResMut<'w, RingDeadlineState>,
    ready_latency: ResMut<'w, ChunkReadyLatencyState>,
    time: Res<'w, Time>,
    _marker: std::marker::PhantomData<&'s ()>,
}

#[derive(SystemParam)]
struct ChunkMeshApplyState<'w, 's> {
    pending_mesh: ResMut<'w, PendingMesh>,
    mesh_index: ResMut<'w, ChunkMeshIndex>,
    collider_index: ResMut<'w, ChunkColliderIndex>,
    pending_collider: ResMut<'w, PendingColliderBuild>,
    meshes: ResMut<'w, Assets<Mesh>>,
    chunk_map: ResMut<'w, ChunkMap>,
    ready_set: ResMut<'w, ChunkReadySet>,
    coll_backlog: ResMut<'w, ColliderBacklog>,
    ready_latency: ResMut<'w, ChunkReadyLatencyState>,
    stage_telemetry: ResMut<'w, ChunkStageTelemetry>,
    _marker: std::marker::PhantomData<&'s ()>,
}

/// Represents chunk builder used by the `generator::chunk::chunk_builder` module.
pub struct ChunkBuilder;

impl Plugin for ChunkBuilder {
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
            .init_resource::<PendingChunkSave>()
            .init_resource::<KickQueue>()
            .init_resource::<KickedOnce>()
            .init_resource::<QueuedOnce>()
            .init_resource::<StreamLookaheadState>()
            .init_resource::<RingDeadlineState>()
            .init_resource::<MeshBacklogSet>()
            .init_resource::<ImmediateMeshReadyQueue>()
            .init_resource::<ChunkReadySet>()
            .init_resource::<ChunkStageTelemetry>()
            .init_resource::<ChunkReadyLatencyState>()
            // --- Generation, Meshing, Kick etc. ---
            .add_systems(
                Update,
                (
                    collect_chunk_save_tasks.run_if(
                        in_state(AppState::Loading(LoadingStates::BaseGen))
                            .or(in_state(AppState::InGame(InGameStates::Game))),
                    ),
                    schedule_chunk_generation.run_if(
                        in_state(AppState::Loading(LoadingStates::BaseGen))
                            .or(in_state(AppState::InGame(InGameStates::Game))),
                    ),
                    collect_generated_chunks.run_if(
                        in_state(AppState::Loading(LoadingStates::BaseGen))
                            .or(in_state(AppState::InGame(InGameStates::Game))),
                    ),
                    schedule_remesh_tasks_from_events
                        .in_set(VoxelStage::Meshing)
                        .run_if(
                            in_state(AppState::Loading(LoadingStates::BaseGen))
                                .or(in_state(AppState::Loading(LoadingStates::CaveGen)))
                                .or(in_state(AppState::Loading(LoadingStates::WaterGen)))
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
                                .or(in_state(AppState::Loading(LoadingStates::CaveGen)))
                                .or(in_state(AppState::Loading(LoadingStates::WaterGen)))
                                .or(in_state(AppState::InGame(InGameStates::Game))),
                        ),
                    drain_mesh_backlog.run_if(
                        in_state(AppState::Loading(LoadingStates::BaseGen))
                            .or(in_state(AppState::Loading(LoadingStates::CaveGen)))
                            .or(in_state(AppState::Loading(LoadingStates::WaterGen)))
                            .or(in_state(AppState::InGame(InGameStates::Game))),
                    ),
                    (
                        schedule_collider_build_tasks,
                        collect_finished_collider_builds,
                    )
                        .chain()
                        .run_if(
                            in_state(AppState::Loading(LoadingStates::BaseGen))
                                .or(in_state(AppState::Loading(LoadingStates::CaveGen)))
                                .or(in_state(AppState::Loading(LoadingStates::WaterGen)))
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
                        .or(in_state(AppState::Loading(LoadingStates::CaveGen)))
                        .or(in_state(AppState::Loading(LoadingStates::WaterGen)))
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

    let ready = if multiplayer_connection.uses_local_save_data() {
        area_ready(
            center,
            initial_radius,
            &chunk_map,
            &pending_gen,
            &pending_mesh,
            &backlog,
        )
    } else {
        area_chunks_in_map(center, initial_radius, &chunk_map)
    };

    if ready {
        commands.remove_resource::<LoadCenter>();
        if multiplayer_connection.uses_local_save_data() {
            next.set(AppState::Loading(LoadingStates::WaterGen));
        } else {
            next.set(AppState::InGame(InGameStates::Game));
        }
    }
}

//System
/// Runs the `schedule_chunk_generation` routine for schedule chunk generation in the `generator::chunk::chunk_builder` module.
fn schedule_chunk_generation(
    mut pending: ResMut<PendingGen>,
    pending_mesh: Res<PendingMesh>,
    backlog: Res<MeshBacklog>,
    chunk_map: Res<ChunkMap>,
    pending_save: Res<PendingChunkSave>,
    reg: Res<BlockRegistry>,
    biomes: Res<BiomeRegistry>,
    trees: Res<TreeRegistry>,
    gen_cfg: Res<WorldGenConfig>,
    game_config: Res<GlobalConfig>,
    ws: Res<WorldSave>,
    q_cam: Query<&GlobalTransform, With<Camera3d>>,
    load_center: Option<Res<LoadCenter>>,
    app_state: Res<State<AppState>>,
    mut schedule_state: ChunkScheduleState,
    multiplayer_connection: Res<MultiplayerConnectionState>,
) {
    if !multiplayer_connection.uses_local_save_data() {
        return;
    }

    let mut frame_move_dir = Vec2::ZERO;
    let mut local_in_chunk = UVec2::new((CX / 2) as u32, (CZ / 2) as u32);
    let center_c = if let Ok(t) = q_cam.single() {
        let tr = t.translation();
        let cam_xz = Vec2::new(tr.x, tr.z);
        if let Some(last) = schedule_state.stream_lookahead.last_cam_xz {
            let delta = cam_xz - last;
            if delta.length_squared() > 0.0001 {
                frame_move_dir = delta.normalize();
            }
        }
        schedule_state.stream_lookahead.last_cam_xz = Some(cam_xz);

        let (c, lc) = world_to_chunk_xz(
            (tr.x / VOXEL_SIZE).floor() as i32,
            (tr.z / VOXEL_SIZE).floor() as i32,
        );
        local_in_chunk = lc;
        c
    } else if let Some(lc) = load_center {
        lc.world_xz
    } else {
        IVec2::ZERO
    };

    let waiting = is_waiting(&app_state);
    let mut max_inflight = if waiting {
        BIG
    } else {
        game_config.graphics.chunk_gen_max_inflight.max(1)
    };
    let mut per_frame_submit = if waiting {
        BIG
    } else {
        game_config.graphics.chunk_gen_submit_per_frame.max(1)
    };

    if frame_move_dir.length_squared() > 0.0 {
        let blended = schedule_state.stream_lookahead.smoothed_dir * 0.8 + frame_move_dir * 0.2;
        schedule_state.stream_lookahead.smoothed_dir = blended.normalize_or_zero();
    } else {
        schedule_state.stream_lookahead.smoothed_dir *= 0.9;
        if schedule_state
            .stream_lookahead
            .smoothed_dir
            .length_squared()
            < 0.0001
        {
            schedule_state.stream_lookahead.smoothed_dir = Vec2::ZERO;
        }
    }

    if !waiting
        && schedule_state
            .stream_lookahead
            .smoothed_dir
            .length_squared()
            > 0.01
    {
        per_frame_submit = (per_frame_submit + per_frame_submit / 2).min(max_inflight);
    }

    if !waiting {
        let mesh_pressure = pending_mesh.0.len() + backlog.0.len();
        if mesh_pressure > 4_000 {
            max_inflight = max_inflight.min(6);
            per_frame_submit = per_frame_submit.min(1);
        } else if mesh_pressure > 2_500 {
            max_inflight = max_inflight.min(10);
            per_frame_submit = per_frame_submit.min(2);
        } else if mesh_pressure > 1_500 {
            max_inflight = max_inflight.min(16);
            per_frame_submit = per_frame_submit.min(3);
        } else if mesh_pressure > 800 {
            max_inflight = max_inflight.min(24);
            per_frame_submit = per_frame_submit.min(5);
        }

        let urgent = (visible_radius(game_config.graphics.chunk_range) + 1).max(1);
        let mut missing_near = 0usize;
        for dz in -urgent..=urgent {
            for dx in -urgent..=urgent {
                let c = IVec2::new(center_c.x + dx, center_c.y + dz);
                if !chunk_map.chunks.contains_key(&c)
                    && !pending.0.contains_key(&c)
                    && !pending_save.0.contains_key(&c)
                {
                    missing_near += 1;
                }
            }
        }
        if missing_near > 0 && mesh_pressure < 1_200 {
            per_frame_submit = per_frame_submit.max(12).min(max_inflight);
        }
    }

    if pending.0.len() >= max_inflight {
        return;
    }

    let load_radius = if waiting {
        visible_radius(game_config.graphics.chunk_range)
    } else {
        loaded_radius(game_config.graphics.chunk_range)
    };
    let mut lookahead_center = center_c;
    if !waiting
        && schedule_state
            .stream_lookahead
            .smoothed_dir
            .length_squared()
            > 0.01
    {
        let visible = visible_radius(game_config.graphics.chunk_range);
        let hidden_ring = (load_radius - visible).max(0);
        let lookahead_chunks = hidden_ring.max(1);
        let ox = (schedule_state.stream_lookahead.smoothed_dir.x * lookahead_chunks as f32).round()
            as i32;
        let oz = (schedule_state.stream_lookahead.smoothed_dir.y * lookahead_chunks as f32).round()
            as i32;
        if ox != 0 || oz != 0 {
            lookahead_center = center_c + IVec2::new(ox, oz);
        }
    }

    if !waiting {
        let mut edge_bias = IVec2::ZERO;
        let edge_lo_x = (CX as u32) / 4;
        let edge_hi_x = (CX as u32 * 3) / 4;
        let edge_lo_z = (CZ as u32) / 4;
        let edge_hi_z = (CZ as u32 * 3) / 4;

        if local_in_chunk.x <= edge_lo_x {
            edge_bias.x = -1;
        } else if local_in_chunk.x >= edge_hi_x {
            edge_bias.x = 1;
        }
        if local_in_chunk.y <= edge_lo_z {
            edge_bias.y = -1;
        } else if local_in_chunk.y >= edge_hi_z {
            edge_bias.y = 1;
        }

        if edge_bias != IVec2::ZERO {
            let visible = visible_radius(game_config.graphics.chunk_range);
            let hidden_ring = (load_radius - visible).max(0);
            let edge_push = (hidden_ring / 2).max(1);
            lookahead_center += edge_bias * edge_push;
        }
    }

    // --- NEW: Arc-wrap registries once per system tick (cheap per task) ---
    let reg_arc = Arc::new(reg.clone());
    let biomes_arc = Arc::new(biomes.clone());
    let trees_arc = Arc::new(trees.clone());

    let cfg_clone = gen_cfg.clone();
    let ws_root = ws.root.clone();
    let pool = AsyncComputeTaskPool::get();

    let visible = visible_radius(game_config.graphics.chunk_range);
    let mut visible_candidates: Vec<IVec2> = Vec::new();
    for dz in -visible..=visible {
        for dx in -visible..=visible {
            let c = IVec2::new(center_c.x + dx, center_c.y + dz);
            if chunk_map.chunks.contains_key(&c)
                || pending.0.contains_key(&c)
                || pending_save.0.contains_key(&c)
            {
                continue;
            }
            visible_candidates.push(c);
        }
    }

    let mut preload_candidates: Vec<IVec2> = Vec::new();
    let search_center = if waiting { center_c } else { lookahead_center };
    for dz in -load_radius..=load_radius {
        for dx in -load_radius..=load_radius {
            let c = IVec2::new(search_center.x + dx, search_center.y + dz);
            if (c.x - center_c.x).abs() <= visible && (c.y - center_c.y).abs() <= visible {
                continue;
            }
            if chunk_map.chunks.contains_key(&c)
                || pending.0.contains_key(&c)
                || pending_save.0.contains_key(&c)
            {
                continue;
            }
            preload_candidates.push(c);
        }
    }

    visible_candidates.sort_by_key(|c| {
        let dx = c.x - center_c.x;
        let dz = c.y - center_c.y;
        dx * dx + dz * dz
    });

    preload_candidates.sort_by_key(|c| {
        let ldx = c.x - lookahead_center.x;
        let ldz = c.y - lookahead_center.y;
        let lookahead_dist = ldx * ldx + ldz * ldz;
        let dx = c.x - center_c.x;
        let dz = c.y - center_c.y;
        let center_dist = dx * dx + dz * dz;
        (lookahead_dist, center_dist)
    });

    if !waiting
        && !visible_candidates.is_empty()
        && schedule_state.ring_deadlines.visible_miss_frames >= 2
    {
        per_frame_submit = per_frame_submit.max(24).min(max_inflight);
    } else if !waiting
        && !preload_candidates.is_empty()
        && schedule_state.ring_deadlines.preload_miss_frames >= 6
    {
        per_frame_submit = per_frame_submit.max(10).min(max_inflight);
    }

    let mut budget = max_inflight
        .saturating_sub(pending.0.len())
        .min(per_frame_submit);

    let mut submitted_visible = 0usize;
    let mut submitted_preload = 0usize;
    let now_secs = schedule_state.time.elapsed_secs_f64();

    for c in visible_candidates.iter().copied() {
        if budget == 0 {
            break;
        }

        // clone the inexpensive Arcs for this task
        let reg_for_task = Arc::clone(&reg_arc);
        let biomes_for_task = Arc::clone(&biomes_arc);
        let trees_for_task = Arc::clone(&trees_arc);
        let cfg = cfg_clone.clone();
        let root = ws_root.clone();

        let task = pool.spawn(async move {
            // NOTE: load_or_gen signature: (root, coord, &BlockRegistry, &BiomeRegistry, &TreeRegistry, cfg)
            let data = load_or_gen_chunk_async(
                root,
                c,
                &*reg_for_task,    // deref Arc -> &BlockRegistry
                &*biomes_for_task, // deref Arc -> &BiomeRegistry
                &*trees_for_task,
                cfg,
            )
            .await;
            (c, data)
        });

        pending.0.insert(c, task);
        telemetry_mark_chunk_requested(c, now_secs, &mut schedule_state.ready_latency);
        budget -= 1;
        submitted_visible += 1;
    }

    for c in preload_candidates.iter().copied() {
        if budget == 0 {
            break;
        }

        let reg_for_task = Arc::clone(&reg_arc);
        let biomes_for_task = Arc::clone(&biomes_arc);
        let trees_for_task = Arc::clone(&trees_arc);
        let cfg = cfg_clone.clone();
        let root = ws_root.clone();

        let task = pool.spawn(async move {
            let data = load_or_gen_chunk_async(
                root,
                c,
                &*reg_for_task,
                &*biomes_for_task,
                &*trees_for_task,
                cfg,
            )
            .await;
            (c, data)
        });

        pending.0.insert(c, task);
        telemetry_mark_chunk_requested(c, now_secs, &mut schedule_state.ready_latency);
        budget -= 1;
        submitted_preload += 1;
    }

    if !waiting {
        if visible_candidates.is_empty() || submitted_visible > 0 {
            schedule_state.ring_deadlines.visible_miss_frames = 0;
        } else {
            schedule_state.ring_deadlines.visible_miss_frames = schedule_state
                .ring_deadlines
                .visible_miss_frames
                .saturating_add(1);
        }

        if preload_candidates.is_empty() || submitted_preload > 0 {
            schedule_state.ring_deadlines.preload_miss_frames = 0;
        } else {
            schedule_state.ring_deadlines.preload_miss_frames = schedule_state
                .ring_deadlines
                .preload_miss_frames
                .saturating_add(1);
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
    q_cam: Query<&GlobalTransform, With<Camera3d>>,
    load_center: Option<Res<LoadCenter>>,
    app_state: Res<State<AppState>>,
) {
    if chunk_map.chunks.is_empty() {
        backlog.0.clear();
        backlog_set.0.clear();
        return;
    }

    let waiting = is_waiting(&app_state);
    let max_inflight_mesh = if waiting {
        BIG
    } else {
        game_config.graphics.chunk_mesh_max_inflight.max(1)
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

    while pending_mesh.0.len() < max_inflight_mesh {
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
        backlog_set.0.remove(&(coord, sub));
        if pending_mesh.0.contains_key(&(coord, sub)) {
            continue;
        }

        let mut subs = vec![sub];
        if !waiting && pending_mesh.0.len() + 1 < max_inflight_mesh {
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
) {
    let stage_start = Instant::now();
    let waiting = is_waiting(&app_state);
    let max_inflight_mesh = if waiting {
        BIG
    } else {
        game_config.graphics.chunk_mesh_max_inflight.max(1)
    };
    let mesh_pressure = pending_mesh.0.len() + backlog.0.len();
    let gen_apply_cap = if waiting {
        BIG
    } else if mesh_pressure > 4_000 {
        1
    } else if mesh_pressure > 2_500 {
        2
    } else if mesh_pressure > 1_500 {
        3
    } else {
        6
    };
    let allow_neighbor_enqueue = waiting || mesh_pressure < 1_200;

    let reg_lite = RegLite::from_reg(&reg);
    let mut finished = Vec::new();
    let mut applied_gen = 0usize;

    for (coord, task) in pending_gen.0.iter_mut() {
        if applied_gen >= gen_apply_cap {
            break;
        }
        if let Some((c, mut data)) = future::block_on(future::poll_once(task)) {
            clear_air_only_subchunks_dirty(&mut data);
            chunk_map.chunks.insert(c, data.clone());
            ready_set.0.remove(&c);
            if data.dirty_mask == 0 {
                ready_set.0.insert(c);
                telemetry_mark_chunk_ready(
                    c,
                    time.elapsed_secs_f64(),
                    &mut ready_latency,
                    &mut stage_telemetry,
                );
            }

            let pool = AsyncComputeTaskPool::get();
            let chunk_shared = Arc::new(data.clone());
            let order = sub_priority_order(&data);
            for sub in order {
                if !data.is_dirty(sub) {
                    continue;
                }
                let key = (c, sub);
                let y0 = sub * SEC_H;
                let y1 = (y0 + SEC_H).min(CY);
                let borders = snapshot_borders(&chunk_map, c, y0, y1);

                if pending_mesh.0.len() < max_inflight_mesh {
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
                    pending_mesh.0.insert(key, t);
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

                            let y0 = sub * SEC_H;
                            let y1 = (y0 + SEC_H).min(CY);
                            let borders = snapshot_borders(&chunk_map, n_coord, y0, y1);

                            if pending_mesh.0.len() < max_inflight_mesh {
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
    mut immediate_ready: ResMut<ImmediateMeshReadyQueue>,
    reg: Res<BlockRegistry>,
    terrain_mats: Res<TerrainChunkMatIndex>,
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
    let apply_cap = if waiting {
        BIG
    } else {
        game_config.graphics.chunk_mesh_apply_per_frame.max(1)
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
        BIG
    } else {
        (apply_cap.saturating_mul(4)).clamp(16, 96)
    };
    let mut polled_done_keys: Vec<(IVec2, usize)> = Vec::new();
    let mut scanned = 0usize;
    for (key, task) in apply_state.pending_mesh.0.iter_mut() {
        if scanned >= poll_scan_limit {
            break;
        }
        scanned += 1;
        if let Some((ready_key, builds)) = future::block_on(future::poll_once(task)) {
            polled_done_keys.push(*key);
            immediate_ready.0.retain(|item| item.key != ready_key);
            immediate_ready.0.push_back(ReadyMeshItem {
                key: ready_key,
                builds,
                immediate: false,
            });
        }
    }
    for key in polled_done_keys {
        apply_state.pending_mesh.0.remove(&key);
    }

    let mut ready_results: Vec<ReadyMeshItem> = Vec::new();
    while ready_results.len() < apply_cap {
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

    for item in ready_results {
        let ((coord, sub), builds, immediate) = (item.key, item.builds, item.immediate);
        // Despawn render meshes for this (coord,sub) first (safe).
        let old_keys: Vec<_> = apply_state
            .mesh_index
            .map
            .keys()
            .cloned()
            .filter(|(c, s, _)| c == &coord && *s as usize == sub)
            .collect();
        despawn_mesh_set(
            old_keys,
            &mut apply_state.mesh_index,
            &mut commands,
            &q_mesh,
            &mut apply_state.meshes,
        );

        let s = VOXEL_SIZE;
        let origin = Vec3::new(
            (coord.x * CX as i32) as f32 * s,
            (Y_MIN as f32) * s,
            (coord.y * CZ as i32) as f32 * s,
        );

        // Build, render meshes, collect physics arrays.
        let mut phys_positions: Vec<[f32; 3]> = Vec::new();
        let mut phys_indices: Vec<u32> = Vec::new();

        for (bid, mb) in builds {
            if mb.pos.is_empty() {
                continue;
            }

            // Fluids (e.g. water) are rendered but must not become solid colliders.
            if reg.is_solid_for_collision(bid) {
                let base = phys_positions.len() as u32;
                phys_positions.extend_from_slice(&mb.pos);
                phys_indices.extend(mb.idx.iter().map(|i| base + *i));
            }

            let mesh = mb.into_mesh();

            let Some(handle) = terrain_mats.0.get(&bid).cloned() else {
                continue;
            };
            let ent = commands
                .spawn((
                    Mesh3d(apply_state.meshes.add(mesh)),
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
                .id();
            apply_state
                .mesh_index
                .map
                .insert((coord, sub as u8, bid), ent);
        }

        // ----- Physics collider handling -----
        let need_collider = !phys_positions.is_empty();
        let collider_key = (coord, sub as u8);

        if need_collider {
            if immediate {
                // For local player edits: update physics immediately in the same frame.
                apply_state.coll_backlog.0.remove(&collider_key);
                apply_state.pending_collider.0.remove(&collider_key);
                if let Some(ent) = apply_state.collider_index.0.remove(&collider_key) {
                    safe_despawn_entity(&mut commands, ent);
                }

                // Use a cheap placeholder immediately and build exact collider async.
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
                            ColliderDisabled,
                            Name::new(format!(
                                "collider chunk({},{}) sub{}",
                                coord.x, coord.y, sub
                            )),
                        ))
                        .id();
                    apply_state.collider_index.0.insert(collider_key, ent);
                }

                apply_state.coll_backlog.0.insert(
                    collider_key,
                    ColliderTodo {
                        coord,
                        sub: sub as u8,
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
                                ColliderDisabled,
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
                        origin,
                        positions: phys_positions,
                        indices: phys_indices,
                    },
                );
            }
        } else {
            // No geometry → ensure collider is removed (solid gone).
            apply_state.coll_backlog.0.remove(&collider_key);
            apply_state.pending_collider.0.remove(&collider_key);
            if let Some(ent) = apply_state.collider_index.0.remove(&collider_key) {
                safe_despawn_entity(&mut commands, ent);
            }
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
    game_config: Res<GlobalConfig>,
    app_state: Res<State<AppState>>,
    mut stage_telemetry: ResMut<ChunkStageTelemetry>,
    q_cam: Query<&GlobalTransform, With<Camera3d>>,
    load_center: Option<Res<LoadCenter>>,
) {
    let stage_start = Instant::now();
    let waiting = is_waiting(&app_state);
    let max_inflight = if waiting {
        BIG
    } else {
        game_config.graphics.chunk_collider_max_inflight.max(1)
    };
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

    while pending.0.len() < max_inflight {
        let Some(key) = backlog
            .0
            .keys()
            .take(1024)
            .copied()
            .min_by_key(|(coord, sub)| {
                let dx = coord.x - center_c.x;
                let dz = coord.y - center_c.y;
                (dx * dx + dz * dz, *sub)
            })
        else {
            break;
        };
        let Some(todo) = backlog.0.remove(&key) else {
            continue;
        };

        let task = pool.spawn(async move {
            let flags = TriMeshFlags::FIX_INTERNAL_EDGES
                | TriMeshFlags::MERGE_DUPLICATE_VERTICES
                | TriMeshFlags::DELETE_DEGENERATE_TRIANGLES
                | TriMeshFlags::ORIENTED;
            let (collider, local_offset) =
                build_collider_with_fallback(todo.positions, todo.indices, flags);
            (
                (todo.coord, todo.sub),
                ColliderBuild {
                    origin: todo.origin + local_offset,
                    collider,
                },
            )
        });
        pending.0.insert(key, task);
    }

    stage_telemetry.stage_collider_schedule_ms = smooth_stage_ms(
        stage_telemetry.stage_collider_schedule_ms,
        stage_start.elapsed().as_secs_f32() * 1000.0,
    );
}

/// Runs the `collect_finished_collider_builds` routine for collect finished collider builds in the `generator::chunk::chunk_builder` module.
fn collect_finished_collider_builds(
    mut commands: Commands,
    mut pending: ResMut<PendingColliderBuild>,
    mut ready_queue: ResMut<ColliderReadyQueue>,
    backlog: Res<ColliderBacklog>,
    mut collider_index: ResMut<ChunkColliderIndex>,
    chunk_map: Res<ChunkMap>,
    game_config: Res<GlobalConfig>,
    app_state: Res<State<AppState>>,
    mut stage_telemetry: ResMut<ChunkStageTelemetry>,
    q_cam: Query<&GlobalTransform, With<Camera3d>>,
    load_center: Option<Res<LoadCenter>>,
) {
    let stage_start = Instant::now();
    let waiting = is_waiting(&app_state);
    let apply_cap = if waiting {
        BIG
    } else {
        game_config.graphics.chunk_collider_apply_per_frame.max(1)
    };
    let mut done_keys = Vec::new();
    let poll_scan_limit = if waiting { BIG } else { 256usize };
    let mut scanned = 0usize;
    for (key, task) in pending.0.iter_mut() {
        if scanned >= poll_scan_limit {
            break;
        }
        scanned += 1;
        if let Some((k, build)) = future::block_on(future::poll_once(task)) {
            done_keys.push(*key);
            ready_queue.0.retain(|(old_k, _)| *old_k != k);
            ready_queue.0.push_back((k, build));
        }
    }

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
    let mut applied = 0usize;
    while applied < apply_cap {
        let next = if waiting {
            ready_queue.0.pop_front()
        } else {
            let best = ready_queue
                .0
                .iter()
                .take(512)
                .enumerate()
                .min_by_key(|(_, ((coord, sub), _))| {
                    let dx = coord.x - center_c.x;
                    let dz = coord.y - center_c.y;
                    (dx * dx + dz * dz, *sub)
                })
                .map(|(idx, _)| idx);
            best.and_then(|idx| ready_queue.0.remove(idx))
        };
        let Some(((coord, sub), build)) = next else {
            break;
        };
        if backlog.0.contains_key(&(coord, sub)) {
            continue;
        }

        if !chunk_map.chunks.contains_key(&coord) {
            if let Some(ent) = collider_index.0.remove(&(coord, sub)) {
                safe_despawn_entity(&mut commands, ent);
            }
            continue;
        }

        let Some(collider) = build.collider else {
            continue;
        };

        if let Some(ent) = collider_index.0.remove(&(coord, sub)) {
            safe_despawn_entity(&mut commands, ent);
        }

        let ent = commands
            .spawn((
                RigidBody::Fixed,
                collider,
                Transform::from_translation(build.origin),
                ChunkColliderProxy { coord },
                ColliderDisabled,
                Name::new(format!(
                    "collider chunk({},{}) sub{}",
                    coord.x, coord.y, sub
                )),
            ))
            .id();
        collider_index.0.insert((coord, sub), ent);
        applied += 1;
    }

    for k in done_keys {
        pending.0.remove(&k);
    }

    stage_telemetry.stage_collider_apply_ms = smooth_stage_ms(
        stage_telemetry.stage_collider_apply_ms,
        stage_start.elapsed().as_secs_f32() * 1000.0,
    );
}

/// Enables/disables chunk colliders based on nearby gameplay entities.
fn update_chunk_collider_activation(
    mut commands: Commands,
    game_config: Res<GlobalConfig>,
    q_players: Query<&GlobalTransform, With<Player>>,
    q_mobs: Query<
        (&GlobalTransform, &Name),
        (
            With<RigidBody>,
            Without<Player>,
            Without<ChunkColliderProxy>,
        ),
    >,
    q_colliders: Query<(Entity, &ChunkColliderProxy, Option<&ColliderDisabled>)>,
) {
    if q_colliders.is_empty() {
        return;
    }

    let radius_blocks = game_config
        .graphics
        .chunk_collider_activation_radius_blocks
        .max(1);
    let radius_sq = i64::from(radius_blocks) * i64::from(radius_blocks);

    let mut centers_xz_blocks: Vec<IVec2> = Vec::new();
    for t in q_players.iter() {
        centers_xz_blocks.push(IVec2::new(
            (t.translation().x / VOXEL_SIZE).floor() as i32,
            (t.translation().z / VOXEL_SIZE).floor() as i32,
        ));
    }
    for (t, name) in q_mobs.iter() {
        let lowered = name.as_str().to_ascii_lowercase();
        if !(lowered.contains("monster") || lowered.contains("mob")) {
            continue;
        }
        centers_xz_blocks.push(IVec2::new(
            (t.translation().x / VOXEL_SIZE).floor() as i32,
            (t.translation().z / VOXEL_SIZE).floor() as i32,
        ));
    }

    for (entity, proxy, disabled) in q_colliders.iter() {
        let center_x = proxy.coord.x * CX as i32 + (CX as i32 / 2);
        let center_z = proxy.coord.y * CZ as i32 + (CZ as i32 / 2);
        let should_enable = centers_xz_blocks.iter().any(|p| {
            let dx = i64::from(center_x - p.x);
            let dz = i64::from(center_z - p.y);
            dx * dx + dz * dz <= radius_sq
        });

        if should_enable {
            if disabled.is_some() {
                commands.entity(entity).remove::<ColliderDisabled>();
            }
        } else if disabled.is_none() {
            commands.entity(entity).insert(ColliderDisabled);
        }
    }
}

//System
/// Runs the `schedule_remesh_tasks_from_events` routine for schedule remesh tasks from events in the `generator::chunk::chunk_builder` module.
fn schedule_remesh_tasks_from_events(
    mut pending_mesh: ResMut<PendingMesh>,
    chunk_map: Res<ChunkMap>,
    reg: Res<BlockRegistry>,
    mut backlog: ResMut<MeshBacklog>,
    mut backlog_set: ResMut<MeshBacklogSet>,
    mut immediate_ready: ResMut<ImmediateMeshReadyQueue>,
    mut ev_dirty: MessageReader<SubChunkNeedRemeshEvent>,
    game_config: Res<GlobalConfig>,
    app_state: Res<State<AppState>>,
    q_cam: Query<&GlobalTransform, With<Camera3d>>,
    load_center: Option<Res<LoadCenter>>,
) {
    if chunk_map.chunks.is_empty() {
        ev_dirty.clear();
        return;
    }

    let waiting = is_waiting(&app_state);
    let in_game_immediate = matches!(app_state.get(), AppState::InGame(InGameStates::Game));
    let max_inflight_mesh = if waiting {
        BIG
    } else {
        game_config.graphics.chunk_mesh_max_inflight.max(1)
    };
    let immediate_budget = if in_game_immediate {
        game_config.graphics.chunk_mesh_apply_per_frame.clamp(1, 6)
    } else {
        0
    };
    let mut immediate_used = 0usize;

    let reg_lite = RegLite::from_reg(&reg);
    let pool = AsyncComputeTaskPool::get();
    let mut by_chunk: HashMap<IVec2, Vec<usize>> = HashMap::new();
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

    for e in ev_dirty.read().copied() {
        if e.sub < SEC_COUNT {
            by_chunk.entry(e.coord).or_default().push(e.sub);
        }
    }

    for (coord, mut subs) in by_chunk {
        subs.sort_unstable();
        subs.dedup();

        let Some(chunk) = chunk_map.chunks.get(&coord) else {
            for sub in subs {
                let key = (coord, sub);
                immediate_ready.0.retain(|item| item.key != key);
                enqueue_mesh_fast(&mut backlog, &mut backlog_set, &pending_mesh, key);
            }
            continue;
        };

        let chunk_shared = Arc::new(chunk.clone());
        for sub in subs {
            let key = (coord, sub);
            immediate_ready.0.retain(|item| item.key != key);
            if pending_mesh.0.remove(&key).is_some() {
                // Replace stale in-flight mesh task with a fresh one that includes
                // the newest block edits, instead of waiting for old results first.
                backlog_set.0.remove(&key);
                backlog.0.retain(|queued| *queued != key);
            }

            let y0 = sub * SEC_H;
            let y1 = (y0 + SEC_H).min(CY);
            let borders = snapshot_borders(&chunk_map, coord, y0, y1);
            if in_game_immediate && immediate_used < immediate_budget {
                // Apply player edits immediately in the current frame path.
                let chunk_copy = Arc::clone(&chunk_shared);
                let reg_copy = reg_lite.clone();
                let builds = future::block_on(mesh_subchunk_async(
                    &chunk_copy,
                    &reg_copy,
                    sub,
                    VOXEL_SIZE,
                    Some(borders),
                ));
                immediate_ready.0.push_back(ReadyMeshItem {
                    key,
                    builds,
                    immediate: true,
                });
                immediate_used += 1;
                continue;
            }

            let has_slot = pending_mesh.0.len() < max_inflight_mesh
                || (!waiting
                    && reserve_pending_mesh_slot_for_priority(
                        &mut pending_mesh,
                        &mut backlog,
                        &mut backlog_set,
                        center_c,
                    ));

            if has_slot {
                let chunk_copy = Arc::clone(&chunk_shared);
                let reg_copy = reg_lite.clone();
                let t = pool.spawn(async move {
                    let builds =
                        mesh_subchunk_async(&chunk_copy, &reg_copy, sub, VOXEL_SIZE, Some(borders))
                            .await;
                    (key, builds)
                });

                pending_mesh.0.insert(key, t);
            } else {
                enqueue_mesh_fast(&mut backlog, &mut backlog_set, &pending_mesh, key);
            }
        }
    }
}

#[inline]
fn reserve_pending_mesh_slot_for_priority(
    pending_mesh: &mut PendingMesh,
    backlog: &mut MeshBacklog,
    backlog_set: &mut MeshBacklogSet,
    center_c: IVec2,
) -> bool {
    let Some(victim) = pending_mesh
        .0
        .keys()
        .take(1024)
        .copied()
        .max_by_key(|(coord, sub)| {
            let dx = coord.x - center_c.x;
            let dz = coord.y - center_c.y;
            (dx * dx + dz * dz, *sub)
        })
    else {
        return false;
    };

    if pending_mesh.0.remove(&victim).is_some() {
        enqueue_mesh_fast(backlog, backlog_set, pending_mesh, victim);
        true
    } else {
        false
    }
}

//System
/// Runs the `unload_far_chunks` routine for unload far chunks in the `generator::chunk::chunk_builder` module.
fn unload_far_chunks(
    mut commands: Commands,
    mut chunk_map: ResMut<ChunkMap>,
    mut mesh_index: ResMut<ChunkMeshIndex>,
    mut collider_index: ResMut<ChunkColliderIndex>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut unload_state: ChunkUnloadState,
    game_config: Res<GlobalConfig>,
    ws: Res<WorldSave>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
    q_mesh: Query<&Mesh3d>,
    q_cam: Query<&GlobalTransform, With<Camera3d>>,
    mut ev_water_unload: MessageWriter<ChunkUnloadEvent>,
    mut ready_latency: ResMut<ChunkReadyLatencyState>,
    mut immediate_ready: ResMut<ImmediateMeshReadyQueue>,
) {
    let cam = if let Ok(t) = q_cam.single() {
        t
    } else {
        return;
    };
    let cam_pos = cam.translation();
    let (center_c, _) = world_to_chunk_xz(
        (cam_pos.x / VOXEL_SIZE).floor() as i32,
        (cam_pos.z / VOXEL_SIZE).floor() as i32,
    );

    let keep_radius = loaded_radius(game_config.graphics.chunk_range) + HIDDEN_PRELOAD_RING + 1;
    let unload_budget = game_config.graphics.chunk_unload_budget_per_frame.max(1);

    let mut to_remove: Vec<IVec2> = chunk_map
        .chunks
        .keys()
        .filter(|coord| {
            (coord.x - center_c.x).abs() > keep_radius || (coord.y - center_c.y).abs() > keep_radius
        })
        .cloned()
        .collect();
    to_remove.sort_by_key(|coord| {
        Reverse(
            (coord.x - center_c.x)
                .abs()
                .max((coord.y - center_c.y).abs()),
        )
    });
    to_remove.truncate(unload_budget);

    for coord in &to_remove {
        if multiplayer_connection.uses_local_save_data() {
            if let Some(chunk) = chunk_map.chunks.get(coord) {
                let root = ws.root.clone();
                let chunk_copy = chunk.clone();
                let c = *coord;
                let pool = AsyncComputeTaskPool::get();
                let task = pool.spawn(async move {
                    let _ = save_chunk_at_root_sync(root, c, &chunk_copy);
                    c
                });
                unload_state.pending_save.0.insert(c, task);
            }
        }

        unload_state.pending_gen.0.remove(coord);
        unload_state.pending_mesh.0.retain(|(c, _), _| c != coord);
        unload_state
            .pending_collider
            .0
            .retain(|(c, _), _| c != coord);
        unload_state
            .collider_ready
            .0
            .retain(|((c, _), _)| c != coord);

        let old_keys: Vec<_> = mesh_index
            .map
            .keys()
            .cloned()
            .filter(|(c, _, _)| c == coord)
            .collect();
        despawn_mesh_set(
            old_keys,
            &mut mesh_index,
            &mut commands,
            &q_mesh,
            &mut meshes,
        );

        let col_keys: Vec<_> = collider_index
            .0
            .keys()
            .cloned()
            .filter(|(c, _)| c == coord)
            .collect();
        for k in col_keys {
            if let Some(ent) = collider_index.0.remove(&k) {
                safe_despawn_entity(&mut commands, ent);
            }
        }

        ev_water_unload.write(ChunkUnloadEvent { coord: *coord });

        chunk_map.chunks.remove(coord);
        unload_state.backlog.0.retain(|(c, _)| c != coord);
        unload_state.backlog_set.0.retain(|(c, _)| c != coord);
        unload_state.coll_backlog.0.retain(|(c, _), _| *c != *coord);
        immediate_ready.0.retain(|item| item.key.0 != *coord);
        unload_state.ready_set.0.remove(coord);
        ready_latency.requested_at.remove(coord);
    }
}

/// Runs the `cleanup_chunk_runtime_on_exit` routine for cleanup chunk runtime on exit in the `generator::chunk::chunk_builder` module.
fn cleanup_chunk_runtime_on_exit(
    mut commands: Commands,
    mut chunk_map: ResMut<ChunkMap>,
    mut mesh_index: ResMut<ChunkMeshIndex>,
    mut collider_index: ResMut<ChunkColliderIndex>,
    mut meshes: ResMut<Assets<Mesh>>,
    ws: Option<Res<WorldSave>>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
    mut cleanup: ChunkCleanupState,
    mut stream_lookahead: ResMut<StreamLookaheadState>,
    mut ready_latency: ResMut<ChunkReadyLatencyState>,
    mut stage_telemetry: ResMut<ChunkStageTelemetry>,
    mut immediate_ready: ResMut<ImmediateMeshReadyQueue>,
    q_mesh: Query<&Mesh3d>,
) {
    let should_save = ws.is_some() && multiplayer_connection.uses_local_save_data();

    if should_save && let Some(ws) = ws {
        let root = ws.root.clone();
        for (&coord, chunk) in &chunk_map.chunks {
            let _ = save_chunk_at_root_sync(root.clone(), coord, chunk);
        }
    }

    let old_keys: Vec<_> = mesh_index.map.keys().cloned().collect();
    despawn_mesh_set(
        old_keys,
        &mut mesh_index,
        &mut commands,
        &q_mesh,
        &mut meshes,
    );

    for (_, ent) in collider_index.0.drain() {
        safe_despawn_entity(&mut commands, ent);
    }

    chunk_map.chunks.clear();
    cleanup.pending_gen.0.clear();
    cleanup.pending_mesh.0.clear();
    cleanup.backlog.0.clear();
    cleanup.backlog_set.0.clear();
    cleanup.pending_collider.0.clear();
    cleanup.collider_ready.0.clear();
    cleanup.pending_save.0.clear();
    cleanup.coll_backlog.0.clear();
    immediate_ready.0.clear();
    cleanup.kick_queue.0.clear();
    cleanup.kicked.0.clear();
    cleanup.queued.0.clear();
    cleanup.ready_set.0.clear();
    ready_latency.requested_at.clear();
    ready_latency.recent_samples_ms.clear();
    *stage_telemetry = ChunkStageTelemetry::default();
    stream_lookahead.last_cam_xz = None;
    stream_lookahead.smoothed_dir = Vec2::ZERO;
    commands.remove_resource::<LoadCenter>();
}

/// Runs the `cleanup_kick_flags_on_unload` routine for cleanup kick flags on unload in the `generator::chunk::chunk_builder` module.
fn cleanup_kick_flags_on_unload(
    mut ev_unload: MessageReader<ChunkUnloadEvent>,
    mut kicked: ResMut<KickedOnce>,
    mut queued: ResMut<QueuedOnce>,
    mut queue: ResMut<KickQueue>,
) {
    for e in ev_unload.read() {
        let coord = e.coord;
        kicked.0.retain(|(c, _)| *c != coord);
        queued.0.retain(|(c, _)| *c != coord);
        queue.0.retain(|it| it.coord != coord);
    }
}

/// Runs the `collect_chunk_save_tasks` routine for collect chunk save tasks in the `generator::chunk::chunk_builder` module.
fn collect_chunk_save_tasks(mut pending: ResMut<PendingChunkSave>) {
    let mut done = Vec::new();
    for (coord, task) in pending.0.iter_mut() {
        if future::block_on(future::poll_once(task)).is_some() {
            done.push(*coord);
        }
    }
    for coord in done {
        pending.0.remove(&coord);
    }
}

/// Builds trimesh collider for the `generator::chunk::chunk_builder` module.
fn build_trimesh_collider(
    positions: &[[f32; 3]],
    indices: &[u32],
    flags: TriMeshFlags,
) -> Option<Collider> {
    if indices.len() < 3 || indices.len() % 3 != 0 {
        return None;
    }

    let verts: Vec<Vec3> = positions
        .iter()
        .map(|p| Vec3::new(p[0], p[1], p[2]))
        .collect();
    let tris: Vec<[u32; 3]> = indices
        .chunks_exact(3)
        .map(|tri| [tri[0], tri[1], tri[2]])
        .collect();

    Collider::trimesh_with_flags(verts, tris, flags).ok()
}

fn build_surface_placeholder_collider(
    chunk: &ChunkData,
    reg: &BlockRegistry,
    sub: usize,
) -> Option<(Collider, Vec3)> {
    let y0 = sub * SEC_H;
    let y1 = (y0 + SEC_H).min(CY);
    if y0 >= y1 {
        return None;
    }

    let s = VOXEL_SIZE;
    let half = (s * 0.5).max(0.05);
    let mut parts: Vec<(Vec3, Quat, Collider)> = Vec::with_capacity(CX * CZ);

    for z in 0..CZ {
        for x in 0..CX {
            let mut fallback_ly: Option<usize> = None;
            let mut top_ly: Option<usize> = None;

            for ly in (y0..y1).rev() {
                let bid = chunk.get(x, ly, z);
                if !reg.is_solid_for_collision(bid) {
                    continue;
                }

                if fallback_ly.is_none() {
                    fallback_ly = Some(ly);
                }

                let exposed_top = if ly + 1 < CY {
                    let above = chunk.get(x, ly + 1, z);
                    !reg.is_solid_for_collision(above)
                } else {
                    true
                };
                if exposed_top {
                    top_ly = Some(ly);
                    break;
                }
            }

            let Some(ly) = top_ly.or(fallback_ly) else {
                continue;
            };

            let center = Vec3::new(
                (x as f32 + 0.5) * s,
                (ly as f32 + 0.5) * s,
                (z as f32 + 0.5) * s,
            );
            parts.push((center, Quat::IDENTITY, Collider::cuboid(half, half, half)));
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some((Collider::compound(parts), Vec3::ZERO))
    }
}

fn build_bounds_collider(positions: &[[f32; 3]]) -> Option<(Collider, Vec3)> {
    if positions.is_empty() {
        return None;
    }

    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for p in positions {
        let v = Vec3::new(p[0], p[1], p[2]);
        min = min.min(v);
        max = max.max(v);
    }

    let center = (min + max) * 0.5;
    let half = ((max - min) * 0.5).max(Vec3::splat(0.05));
    Some((Collider::cuboid(half.x, half.y, half.z), center))
}

fn build_collider_with_fallback(
    positions: Vec<[f32; 3]>,
    indices: Vec<u32>,
    flags: TriMeshFlags,
) -> (Option<Collider>, Vec3) {
    if let Some(collider) = build_trimesh_collider(&positions, &indices, flags) {
        return (Some(collider), Vec3::ZERO);
    }

    match build_bounds_collider(&positions) {
        Some((collider, center)) => (Some(collider), center),
        None => (None, Vec3::ZERO),
    }
}

#[inline]
fn clear_air_only_subchunks_dirty(chunk: &mut ChunkData) {
    let plane = CX * CZ;
    for sub in 0..SEC_COUNT {
        if !chunk.is_dirty(sub) {
            continue;
        }
        let y0 = sub * SEC_H;
        let y1 = (y0 + SEC_H).min(CY);
        let start = y0 * plane;
        let end = y1 * plane;
        let has_solid = chunk.blocks[start..end].iter().any(|&id| id != 0);
        if !has_solid {
            chunk.clear_dirty(sub);
        }
    }
}

/// Runs the `estimate_surface_sub_fast` routine for estimate surface sub fast in the `generator::chunk::chunk_builder` module.
#[inline]
fn estimate_surface_sub_fast(chunk: &ChunkData) -> usize {
    let mut max_wy = Y_MIN - 1;
    for z in (0..CZ).step_by(4) {
        for x in (0..CX).step_by(4) {
            for ly in (0..CY).rev() {
                if chunk.get(x, ly, z) != 0 {
                    let wy = Y_MIN + ly as i32;
                    if wy > max_wy {
                        max_wy = wy;
                    }
                    break;
                }
            }
        }
    }
    let ly = (max_wy - Y_MIN).max(0) as usize;
    (ly / SEC_H).clamp(0, SEC_COUNT.saturating_sub(1))
}

/// Runs the `sub_priority_order` routine for sub priority order in the `generator::chunk::chunk_builder` module.
fn sub_priority_order(chunk: &ChunkData) -> Vec<usize> {
    let mut out = Vec::with_capacity(SEC_COUNT);
    let mut used = vec![false; SEC_COUNT];
    let mid = estimate_surface_sub_fast(chunk);

    out.push(mid);
    used[mid] = true;

    let mut off = 1isize;
    while out.len() < SEC_COUNT {
        let below = mid as isize - off;
        if below >= 0 && !used[below as usize] {
            out.push(below as usize);
            used[below as usize] = true;
        }
        let above = mid as isize + off;
        if above < SEC_COUNT as isize && !used[above as usize] {
            out.push(above as usize);
            used[above as usize] = true;
        }
        off += 1;
    }
    out
}

#[inline]
fn visible_radius(chunk_range: i32) -> i32 {
    chunk_range.max(0)
}

#[inline]
fn loaded_radius(chunk_range: i32) -> i32 {
    let r = visible_radius(chunk_range);
    if r >= HIGH_RANGE_PRELOAD_THRESHOLD {
        r + HIDDEN_PRELOAD_RING
    } else {
        r
    }
}

#[inline]
fn smooth_stage_ms(current: f32, sample_ms: f32) -> f32 {
    let sample = sample_ms.max(0.0);
    if current <= 0.0 {
        sample
    } else {
        current + (sample - current) * 0.2
    }
}

fn telemetry_mark_chunk_requested(
    coord: IVec2,
    now_secs: f64,
    ready_latency: &mut ChunkReadyLatencyState,
) {
    ready_latency.requested_at.entry(coord).or_insert(now_secs);
}

fn telemetry_mark_chunk_ready(
    coord: IVec2,
    now_secs: f64,
    ready_latency: &mut ChunkReadyLatencyState,
    stage_telemetry: &mut ChunkStageTelemetry,
) {
    let Some(start_secs) = ready_latency.requested_at.remove(&coord) else {
        return;
    };

    let latency_ms = ((now_secs - start_secs).max(0.0) * 1000.0) as f32;
    if stage_telemetry.chunk_ready_latency_ms <= 0.0 {
        stage_telemetry.chunk_ready_latency_ms = latency_ms;
    } else {
        stage_telemetry.chunk_ready_latency_ms +=
            (latency_ms - stage_telemetry.chunk_ready_latency_ms) * 0.15;
    }

    ready_latency.recent_samples_ms.push_back(latency_ms);
    const LATENCY_WINDOW: usize = 256;
    while ready_latency.recent_samples_ms.len() > LATENCY_WINDOW {
        ready_latency.recent_samples_ms.pop_front();
    }

    if !ready_latency.recent_samples_ms.is_empty() {
        let mut samples: Vec<f32> = ready_latency.recent_samples_ms.iter().copied().collect();
        samples.sort_by(|a, b| a.total_cmp(b));
        let p95_idx = ((samples.len() - 1) as f32 * 0.95).round() as usize;
        stage_telemetry.chunk_ready_latency_p95_ms = samples[p95_idx.min(samples.len() - 1)];
    }
}

fn sync_chunk_mesh_visibility(
    mut q_mesh: Query<(&SubchunkMesh, &mut Visibility)>,
    q_cam: Query<&GlobalTransform, With<Camera3d>>,
    load_center: Option<Res<LoadCenter>>,
    game_config: Res<GlobalConfig>,
    ready_set: Res<ChunkReadySet>,
    water_ready_set: Option<Res<WaterReadySet>>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
    app_state: Res<State<AppState>>,
) {
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

    let visible = loaded_radius(game_config.graphics.chunk_range);
    let require_chunk_ready = !matches!(app_state.get(), AppState::InGame(InGameStates::Game));
    let require_water_ready = multiplayer_connection.uses_local_save_data()
        && matches!(app_state.get(), AppState::Loading(LoadingStates::WaterGen));
    for (mesh, mut vis) in &mut q_mesh {
        let in_visible = (mesh.coord.x - center_c.x).abs() <= visible
            && (mesh.coord.y - center_c.y).abs() <= visible;
        let ready = if require_chunk_ready {
            ready_set.0.contains(&mesh.coord)
        } else {
            true
        };
        let water_ready = if require_water_ready {
            water_ready_set
                .as_ref()
                .map(|set| set.0.contains(&mesh.coord))
                .unwrap_or(false)
        } else {
            true
        };
        *vis = if in_visible && ready && water_ready {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}
