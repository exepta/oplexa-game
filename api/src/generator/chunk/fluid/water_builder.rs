use crate::core::config::{GlobalConfig, WorldGenConfig};
use crate::core::events::block::block_player_events::BlockBreakByPlayerEvent;
use crate::core::events::chunk_events::{ChunkUnloadEvent, SubChunkNeedRemeshEvent};
use crate::core::multiplayer::MultiplayerConnectionState;
use crate::core::shader::water_shader::{WaterMatHandle, WaterMaterial};
use crate::core::states::states::{AppState, InGameStates, LoadingStates};
use crate::core::world::block::{BlockRegistry, VOXEL_SIZE, id_any};
use crate::core::world::chunk::{ChunkMap, LoadCenter, MAX_UPDATE_FRAMES, SEA_LEVEL};
use crate::core::world::chunk_dimension::*;
use crate::core::world::fluid::*;
use crate::core::world::save::{RegionCache, WorldSave};
use crate::generator::chunk::water_utils::*;
use bevy::ecs::system::SystemParam;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;
use futures_lite::future;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

const WATER_GEN_BUDGET_PER_FRAME: usize = 24;
const MAX_INFLIGHT_WATER_LOAD: usize = 16;
const WATER_FINISH_HOLD_SECS: f32 = 1.0;

const MAX_INFLIGHT_WATER_MESH: usize = 24;
const MAX_WATER_MESH_APPLY_PER_FRAME: usize = MAX_UPDATE_FRAMES / 6;

/// Represents water boot used by the `generator::chunk::water_builder` module.
#[derive(Resource, Default)]
struct WaterBoot {
    started: bool,
}

/// Represents water gen queue used by the `generator::chunk::water_builder` module.
#[derive(Resource, Default)]
struct WaterGenQueue {
    work: VecDeque<IVec2>,
}

/// Represents water meshing todo used by the `generator::chunk::water_builder` module.
#[derive(Resource, Default)]
pub struct WaterMeshingTodo(pub HashSet<IVec2>);

/// Represents pending water load used by the `generator::chunk::water_builder` module.
#[derive(Resource, Default)]
pub struct PendingWaterLoad(pub HashMap<IVec2, bevy::tasks::Task<(IVec2, FluidChunk)>>);

/// Represents water mesh backlog used by the `generator::chunk::water_builder` module.
#[derive(Resource, Default)]
pub struct WaterMeshBacklog(pub VecDeque<(IVec2, usize)>);

/// Represents pending water mesh used by the `generator::chunk::water_builder` module.
#[derive(Resource, Default)]
pub struct PendingWaterMesh(
    pub HashMap<(IVec2, usize), bevy::tasks::Task<((IVec2, usize), WaterMeshBuild)>>,
);

/// Represents pending water save used by the `generator::chunk::water_builder` module.
#[derive(Resource, Default)]
pub struct PendingWaterSave(pub HashMap<IVec2, bevy::tasks::Task<IVec2>>);

/// Represents water flow queue used by the `generator::chunk::water_builder` module.
#[derive(Resource, Default)]
pub struct WaterFlowQueue(pub(crate) VecDeque<FlowJob>);

/// Represents pending water flow used by the `generator::chunk::water_builder` module.
#[derive(Resource, Default)]
pub struct PendingWaterFlow(pub HashMap<u64, bevy::tasks::Task<(u64, FlowResult)>>);

#[derive(Resource, Default)]
pub struct WaterReadySet(pub HashSet<IVec2>);

#[derive(Component, Copy, Clone)]
struct WaterSubchunkMesh {
    coord: IVec2,
}

/// Represents water flow ids used by the `generator::chunk::water_builder` module.
#[derive(Resource, Default)]
pub struct WaterFlowIds {
    next: u64,
}
impl WaterFlowIds {
    /// Runs the `next` routine for next in the `generator::chunk::water_builder` module.
    fn next(&mut self) -> u64 {
        let id = self.next;
        self.next += 1;
        id
    }
}

/// Holds the final transition from WaterGen to InGame for a short moment.
#[derive(Resource)]
struct WaterFinishDelay {
    armed: bool,
    timer: Timer,
}

impl Default for WaterFinishDelay {
    fn default() -> Self {
        Self {
            armed: false,
            timer: Timer::from_seconds(WATER_FINISH_HOLD_SECS, TimerMode::Once),
        }
    }
}

/// Represents water cleanup state used by the `generator::chunk::water_builder` module.
#[derive(SystemParam)]
struct WaterCleanupState<'w, 's> {
    boot: ResMut<'w, WaterBoot>,
    q: ResMut<'w, WaterGenQueue>,
    todo: ResMut<'w, WaterMeshingTodo>,
    pending_load: ResMut<'w, PendingWaterLoad>,
    backlog: ResMut<'w, WaterMeshBacklog>,
    pending_mesh: ResMut<'w, PendingWaterMesh>,
    pending_save: ResMut<'w, PendingWaterSave>,
    flow_q: ResMut<'w, WaterFlowQueue>,
    pending_flow: ResMut<'w, PendingWaterFlow>,
    flow_ids: ResMut<'w, WaterFlowIds>,
    ready_set: ResMut<'w, WaterReadySet>,
    _marker: std::marker::PhantomData<&'s ()>,
}

#[derive(SystemParam)]
struct WaterUnloadState<'w, 's> {
    pending_save: ResMut<'w, PendingWaterSave>,
    todo: Option<ResMut<'w, WaterMeshingTodo>>,
    backlog: Option<ResMut<'w, WaterMeshBacklog>>,
    pending_mesh: Option<ResMut<'w, PendingWaterMesh>>,
    pending_load: Option<ResMut<'w, PendingWaterLoad>>,
    flow_q: Option<ResMut<'w, WaterFlowQueue>>,
    q: Option<ResMut<'w, WaterGenQueue>>,
    ready_set: ResMut<'w, WaterReadySet>,
    _marker: std::marker::PhantomData<&'s ()>,
}

