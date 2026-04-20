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
    time: Res<Time>,
) {
    let stage_start = Instant::now();
    let waiting = is_waiting(&app_state);
    let in_game = matches!(app_state.get(), AppState::InGame(InGameStates::Game));
    let frame_ms = time.delta_secs() * 1000.0;
    let dynamic_divisor = frame_pressure_divisor(frame_ms);
    let ingame_apply_cap = game_config.graphics.chunk_collider_apply_per_frame.max(1);
    let ingame_apply_cap = if in_game {
        (ingame_apply_cap / dynamic_divisor).max(1)
    } else {
        ingame_apply_cap
    };
    let waiting_collider_apply_cap =
        (AsyncComputeTaskPool::get().thread_num().max(1) * 4).clamp(16, 96);
    let apply_cap = if waiting {
        waiting_collider_apply_cap
    } else {
        ingame_apply_cap
    };
    let mut done_keys = Vec::new();
    let poll_scan_limit = if waiting { 512usize } else { 128usize };
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
    let apply_budget_ms = if waiting {
        8.0
    } else if in_game {
        if frame_ms > 34.0 {
            0.8
        } else if frame_ms > 26.0 {
            1.1
        } else if frame_ms > 20.0 {
            1.4
        } else {
            1.8
        }
    } else {
        5.0
    };
    while applied < apply_cap {
        if applied > 0 && stage_start.elapsed().as_secs_f32() * 1000.0 >= apply_budget_ms {
            break;
        }
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

        let ent = if let Some(existing) = collider_index.0.remove(&(coord, sub)) {
            commands.entity(existing).insert((
                RigidBody::Fixed,
                collider,
                Transform::from_translation(build.origin),
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
                    Transform::from_translation(build.origin),
                    ChunkColliderProxy { coord },
                    Name::new(format!(
                        "collider chunk({},{}) sub{}",
                        coord.x, coord.y, sub
                    )),
                ))
                .id()
        };
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

#[inline]
fn chunk_min_distance_sq_blocks(coord: IVec2, point_blocks: IVec2) -> i64 {
    let min_x = coord.x * CX as i32;
    let max_x = min_x + CX as i32 - 1;
    let min_z = coord.y * CZ as i32;
    let max_z = min_z + CZ as i32 - 1;

    let dx = if point_blocks.x < min_x {
        i64::from(min_x - point_blocks.x)
    } else if point_blocks.x > max_x {
        i64::from(point_blocks.x - max_x)
    } else {
        0
    };
    let dz = if point_blocks.y < min_z {
        i64::from(min_z - point_blocks.y)
    } else if point_blocks.y > max_z {
        i64::from(point_blocks.y - max_z)
    } else {
        0
    };

    dx * dx + dz * dz
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
    if centers_xz_blocks.is_empty() {
        return;
    }

    for (entity, proxy, disabled) in q_colliders.iter() {
        let should_enable = centers_xz_blocks
            .iter()
            .any(|p| chunk_min_distance_sq_blocks(proxy.coord, *p) <= radius_sq);

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
    mut mesh_update: ResMut<MeshUpdateState>,
    mut ev_dirty: MessageReader<SubChunkNeedRemeshEvent>,
    game_config: Res<GlobalConfig>,
    app_state: Res<State<AppState>>,
) {
    if chunk_map.chunks.is_empty() {
        ev_dirty.clear();
        return;
    }

    let waiting = is_waiting(&app_state);
    let in_game_immediate = matches!(app_state.get(), AppState::InGame(InGameStates::Game));
    let max_inflight_mesh = if waiting {
        BIG
    } else if in_game_immediate {
        game_config.graphics.chunk_mesh_max_inflight.clamp(4, 16)
    } else {
        game_config.graphics.chunk_mesh_max_inflight.max(1)
    };
    let mut immediate_budget = 0usize;
    let mut immediate_used = 0usize;

    let reg_lite = RegLite::from_reg(&reg);
    let pool = AsyncComputeTaskPool::get();
    let mut by_chunk: HashMap<IVec2, Vec<usize>> = HashMap::new();

    for e in ev_dirty.read().copied() {
        if e.sub < SEC_COUNT {
            by_chunk.entry(e.coord).or_default().push(e.sub);
        }
    }
    for subs in by_chunk.values_mut() {
        subs.sort_unstable();
        subs.dedup();
    }
    let total_sub_events = by_chunk.values().map(Vec::len).sum::<usize>();
    let can_run_immediate = in_game_immediate && total_sub_events <= 2 && pending_mesh.0.len() < 4;
    if can_run_immediate {
        immediate_budget = total_sub_events.max(1);
    }

    for (coord, subs) in by_chunk {
        let Some(chunk) = chunk_map.chunks.get(&coord) else {
            for sub in subs {
                let key = (coord, sub);
                let desired_version = mesh_update.desired_mesh_versions.entry(key).or_insert(0);
                *desired_version = desired_version.saturating_add(1);
                immediate_ready.0.retain(|item| item.key != key);
                enqueue_mesh_fast(&mut backlog, &mut backlog_set, &pending_mesh, key);
            }
            continue;
        };

        let chunk_shared = Arc::new(chunk.clone());
        for sub in subs {
            let key = (coord, sub);
            let desired_version = mesh_update.desired_mesh_versions.entry(key).or_insert(0);
            *desired_version = desired_version.saturating_add(1);
            let desired_version = *desired_version;
            immediate_ready.0.retain(|item| item.key != key);

            if in_game_immediate && immediate_used < immediate_budget {
                // Apply player edits immediately in the current frame path.
                let y0 = sub * SEC_H;
                let y1 = (y0 + SEC_H).min(CY);
                let borders = snapshot_borders(&chunk_map, coord, y0, y1);
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
                    version: desired_version,
                    builds,
                    immediate: true,
                });
                immediate_used += 1;
                continue;
            }

            if pending_mesh.0.contains_key(&key) {
                continue;
            }

            if pending_mesh.0.len() < max_inflight_mesh {
                let y0 = sub * SEC_H;
                let y1 = (y0 + SEC_H).min(CY);
                let borders = snapshot_borders(&chunk_map, coord, y0, y1);
                let chunk_copy = Arc::clone(&chunk_shared);
                let reg_copy = reg_lite.clone();
                let t = pool.spawn(async move {
                    let builds =
                        mesh_subchunk_async(&chunk_copy, &reg_copy, sub, VOXEL_SIZE, Some(borders))
                            .await;
                    (key, builds)
                });

                mesh_update
                    .pending_mesh_versions
                    .insert(key, desired_version);
                pending_mesh.0.insert(key, t);
            } else {
                enqueue_mesh_fast(&mut backlog, &mut backlog_set, &pending_mesh, key);
            }
        }
    }
}

#[inline]
fn preempt_pending_gen_for_visible(
    pending_gen: &mut PendingGen,
    ready_latency: &mut ChunkReadyLatencyState,
    center_c: IVec2,
    protected_radius: i32,
    target_count: usize,
) -> usize {
    let mut preempted = 0usize;
    for _ in 0..target_count {
        let Some(victim) = pending_gen
            .0
            .keys()
            .take(2048)
            .copied()
            .filter(|coord| {
                (coord.x - center_c.x).abs() > protected_radius
                    || (coord.y - center_c.y).abs() > protected_radius
            })
            .max_by_key(|coord| {
                let dx = i64::from(coord.x - center_c.x);
                let dz = i64::from(coord.y - center_c.y);
                dx * dx + dz * dz
            })
        else {
            break;
        };

        if pending_gen.0.remove(&victim).is_some() {
            ready_latency.requested_at.remove(&victim);
            preempted += 1;
        }
    }

    preempted
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
    load_center: Option<Res<LoadCenter>>,
    mut ev_water_unload: MessageWriter<ChunkUnloadEvent>,
    mut ready_latency: ResMut<ChunkReadyLatencyState>,
    mut immediate_ready: ResMut<ImmediateMeshReadyQueue>,
) {
    let center_c = if let Some(lc) = load_center {
        lc.world_xz
    } else if let Some(cam) = q_cam.iter().next() {
        let cam_pos = cam.translation();
        let (coord, _) = world_to_chunk_xz(
            (cam_pos.x / VOXEL_SIZE).floor() as i32,
            (cam_pos.z / VOXEL_SIZE).floor() as i32,
        );
        coord
    } else {
        return;
    };

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
    let max_pending_save = (IoTaskPool::get().thread_num().max(1) * 8).clamp(32, 256);

    for coord in &to_remove {
        if multiplayer_connection.uses_local_save_data() {
            if unload_state.pending_save.0.len() >= max_pending_save {
                break;
            }
            if let Some(chunk) = chunk_map.chunks.get(coord) {
                let root = ws.root.clone();
                let chunk_copy = chunk.clone();
                let c = *coord;
                let pool = IoTaskPool::get();
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
            .mesh_update
            .pending_mesh_versions
            .retain(|(c, _), _| c != coord);
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
        unload_state
            .mesh_update
            .desired_mesh_versions
            .retain(|(c, _), _| c != coord);
        unload_state
            .mesh_update
            .last_mesh_fingerprint
            .retain(|(c, _), _| c != coord);
        unload_state
            .mesh_update
            .last_collider_fingerprint
            .retain(|(c, _), _| c != coord);
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
    cleanup.mesh_update.desired_mesh_versions.clear();
    cleanup.mesh_update.pending_mesh_versions.clear();
    cleanup.mesh_update.last_mesh_fingerprint.clear();
    cleanup.mesh_update.last_collider_fingerprint.clear();
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

#[inline]
fn hash_sample_indices(len: usize, mut f: impl FnMut(usize)) {
    if len == 0 {
        return;
    }
    let sample_count = len.min(16);
    if sample_count == len {
        for i in 0..len {
            f(i);
        }
        return;
    }

    let last = len - 1;
    let denom = (sample_count - 1).max(1);
    for s in 0..sample_count {
        let idx = (s * last) / denom;
        f(idx);
    }
}

#[inline]
fn hash_f32x3_sampled(values: &[[f32; 3]], hasher: &mut DefaultHasher) {
    values.len().hash(hasher);
    hash_sample_indices(values.len(), |i| {
        for c in values[i] {
            c.to_bits().hash(hasher);
        }
    });
}

#[inline]
fn hash_f32x2_sampled(values: &[[f32; 2]], hasher: &mut DefaultHasher) {
    values.len().hash(hasher);
    hash_sample_indices(values.len(), |i| {
        for c in values[i] {
            c.to_bits().hash(hasher);
        }
    });
}

#[inline]
fn hash_f32x4_sampled(values: &[[f32; 4]], hasher: &mut DefaultHasher) {
    values.len().hash(hasher);
    hash_sample_indices(values.len(), |i| {
        for c in values[i] {
            c.to_bits().hash(hasher);
        }
    });
}

#[inline]
fn hash_u32_sampled(values: &[u32], hasher: &mut DefaultHasher) {
    values.len().hash(hasher);
    hash_sample_indices(values.len(), |i| {
        values[i].hash(hasher);
    });
}

fn fingerprint_mesh_builds(builds: &[(BlockId, MeshBuild)]) -> u64 {
    let mut hasher = DefaultHasher::new();
    builds.len().hash(&mut hasher);
    for (bid, mb) in builds {
        bid.hash(&mut hasher);
        hash_f32x3_sampled(&mb.pos, &mut hasher);
        hash_f32x3_sampled(&mb.nrm, &mut hasher);
        hash_f32x2_sampled(&mb.uv, &mut hasher);
        hash_f32x2_sampled(&mb.ctm, &mut hasher);
        hash_f32x4_sampled(&mb.tile_rect, &mut hasher);
        hash_u32_sampled(&mb.idx, &mut hasher);
    }
    hasher.finish()
}

fn fingerprint_collider_geometry(positions: &[[f32; 3]], indices: &[u32]) -> u64 {
    let mut hasher = DefaultHasher::new();
    hash_f32x3_sampled(positions, &mut hasher);
    hash_u32_sampled(indices, &mut hasher);
    hasher.finish()
}

fn append_custom_box_colliders_for_subchunk(
    chunk: &ChunkData,
    reg: &BlockRegistry,
    sub: usize,
    voxel_size: f32,
    positions: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
) {
    let y0 = sub * SEC_H;
    let y1 = (y0 + SEC_H).min(CY);
    if y0 >= y1 {
        return;
    }

    for ly in y0..y1 {
        for z in 0..CZ {
            for x in 0..CX {
                let id = chunk.get(x, ly, z);
                let Some((size_m, offset_m)) = reg.collision_box(id) else {
                    continue;
                };
                let center = Vec3::new(
                    (x as f32 + 0.5 + offset_m[0]) * voxel_size,
                    (ly as f32 + 0.5 + offset_m[1]) * voxel_size,
                    (z as f32 + 0.5 + offset_m[2]) * voxel_size,
                );
                let half = Vec3::new(
                    size_m[0] * voxel_size * 0.5,
                    size_m[1] * voxel_size * 0.5,
                    size_m[2] * voxel_size * 0.5,
                )
                .max(Vec3::splat(0.001));
                append_box_collider_triangles(center, half, positions, indices);
            }
        }
    }

    for ly in y0..y1 {
        for z in 0..CZ {
            for x in 0..CX {
                let id = chunk.get_stacked(x, ly, z);
                let Some((size_m, offset_m)) = reg.collision_box(id) else {
                    continue;
                };
                let center = Vec3::new(
                    (x as f32 + 0.5 + offset_m[0]) * voxel_size,
                    (ly as f32 + 0.5 + offset_m[1]) * voxel_size,
                    (z as f32 + 0.5 + offset_m[2]) * voxel_size,
                );
                let half = Vec3::new(
                    size_m[0] * voxel_size * 0.5,
                    size_m[1] * voxel_size * 0.5,
                    size_m[2] * voxel_size * 0.5,
                )
                .max(Vec3::splat(0.001));
                append_box_collider_triangles(center, half, positions, indices);
            }
        }
    }
}

fn append_box_collider_triangles(
    center: Vec3,
    half: Vec3,
    positions: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
) {
    const BOX_INDICES: [u32; 36] = [
        0, 1, 2, 0, 2, 3, // -Z
        4, 6, 5, 4, 7, 6, // +Z
        0, 3, 7, 0, 7, 4, // -X
        1, 5, 6, 1, 6, 2, // +X
        3, 2, 6, 3, 6, 7, // +Y
        0, 4, 5, 0, 5, 1, // -Y
    ];

    let min = center - half;
    let max = center + half;
    let base = positions.len() as u32;
    positions.extend_from_slice(&[
        [min.x, min.y, min.z],
        [max.x, min.y, min.z],
        [max.x, max.y, min.z],
        [min.x, max.y, min.z],
        [min.x, min.y, max.z],
        [max.x, min.y, max.z],
        [max.x, max.y, max.z],
        [min.x, max.y, max.z],
    ]);
    indices.extend(BOX_INDICES.iter().map(|idx| base + *idx));
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
    let half_xz = (s * 0.5).max(0.05);
    let mut parts: Vec<(Vec3, Quat, Collider)> = Vec::with_capacity(CX * CZ);

    for z in 0..CZ {
        for x in 0..CX {
            // Build contiguous vertical runs to keep placeholder collision robust in caves,
            // not only on top surfaces.
            let mut run_start: Option<usize> = None;
            for ly in y0..=y1 {
                let solid = ly < y1 && reg.is_solid_for_collision(chunk.get(x, ly, z));
                match (run_start, solid) {
                    (None, true) => run_start = Some(ly),
                    (Some(start), false) => {
                        let end = ly - 1;
                        let blocks = (end - start + 1) as f32;
                        let half_y = (blocks * s * 0.5).max(0.05);
                        let center = Vec3::new(
                            (x as f32 + 0.5) * s,
                            (start as f32 * s) + half_y,
                            (z as f32 + 0.5) * s,
                        );
                        parts.push((
                            center,
                            Quat::IDENTITY,
                            Collider::cuboid(half_xz, half_y, half_xz),
                        ));
                        run_start = None;
                    }
                    _ => {}
                }
            }
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
fn frame_pressure_divisor(frame_ms: f32) -> usize {
    if frame_ms > 45.0 {
        8
    } else if frame_ms > 34.0 {
        6
    } else if frame_ms > 26.0 {
        4
    } else if frame_ms > 20.0 {
        3
    } else if frame_ms > 16.0 {
        2
    } else {
        1
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

    let visible = visible_radius(game_config.graphics.chunk_range);
    let require_chunk_ready = !matches!(app_state.get(), AppState::InGame(InGameStates::Game));
    for (mesh, mut vis) in &mut q_mesh {
        let in_visible = (mesh.coord.x - center_c.x).abs() <= visible
            && (mesh.coord.y - center_c.y).abs() <= visible;
        let ready = if require_chunk_ready {
            ready_set.0.contains(&mesh.coord)
        } else {
            true
        };
        let desired = if in_visible && ready {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != desired {
            *vis = desired;
        }
    }
}
