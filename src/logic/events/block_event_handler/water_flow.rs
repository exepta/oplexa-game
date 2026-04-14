/// Water-flow simulation systems and helper routines.
fn init_water_flow_ids(registry: Res<BlockRegistry>, mut flow_ids: ResMut<WaterFlowIds>) {
    if flow_ids.initialized {
        return;
    }

    let source_id = id_any(&registry, &["water_block", "water"]);
    if source_id == 0 {
        return;
    }

    flow_ids.source_id = source_id;
    flow_ids.settled_id = 0;
    flow_ids.by_level = [0; 11];
    flow_ids.by_level[WATER_FLOW_SOURCE_LEVEL as usize] = source_id;
    let source_name = registry.def(source_id).localized_name.as_str();
    let settled_name = format!("{source_name}_flow_{WATER_FLOW_SOURCE_LEVEL}");
    let legacy_settled_name = format!("water_flow_{WATER_FLOW_SOURCE_LEVEL}");
    flow_ids.settled_id = registry
        .id_opt(settled_name.as_str())
        .or_else(|| registry.id_opt(legacy_settled_name.as_str()))
        .unwrap_or(0);
    for level in 1..WATER_FLOW_SOURCE_LEVEL {
        let generated_name = format!("{source_name}_flow_{level}");
        let legacy_name = format!("water_flow_{level}");
        flow_ids.by_level[level as usize] = registry
            .id_opt(generated_name.as_str())
            .or_else(|| registry.id_opt(legacy_name.as_str()))
            .unwrap_or(0);
    }
    flow_ids.initialized = true;
}

fn track_water_flow_sources_from_block_events(
    mut place_events: MessageReader<BlockPlaceByPlayerEvent>,
    mut break_events: MessageReader<BlockBreakByPlayerEvent>,
    mut observed_place_events: MessageReader<BlockPlaceObservedEvent>,
    mut observed_break_events: MessageReader<BlockBreakObservedEvent>,
    chunk_map: Res<ChunkMap>,
    flow_ids: Res<WaterFlowIds>,
    mut flow_state: ResMut<WaterFlowState>,
) {
    if !flow_ids.initialized || flow_ids.source_id == 0 {
        return;
    }

    for event in place_events.read() {
        if event.block_id == flow_ids.source_id {
            flow_state.sources.insert(event.location);
        } else {
            flow_state.sources.remove(&event.location);
        }
        queue_water_reactivation_for_change(event.location, &chunk_map, &flow_ids, &mut flow_state);
    }

    for event in break_events.read() {
        flow_state.sources.remove(&event.location);
        queue_water_reactivation_for_change(event.location, &chunk_map, &flow_ids, &mut flow_state);
    }

    for event in observed_place_events.read() {
        if event.block_id == flow_ids.source_id {
            flow_state.sources.insert(event.location);
        } else {
            flow_state.sources.remove(&event.location);
        }
        queue_water_reactivation_for_change(event.location, &chunk_map, &flow_ids, &mut flow_state);
    }

    for event in observed_break_events.read() {
        flow_state.sources.remove(&event.location);
        queue_water_reactivation_for_change(event.location, &chunk_map, &flow_ids, &mut flow_state);
    }
}

#[inline]
fn queue_water_reactivation_for_change(
    center: IVec3,
    chunk_map: &ChunkMap,
    flow_ids: &WaterFlowIds,
    flow_state: &mut WaterFlowState,
) {
    let mut water_nearby = false;
    for off in [
        IVec3::ZERO,
        IVec3::X,
        IVec3::NEG_X,
        IVec3::Y,
        IVec3::NEG_Y,
        IVec3::Z,
        IVec3::NEG_Z,
        IVec3::new(1, 0, 1),
        IVec3::new(1, 0, -1),
        IVec3::new(-1, 0, 1),
        IVec3::new(-1, 0, -1),
    ] {
        let p = center + off;
        if water_level_at_world(p, chunk_map, flow_ids) > 0 || flow_state.sources.contains(&p) {
            water_nearby = true;
            break;
        }
    }
    if !water_nearby {
        return;
    }

    enqueue_changed_neighbor_rechecks(flow_state, center);
    enqueue_water_frontier_pos(flow_state, center);
}