/// Represents water builder used by the `generator::chunk::water_builder` module.
pub struct WaterBuilder;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum WaterLocalPipelineSet {
    Prep,
    Gen,
    Mesh,
    Events,
    Finish,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum WaterRemotePipelineSet {
    Mesh,
    Events,
}

impl Plugin for WaterBuilder {
    /// Builds this component for the `generator::chunk::water_builder` module.
    fn build(&self, app: &mut App) {
        app.init_resource::<WaterBoot>()
            .init_resource::<WaterGenQueue>()
            .init_resource::<WaterMeshIndex>()
            .init_resource::<FluidMap>()
            .init_resource::<WaterMeshingTodo>()
            .init_resource::<PendingWaterLoad>()
            .init_resource::<WaterMeshBacklog>()
            .init_resource::<PendingWaterMesh>()
            .init_resource::<PendingWaterSave>()
            .init_resource::<WaterFlowQueue>()
            .init_resource::<PendingWaterFlow>()
            .init_resource::<WaterFlowIds>()
            .init_resource::<WaterReadySet>()
            .init_resource::<WaterFinishDelay>()
            .add_systems(
                OnEnter(AppState::Loading(LoadingStates::WaterGen)),
                (reset_water_finish_delay, water_gen_build_worklist).chain(),
            )
            .configure_sets(
                Update,
                (
                    WaterLocalPipelineSet::Prep,
                    WaterLocalPipelineSet::Gen,
                    WaterLocalPipelineSet::Mesh,
                    WaterLocalPipelineSet::Events,
                    WaterLocalPipelineSet::Finish,
                )
                    .chain()
                    .run_if(uses_local_world_data)
                    .run_if(
                        in_state(AppState::Loading(LoadingStates::WaterGen))
                            .or(in_state(AppState::InGame(InGameStates::Game))),
                    ),
            )
            .configure_sets(
                Update,
                (WaterRemotePipelineSet::Mesh, WaterRemotePipelineSet::Events)
                    .chain()
                    .run_if(uses_remote_world_data)
                    .run_if(
                        in_state(AppState::Loading(LoadingStates::BaseGen))
                            .or(in_state(AppState::InGame(InGameStates::Game))),
                    ),
            )
            .add_systems(
                Update,
                (
                    collect_water_save_tasks,
                    water_mark_from_dirty,
                    water_track_new_chunks,
                )
                    .chain()
                    .in_set(WaterLocalPipelineSet::Prep),
            )
            .add_systems(
                Update,
                (
                    schedule_water_generation_jobs,
                    collect_water_generation_jobs,
                )
                    .chain()
                    .in_set(WaterLocalPipelineSet::Gen),
            )
            .add_systems(
                Update,
                (
                    water_backlog_from_todo,
                    water_drain_mesh_backlog,
                    water_collect_meshed_subchunks,
                )
                    .chain()
                    .in_set(WaterLocalPipelineSet::Mesh),
            )
            .add_systems(
                Update,
                (
                    water_unload_on_event,
                    enqueue_flow_on_block_removed,
                    schedule_flow_jobs,
                    collect_flow_jobs,
                )
                    .chain()
                    .in_set(WaterLocalPipelineSet::Events),
            )
            .add_systems(
                Update,
                water_finish_check.in_set(WaterLocalPipelineSet::Finish),
            )
            .add_systems(
                Update,
                (
                    water_mark_from_dirty,
                    water_backlog_from_todo,
                    water_drain_mesh_backlog,
                    water_collect_meshed_subchunks,
                )
                    .chain()
                    .in_set(WaterRemotePipelineSet::Mesh),
            )
            .add_systems(
                Update,
                (
                    water_unload_on_event,
                    enqueue_flow_on_block_removed,
                    schedule_flow_jobs,
                    collect_flow_jobs,
                )
                    .chain()
                    .in_set(WaterRemotePipelineSet::Events),
            )
            .add_systems(
                Update,
                sync_water_mesh_visibility.run_if(
                    in_state(AppState::Loading(LoadingStates::BaseGen))
                        .or(in_state(AppState::Loading(LoadingStates::CaveGen)))
                        .or(in_state(AppState::Loading(LoadingStates::WaterGen)))
                        .or(in_state(AppState::InGame(InGameStates::Game))),
                ),
            )
            .add_systems(
                Update,
                sync_water_ready_set
                    .after(WaterLocalPipelineSet::Events)
                    .before(WaterLocalPipelineSet::Finish)
                    .run_if(uses_local_world_data)
                    .run_if(
                        in_state(AppState::Loading(LoadingStates::BaseGen))
                            .or(in_state(AppState::Loading(LoadingStates::WaterGen)))
                            .or(in_state(AppState::InGame(InGameStates::Game))),
                    ),
            );

        app.add_systems(
            OnExit(AppState::InGame(InGameStates::Game)),
            cleanup_water_runtime_on_exit,
        )
        .add_systems(
            Last,
            save_all_water_on_exit
                .run_if(on_message::<AppExit>)
                .run_if(uses_local_world_data),
        );
    }
}

/// Runs the `uses_local_world_data` routine for uses local world data in the `generator::chunk::water_builder` module.
fn uses_local_world_data(multiplayer_connection: Res<MultiplayerConnectionState>) -> bool {
    multiplayer_connection.uses_local_save_data()
}

/// Runs the `uses_remote_world_data` routine for uses remote world data in the `generator::chunk::water_builder` module.
fn uses_remote_world_data(multiplayer_connection: Res<MultiplayerConnectionState>) -> bool {
    !multiplayer_connection.uses_local_save_data()
}

/// Runs the `flow_task_run` routine for flow task run in the `generator::chunk::water_builder` module.
async fn flow_task_run(
    solid_snap: SolidSnapshot,
    water_snap: WaterSnap,
    mut job: FlowJob,
) -> FlowResult {
    use std::collections::{HashSet, VecDeque};

    let mut res = FlowResult::default();
    let mut q: VecDeque<Seed> = VecDeque::new();
    let mut seen: HashSet<(IVec2, i32, i32, i32)> = HashSet::new();

    // Seed initialization
    for s in job.seeds.drain(..) {
        if seen.insert((s.c, s.x, s.y, s.z)) {
            q.push_back(s);
        }
    }

    let mut filled_count: usize = 0;

    while let Some(cur) = q.pop_front() {
        // Skip if solid or out of snapshot (spill if outside)
        let solid = match snap_is_solid(&solid_snap, cur.c, cur.x, cur.y, cur.z) {
            Some(b) => b,
            None => {
                res.spill.push(cur);
                continue;
            }
        };
        if solid {
            continue;
        }

        // Skip if already has water
        if let Some(true) = snap_has_water(&water_snap, cur.c, cur.x, cur.y, cur.z) {
            continue;
        }

        // --- Support check to avoid creating a floating layer above water ---
        // English: We require either water directly below, OR (if below is solid)
        // side water at the same Y. This allows lateral spread over solid ledges,
        // but prevents starting a new layer above an existing water surface.
        let has_water_below = matches!(
            snap_has_water(&water_snap, cur.c, cur.x, cur.y - 1, cur.z),
            Some(true)
        );
        let below_is_solid =
            snap_is_solid(&solid_snap, cur.c, cur.x, cur.y - 1, cur.z).unwrap_or(true);

        let has_side_water_same_y = {
            let mut any = false;
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nc, nx, nz) = neighbor_lookup_chunked(cur.c, cur.x + dx, cur.z + dz);
                if matches!(snap_has_water(&water_snap, nc, nx, cur.y, nz), Some(true)) {
                    any = true;
                    break;
                }
            }
            any
        };

