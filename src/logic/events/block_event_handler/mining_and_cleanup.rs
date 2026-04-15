/// Systems related to cleanup and mining/break interactions.
fn enforce_block_texture_nearest_sampler_system(
    mut image_events: MessageReader<AssetEvent<Image>>,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
) {
    for event in image_events.read() {
        let image_id = match event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id } => *id,
            AssetEvent::Removed { .. } | AssetEvent::Unused { .. } => continue,
        };
        let Some(path) = asset_server.get_path(image_id) else {
            continue;
        };
        let asset_path = path.path().to_string_lossy();
        if !asset_path.starts_with("textures/blocks/") {
            continue;
        }
        apply_nearest_sampler_to_image(images.as_mut(), image_id, true);
    }
}

fn cleanup_structure_runtime_on_exit(
    mut commands: Commands,
    mut runtime: ResMut<StructureRuntimeState>,
    mut structure_mining: ResMut<StructureMiningState>,
    mut emitter_state: ResMut<MiningDebrisEmitterState>,
) {
    for (_, entity) in runtime.spawned_entities.drain() {
        safe_despawn_entity(&mut commands, entity);
    }
    runtime.entity_to_key.clear();
    runtime.records_by_chunk.clear();
    runtime.loaded_chunks.clear();
    structure_mining.target = None;
    emitter_state.active_target = None;
    emitter_state.next_emit_at = 0.0;
}

fn ensure_mining_debris_fx_assets(
    mut fx_assets: ResMut<MiningDebrisFxAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if fx_assets.initialized {
        return;
    }

    fx_assets.cube = meshes.add(Mesh::from(Cuboid::new(1.0, 1.0, 1.0)));
    fx_assets.initialized = true;
}

fn sanitize_mining_debris_lifetime_secs(seconds: f32) -> f32 {
    if seconds.is_finite() {
        seconds.max(MINING_DEBRIS_MIN_LIFETIME_SECS)
    } else {
        MINING_DEBRIS_FALLBACK_LIFETIME_SECS
    }
}

fn mining_debris_half_extent(transform: &Transform) -> f32 {
    transform.scale.max_element().max(0.01) * 0.5
}

fn mining_debris_ground_top(
    transform: &Transform,
    half_extent: f32,
    registry: &BlockRegistry,
    chunk_map: &ChunkMap,
) -> Option<f32> {
    let foot = transform.translation - Vec3::Y * (half_extent + 0.03);
    let wx = foot.x.floor() as i32;
    let wy = foot.y.floor() as i32;
    let wz = foot.z.floor() as i32;
    let below = get_block_world(chunk_map, IVec3::new(wx, wy, wz));
    let below_is_support = below != 0 && !registry.is_fluid(below) && registry.stats(below).solid;
    if below_is_support {
        Some(wy as f32 + 1.0)
    } else {
        None
    }
}