fn run_water_flow_simulation(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    time: Res<Time>,
    registry: Res<BlockRegistry>,
    item_registry: Res<ItemRegistry>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
    flow_ids: Res<WaterFlowIds>,
    mut flow_state: ResMut<WaterFlowState>,
    mut chunk_map: ResMut<ChunkMap>,
    mut fluids: ResMut<FluidMap>,
    mut ev_dirty: MessageWriter<SubChunkNeedRemeshEvent>,
) {
    if !flow_ids.initialized || flow_ids.source_id == 0 {
        return;
    }

    // Multiplayer stays server-authoritative: this local runtime simulation only runs
    // for local-save worlds.
    if !multiplayer_connection.uses_local_save_data() {
        flow_state.frontier.clear();
        flow_state.queued.clear();
        flow_state.pending_per_subchunk.clear();
        flow_state.sleeping_subchunks.clear();
        flow_state.accumulator_secs = 0.0;
        return;
    }

    let configured_step_ms = registry
        .fluid_flow(flow_ids.source_id)
        .map(|cfg| cfg.step_ms)
        .unwrap_or(WATER_FLOW_DEFAULT_STEP_MS)
        .clamp(50.0, 60_000.0);
    if (flow_state.step_ms - configured_step_ms).abs() > 0.5 {
        flow_state.step_ms = configured_step_ms;
        flow_state.step_secs = configured_step_ms / 1000.0;
    }

    flow_state.accumulator_secs += time.delta_secs().max(0.0);
    if flow_state.accumulator_secs < flow_state.step_secs {
        return;
    }

    let mut steps = (flow_state.accumulator_secs / flow_state.step_secs).floor() as usize;
    steps = steps.clamp(1, WATER_FLOW_MAX_STEPS_PER_FRAME);
    flow_state.accumulator_secs -= flow_state.step_secs * steps as f32;

    let now = time.elapsed_secs();
    let mut dirty_subchunks = HashSet::<(IVec2, usize)>::new();
    for _ in 0..steps {
        run_single_water_flow_step(
            &mut commands,
            &mut meshes,
            &registry,
            &item_registry,
            &multiplayer_connection,
            &flow_ids,
            &mut flow_state,
            &mut chunk_map,
            &mut fluids,
            now,
            &mut dirty_subchunks,
        );
        if flow_state.frontier.is_empty() {
            break;
        }
    }

    apply_batched_dirty_subchunks(&mut chunk_map, &mut ev_dirty, &dirty_subchunks);
}