        if !(has_water_below || (below_is_solid && has_side_water_same_y)) {
            // No valid support -> do not fill this cell
            continue;
        }
        // --- end support check ---

        // Accept fill
        res.filled.push(cur);
        filled_count += 1;
        if filled_count >= job.cap {
            res.more.extend(q.drain(..));
            break;
        }

        // Enqueue neighbors (down first for gravity-like behavior)
        let mut push = |c: IVec2, x: i32, y: i32, z: i32| {
            if y < 0 || y >= CY as i32 {
                return;
            }
            if let Some(true) = snap_has_water(&water_snap, c, x, y, z) {
                return;
            }
            let k = (c, x, y, z);
            if seen.insert(k) {
                if in_snapshot(&solid_snap, c) {
                    q.push_back(Seed { c, x, y, z });
                } else {
                    res.spill.push(Seed { c, x, y, z });
                }
            }
        };

        push(cur.c, cur.x, cur.y - 1, cur.z);
        let (c1, x1, z1) = neighbor_lookup_chunked(cur.c, cur.x + 1, cur.z);
        push(c1, x1, cur.y, z1);
        let (c2, x2, z2) = neighbor_lookup_chunked(cur.c, cur.x - 1, cur.z);
        push(c2, x2, cur.y, z2);
        let (c3, x3, z3) = neighbor_lookup_chunked(cur.c, cur.x, cur.z + 1);
        push(c3, x3, cur.y, z3);
        let (c4, x4, z4) = neighbor_lookup_chunked(cur.c, cur.x, cur.z - 1);
        push(c4, x4, cur.y, z4);
    }

    res
}

/// Runs the `enqueue_flow_on_block_removed` routine for enqueue flow on block removed in the `generator::chunk::water_builder` module.
fn enqueue_flow_on_block_removed(
    mut ev: MessageReader<BlockBreakByPlayerEvent>,
    fluids: Res<FluidMap>,
    chunks: Res<ChunkMap>,
    mut queue: ResMut<WaterFlowQueue>,
) {
    for e in ev.read() {
        let c = e.chunk_coord;

        // Skip if cell isn't air now
        if let Some(ch) = chunks.chunks.get(&c) {
            if ch.get(e.chunk_x as usize, e.chunk_y as usize, e.chunk_z as usize) != 0 {
                continue;
            }
        } else {
            continue;
        }

        let mut sea_level = None;
        let mut has_water = false;

        // Choose a seed position; start at the broken block
        let seed_x = e.chunk_x as i32;
        let mut seed_y = e.chunk_y as i32;
        let seed_z = e.chunk_z as i32;

        for (dx, dy, dz) in [
            (1, 0, 0),
            (-1, 0, 0),
            (0, 0, 1),
            (0, 0, -1),
            (0, -1, 0),
            (0, 1, 0),
        ] {
            let lx = seed_x + dx;
            let ly = seed_y + dy;
            let lz = seed_z + dz;
            if ly < 0 || ly >= CY as i32 {
                continue;
            }
            let (nc, nx, nz) = neighbor_lookup_chunked(c, lx, lz);
            if let Some(fc) = fluids.0.get(&nc) {
                if fc.get(nx as usize, ly as usize, nz as usize) {
                    sea_level.get_or_insert(fc.sea_level);
                    has_water = true;

                    // If water is directly below, snap seed down to the surface.
                    if dy == -1 {
                        seed_y -= 1;
                    }
                    break;
                }
            }
        }
        if !has_water {
            continue;
        }

        queue.0.push_back(FlowJob {
            seeds: vec![Seed {
                c,
                x: seed_x,
                y: seed_y,
                z: seed_z,
            }],
            sea_level: sea_level.unwrap_or(SEA_LEVEL),
            cap: WATER_FLOW_CAP,
        });
    }
}

/// Runs the `schedule_flow_jobs` routine for schedule flow jobs in the `generator::chunk::water_builder` module.
fn schedule_flow_jobs(
    mut queue: ResMut<WaterFlowQueue>,
    mut pending: ResMut<PendingWaterFlow>,
    mut ids: ResMut<WaterFlowIds>,
    chunks: Res<ChunkMap>,
    fluids: Res<FluidMap>,
) {
    if queue.0.is_empty() {
        return;
    }
    let pool = AsyncComputeTaskPool::get();

    let mut budget =
        WATER_FLOW_BUDGET_PER_FRAME.min(WATER_FLOW_MAX_INFLIGHT.saturating_sub(pending.0.len()));
    if budget == 0 {
        return;
    }

    while budget > 0 {
        let Some(job) = queue.0.pop_front() else {
            break;
        };
        let anchor = job.seeds.get(0).map(|s| s.c).unwrap_or(IVec2::ZERO);
        let snapshot = build_solid_snapshot_3x3(&chunks, anchor);
        let water_snap = build_water_snapshot_3x3(&fluids, anchor);

        let id = ids.next();
        let task = pool.spawn(async move {
            let res = flow_task_run(snapshot, water_snap, job).await;
            (id, res)
        });
        pending.0.insert(id, task);
        budget -= 1;
    }
}