fn update_mining_debris_fx(
    mut commands: Commands,
    time: Res<Time>,
    registry: Res<BlockRegistry>,
    chunk_map: Res<ChunkMap>,
    mut debris_q: Query<
        (
            Entity,
            &mut Transform,
            &mut MiningDebrisLifetime,
            Option<&mut MiningDebrisMotion>,
            Option<&MiningRubblePiece>,
        ),
        With<MiningDebrisVisual>,
    >,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    for (entity, mut transform, mut lifetime, maybe_motion, maybe_rubble) in &mut debris_q {
        lifetime.age += dt;
        if lifetime.age >= lifetime.lifetime {
            safe_despawn_entity(&mut commands, entity);
            continue;
        }

        if let Some(mut motion) = maybe_motion {
            let half = mining_debris_half_extent(&transform);
            if motion.resting {
                motion.velocity = Vec3::ZERO;
                motion.angular_velocity = Vec3::ZERO;
                if let Some(ground_top) =
                    mining_debris_ground_top(&transform, half, &registry, &chunk_map)
                {
                    if transform.translation.y - half <= ground_top + 0.001 {
                        transform.translation.y = ground_top + half;
                        continue;
                    }
                }
                motion.resting = false;
            }

            motion.velocity.y -= 6.8 * dt;
            let damping = if maybe_rubble.is_some() { 4.2 } else { 2.1 };
            motion.velocity *= 1.0 - (damping * dt).clamp(0.0, 0.92);
            transform.translation += motion.velocity * dt;

            if let Some(rubble) = maybe_rubble {
                let delta = transform.translation - rubble.origin;
                let horiz = Vec2::new(delta.x, delta.z);
                let dist = horiz.length();
                if dist > rubble.max_radius && dist > f32::EPSILON {
                    let clamped = horiz / dist * rubble.max_radius;
                    transform.translation.x = rubble.origin.x + clamped.x;
                    transform.translation.z = rubble.origin.z + clamped.y;
                    motion.velocity.x *= 0.25;
                    motion.velocity.z *= 0.25;
                }
            }

            if let Some(ground_top) =
                mining_debris_ground_top(&transform, half, &registry, &chunk_map)
            {
                if motion.velocity.y <= 0.0 && transform.translation.y - half <= ground_top {
                    transform.translation.y = ground_top + half;
                    motion.velocity = Vec3::ZERO;
                    motion.angular_velocity = Vec3::ZERO;
                    motion.resting = true;
                    continue;
                }
            }

            if motion.angular_velocity.length_squared() > 0.000_001 {
                let ang = motion.angular_velocity * dt;
                let spin = Quat::from_euler(EulerRot::XYZ, ang.x, ang.y, ang.z);
                transform.rotation = spin * transform.rotation;
            }
        }
    }
}

fn cleanup_mining_debris_fx(
    mut commands: Commands,
    q_debris: Query<Entity, With<MiningDebrisVisual>>,
    mut emitter_state: ResMut<MiningDebrisEmitterState>,
) {
    for entity in &q_debris {
        safe_despawn_entity(&mut commands, entity);
    }
    emitter_state.active_target = None;
    emitter_state.next_emit_at = 0.0;
}