#[allow(clippy::too_many_arguments)]
fn run_single_water_flow_step(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    registry: &BlockRegistry,
    item_registry: &ItemRegistry,
    multiplayer_connection: &MultiplayerConnectionState,
    flow_ids: &WaterFlowIds,
    flow_state: &mut WaterFlowState,
    chunk_map: &mut ChunkMap,
    fluids: &mut FluidMap,
    now: f32,
    dirty_subchunks: &mut HashSet<(IVec2, usize)>,
) {
    let stale_sources: Vec<IVec3> = flow_state
        .sources
        .iter()
        .copied()
        .filter(|pos| get_block_world(chunk_map, *pos) != flow_ids.source_id)
        .collect();
    for pos in stale_sources {
        flow_state.sources.remove(&pos);
        queue_water_reactivation_for_change(pos, chunk_map, flow_ids, flow_state);
    }

    if flow_state.frontier.is_empty() {
        return;
    }

    let mut changed_positions = HashSet::<IVec3>::new();
    let mut touched_subchunks = HashSet::<(IVec2, usize)>::new();
    let mut changed_subchunks = HashSet::<(IVec2, usize)>::new();
    let budget = flow_state.tick_cell_budget.max(1);
    // Process only the current frontier slice per tick. Newly enqueued neighbors
    // are processed on the next simulation tick, creating visible flow delay.
    let process_count = flow_state.frontier.len().min(budget);
    for _ in 0..process_count {
        let Some(pos) = dequeue_water_frontier_pos(flow_state) else {
            break;
        };
        if pos.y < Y_MIN || pos.y > Y_MAX {
            continue;
        }
        let Some(sub_key) = water_subchunk_key_for_world_pos(pos) else {
            continue;
        };

        let (chunk_coord, _) = world_to_chunk_xz(pos.x, pos.z);
        if !chunk_map.chunks.contains_key(&chunk_coord) {
            continue;
        }
        touched_subchunks.insert(sub_key);
        if get_block_world(chunk_map, pos) == flow_ids.settled_id && !flow_state.sources.contains(&pos)
        {
            continue;
        }

        let is_explicit_source = flow_state.sources.contains(&pos);
        let desired_level = if is_explicit_source {
            WATER_FLOW_SOURCE_LEVEL
        } else {
            desired_water_level_two_phase(pos, chunk_map, registry, flow_ids, flow_state)
        };

        let current = get_block_world(chunk_map, pos);
        let current_stacked = get_stacked_block_world(chunk_map, pos);
        let mut changed = false;
        if desired_level == 0 {
            let is_runtime_flow = flow_ids.contains(current)
                && current != flow_ids.settled_id
                && !(current == flow_ids.source_id && is_explicit_source);
            if is_runtime_flow {
                if let Some(mut access) = world_access_mut(chunk_map, pos) {
                    if current_stacked != 0 {
                        access.set(current_stacked);
                        access.set_stacked(0);
                    } else {
                        access.set(0);
                        access.set_stacked(0);
                    }
                    set_fluid_bit_for_world_pos(fluids, pos, false);
                    changed = true;
                }
            } else if flow_ids.contains(current) {
                set_fluid_bit_for_world_pos(fluids, pos, true);
            }
        } else {
            let target_id = flow_ids.id_for_level(desired_level);
            if target_id == 0 {
                continue;
            }
            let push_primary_to_stacked = current_stacked == 0
                && current != 0
                && !registry.is_fluid(current)
                && registry.is_water_logged(current);
            let keep_stacked = (current_stacked != 0 && registry.is_water_logged(current_stacked))
                || push_primary_to_stacked;
            if current != target_id
                || (current_stacked != 0 && !keep_stacked)
                || push_primary_to_stacked
            {
                if let Some(mut access) = world_access_mut(chunk_map, pos) {
                    access.set(target_id);
                    if push_primary_to_stacked {
                        access.set_stacked(current);
                    } else if !keep_stacked {
                        access.set_stacked(0);
                    }
                    changed = true;
                }
            }
            set_fluid_bit_for_world_pos(fluids, pos, true);
        }

        if changed {
            changed_positions.insert(pos);
            changed_subchunks.insert(sub_key);
            enqueue_changed_neighbor_rechecks(flow_state, pos);
        }
    }

    let changed_positions_snapshot: Vec<IVec3> = changed_positions.iter().copied().collect();
    for pos in changed_positions_snapshot {
        if environment_matches(BlockEnvironment::Water, pos, chunk_map, registry, fluids) {
            continue;
        }

        let stacked = get_stacked_block_world(chunk_map, pos);
        if stacked != 0 && prop_requires_water_environment(stacked, registry) {
            if remove_hit_block_occupant(chunk_map, pos, stacked, true) {
                changed_positions.insert(pos);
                if let Some(key) = water_subchunk_key_for_world_pos(pos) {
                    changed_subchunks.insert(key);
                }
                enqueue_changed_neighbor_rechecks(flow_state, pos);
                spawn_prop_drop_due_water_loss(
                    commands,
                    meshes,
                    registry,
                    item_registry,
                    multiplayer_connection,
                    stacked,
                    pos,
                    now,
                );
            }
        }

        let primary = get_block_world(chunk_map, pos);
        if primary != 0 && prop_requires_water_environment(primary, registry) {
            if remove_hit_block_occupant(chunk_map, pos, primary, false) {
                changed_positions.insert(pos);
                if let Some(key) = water_subchunk_key_for_world_pos(pos) {
                    changed_subchunks.insert(key);
                }
                enqueue_changed_neighbor_rechecks(flow_state, pos);
                spawn_prop_drop_due_water_loss(
                    commands,
                    meshes,
                    registry,
                    item_registry,
                    multiplayer_connection,
                    primary,
                    pos,
                    now,
                );
            }
        }
    }

    for pos in changed_positions {
        collect_dirty_subchunks_for_block_and_neighbors(pos, chunk_map, dirty_subchunks);
    }

    for key in touched_subchunks {
        let pending = flow_state.pending_per_subchunk.get(&key).copied().unwrap_or(0);
        if changed_subchunks.contains(&key) || pending > 0 {
            flow_state.sleeping_subchunks.remove(&key);
        } else {
            flow_state.sleeping_subchunks.insert(key);
        }
    }
}