/// Runs the `collect_flow_jobs` routine for collect flow jobs in the `generator::chunk::water_builder` module.
fn collect_flow_jobs(
    mut pending: ResMut<PendingWaterFlow>,
    mut fluids: ResMut<FluidMap>,
    mut backlog: ResMut<WaterMeshBacklog>,
    mut queue: ResMut<WaterFlowQueue>,
    chunks: Res<ChunkMap>,
) {
    let mut done_ids = Vec::new();

    for (id, task) in pending.0.iter_mut() {
        if let Some((_id, mut res)) = future::block_on(future::poll_once(task)) {
            res.filled.retain(|s| chunks.chunks.contains_key(&s.c));

            for s in res.filled {
                let fc = fluids
                    .0
                    .entry(s.c)
                    .or_insert_with(|| FluidChunk::new(SEA_LEVEL));

                if !fc.get(s.x as usize, s.y as usize, s.z as usize) {
                    fc.set(s.x as usize, s.y as usize, s.z as usize, true);

                    enqueue_mesh_for_cell(&mut backlog, s.c, s.x, s.y, s.z);
                }
            }

            let mut more_loaded: Vec<Seed> = res
                .more
                .into_iter()
                .filter(|s| chunks.chunks.contains_key(&s.c))
                .collect();
            let mut spill_loaded: Vec<Seed> = res
                .spill
                .into_iter()
                .filter(|s| chunks.chunks.contains_key(&s.c))
                .collect();

            let mut push_job = |seeds: &mut Vec<Seed>| {
                if seeds.is_empty() {
                    return;
                }
                let sea = fluids
                    .0
                    .get(&seeds[0].c)
                    .map(|f| f.sea_level)
                    .unwrap_or(SEA_LEVEL);
                queue.0.push_back(FlowJob {
                    seeds: std::mem::take(seeds),
                    sea_level: sea,
                    cap: WATER_FLOW_CAP,
                });
            };

            push_job(&mut more_loaded);
            push_job(&mut spill_loaded);

            done_ids.push(*id);
        }
    }
    for id in done_ids {
        pending.0.remove(&id);
    }
}

/// Runs the `enqueue_mesh_for_cell` routine for enqueue mesh for cell in the `generator::chunk::water_builder` module.
#[inline]
fn enqueue_mesh_for_cell(backlog: &mut WaterMeshBacklog, c: IVec2, x: i32, y: i32, z: i32) {
    let clamp = |v: i32, lo: i32, hi: i32| v.max(lo).min(hi);
    let y = clamp(y, 0, (CY as i32) - 1);
    let sub = (y as usize) / SEC_H;

    let mut push = |cc: IVec2, ss: usize| {
        let key = (cc, ss);
        if !backlog.0.iter().any(|k| *k == key) {
            backlog.0.push_back(key);
        }
    };

    push(c, sub);

    if x == 0 {
        push(c - IVec2::X, sub);
    }
    if x == (CX as i32 - 1) {
        push(c + IVec2::X, sub);
    }
    if z == 0 {
        push(c - IVec2::Y, sub);
    }
    if z == (CZ as i32 - 1) {
        push(c + IVec2::Y, sub);
    }
}

/// Runs the `water_gen_build_worklist` routine for water gen build worklist in the `generator::chunk::water_builder` module.
fn water_gen_build_worklist(
    mut q: ResMut<WaterGenQueue>,
    chunk_map: Res<ChunkMap>,
    water: Res<FluidMap>,
    pending: Res<PendingWaterLoad>,
    mut boot: ResMut<WaterBoot>,
) {
    q.work.clear();
    rebuild_water_work_queue_impl(&mut q.work, &chunk_map, &water, &pending);
    boot.started = true;
}

/// Runs the `water_track_new_chunks` routine for water track new chunks in the `generator::chunk::water_builder` module.
fn water_track_new_chunks(
    mut q: ResMut<WaterGenQueue>,
    chunk_map: Res<ChunkMap>,
    water: Res<FluidMap>,
    pending: Res<PendingWaterLoad>,
) {
    rebuild_water_work_queue_impl(&mut q.work, &chunk_map, &water, &pending);
}