/// Handles left-click mining and block breaking, including drops and FX.
fn block_break_handler(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    time: Res<Time>,
    fx_assets: Res<MiningDebrisFxAssets>,
    mut emitter_state: ResMut<MiningDebrisEmitterState>,
    buttons: Res<ButtonInput<MouseButton>>,
    selection: Res<SelectionState>,
    registry: Res<BlockRegistry>,
    item_registry: Res<ItemRegistry>,
    game_mode: Res<GameModeState>,
    inventory: Res<PlayerInventory>,
    hotbar_selection: Option<Res<HotbarSelectionState>>,
    structure_cancel: ActiveStructureCancelState,
    structure_break_deps: StructureBreakDeps,
    break_world: BreakWorldMut,
    break_input_context: BreakInputContext,
) {
    let BreakInputContext {
        multiplayer_connection,
        ui_state,
        global_config,
    } = break_input_context;
    let debris_lifetime_secs = sanitize_mining_debris_lifetime_secs(
        global_config.interface.mining_debris_lifetime_seconds,
    );
    let ActiveStructureCancelState {
        mut active_structure_recipe,
        mut active_structure_placement,
    } = structure_cancel;
    let StructureBreakDeps {
        mut structure_runtime,
        mut structure_mining,
        q_structure_meta,
        ws,
        mut region_cache,
    } = structure_break_deps;
    let BreakWorldMut {
        mut state,
        mut chunk_map,
        mut ev_dirty,
        mut break_ev,
    } = break_world;

    if ui_state
        .as_ref()
        .is_some_and(|state| state.blocks_game_input())
    {
        state.target = None;
        structure_mining.target = None;
        emitter_state.active_target = None;
        return;
    }

    let held_item_id = selected_hotbar_item_id(&inventory, hotbar_selection.as_deref());
    let holding_hammer = held_item_id
        .and_then(|item_id| item_registry.def_opt(item_id))
        .is_some_and(|item| item.localized_name == "oplexa:hammer" || item.key == "hammer");
    if holding_hammer && active_structure_recipe.selected_recipe_name.is_some() {
        if buttons.just_pressed(MouseButton::Left) {
            active_structure_recipe.selected_recipe_name = None;
            active_structure_placement.rotation_quarters = 0;
        }
        state.target = None;
        structure_mining.target = None;
        emitter_state.active_target = None;
        return;
    }

    let multiplayer_connected = multiplayer_connection
        .as_ref()
        .is_some_and(|state| state.connected);

    if game_mode.0.eq(&GameMode::Spectator) {
        structure_mining.target = None;
        emitter_state.active_target = None;
        return;
    }
    if !buttons.pressed(MouseButton::Left) {
        state.target = None;
        structure_mining.target = None;
        emitter_state.active_target = None;
        return;
    }

    if let Some(structure_hit) = selection.structure_hit {
        state.target = None;
        emitter_state.active_target = None;
        handle_structure_break(
            &mut commands,
            &mut meshes,
            &time,
            &buttons,
            &game_mode,
            &registry,
            &item_registry,
            &inventory,
            hotbar_selection.as_deref(),
            structure_hit.entity,
            &q_structure_meta,
            &mut structure_runtime,
            &mut structure_mining,
            multiplayer_connection.as_deref(),
            ws.as_deref(),
            region_cache.as_deref_mut(),
        );
        return;
    }
    structure_mining.target = None;

    let Some(hit) = selection.hit else {
        state.target = None;
        emitter_state.active_target = None;
        return;
    };

    let id_now = hit.block_id;
    if id_now == 0 {
        state.target = None;
        emitter_state.active_target = None;
        return;
    }

    let creative_mode = matches!(game_mode.0, GameMode::Creative);
    let prop_block = registry.is_prop(id_now);
    let now = time.elapsed_secs();
    let held_tool = selected_hotbar_tool(&inventory, hotbar_selection.as_deref(), &item_registry);
    let requirement = block_requirement_for_id(id_now, &registry);

    if creative_mode {
        if !buttons.just_pressed(MouseButton::Left) {
            return;
        }
        state.target = None;
        emitter_state.active_target = None;
    } else if prop_block {
        // Props (e.g. tall grass) break instantly in survival.
        state.target = None;
        emitter_state.active_target = None;
    } else {
        let duration = (break_time_for(id_now, &registry)
            / mining_speed_multiplier(requirement, held_tool))
        .max(0.05);
        let target_matches = state
            .target
            .is_some_and(|target| target.loc == hit.block_pos && target.id == id_now);

        if !target_matches {
            state.target = Some(MiningTarget {
                loc: hit.block_pos,
                id: id_now,
                started_at: now,
                duration,
            });
            emitter_state.active_target = Some((hit.block_pos, id_now));
            emitter_state.next_emit_at = now;
            return;
        }

        if let Some(target) = state.target {
            if mining_progress(now, &target) < 1.0 {
                spawn_mining_hit_particles(
                    &mut commands,
                    fx_assets.as_ref(),
                    &registry,
                    hit,
                    id_now,
                    now,
                    &mut emitter_state,
                    debris_lifetime_secs,
                );
                return;
            }
        } else {
            emitter_state.active_target = None;
            return;
        }
    }

    let world_loc = hit.block_pos;
    if !remove_hit_block_occupant(&mut chunk_map, world_loc, id_now, hit.is_stacked) {
        state.target = None;
        emitter_state.active_target = None;
        return;
    }
    mark_dirty_block_and_neighbors(&mut chunk_map, world_loc, &mut ev_dirty);

    let (chunk_coord, l) = world_to_chunk_xz(world_loc.x, world_loc.z);
    let lx = l.x as u8;
    let lz = l.y as u8;
    let ly = (world_loc.y - Y_MIN).clamp(0, CY as i32 - 1) as usize;
    let (drop_item_id, drops_item) = if creative_mode {
        (0, false)
    } else {
        let can_drop = can_drop_from_block(requirement, held_tool);
        let drop_item_id = if can_drop {
            item_registry.item_for_block(id_now).unwrap_or(0)
        } else {
            0
        };
        let drops_item = !registry.is_fluid(id_now) && drop_item_id != 0;
        (drop_item_id, drops_item)
    };

    break_ev.write(BlockBreakByPlayerEvent {
        chunk_coord,
        location: world_loc,
        chunk_x: lx,
        chunk_y: ly as u16,
        chunk_z: lz,
        block_id: id_now,
        drop_item_id,
        block_name: registry.name_opt(id_now).unwrap_or("").to_string(),
        drops_item,
    });

    remove_unsupported_props_above(
        &mut chunk_map,
        &registry,
        world_loc,
        &mut ev_dirty,
        &mut break_ev,
    );

    if !multiplayer_connected && drops_item {
        spawn_world_item_for_block_break(
            &mut commands,
            &mut meshes,
            &registry,
            &chunk_map,
            &item_registry,
            id_now,
            world_loc,
            now,
        );
    }

    if !creative_mode {
        spawn_mining_rubble_pile(
            &mut commands,
            fx_assets.as_ref(),
            &registry,
            id_now,
            world_loc,
            debris_lifetime_secs,
        );
    }

    state.target = None;
    emitter_state.active_target = None;
}