#[inline]
fn enqueue_water_frontier_pos(flow_state: &mut WaterFlowState, pos: IVec3) {
    if pos.y < Y_MIN || pos.y > Y_MAX {
        return;
    }
    if !flow_state.queued.insert(pos) {
        return;
    }
    flow_state.frontier.push_back(pos);
    if let Some(key) = water_subchunk_key_for_world_pos(pos) {
        *flow_state.pending_per_subchunk.entry(key).or_insert(0) += 1;
        flow_state.sleeping_subchunks.remove(&key);
    }
}

fn dequeue_water_frontier_pos(flow_state: &mut WaterFlowState) -> Option<IVec3> {
    while let Some(pos) = flow_state.frontier.pop_front() {
        if !flow_state.queued.remove(&pos) {
            continue;
        }
        if let Some(key) = water_subchunk_key_for_world_pos(pos)
            && let Some(pending) = flow_state.pending_per_subchunk.get_mut(&key)
        {
            *pending = pending.saturating_sub(1);
            if *pending == 0 {
                flow_state.pending_per_subchunk.remove(&key);
            }
        }
        return Some(pos);
    }
    None
}

#[inline]
fn water_subchunk_key_for_world_pos(world_pos: IVec3) -> Option<(IVec2, usize)> {
    if world_pos.y < Y_MIN || world_pos.y > Y_MAX {
        return None;
    }
    let (chunk_coord, _) = world_to_chunk_xz(world_pos.x, world_pos.z);
    let ly = world_y_to_local(world_pos.y);
    Some((chunk_coord, ly / SEC_H))
}

#[inline]
fn enqueue_changed_neighbor_rechecks(flow_state: &mut WaterFlowState, center: IVec3) {
    enqueue_water_frontier_pos(flow_state, center + IVec3::X);
    enqueue_water_frontier_pos(flow_state, center + IVec3::NEG_X);
    enqueue_water_frontier_pos(flow_state, center + IVec3::Y);
    enqueue_water_frontier_pos(flow_state, center + IVec3::NEG_Y);
    enqueue_water_frontier_pos(flow_state, center + IVec3::Z);
    enqueue_water_frontier_pos(flow_state, center + IVec3::NEG_Z);
    enqueue_water_frontier_pos(flow_state, center + IVec3::new(1, 0, 1));
    enqueue_water_frontier_pos(flow_state, center + IVec3::new(1, 0, -1));
    enqueue_water_frontier_pos(flow_state, center + IVec3::new(-1, 0, 1));
    enqueue_water_frontier_pos(flow_state, center + IVec3::new(-1, 0, -1));
}