/// Runs the `schedule_water_generation_jobs` routine for schedule water generation jobs in the `generator::chunk::water_builder` module.
fn schedule_water_generation_jobs(
    mut q: ResMut<WaterGenQueue>,
    chunk_map: Res<ChunkMap>,
    water: Res<FluidMap>,
    pending_save: Res<PendingWaterSave>,
    ws: Res<WorldSave>,
    gen_cfg: Res<WorldGenConfig>,
    mut pending: ResMut<PendingWaterLoad>,
    q_cam: Query<&GlobalTransform, With<Camera3d>>,
    load_center: Option<Res<LoadCenter>>,
    game_config: Res<GlobalConfig>,
    app_state: Res<State<AppState>>,
    time: Res<Time>,
) {
    let in_game = matches!(app_state.get(), AppState::InGame(InGameStates::Game));
    let frame_ms = time.delta_secs() * 1000.0;
    let dynamic_divisor = if frame_ms > 30.0 {
        4
    } else if frame_ms > 22.0 {
        3
    } else if frame_ms > 17.0 {
        2
    } else {
        1
    };
    let async_threads = AsyncComputeTaskPool::get().thread_num().max(1);
    let loading_max_inflight = (async_threads * 4).clamp(16, 96);
    let loading_submit = (async_threads * 3).clamp(8, 64);
    let max_inflight = if in_water_gen(&app_state) {
        loading_max_inflight
    } else if in_game {
        (MAX_INFLIGHT_WATER_LOAD / dynamic_divisor).clamp(2, 6)
    } else {
        MAX_INFLIGHT_WATER_LOAD
    };
    let per_frame = if in_water_gen(&app_state) {
        loading_submit
    } else if in_game {
        (WATER_GEN_BUDGET_PER_FRAME / dynamic_divisor).clamp(1, 3)
    } else {
        WATER_GEN_BUDGET_PER_FRAME
    };

    if chunk_map.chunks.is_empty() {
        q.work.clear();
        return;
    }

    let mut budget = max_inflight.saturating_sub(pending.0.len()).min(per_frame);
    if budget == 0 {
        return;
    }

    let pool = AsyncComputeTaskPool::get();
    let seed = gen_cfg.seed as u32;
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
    let visible = game_config.graphics.chunk_range.max(0);
    let loaded = if in_game {
        visible + 1
    } else if visible >= 10 {
        visible + 4
    } else {
        visible
    };

    while budget > 0 {
        let next = if in_water_gen(&app_state) {
            q.work.pop_front()
        } else {
            let best = q
                .work
                .iter()
                .take(1024)
                .enumerate()
                .min_by_key(|(_, coord)| {
                    let dx = coord.x - center_c.x;
                    let dz = coord.y - center_c.y;
                    let rank = if dx.abs() <= visible && dz.abs() <= visible {
                        0
                    } else if dx.abs() <= loaded && dz.abs() <= loaded {
                        1
                    } else {
                        2
                    };
                    (rank, dx * dx + dz * dz)
                })
                .map(|(idx, _)| idx);
            best.and_then(|idx| q.work.remove(idx))
                .or_else(|| q.work.pop_front())
        };

        let Some(coord) = next else {
            break;
        };
        if water.0.contains_key(&coord) {
            continue;
        }
        if pending_save.0.contains_key(&coord) {
            continue;
        }
        let Some(chunk) = chunk_map.chunks.get(&coord) else {
            continue;
        };
        if pending.0.contains_key(&coord) {
            continue;
        }

        let chunk_copy = chunk.clone();
        let root = ws.root.clone();

        let task = pool.spawn(async move {
            if let Some((mut wc, ver)) = load_water_chunk_from_disk_any(root.clone(), coord) {
                if ver == WATER_MAGIC_V1 {
                    water_mask_with_solids(&mut wc, &chunk_copy);
                }
                return (coord, wc);
            }
            let wc = generate_water_for_chunk(coord, &chunk_copy, SEA_LEVEL, seed, false);
            (coord, wc)
        });

        pending.0.insert(coord, task);
        budget -= 1;
    }
}

/// Runs the `collect_water_generation_jobs` routine for collect water generation jobs in the `generator::chunk::water_builder` module.
fn collect_water_generation_jobs(
    mut pending: ResMut<PendingWaterLoad>,
    chunk_map: Res<ChunkMap>,
    mut water: ResMut<FluidMap>,
    mut to_mesh: ResMut<WaterMeshingTodo>,
    app_state: Res<State<AppState>>,
    time: Res<Time>,
) {
    let mut done: Vec<IVec2> = Vec::new();
    let in_game = matches!(app_state.get(), AppState::InGame(InGameStates::Game));
    let frame_ms = time.delta_secs() * 1000.0;
    let dynamic_divisor = if frame_ms > 30.0 {
        4
    } else if frame_ms > 22.0 {
        3
    } else if frame_ms > 17.0 {
        2
    } else {
        1
    };
    let apply_cap = if in_water_gen(&app_state) {
        (AsyncComputeTaskPool::get().thread_num().max(1) * 4).clamp(16, 96)
    } else if in_game {
        (WATER_GEN_BUDGET_PER_FRAME / dynamic_divisor).clamp(1, 2)
    } else {
        WATER_GEN_BUDGET_PER_FRAME
    };
    let mut applied = 0usize;

    for (_, task) in pending.0.iter_mut() {
        if applied >= apply_cap {
            break;
        }

        if let Some((c, wc)) = future::block_on(future::poll_once(task)) {
            if chunk_map.chunks.contains_key(&c) {
                if let Some(chunk) = chunk_map.chunks.get(&c) {
                    let mut wc2 = wc;
                    water_reconcile_ocean_with_solids(&mut wc2, chunk, SEA_LEVEL);
                    water_mask_with_solids(&mut wc2, chunk);
                    water.0.insert(c, wc2);
                } else {
                    water.0.insert(c, wc);
                }
                to_mesh.0.insert(c);
                for d in [IVec2::X, -IVec2::X, IVec2::Y, -IVec2::Y] {
                    to_mesh.0.insert(c + d);
                }
                applied += 1;
            }
            done.push(c);
        }
    }
    for c in done {
        pending.0.remove(&c);
    }
}

/// Runs the `water_mark_from_dirty` routine for water mark from dirty in the `generator::chunk::water_builder` module.
fn water_mark_from_dirty(
    mut ev: MessageReader<SubChunkNeedRemeshEvent>,
    mut todo: ResMut<WaterMeshingTodo>,
) {
    for e in ev.read() {
        todo.0.insert(e.coord);
        for d in [IVec2::X, -IVec2::X, IVec2::Y, -IVec2::Y] {
            todo.0.insert(e.coord + d);
        }
    }
}

fn reset_water_finish_delay(mut delay: ResMut<WaterFinishDelay>) {
    delay.armed = false;
    delay.timer.reset();
}

