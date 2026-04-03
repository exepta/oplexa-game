use crate::core::config::{GlobalConfig, WorldGenConfig};
use crate::core::events::chunk_events::{ChunkUnloadEvent, SubChunkNeedRemeshEvent};
use crate::core::multiplayer::MultiplayerConnectionState;
use crate::core::states::states::{AppState, InGameStates, LoadingStates};
use crate::core::world::biome::registry::BiomeRegistry;
use crate::core::world::block::*;
use crate::core::world::chunk::*;
use crate::core::world::chunk_dimension::*;
use crate::core::world::save::WorldSave;
use crate::generator::chunk::chunk_struct::*;
use crate::generator::chunk::chunk_utils::*;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;
use bevy::tasks::futures_lite::future;
use bevy_rapier3d::prelude::{Collider, RigidBody, TriMeshFlags};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

const MAX_MESH_APPLY_PER_FRAME: usize = MAX_UPDATE_FRAMES / 2;
const MAX_COLLIDER_APPLY_PER_FRAME: usize = MAX_UPDATE_FRAMES / 4;
const MAX_INFLIGHT_COLLIDER_BUILD: usize = 20;

/// Represents collider backlog used by the `generator::chunk::chunk_builder` module.
#[derive(Default, Resource)]
struct ColliderBacklog(HashMap<(IVec2, u8), ColliderTodo>);

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

/// Represents pending collider build used by the `generator::chunk::chunk_builder` module.
#[derive(Resource, Default)]
struct PendingColliderBuild(
    pub HashMap<(IVec2, u8), bevy::tasks::Task<((IVec2, u8), ColliderBuild)>>,
);

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

/// Represents chunk unload state used by the `generator::chunk::chunk_builder` module.
#[derive(SystemParam)]
struct ChunkUnloadState<'w, 's> {
    pending_gen: ResMut<'w, PendingGen>,
    pending_mesh: ResMut<'w, PendingMesh>,
    backlog: ResMut<'w, MeshBacklog>,
    pending_collider: ResMut<'w, PendingColliderBuild>,
    pending_save: ResMut<'w, PendingChunkSave>,
    coll_backlog: ResMut<'w, ColliderBacklog>,
    _marker: std::marker::PhantomData<&'s ()>,
}

/// Represents chunk cleanup state used by the `generator::chunk::chunk_builder` module.
#[derive(SystemParam)]
struct ChunkCleanupState<'w, 's> {
    pending_gen: ResMut<'w, PendingGen>,
    pending_mesh: ResMut<'w, PendingMesh>,
    backlog: ResMut<'w, MeshBacklog>,
    pending_collider: ResMut<'w, PendingColliderBuild>,
    pending_save: ResMut<'w, PendingChunkSave>,
    coll_backlog: ResMut<'w, ColliderBacklog>,
    kick_queue: ResMut<'w, KickQueue>,
    kicked: ResMut<'w, KickedOnce>,
    queued: ResMut<'w, QueuedOnce>,
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
            .init_resource::<PendingChunkSave>()
            .init_resource::<KickQueue>()
            .init_resource::<KickedOnce>()
            .init_resource::<QueuedOnce>()
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
                    (
                        collect_meshed_subchunks,
                        schedule_collider_build_tasks,
                        collect_finished_collider_builds,
                        enqueue_kick_for_new_subchunks,
                        process_kick_queue,
                    )
                        .chain()
                        .run_if(
                            in_state(AppState::Loading(LoadingStates::BaseGen))
                                .or(in_state(AppState::Loading(LoadingStates::CaveGen)))
                                .or(in_state(AppState::InGame(InGameStates::Game))),
                        ),
                    schedule_remesh_tasks_from_events
                        .in_set(VoxelStage::Meshing)
                        .run_if(
                            in_state(AppState::Loading(LoadingStates::BaseGen))
                                .or(in_state(AppState::Loading(LoadingStates::CaveGen)))
                                .or(in_state(AppState::InGame(InGameStates::Game))),
                        ),
                    drain_mesh_backlog.run_if(
                        in_state(AppState::Loading(LoadingStates::BaseGen))
                            .or(in_state(AppState::Loading(LoadingStates::CaveGen)))
                            .or(in_state(AppState::InGame(InGameStates::Game))),
                    ),
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
            queued.0.remove(&(item.coord, item.sub));
            queue.0.swap_remove(i);
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
    let initial_radius = game_config.graphics.chunk_range.min(3);
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
            next.set(AppState::Loading(LoadingStates::CaveGen));
        } else {
            next.set(AppState::InGame(InGameStates::Game));
        }
    }
}