#[inline]
fn desired_water_level_two_phase(
    pos: IVec3,
    chunk_map: &ChunkMap,
    registry: &BlockRegistry,
    flow_ids: &WaterFlowIds,
    flow_state: &WaterFlowState,
) -> u8 {
    if !water_can_flow_into(pos, chunk_map, registry, flow_ids) {
        return 0;
    }

    // Phase 1: vertical propagation first.
    let above_level = water_level_at_world(pos + IVec3::Y, chunk_map, flow_ids);
    if above_level > 0 {
        return WATER_FALL_VERTICAL_LEVEL
            .max(above_level.saturating_sub(1))
            .max(1)
            .clamp(1, WATER_FLOW_SOURCE_LEVEL);
    }

    // Phase 2: horizontal spread by gradient attenuation.
    // Diagonals consume two flow levels.
    let mut target_level = 0u8;
    for (off, attenuation) in water_horizontal_spread_offsets() {
        let neigh_pos = pos + off;
        let incoming_dir = -off;
        if !water_can_move_horizontally(
            neigh_pos,
            incoming_dir,
            chunk_map,
            registry,
            flow_ids,
        ) {
            continue;
        }
        let neigh_level = water_level_at_world(neigh_pos, chunk_map, flow_ids);
        let neigh_is_source = water_is_source_like(neigh_pos, chunk_map, flow_ids, flow_state);
        let neigh_supported_from_below =
            water_has_horizontal_spread_support(neigh_pos, chunk_map, registry);
        if !neigh_is_source && !neigh_supported_from_below {
            continue;
        }
        if !water_prefers_towards_next_drop(
            neigh_pos,
            incoming_dir,
            chunk_map,
            registry,
            flow_ids,
        ) {
            continue;
        }
        if neigh_level > attenuation {
            target_level = target_level.max(neigh_level - attenuation);
        }
    }
    if !flow_state.sources.is_empty()
        && water_should_promote_to_source(pos, chunk_map, flow_state, registry)
    {
        target_level = WATER_FLOW_SOURCE_LEVEL;
    }
    target_level.clamp(0, WATER_FLOW_SOURCE_LEVEL)
}

#[inline]
fn water_horizontal_spread_offsets() -> [(IVec3, u8); 8] {
    [
        (IVec3::X, 1),
        (IVec3::NEG_X, 1),
        (IVec3::Z, 1),
        (IVec3::NEG_Z, 1),
        (IVec3::new(1, 0, 1), 2),
        (IVec3::new(1, 0, -1), 2),
        (IVec3::new(-1, 0, 1), 2),
        (IVec3::new(-1, 0, -1), 2),
    ]
}

const WATER_DOWNFALL_LOOKAHEAD_RADIUS: i32 = 2;

#[inline]
fn water_prefers_towards_next_drop(
    from: IVec3,
    towards: IVec3,
    chunk_map: &ChunkMap,
    registry: &BlockRegistry,
    flow_ids: &WaterFlowIds,
) -> bool {
    let mut best_drop_cost: Option<u8> = None;
    let mut requested_drop_cost: Option<u8> = None;
    for (dir, step_cost) in water_horizontal_spread_offsets() {
        let Some(drop_cost) = water_first_drop_cost_in_direction(
            from, dir, step_cost, chunk_map, registry, flow_ids,
        ) else {
            continue;
        };
        if best_drop_cost.is_none_or(|best| drop_cost < best) {
            best_drop_cost = Some(drop_cost);
        }
        if dir == towards {
            requested_drop_cost = Some(drop_cost);
        }
    }

    match (best_drop_cost, requested_drop_cost) {
        (Some(best), Some(candidate)) => candidate == best,
        (Some(_), None) => false,
        // No reachable drop direction: keep legacy spread behavior.
        (None, _) => true,
    }
}

#[inline]
fn water_first_drop_cost_in_direction(
    from: IVec3,
    dir: IVec3,
    step_cost: u8,
    chunk_map: &ChunkMap,
    registry: &BlockRegistry,
    flow_ids: &WaterFlowIds,
) -> Option<u8> {
    if step_cost == 0 {
        return None;
    }

    let mut cursor = from;
    for step in 1..=WATER_DOWNFALL_LOOKAHEAD_RADIUS {
        if !water_can_move_horizontally(cursor, dir, chunk_map, registry, flow_ids) {
            return None;
        }
        cursor += dir;
        if water_can_flow_into(cursor + IVec3::NEG_Y, chunk_map, registry, flow_ids) {
            return Some((step as u8).saturating_mul(step_cost));
        }
    }
    None
}