/// Runs the `water_finish_check` routine for water finish check in the `generator::chunk::water_builder` module.
fn water_finish_check(
    chunk_map: Res<ChunkMap>,
    water: Res<FluidMap>,
    q: Res<WaterGenQueue>,
    boot: Res<WaterBoot>,
    pending_load: Res<PendingWaterLoad>,
    to_mesh: Res<WaterMeshingTodo>,
    pending_mesh: Res<PendingWaterMesh>,
    backlog: Res<WaterMeshBacklog>,
    ready_set: Res<WaterReadySet>,
    mut delay: ResMut<WaterFinishDelay>,
    time: Res<Time>,
    app_state: Res<State<AppState>>,
    mut next: ResMut<NextState<AppState>>,
) {
    let in_water_gen = matches!(app_state.get(), AppState::Loading(LoadingStates::WaterGen));
    if !in_water_gen {
        delay.armed = false;
        delay.timer.reset();
        return;
    }

    let coverage_ok = all_chunks_have_water(&chunk_map, &water);
    let finalized_ok = chunk_map
        .chunks
        .keys()
        .all(|coord| ready_set.0.contains(coord));

    let gen_done = q.work.is_empty() && pending_load.0.is_empty();
    let mesh_done = to_mesh.0.is_empty() && backlog.0.is_empty() && pending_mesh.0.is_empty();
    let world_ok = !chunk_map.chunks.is_empty();

    if boot.started && world_ok && gen_done && coverage_ok && mesh_done && finalized_ok {
        if !delay.armed {
            delay.armed = true;
            delay.timer.reset();
            return;
        }

        delay.timer.tick(time.delta());
        if delay.timer.is_finished() {
            debug!("Water gen complete");
            delay.armed = false;
            delay.timer.reset();
            next.set(AppState::InGame(InGameStates::Game));
        }
    } else if delay.armed {
        delay.armed = false;
        delay.timer.reset();
    }
}

/// Runs the `water_unload_on_event` routine for water unload on event in the `generator::chunk::water_builder` module.
fn water_unload_on_event(
    mut commands: Commands,
    mut ev: MessageReader<ChunkUnloadEvent>,
    mut water: ResMut<FluidMap>,
    mut windex: ResMut<WaterMeshIndex>,
    mut meshes: ResMut<Assets<Mesh>>,
    q_mesh: Query<&Mesh3d>,
    chunk_map: Res<ChunkMap>,
    ws: Res<WorldSave>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
    mut unload: WaterUnloadState,
) {
    for ChunkUnloadEvent { coord } in ev.read().copied() {
        unload.ready_set.0.remove(&coord);
        if let Some(t) = unload.todo.as_mut() {
            t.0.remove(&coord);
        }
        if let Some(b) = unload.backlog.as_mut() {
            b.0.retain(|(c, _)| *c != coord);
        }
        if let Some(pm) = unload.pending_mesh.as_mut() {
            pm.0.retain(|(c, _), _| *c != coord);
        }
        if let Some(pl) = unload.pending_load.as_mut() {
            pl.0.remove(&coord);
        }
        if let Some(fq) = unload.flow_q.as_mut() {
            fq.0.retain(|job| job.seeds.iter().all(|s| s.c != coord));
        }

        if let Some(mut wc) = water.0.remove(&coord) {
            if multiplayer_connection.uses_local_save_data() {
                if let Some(ch) = chunk_map.chunks.get(&coord) {
                    water_mask_with_solids(&mut wc, ch);
                }
                let root = ws.root.clone();
                let task = AsyncComputeTaskPool::get().spawn(async move {
                    save_water_chunk_at_root_sync(root, coord, &wc);
                    coord
                });
                unload.pending_save.0.insert(coord, task);
            }
        }

        let dead: Vec<_> = windex
            .0
            .keys()
            .copied()
            .filter(|(c, _)| *c == coord)
            .collect();
        for key in dead {
            despawn_water_mesh(key, &mut windex, &mut commands, &q_mesh, &mut meshes);
        }

        if let Some(q) = unload.q.as_mut() {
            q.work.retain(|c| *c != coord);
        }
    }
}

/// Runs the `cleanup_water_runtime_on_exit` routine for cleanup water runtime on exit in the `generator::chunk::water_builder` module.
fn cleanup_water_runtime_on_exit(
    mut commands: Commands,
    mut water: ResMut<FluidMap>,
    mut windex: ResMut<WaterMeshIndex>,
    mut meshes: ResMut<Assets<Mesh>>,
    q_mesh: Query<&Mesh3d>,
    mut cleanup: WaterCleanupState,
    ws: Option<Res<WorldSave>>,
    chunk_map: Option<Res<ChunkMap>>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
) {
    let should_save = ws.is_some() && multiplayer_connection.uses_local_save_data();

    if should_save && let Some(ws) = ws {
        let root = ws.root.clone();
        for (&coord, fluid_chunk) in &water.0 {
            let mut chunk_copy = fluid_chunk.clone();
            if let Some(chunk_map) = chunk_map.as_ref()
                && let Some(chunk) = chunk_map.chunks.get(&coord)
            {
                water_mask_with_solids(&mut chunk_copy, chunk);
            }
            save_water_chunk_at_root_sync(root.clone(), coord, &chunk_copy);
        }
    }

    let dead: Vec<_> = windex.0.keys().copied().collect();
    for key in dead {
        despawn_water_mesh(key, &mut windex, &mut commands, &q_mesh, &mut meshes);
    }

    water.0.clear();
    cleanup.boot.started = false;
    cleanup.q.work.clear();
    cleanup.todo.0.clear();
    cleanup.pending_load.0.clear();
    cleanup.backlog.0.clear();
    cleanup.pending_mesh.0.clear();
    cleanup.pending_save.0.clear();
    cleanup.flow_q.0.clear();
    cleanup.pending_flow.0.clear();
    cleanup.flow_ids.next = 0;
    cleanup.ready_set.0.clear();
}