//System
/// Runs the `schedule_chunk_generation` routine for schedule chunk generation in the `generator::chunk::chunk_builder` module.
fn schedule_chunk_generation(
    mut pending: ResMut<PendingGen>,
    chunk_map: Res<ChunkMap>,
    pending_save: Res<PendingChunkSave>,
    reg: Res<BlockRegistry>,
    biomes: Res<BiomeRegistry>,
    gen_cfg: Res<WorldGenConfig>,
    game_config: Res<GlobalConfig>,
    ws: Res<WorldSave>,
    q_cam: Query<&GlobalTransform, With<Camera3d>>,
    load_center: Option<Res<LoadCenter>>,
    app_state: Res<State<AppState>>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
) {
    if !multiplayer_connection.uses_local_save_data() {
        return;
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

    let waiting = is_waiting(&app_state);
    let max_inflight = if waiting { BIG } else { MAX_INFLIGHT_GEN };
    let per_frame_submit = if waiting { BIG } else { 8 };

    if pending.0.len() >= max_inflight {
        return;
    }

    let load_radius = game_config.graphics.chunk_range;
    let mut budget = max_inflight
        .saturating_sub(pending.0.len())
        .min(per_frame_submit);

    // --- NEW: Arc-wrap registries once per system tick (cheap per task) ---
    let reg_arc = Arc::new(reg.clone());
    let biomes_arc = Arc::new(biomes.clone());

    let cfg_clone = gen_cfg.clone();
    let ws_root = ws.root.clone();
    let pool = AsyncComputeTaskPool::get();

    for dz in -load_radius..=load_radius {
        for dx in -load_radius..=load_radius {
            if budget == 0 {
                return;
            }

            let c = IVec2::new(center_c.x + dx, center_c.y + dz);
            if chunk_map.chunks.contains_key(&c)
                || pending.0.contains_key(&c)
                || pending_save.0.contains_key(&c)
            {
                continue;
            }

            // clone the inexpensive Arcs for this task
            let reg_for_task = Arc::clone(&reg_arc);
            let biomes_for_task = Arc::clone(&biomes_arc);
            let cfg = cfg_clone.clone();
            let root = ws_root.clone();

            let task = pool.spawn(async move {
                // NOTE: new load_or_gen signature: (root, coord, &BlockRegistry, &BiomeRegistry, cfg)
                let data = load_or_gen_chunk_async(
                    root,
                    c,
                    &*reg_for_task,    // deref Arc -> &BlockRegistry
                    &*biomes_for_task, // deref Arc -> &BiomeRegistry
                    cfg,
                )
                .await;
                (c, data)
            });

            pending.0.insert(c, task);
            budget -= 1;
        }
    }
}

//System
/// Runs the `drain_mesh_backlog` routine for drain mesh backlog in the `generator::chunk::chunk_builder` module.
fn drain_mesh_backlog(
    mut backlog: ResMut<MeshBacklog>,
    mut pending_mesh: ResMut<PendingMesh>,
    chunk_map: Res<ChunkMap>,
    reg: Res<BlockRegistry>,
    app_state: Res<State<AppState>>,
) {
    if chunk_map.chunks.is_empty() {
        backlog.0.clear();
        return;
    }

    let waiting = is_waiting(&app_state);
    let max_inflight_mesh = if waiting { BIG } else { MAX_INFLIGHT_MESH };

    let reg_lite = RegLite::from_reg(&reg);
    let pool = AsyncComputeTaskPool::get();

    while pending_mesh.0.len() < max_inflight_mesh {
        let Some((coord, sub)) = backlog.0.pop_front() else {
            break;
        };
        if pending_mesh.0.contains_key(&(coord, sub)) {
            continue;
        }
        let Some(chunk) = chunk_map.chunks.get(&coord) else {
            continue;
        };

        let chunk_copy = chunk.clone();
        let reg_copy = reg_lite.clone();
        let y0 = sub * SEC_H;
        let y1 = (y0 + SEC_H).min(CY);
        let borders = snapshot_borders(&chunk_map, coord, y0, y1);

        let key = (coord, sub);
        let t = pool.spawn(async move {
            let builds =
                mesh_subchunk_async(&chunk_copy, &reg_copy, sub, VOXEL_SIZE, Some(borders)).await;
            (key, builds)
        });
        pending_mesh.0.insert(key, t);
    }
}

//System
/// Runs the `collect_generated_chunks` routine for collect generated chunks in the `generator::chunk::chunk_builder` module.
fn collect_generated_chunks(
    mut pending_gen: ResMut<PendingGen>,
    mut pending_mesh: ResMut<PendingMesh>,
    mut backlog: ResMut<MeshBacklog>,
    mut chunk_map: ResMut<ChunkMap>,
    reg: Res<BlockRegistry>,
    app_state: Res<State<AppState>>,
) {
    let waiting = is_waiting(&app_state);
    let max_inflight_mesh = if waiting { BIG } else { MAX_INFLIGHT_MESH };

    let reg_lite = RegLite::from_reg(&reg);
    let mut finished = Vec::new();

    for (coord, task) in pending_gen.0.iter_mut() {
        if let Some((c, data)) = future::block_on(future::poll_once(task)) {
            chunk_map.chunks.insert(c, data.clone());

            let pool = AsyncComputeTaskPool::get();
            let order = sub_priority_order(&data);
            for sub in order {
                let key = (c, sub);
                let y0 = sub * SEC_H;
                let y1 = (y0 + SEC_H).min(CY);
                let borders = snapshot_borders(&chunk_map, c, y0, y1);

                if pending_mesh.0.len() < max_inflight_mesh {
                    let chunk_copy = data.clone();
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
                    enqueue_mesh(&mut backlog, &pending_mesh, key);
                }
            }

            for n_coord in neighbors4_iter(c) {
                if let Some(n_chunk) = chunk_map.chunks.get(&n_coord) {
                    let order_n = sub_priority_order(n_chunk);
                    for sub in order_n {
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
                            let chunk_copy = n_chunk.clone();
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
                            enqueue_mesh(&mut backlog, &pending_mesh, key);
                        }
                    }
                }
            }

            finished.push(*coord);
        }
    }

    for c in finished {
        pending_gen.0.remove(&c);
    }
}