#[inline]
fn water_can_move_horizontally(
    from: IVec3,
    dir: IVec3,
    chunk_map: &ChunkMap,
    registry: &BlockRegistry,
    flow_ids: &WaterFlowIds,
) -> bool {
    if dir.x == 0 && dir.z == 0 {
        return false;
    }
    if dir.y != 0 {
        return false;
    }

    let to = from + dir;
    if !water_can_flow_into(to, chunk_map, registry, flow_ids) {
        return false;
    }

    // No diagonal corner-cutting through fully blocked edges.
    if dir.x != 0 && dir.z != 0 {
        let side_x = from + IVec3::new(dir.x.signum(), 0, 0);
        let side_z = from + IVec3::new(0, 0, dir.z.signum());
        let side_x_open = water_can_flow_into(side_x, chunk_map, registry, flow_ids);
        let side_z_open = water_can_flow_into(side_z, chunk_map, registry, flow_ids);
        if !side_x_open && !side_z_open {
            return false;
        }
    }

    true
}

fn collect_dirty_subchunks_for_block_and_neighbors(
    center: IVec3,
    chunk_map: &ChunkMap,
    dirty_subchunks: &mut HashSet<(IVec2, usize)>,
) {
    for off in [
        IVec3::ZERO,
        IVec3::X,
        IVec3::NEG_X,
        IVec3::Y,
        IVec3::NEG_Y,
        IVec3::Z,
        IVec3::NEG_Z,
    ] {
        let p = center + off;
        if p.y < Y_MIN || p.y > Y_MAX {
            continue;
        }

        let (coord, _) = world_to_chunk_xz(p.x, p.z);
        if !chunk_map.chunks.contains_key(&coord) {
            continue;
        }
        let ly = world_y_to_local(p.y);
        let sub = ly / SEC_H;
        if sub < SEC_COUNT {
            dirty_subchunks.insert((coord, sub));
        }
        if ly % SEC_H == 0 && sub > 0 {
            dirty_subchunks.insert((coord, sub - 1));
        }
        if ly % SEC_H == SEC_H - 1 && sub + 1 < SEC_COUNT {
            dirty_subchunks.insert((coord, sub + 1));
        }
    }
}

fn apply_batched_dirty_subchunks(
    chunk_map: &mut ChunkMap,
    ev_dirty: &mut MessageWriter<SubChunkNeedRemeshEvent>,
    dirty_subchunks: &HashSet<(IVec2, usize)>,
) {
    for (coord, sub) in dirty_subchunks {
        if *sub >= SEC_COUNT {
            continue;
        }
        let Some(chunk) = chunk_map.chunks.get_mut(coord) else {
            continue;
        };
        chunk.mark_dirty_local_y(*sub);
        ev_dirty.write(SubChunkNeedRemeshEvent {
            coord: *coord,
            sub: *sub,
        });
    }
}

#[inline]
fn prop_requires_water_environment(block_id: BlockId, registry: &BlockRegistry) -> bool {
    registry.is_prop(block_id)
        && registry
            .allowed_environments(block_id)
            .contains(&BlockEnvironment::Water)
}

#[inline]
fn spawn_prop_drop_due_water_loss(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    registry: &BlockRegistry,
    item_registry: &ItemRegistry,
    multiplayer_connection: &MultiplayerConnectionState,
    block_id: BlockId,
    world_pos: IVec3,
    now: f32,
) {
    if multiplayer_connection.connected {
        return;
    }
    let drop_item_id = item_registry.item_for_block(block_id).unwrap_or(0);
    if drop_item_id == 0 {
        return;
    }
    spawn_world_item_with_motion(
        commands,
        meshes,
        registry,
        item_registry,
        drop_item_id,
        1,
        world_pos.as_vec3() + Vec3::new(0.5, 0.35, 0.5),
        Vec3::ZERO,
        world_pos,
        now,
    );
}

#[inline]
fn water_level_at_world(pos: IVec3, chunk_map: &ChunkMap, flow_ids: &WaterFlowIds) -> u8 {
    flow_ids.level_for_id(get_block_world(chunk_map, pos))
}

#[inline]
fn water_is_source_like(
    pos: IVec3,
    chunk_map: &ChunkMap,
    flow_ids: &WaterFlowIds,
    flow_state: &WaterFlowState,
) -> bool {
    if flow_state.sources.contains(&pos) {
        return true;
    }
    let id = get_block_world(chunk_map, pos);
    id != 0 && id == flow_ids.settled_id
}