/// Runs the `collect_water_save_tasks` routine for collect water save tasks in the `generator::chunk::water_builder` module.
fn collect_water_save_tasks(mut pending: ResMut<PendingWaterSave>) {
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

/// Saves all water on exit for the `generator::chunk::water_builder` module.
fn save_all_water_on_exit(
    ws: Res<WorldSave>,
    mut cache: ResMut<RegionCache>,
    chunks: Res<ChunkMap>,
    water: Res<FluidMap>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
) {
    if !multiplayer_connection.uses_local_save_data() {
        return;
    }

    debug!("Exit: Saving all water");
    for (&coord, fc) in water.0.iter() {
        let mut w = fc.clone();
        if let Some(ch) = chunks.chunks.get(&coord) {
            water_mask_with_solids(&mut w, ch);
        }
        save_water_chunk_sync(&ws, &mut cache, coord, &w);
    }
}

/// Runs the `water_backlog_from_todo` routine for water backlog from todo in the `generator::chunk::water_builder` module.
fn water_backlog_from_todo(
    mut todo: ResMut<WaterMeshingTodo>,
    chunk_map: Res<ChunkMap>,
    water: Res<FluidMap>,
    mut backlog: ResMut<WaterMeshBacklog>,
) {
    if todo.0.is_empty() {
        return;
    }

    let coords: Vec<_> = todo.0.drain().collect();

    for coord in coords {
        if !chunk_map.chunks.contains_key(&coord) {
            continue;
        }
        if !chunk_base_ready(coord, &chunk_map) {
            // Base terrain not ready yet -> try again later.
            todo.0.insert(coord);
            continue;
        }
        if let Some(fc) = water.0.get(&coord) {
            for sub in 0..SEC_COUNT {
                if fc.sub_has_any(sub) {
                    let key = (coord, sub);
                    if !backlog.0.iter().any(|k| *k == key) {
                        backlog.0.push_back(key);
                    }
                }
            }
        }
    }
}

/// Runs the `water_drain_mesh_backlog` routine for water drain mesh backlog in the `generator::chunk::water_builder` module.
fn water_drain_mesh_backlog(
    mut backlog: ResMut<WaterMeshBacklog>,
    mut pending: ResMut<PendingWaterMesh>,
    chunk_map: Res<ChunkMap>,
    water: Res<FluidMap>,
    q_cam: Query<&GlobalTransform, With<Camera3d>>,
    load_center: Option<Res<LoadCenter>>,
    game_config: Res<GlobalConfig>,
    app_state: Res<State<AppState>>,
    time: Res<Time>,
) {
    let in_game = matches!(app_state.get(), AppState::InGame(InGameStates::Game));
    let frame_ms = time.delta_secs() * 1000.0;
    let dynamic_divisor = if frame_ms > 30.0 {
        4
    } else if frame_ms > 22.0 {
        3
    } else if frame_ms > 17.0 {
        2
    } else {
        1
    };
    let max_inflight = if in_water_gen(&app_state) {
        (AsyncComputeTaskPool::get().thread_num().max(1) * 6).clamp(24, 144)
    } else if in_game {
        (MAX_INFLIGHT_WATER_MESH / dynamic_divisor).clamp(2, 6)
    } else {
        MAX_INFLIGHT_WATER_MESH
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
    let visible = game_config.graphics.chunk_range.max(0);
    let loaded = if visible >= 10 { visible + 4 } else { visible };

    let mut processed = 0usize;
    let limit = backlog.0.len();

    while pending.0.len() < max_inflight && processed < limit {
        let next = if in_water_gen(&app_state) {
            backlog.0.pop_front()
        } else {
            let best = backlog
                .0
                .iter()
                .take(1024)
                .enumerate()
                .min_by_key(|(_, (coord, sub))| {
                    let dx = coord.x - center_c.x;
                    let dz = coord.y - center_c.y;
                    let rank = if dx.abs() <= visible && dz.abs() <= visible {
                        0
                    } else if dx.abs() <= loaded && dz.abs() <= loaded {
                        1
                    } else {
                        2
                    };
                    (rank, dx * dx + dz * dz, *sub)
                })
                .map(|(idx, _)| idx);
            best.and_then(|idx| backlog.0.remove(idx))
                .or_else(|| backlog.0.pop_front())
        };

        let Some((coord, sub)) = next else {
            break;
        };
        processed += 1;

        if !chunk_base_ready(coord, &chunk_map) {
            backlog.0.push_back((coord, sub));
            continue;
        }

        if !water_meshing_ready(coord, &water, &chunk_map) {
            backlog.0.push_back((coord, sub));
            continue;
        }

        let fc = match water.0.get(&coord).cloned() {
            Some(v) => v,
            None => {
                backlog.0.push_back((coord, sub));
                continue;
            }
        };
        let chunk_copy = match chunk_map.chunks.get(&coord).cloned() {
            Some(v) => v,
            None => {
                backlog.0.push_back((coord, sub));
                continue;
            }
        };

        let y0 = sub * SEC_H;
        let y1 = (y0 + SEC_H).min(CY);
        let borders = water_snapshot_borders(&chunk_map, &water, coord, y0, y1, fc.sea_level);

        let task = pool.spawn(async move {
            build_water_mesh_subchunk_async(coord, sub, chunk_copy, fc, borders).await
        });
        pending.0.insert((coord, sub), task);
    }
}

/// Runs the `water_collect_meshed_subchunks` routine for water collect meshed subchunks in the `generator::chunk::water_builder` module.
fn water_collect_meshed_subchunks(
    mut commands: Commands,
    mut pending: ResMut<PendingWaterMesh>,
    mut windex: ResMut<WaterMeshIndex>,
    mut backlog: ResMut<WaterMeshBacklog>,
    mut meshes: ResMut<Assets<Mesh>>,
    q_mesh: Query<&Mesh3d>,
    water_handle: Res<WaterMatHandle>,
    reg: Res<BlockRegistry>,
    app_state: Res<State<AppState>>,
    chunk_map: Res<ChunkMap>,
    water: Res<FluidMap>,
    time: Res<Time>,
) {
    let water_mat = id_any(&reg, &["water_block", "water"]);
    if water_mat == 0 {
        warn!("water_mat not found");
        return;
    }

    let in_game = matches!(app_state.get(), AppState::InGame(InGameStates::Game));
    let frame_ms = time.delta_secs() * 1000.0;
    let dynamic_divisor = if frame_ms > 30.0 {
        4
    } else if frame_ms > 22.0 {
        3
    } else if frame_ms > 17.0 {
        2
    } else {
        1
    };
    let apply_cap = if in_water_gen(&app_state) {
        (AsyncComputeTaskPool::get().thread_num().max(1) * 6).clamp(24, 144)
    } else {
        (MAX_WATER_MESH_APPLY_PER_FRAME / dynamic_divisor).max(1)
    };
    let apply_budget_ms = if in_game { 1.6 } else { 4.0 };
    let stage_start = Instant::now();
    let mut done = Vec::new();
    let mut applied = 0usize;

    for (key, task) in pending.0.iter_mut() {
        if applied >= apply_cap {
            break;
        }
        if applied > 0 && stage_start.elapsed().as_secs_f32() * 1000.0 >= apply_budget_ms {
            break;
        }

        if let Some(((coord, sub), build)) = future::block_on(future::poll_once(task)) {
            if !chunk_base_ready(coord, &chunk_map) {
                if !backlog.0.iter().any(|k| *k == (coord, sub)) {
                    backlog.0.push_back((coord, sub));
                }
                done.push(*key);
                continue;
            }

            if !chunk_map.chunks.contains_key(&coord) || !water.0.contains_key(&coord) {
                done.push(*key);
                continue;
            }

            despawn_water_mesh(
                (coord, sub as u8),
                &mut windex,
                &mut commands,
                &q_mesh,
                &mut meshes,
            );

            if !build.is_empty() {
                let mesh = build.into_mesh();
                let s = VOXEL_SIZE;
                let origin = Vec3::new(
                    (coord.x * CX as i32) as f32 * s,
                    (Y_MIN as f32) * s,
                    (coord.y * CZ as i32) as f32 * s,
                );

                let ent = commands
                    .spawn((
                        Mesh3d(meshes.add(mesh)),
                        MeshMaterial3d::<WaterMaterial>(water_handle.0.clone()),
                        Transform::from_translation(origin),
                        WaterSubchunkMesh { coord },
                        NotShadowReceiver,
                        NotShadowCaster,
                        Name::new(format!("water chunk({},{}) sub{}", coord.x, coord.y, sub)),
                    ))
                    .id();
                windex.0.insert((coord, sub as u8), ent);
            }

            applied += 1;
            done.push(*key);
        }
    }
    for k in done {
        pending.0.remove(&k);
    }
}

/// Runs the `try_enqueue` routine for try enqueue in the `generator::chunk::water_builder` module.
#[inline]
fn try_enqueue(
    c: IVec2,
    work: &mut VecDeque<IVec2>,
    queued: &mut HashSet<IVec2>,
    chunk_map: &ChunkMap,
    water: &FluidMap,
    pending: &PendingWaterLoad,
) {
    if !chunk_map.chunks.contains_key(&c) {
        return;
    }
    if water.0.contains_key(&c) || pending.0.contains_key(&c) {
        return;
    }
    if queued.insert(c) {
        work.push_back(c);
    }
}

#[inline]
fn chunk_base_ready(coord: IVec2, chunk_map: &ChunkMap) -> bool {
    chunk_map
        .chunks
        .get(&coord)
        .map(|chunk| chunk.dirty_mask == 0)
        .unwrap_or(false)
}

/// Runs the `rebuild_water_work_queue_impl` routine for rebuild water work queue impl in the `generator::chunk::water_builder` module.
fn rebuild_water_work_queue_impl(
    work: &mut VecDeque<IVec2>,
    chunk_map: &ChunkMap,
    water: &FluidMap,
    pending: &PendingWaterLoad,
) {
    work.retain(|c| chunk_map.chunks.contains_key(c));

    let mut queued: HashSet<IVec2> = work.iter().copied().collect();

    for &c in chunk_map.chunks.keys() {
        try_enqueue(c, work, &mut queued, chunk_map, water, pending);
        for d in [IVec2::X, -IVec2::X, IVec2::Y, -IVec2::Y] {
            try_enqueue(c + d, work, &mut queued, chunk_map, water, pending);
        }
    }
}

fn sync_water_ready_set(
    chunk_map: Res<ChunkMap>,
    water: Res<FluidMap>,
    q: Res<WaterGenQueue>,
    pending_load: Res<PendingWaterLoad>,
    todo: Res<WaterMeshingTodo>,
    backlog: Res<WaterMeshBacklog>,
    pending_mesh: Res<PendingWaterMesh>,
    mut ready_set: ResMut<WaterReadySet>,
) {
    ready_set
        .0
        .retain(|coord| chunk_map.chunks.contains_key(coord));

    let mut busy: HashSet<IVec2> = HashSet::new();
    busy.extend(q.work.iter().copied());
    busy.extend(pending_load.0.keys().copied());
    busy.extend(todo.0.iter().copied());
    busy.extend(backlog.0.iter().map(|(c, _)| *c));
    busy.extend(pending_mesh.0.keys().map(|(c, _)| *c));

    for &coord in chunk_map.chunks.keys() {
        let ready = chunk_base_ready(coord, &chunk_map)
            && water.0.contains_key(&coord)
            && !busy.contains(&coord);
        if ready {
            ready_set.0.insert(coord);
        } else {
            ready_set.0.remove(&coord);
        }
    }
}

fn sync_water_mesh_visibility(
    mut q_mesh: Query<(&WaterSubchunkMesh, &mut Visibility)>,
    q_cam: Query<&GlobalTransform, With<Camera3d>>,
    load_center: Option<Res<LoadCenter>>,
    game_config: Res<GlobalConfig>,
    ready_set: Option<Res<WaterReadySet>>,
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

    let visible = game_config.graphics.chunk_range.max(0);
    let hide_radius = visible + 1;
    let require_water_ready = multiplayer_connection.uses_local_save_data()
        && matches!(app_state.get(), AppState::Loading(LoadingStates::WaterGen));
    for (mesh, mut vis) in &mut q_mesh {
        let dx = (mesh.coord.x - center_c.x).abs();
        let dz = (mesh.coord.y - center_c.y).abs();
        let in_visible = dx <= visible && dz <= visible;
        let in_hide_band = dx <= hide_radius && dz <= hide_radius;
        let ready = if require_water_ready {
            ready_set
                .as_ref()
                .map(|set| set.0.contains(&mesh.coord))
                .unwrap_or(false)
        } else {
            true
        };
        let desired = match *vis {
            Visibility::Inherited => {
                if in_hide_band && ready {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                }
            }
            _ => {
                if in_visible && ready {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                }
            }
        };
        if *vis != desired {
            *vis = desired;
        }
    }
}