//System
/// Runs the `collect_meshed_subchunks` routine for collect meshed subchunks in the `generator::chunk::chunk_builder` module.
fn collect_meshed_subchunks(
    mut commands: Commands,
    mut pending_mesh: ResMut<PendingMesh>,
    mut mesh_index: ResMut<ChunkMeshIndex>,
    mut collider_index: ResMut<ChunkColliderIndex>,
    mut pending_collider: ResMut<PendingColliderBuild>,
    mut meshes: ResMut<Assets<Mesh>>,
    reg: Res<BlockRegistry>,
    mut chunk_map: ResMut<ChunkMap>,
    q_mesh: Query<&Mesh3d>,
    app_state: Res<State<AppState>>,
    mut coll_backlog: ResMut<ColliderBacklog>,
) {
    let waiting = is_waiting(&app_state);
    let apply_cap = if waiting {
        BIG
    } else {
        MAX_MESH_APPLY_PER_FRAME
    };
    let mut done_keys = Vec::new();
    let mut applied = 0usize;

    for (key, task) in pending_mesh.0.iter_mut() {
        if applied >= apply_cap {
            break;
        }

        if let Some(((coord, sub), builds)) = future::block_on(future::poll_once(task)) {
            // Despawn render meshes for this (coord,sub) first (safe).
            let old_keys: Vec<_> = mesh_index
                .map
                .keys()
                .cloned()
                .filter(|(c, s, _)| c == &coord && *s as usize == sub)
                .collect();
            despawn_mesh_set(
                old_keys,
                &mut mesh_index,
                &mut commands,
                &q_mesh,
                &mut meshes,
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
                if !reg.is_fluid(bid) {
                    let base = phys_positions.len() as u32;
                    phys_positions.extend_from_slice(&mb.pos);
                    phys_indices.extend(mb.idx.iter().map(|i| base + *i));
                }

                let mesh = mb.into_mesh();
                let ent = commands
                    .spawn((
                        Mesh3d(meshes.add(mesh)),
                        MeshMaterial3d(reg.material(bid)),
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
                mesh_index.map.insert((coord, sub as u8, bid), ent);
            }

            // ----- Physics collider handling -----
            let need_collider = !phys_positions.is_empty();

            if need_collider {
                coll_backlog.0.insert(
                    (coord, sub as u8),
                    ColliderTodo {
                        coord,
                        sub: sub as u8,
                        origin,
                        positions: phys_positions,
                        indices: phys_indices,
                    },
                );
            } else {
                // No geometry → ensure collider is removed (solid gone).
                coll_backlog.0.remove(&(coord, sub as u8));
                pending_collider.0.remove(&(coord, sub as u8));
                if let Some(ent) = collider_index.0.remove(&(coord, sub as u8)) {
                    safe_despawn_entity(&mut commands, ent);
                }
            }

            if let Some(chunk) = chunk_map.chunks.get_mut(&coord) {
                chunk.clear_dirty(sub);
            }

            applied += 1;
            done_keys.push(*key);
        }
    }

    for k in done_keys {
        pending_mesh.0.remove(&k);
    }
}

/// Runs the `schedule_collider_build_tasks` routine for schedule collider build tasks in the `generator::chunk::chunk_builder` module.
fn schedule_collider_build_tasks(
    mut backlog: ResMut<ColliderBacklog>,
    mut pending: ResMut<PendingColliderBuild>,
    app_state: Res<State<AppState>>,
) {
    let waiting = is_waiting(&app_state);
    let max_inflight = if waiting {
        BIG
    } else {
        MAX_INFLIGHT_COLLIDER_BUILD
    };
    let pool = AsyncComputeTaskPool::get();

    while pending.0.len() < max_inflight {
        let Some(key) = backlog.0.keys().next().copied() else {
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
            let collider = build_trimesh_collider(todo.positions, todo.indices, flags);
            (
                (todo.coord, todo.sub),
                ColliderBuild {
                    origin: todo.origin,
                    collider,
                },
            )
        });
        pending.0.insert(key, task);
    }
}

/// Runs the `collect_finished_collider_builds` routine for collect finished collider builds in the `generator::chunk::chunk_builder` module.
fn collect_finished_collider_builds(
    mut commands: Commands,
    mut pending: ResMut<PendingColliderBuild>,
    backlog: Res<ColliderBacklog>,
    mut collider_index: ResMut<ChunkColliderIndex>,
    chunk_map: Res<ChunkMap>,
    app_state: Res<State<AppState>>,
) {
    let waiting = is_waiting(&app_state);
    let apply_cap = if waiting {
        BIG
    } else {
        MAX_COLLIDER_APPLY_PER_FRAME
    };
    let mut done_keys = Vec::new();
    let mut applied = 0usize;

    for (key, task) in pending.0.iter_mut() {
        if applied >= apply_cap {
            break;
        }

        if let Some(((coord, sub), build)) = future::block_on(future::poll_once(task)) {
            done_keys.push(*key);

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
                    Name::new(format!(
                        "collider chunk({},{}) sub{}",
                        coord.x, coord.y, sub
                    )),
                ))
                .id();
            collider_index.0.insert((coord, sub), ent);
            applied += 1;
        }
    }

    for k in done_keys {
        pending.0.remove(&k);
    }
}