#[inline]
fn water_can_flow_into(
    pos: IVec3,
    chunk_map: &ChunkMap,
    registry: &BlockRegistry,
    flow_ids: &WaterFlowIds,
) -> bool {
    if pos.y < Y_MIN || pos.y > Y_MAX {
        return false;
    }
    let (chunk_coord, _) = world_to_chunk_xz(pos.x, pos.z);
    if !chunk_map.chunks.contains_key(&chunk_coord) {
        return false;
    }

    let id = get_block_world(chunk_map, pos);
    if id == 0 || flow_ids.contains(id) || registry.is_overridable(id) {
        let stacked = get_stacked_block_world(chunk_map, pos);
        return stacked == 0 || flow_ids.contains(stacked) || registry.is_overridable(stacked);
    }
    if !registry.is_fluid(id) && registry.is_water_logged(id) {
        // Waterlogged solids are only filled by explicit player placement of water,
        // not by flowing-water simulation.
        return false;
    }
    false
}

#[inline]
fn water_has_horizontal_spread_support(
    pos: IVec3,
    chunk_map: &ChunkMap,
    registry: &BlockRegistry,
) -> bool {
    if pos.y <= Y_MIN {
        return true;
    }
    let below_pos = pos + IVec3::NEG_Y;
    let below = get_block_world(chunk_map, below_pos);
    if below != 0 && !registry.is_fluid(below) && registry.stats(below).solid {
        return true;
    }
    let below_stacked = get_stacked_block_world(chunk_map, below_pos);
    below_stacked != 0 && !registry.is_fluid(below_stacked) && registry.stats(below_stacked).solid
}

#[inline]
fn water_should_promote_to_source(
    pos: IVec3,
    chunk_map: &ChunkMap,
    flow_state: &WaterFlowState,
    registry: &BlockRegistry,
) -> bool {
    if !water_has_horizontal_spread_support(pos, chunk_map, registry) {
        return false;
    }
    let mut source_neighbors = 0u8;
    for off in [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z] {
        if flow_state.sources.contains(&(pos + off)) {
            source_neighbors += 1;
        }
    }
    source_neighbors >= 2
}

#[inline]
fn set_fluid_bit_for_world_pos(fluids: &mut FluidMap, world_pos: IVec3, on: bool) {
    if world_pos.y < Y_MIN || world_pos.y > Y_MAX {
        return;
    }
    let (chunk_coord, local) = world_to_chunk_xz(world_pos.x, world_pos.z);
    let lx = local.x as usize;
    let lz = local.y as usize;
    let ly = world_y_to_local(world_pos.y);
    let fluid_chunk = fluids
        .0
        .entry(chunk_coord)
        .or_insert_with(|| FluidChunk::new(SEA_LEVEL));
    fluid_chunk.set(lx, ly, lz, on);
}

#[inline]
fn block_allows_environment_at(
    block_id: BlockId,
    world_pos: IVec3,
    chunk_map: &ChunkMap,
    registry: &BlockRegistry,
    fluids: &FluidMap,
) -> bool {
    let allowed = registry.allowed_environments(block_id);
    if allowed.is_empty() {
        return true;
    }
    allowed
        .iter()
        .any(|env| environment_matches(*env, world_pos, chunk_map, registry, fluids))
}

#[inline]
fn environment_matches(
    env: BlockEnvironment,
    world_pos: IVec3,
    chunk_map: &ChunkMap,
    registry: &BlockRegistry,
    fluids: &FluidMap,
) -> bool {
    match env {
        BlockEnvironment::Water => {
            let primary = get_block_world(chunk_map, world_pos);
            (primary != 0 && registry.is_fluid(primary))
                || fluid_at_world(fluids, world_pos.x, world_pos.y, world_pos.z)
        }
        BlockEnvironment::Overworld => world_pos.y > 50,
        BlockEnvironment::Cave => world_pos.y <= 10,
    }
}