fn spawn_mining_hit_particles(
    commands: &mut Commands,
    fx_assets: &MiningDebrisFxAssets,
    registry: &BlockRegistry,
    hit: crate::core::entities::player::block_selection::BlockHit,
    block_id: BlockId,
    now: f32,
    emitter_state: &mut MiningDebrisEmitterState,
    debris_lifetime_secs: f32,
) {
    if !fx_assets.initialized
        || block_id == 0
        || registry.is_air(block_id)
        || registry.is_fluid(block_id)
    {
        return;
    }

    let target_key = (hit.block_pos, block_id);
    if emitter_state.active_target != Some(target_key) {
        emitter_state.active_target = Some(target_key);
        emitter_state.next_emit_at = now;
    }
    if now < emitter_state.next_emit_at {
        return;
    }
    emitter_state.next_emit_at = now + MINING_PARTICLE_INTERVAL_SECS;

    let spawn_count = if rand_f32() < 0.45 { 2 } else { 1 };
    let s = VOXEL_SIZE;
    let face_normal = face_offset(hit.face).as_vec3().normalize_or_zero();
    let hit_world = Vec3::new(
        (hit.block_pos.x as f32 + hit.hit_local.x) * s,
        (hit.block_pos.y as f32 + hit.hit_local.y) * s,
        (hit.block_pos.z as f32 + hit.hit_local.z) * s,
    ) + face_normal * 0.03;
    let material = registry.material(block_id);

    for _ in 0..spawn_count {
        let tangent = random_unit_vector3();
        let jitter = Vec3::new(
            rand_range(-0.045, 0.045),
            rand_range(-0.045, 0.045),
            rand_range(-0.045, 0.045),
        );
        let velocity = face_normal * rand_range(0.28, 0.72)
            + tangent * rand_range(0.08, 0.42)
            + Vec3::Y * rand_range(0.05, 0.34);
        let size = rand_range(0.028, 0.058) * s;
        let lifetime = debris_lifetime_secs;

        commands.spawn((
            MiningDebrisVisual,
            MiningDebrisLifetime { age: 0.0, lifetime },
            MiningDebrisMotion {
                velocity,
                angular_velocity: random_unit_vector3() * rand_range(5.0, 14.0),
                resting: false,
            },
            Mesh3d(fx_assets.cube.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(hit_world + jitter).with_scale(Vec3::splat(size)),
            Visibility::default(),
            NotShadowCaster,
            NotShadowReceiver,
            Name::new("MiningHitParticle"),
        ));
    }
}

fn spawn_mining_rubble_pile(
    commands: &mut Commands,
    fx_assets: &MiningDebrisFxAssets,
    registry: &BlockRegistry,
    block_id: BlockId,
    world_loc: IVec3,
    debris_lifetime_secs: f32,
) {
    if !fx_assets.initialized
        || block_id == 0
        || registry.is_air(block_id)
        || registry.is_fluid(block_id)
    {
        return;
    }

    let s = VOXEL_SIZE;
    let center = Vec3::new(
        (world_loc.x as f32 + 0.5) * s,
        (world_loc.y as f32 + 0.03) * s,
        (world_loc.z as f32 + 0.5) * s,
    );
    let material = registry.material(block_id);
    let piece_count = rand_i32(10, 18);

    for _ in 0..piece_count {
        let angle = rand_range(0.0, std::f32::consts::TAU);
        let radius = rand_range(0.04, MINING_RUBBLE_RADIUS_MAX_METERS * 0.65);
        let offset = Vec3::new(
            angle.cos() * radius,
            rand_range(0.0, 0.12),
            angle.sin() * radius,
        );
        let size = rand_range(0.045, 0.13) * s;
        let lifetime = debris_lifetime_secs;
        let initial_velocity = Vec3::new(
            rand_range(-0.18, 0.18),
            rand_range(0.05, 0.42),
            rand_range(-0.18, 0.18),
        );
        let spawn_pos = center + offset;

        commands.spawn((
            MiningDebrisVisual,
            MiningDebrisLifetime { age: 0.0, lifetime },
            MiningDebrisMotion {
                velocity: initial_velocity,
                angular_velocity: random_unit_vector3() * rand_range(1.5, 7.5),
                resting: false,
            },
            MiningRubblePiece {
                origin: center,
                max_radius: MINING_RUBBLE_RADIUS_MAX_METERS,
            },
            Mesh3d(fx_assets.cube.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(spawn_pos).with_scale(Vec3::splat(size)),
            Visibility::default(),
            NotShadowCaster,
            NotShadowReceiver,
            Name::new("MiningRubblePiece"),
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_structure_break(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    time: &Time,
    buttons: &ButtonInput<MouseButton>,
    game_mode: &GameModeState,
    registry: &BlockRegistry,
    item_registry: &ItemRegistry,
    inventory: &PlayerInventory,
    hotbar_selection: Option<&HotbarSelectionState>,
    structure_entity: Entity,
    q_structure_meta: &Query<&PlacedStructureMetadata>,
    runtime: &mut StructureRuntimeState,
    structure_mining: &mut StructureMiningState,
    multiplayer_connection: Option<&MultiplayerConnectionState>,
    ws: Option<&WorldSave>,
    region_cache: Option<&mut RegionCache>,
) {
    let Ok(meta) = q_structure_meta.get(structure_entity) else {
        structure_mining.target = None;
        return;
    };

    let held_tool = selected_hotbar_tool(inventory, hotbar_selection, item_registry);
    let duration = ((BASE_BREAK_TIME + meta.stats.hardness.max(0.0) * PER_HARDNESS)
        / mining_speed_multiplier(None, held_tool))
    .clamp(MIN_BREAK_TIME, MAX_BREAK_TIME);
    let now = time.elapsed_secs();

    let creative_mode = matches!(game_mode.0, GameMode::Creative);
    if !creative_mode {
        let target_matches = structure_mining
            .target
            .is_some_and(|target| target.entity == structure_entity);
        if !target_matches {
            structure_mining.target = Some(StructureMiningTarget {
                entity: structure_entity,
                started_at: now,
                duration,
            });
            return;
        }
        if let Some(target) = structure_mining.target {
            if mining_progress(
                now,
                &MiningTarget {
                    loc: meta.place_origin,
                    id: 0,
                    started_at: target.started_at,
                    duration: target.duration,
                },
            ) < 1.0
            {
                return;
            }
        } else {
            return;
        }
    } else if !buttons.just_pressed(MouseButton::Left) {
        return;
    }

    structure_mining.target = None;
    safe_despawn_entity(commands, structure_entity);

    let uses_local_save_data = multiplayer_connection
        .map(MultiplayerConnectionState::uses_local_save_data)
        .unwrap_or(true);
    remove_structure_from_runtime(
        runtime,
        structure_entity,
        uses_local_save_data,
        ws,
        region_cache,
    );

    let multiplayer_connected = multiplayer_connection.is_some_and(|state| state.connected);
    if creative_mode || multiplayer_connected {
        return;
    }
    for requirement in &meta.drop_requirements {
        let (item_id, count) = match &requirement.source {
            BuildingMaterialRequirementSource::Item { item_id, .. } => {
                (*item_id, requirement.count)
            }
            BuildingMaterialRequirementSource::Group { .. } => continue,
        };
        if item_id == 0 || count == 0 {
            continue;
        }
        spawn_world_item_with_motion(
            commands,
            meshes,
            registry,
            item_registry,
            item_id,
            count,
            meta.selection_center_world + Vec3::Y * 0.15,
            Vec3::ZERO,
            meta.place_origin,
            now,
        );
    }
}

fn remove_structure_from_runtime(
    runtime: &mut StructureRuntimeState,
    structure_entity: Entity,
    uses_local_save_data: bool,
    ws: Option<&WorldSave>,
    mut region_cache: Option<&mut RegionCache>,
) {
    let Some(key) = runtime.entity_to_key.remove(&structure_entity) else {
        return;
    };
    runtime.spawned_entities.remove(&key);

    let Some(entries) = runtime.records_by_chunk.get_mut(&key.origin_chunk) else {
        return;
    };
    entries.retain(|entry| {
        !(entry.recipe_name == key.recipe_name
            && entry.place_origin == [key.place_origin.x, key.place_origin.y, key.place_origin.z]
            && normalize_rotation_quarters(entry.rotation_quarters as i32) == key.rotation_quarters
            && normalize_rotation_steps(
                entry
                    .rotation_steps
                    .map_or((entry.rotation_quarters as i32) * 2, i32::from),
            ) == key.rotation_steps)
    });

    if !uses_local_save_data {
        return;
    }
    let (Some(ws), Some(cache)) = (ws, region_cache.as_deref_mut()) else {
        return;
    };
    let _ = persist_structure_records_for_chunk(ws, cache, key.origin_chunk, entries);
}

fn remove_unsupported_props_above(
    chunk_map: &mut ChunkMap,
    registry: &BlockRegistry,
    support_loc: IVec3,
    ev_dirty: &mut MessageWriter<SubChunkNeedRemeshEvent>,
    break_ev: &mut MessageWriter<BlockBreakByPlayerEvent>,
) {
    let mut world_loc = support_loc + IVec3::Y;
    while world_loc.y <= Y_MAX {
        let primary_id = get_block_world(chunk_map, world_loc);
        let stacked_id = get_stacked_block_world(chunk_map, world_loc);

        let (prop_id, remove_stacked) = if stacked_id != 0 && registry.is_prop(stacked_id) {
            (stacked_id, true)
        } else if primary_id != 0 && registry.is_prop(primary_id) {
            (primary_id, false)
        } else {
            break;
        };

        let below_id = get_block_world(chunk_map, world_loc + IVec3::NEG_Y);
        if registry.prop_allows_ground(prop_id, below_id) {
            break;
        }

        if let Some(mut access) = world_access_mut(chunk_map, world_loc) {
            if remove_stacked {
                access.set_stacked(0);
            } else {
                access.set(0);
            }
        }
        mark_dirty_block_and_neighbors(chunk_map, world_loc, ev_dirty);

        let (chunk_coord, l) = world_to_chunk_xz(world_loc.x, world_loc.z);
        let ly = (world_loc.y - Y_MIN).clamp(0, CY as i32 - 1) as usize;
        break_ev.write(BlockBreakByPlayerEvent {
            chunk_coord,
            location: world_loc,
            chunk_x: l.x as u8,
            chunk_y: ly as u16,
            chunk_z: l.y as u8,
            block_id: prop_id,
            drop_item_id: 0,
            block_name: registry.name_opt(prop_id).unwrap_or("").to_string(),
            drops_item: false,
        });

        world_loc += IVec3::Y;
    }
}