//System
/// Runs the `schedule_remesh_tasks_from_events` routine for schedule remesh tasks from events in the `generator::chunk::chunk_builder` module.
fn schedule_remesh_tasks_from_events(
    mut pending_mesh: ResMut<PendingMesh>,
    chunk_map: Res<ChunkMap>,
    reg: Res<BlockRegistry>,
    mut backlog: ResMut<MeshBacklog>,
    mut ev_dirty: MessageReader<SubChunkNeedRemeshEvent>,
    app_state: Res<State<AppState>>,
) {
    if chunk_map.chunks.is_empty() {
        ev_dirty.clear();
        return;
    }

    let waiting = is_waiting(&app_state);
    let max_inflight_mesh = if waiting { BIG } else { MAX_INFLIGHT_MESH };

    let reg_lite = RegLite::from_reg(&reg);
    let pool = AsyncComputeTaskPool::get();

    for e in ev_dirty.read().copied() {
        let coord = e.coord;
        let sub = e.sub;
        let key = (coord, sub);

        if pending_mesh.0.contains_key(&key) {
            continue;
        }

        let Some(chunk) = chunk_map.chunks.get(&coord) else {
            enqueue_mesh(&mut backlog, &pending_mesh, key);
            continue;
        };

        let y0 = sub * SEC_H;
        let y1 = (y0 + SEC_H).min(CY);
        let borders = snapshot_borders(&chunk_map, coord, y0, y1);

        if pending_mesh.0.len() < max_inflight_mesh {
            let chunk_copy = chunk.clone();
            let reg_copy = reg_lite.clone();

            let t = pool.spawn(async move {
                let builds =
                    mesh_subchunk_async(&chunk_copy, &reg_copy, sub, VOXEL_SIZE, Some(borders))
                        .await;
                (key, builds)
            });

            pending_mesh.0.insert(key, t);
        } else {
            enqueue_mesh(&mut backlog, &pending_mesh, key);
        }
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

    let keep_radius = game_config.graphics.chunk_range + 1;

    let to_remove: Vec<IVec2> = chunk_map
        .chunks
        .keys()
        .filter(|coord| {
            (coord.x - center_c.x).abs() > keep_radius || (coord.y - center_c.y).abs() > keep_radius
        })
        .cloned()
        .collect();

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
        unload_state.coll_backlog.0.retain(|(c, _), _| *c != *coord);
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
    cleanup.pending_collider.0.clear();
    cleanup.pending_save.0.clear();
    cleanup.coll_backlog.0.clear();
    cleanup.kick_queue.0.clear();
    cleanup.kicked.0.clear();
    cleanup.queued.0.clear();
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
    positions: Vec<[f32; 3]>,
    indices: Vec<u32>,
    flags: TriMeshFlags,
) -> Option<Collider> {
    if indices.len() < 3 || indices.len() % 3 != 0 {
        return None;
    }

    let verts: Vec<Vec3> = positions
        .into_iter()
        .map(|p| Vec3::new(p[0], p[1], p[2]))
        .collect();
    let tris: Vec<[u32; 3]> = indices
        .chunks_exact(3)
        .map(|tri| [tri[0], tri[1], tri[2]])
        .collect();

    Collider::trimesh_with_flags(verts, tris, flags).ok()
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
