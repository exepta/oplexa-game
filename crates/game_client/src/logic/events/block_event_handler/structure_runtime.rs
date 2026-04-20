/// Structure runtime synchronization, persistence, and multiplayer reconciliation.
fn sync_structures_for_loaded_chunks(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    item_registry: Res<ItemRegistry>,
    chunk_map: Res<ChunkMap>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
    structure_recipe_registry: Option<Res<BuildingStructureRecipeRegistry>>,
    ws: Option<Res<WorldSave>>,
    mut region_cache: Option<ResMut<RegionCache>>,
    mut runtime: ResMut<StructureRuntimeState>,
    mut reconcile_queue: ResMut<MultiplayerStructureReconcileQueue>,
) {
    let Some(structure_recipe_registry) = structure_recipe_registry.as_ref() else {
        return;
    };
    let uses_local_save_data = multiplayer_connection.uses_local_save_data();

    let mut newly_loaded = Vec::new();
    for &coord in chunk_map.chunks.keys() {
        if runtime.loaded_chunks.insert(coord) {
            newly_loaded.push(coord);
        }
    }
    for coord in newly_loaded {
        let entries = if uses_local_save_data {
            if let (Some(ws), Some(cache)) = (ws.as_deref(), region_cache.as_deref_mut()) {
                load_structure_records_for_chunk(ws, cache, coord)
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        if !uses_local_save_data {
            reconcile_queue.pending_chunks.insert(coord);
        }
        if uses_local_save_data || !entries.is_empty() {
            runtime.records_by_chunk.insert(coord, entries.clone());
        }

        for entry in entries {
            let place_origin = IVec3::new(
                entry.place_origin[0],
                entry.place_origin[1],
                entry.place_origin[2],
            );
            let rotation_steps = normalize_rotation_steps(
                entry
                    .rotation_steps
                    .map_or((entry.rotation_quarters as i32) * 2, i32::from),
            );
            let rotation_quarters = rotation_steps_to_placement_quarters(rotation_steps);
            let Some(recipe) = structure_recipe_registry.recipe_by_name(entry.recipe_name.as_str())
            else {
                continue;
            };
            let key = placed_structure_key(
                coord,
                recipe.name.clone(),
                place_origin,
                rotation_quarters,
                rotation_steps,
            );
            if runtime.spawned_entities.contains_key(&key) {
                continue;
            }
            let drop_requirements =
                resolve_structure_drop_requirements_for_entry(&entry, recipe, &item_registry);
            let style_source_item_id = resolve_structure_style_source_item_id_for_entry(
                &entry,
                drop_requirements.as_slice(),
                recipe,
                &item_registry,
            );
            let entity = spawn_structure_model_entity(
                &mut commands,
                &asset_server,
                recipe,
                place_origin,
                rotation_quarters,
                rotation_steps,
                drop_requirements,
                style_source_item_id,
            );
            runtime.spawned_entities.insert(key.clone(), entity);
            runtime.entity_to_key.insert(entity, key);
        }
    }

    let unloaded: Vec<IVec2> = runtime
        .loaded_chunks
        .iter()
        .copied()
        .filter(|coord| !chunk_map.chunks.contains_key(coord))
        .collect();
    for coord in unloaded {
        runtime.loaded_chunks.remove(&coord);
        runtime.records_by_chunk.remove(&coord);

        let keys: Vec<PlacedStructureKey> = runtime
            .spawned_entities
            .keys()
            .filter(|key| key.origin_chunk == coord)
            .cloned()
            .collect();
        for key in keys {
            if let Some(entity) = runtime.spawned_entities.remove(&key) {
                runtime.entity_to_key.remove(&entity);
                safe_despawn_entity(&mut commands, entity);
            }
        }
    }
}

fn sync_chest_inventory_contents_for_opened_ui(
    mut opened: MessageReader<ChestInventoryUiOpened>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
    runtime: Res<StructureRuntimeState>,
    item_registry: Res<ItemRegistry>,
    mut sync: MessageWriter<ChestInventoryContentsSync>,
) {
    if !multiplayer_connection.uses_local_save_data() {
        for _ in opened.read() {}
        return;
    }

    for message in opened.read() {
        let (coord, _) = world_to_chunk_xz(message.world_pos[0], message.world_pos[2]);
        let slots = runtime
            .records_by_chunk
            .get(&coord)
            .and_then(|entries| {
                entries
                    .iter()
                    .find(|entry| entry.place_origin == message.world_pos)
            })
            .map(|entry| chest_payload_slots_from_region_slots(entry, &item_registry))
            .unwrap_or_default();

        sync.write(ChestInventoryContentsSync {
            world_pos: message.world_pos,
            slots,
        });
    }
}

fn sync_chest_inventory_snapshot_requests(
    mut requests: MessageReader<ChestInventorySnapshotRequest>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
    runtime: Res<StructureRuntimeState>,
    item_registry: Res<ItemRegistry>,
    mut sync: MessageWriter<ChestInventoryContentsSync>,
) {
    if !multiplayer_connection.uses_local_save_data() {
        for _ in requests.read() {}
        return;
    }

    for message in requests.read() {
        let (coord, _) = world_to_chunk_xz(message.world_pos[0], message.world_pos[2]);
        let slots = runtime
            .records_by_chunk
            .get(&coord)
            .and_then(|entries| {
                entries
                    .iter()
                    .find(|entry| entry.place_origin == message.world_pos)
            })
            .map(|entry| chest_payload_slots_from_region_slots(entry, &item_registry))
            .unwrap_or_default();

        sync.write(ChestInventoryContentsSync {
            world_pos: message.world_pos,
            slots,
        });
    }
}

fn persist_chest_inventory_from_ui_requests(
    mut requests: MessageReader<ChestInventoryPersistRequest>,
    item_registry: Res<ItemRegistry>,
    multiplayer_connection: Option<Res<MultiplayerConnectionState>>,
    ws: Option<Res<WorldSave>>,
    mut region_cache: Option<ResMut<RegionCache>>,
    mut runtime: ResMut<StructureRuntimeState>,
) {
    let uses_local_save_data = multiplayer_connection
        .as_deref()
        .map(MultiplayerConnectionState::uses_local_save_data)
        .unwrap_or(true);
    if !uses_local_save_data {
        for _ in requests.read() {}
        return;
    }

    for request in requests.read() {
        let (coord, _) = world_to_chunk_xz(request.world_pos[0], request.world_pos[2]);
        let Some(entries) = runtime.records_by_chunk.get_mut(&coord) else {
            continue;
        };
        let Some(entry) = entries
            .iter_mut()
            .find(|entry| entry.place_origin == request.world_pos)
        else {
            continue;
        };

        entry.inventory_slots =
            chest_region_inventory_slots_from_payload(request.slots.as_slice(), &item_registry);
        let (Some(ws), Some(cache)) = (ws.as_deref(), region_cache.as_deref_mut()) else {
            continue;
        };
        let _ = persist_structure_records_for_chunk(ws, cache, coord, entries);
    }
}

fn collect_multiplayer_structure_reconcile_chunks(
    multiplayer_connection: Res<MultiplayerConnectionState>,
    mut dirty_events: MessageReader<SubChunkNeedRemeshEvent>,
    mut queue: ResMut<MultiplayerStructureReconcileQueue>,
) {
    if multiplayer_connection.uses_local_save_data() {
        for _ in dirty_events.read() {}
        queue.pending_chunks.clear();
        return;
    }

    for event in dirty_events.read() {
        queue.pending_chunks.insert(event.coord);
    }
}

fn reconcile_multiplayer_structure_visuals(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    item_registry: Res<ItemRegistry>,
    chunk_map: Res<ChunkMap>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
    structure_recipe_registry: Option<Res<BuildingStructureRecipeRegistry>>,
    mut runtime: ResMut<StructureRuntimeState>,
    mut queue: ResMut<MultiplayerStructureReconcileQueue>,
) {
    if multiplayer_connection.uses_local_save_data() {
        queue.pending_chunks.clear();
        return;
    }
    if queue.pending_chunks.is_empty() {
        return;
    }
    let Some(structure_recipe_registry) = structure_recipe_registry.as_ref() else {
        queue.pending_chunks.clear();
        return;
    };

    let process_limit = (AsyncComputeTaskPool::get().thread_num().max(1) * 2).clamp(
        MULTIPLAYER_STRUCTURE_RECONCILE_MIN_CHUNKS_PER_FRAME,
        MULTIPLAYER_STRUCTURE_RECONCILE_MAX_CHUNKS_PER_FRAME,
    );
    let queued_chunks: Vec<IVec2> = queue
        .pending_chunks
        .iter()
        .copied()
        .take(process_limit)
        .collect();
    for coord in &queued_chunks {
        queue.pending_chunks.remove(coord);
    }
    for coord in queued_chunks {
        if !chunk_map.chunks.contains_key(&coord) {
            continue;
        }

        let mut expected_keys: HashMap<
            PlacedStructureKey,
            (&BuildingStructureRecipe, IVec3, Option<ItemId>),
        > = HashMap::new();
        if let Some(entries) = runtime.records_by_chunk.get(&coord) {
            for entry in entries {
                let Some(recipe) =
                    structure_recipe_registry.recipe_by_name(entry.recipe_name.as_str())
                else {
                    continue;
                };
                let place_origin = IVec3::new(
                    entry.place_origin[0],
                    entry.place_origin[1],
                    entry.place_origin[2],
                );
                let rotation_steps = normalize_rotation_steps(
                    entry
                        .rotation_steps
                        .map_or((entry.rotation_quarters as i32) * 2, i32::from),
                );
                let rotation_quarters = rotation_steps_to_placement_quarters(rotation_steps);
                let key = placed_structure_key(
                    coord,
                    recipe.name.clone(),
                    place_origin,
                    rotation_quarters,
                    rotation_steps,
                );
                let style_source_item_id = resolve_structure_style_source_item_id_for_entry(
                    entry,
                    resolve_structure_drop_requirements_for_entry(entry, recipe, &item_registry)
                        .as_slice(),
                    recipe,
                    &item_registry,
                );
                expected_keys
                    .entry(key)
                    .or_insert((recipe, place_origin, style_source_item_id));
            }
        }

        let existing_keys: Vec<PlacedStructureKey> = runtime
            .spawned_entities
            .keys()
            .filter(|key| key.origin_chunk == coord)
            .cloned()
            .collect();
        for key in existing_keys {
            if expected_keys.contains_key(&key) {
                continue;
            }
            if let Some(entity) = runtime.spawned_entities.remove(&key) {
                runtime.entity_to_key.remove(&entity);
                safe_despawn_entity(&mut commands, entity);
            }
        }

        for (key, (recipe, place_origin, stacked_style_item_id)) in expected_keys {
            if runtime.spawned_entities.contains_key(&key) {
                continue;
            }
            let matching_entry = runtime.records_by_chunk.get(&coord).and_then(|entries| {
                entries
                    .iter()
                    .find(|entry| structure_entry_matches_key(entry, &key))
            });
            let (drop_requirements, style_source_item_id) = if let Some(entry) = matching_entry {
                let drop_requirements =
                    resolve_structure_drop_requirements_for_entry(entry, recipe, &item_registry);
                let style_source_item_id = resolve_structure_style_source_item_id_for_entry(
                    entry,
                    drop_requirements.as_slice(),
                    recipe,
                    &item_registry,
                );
                (drop_requirements, style_source_item_id)
            } else {
                let fallback_entry = StructureRegionEntry {
                    recipe_name: recipe.name.clone(),
                    place_origin: [place_origin.x, place_origin.y, place_origin.z],
                    rotation_quarters: key.rotation_quarters,
                    rotation_steps: Some(key.rotation_steps),
                    style_item: String::new(),
                    drop_items: Vec::new(),
                    inventory_slots: Vec::new(),
                };
                let drop_requirements = resolve_structure_drop_requirements_for_entry(
                    &fallback_entry,
                    recipe,
                    &item_registry,
                );
                let style_source_item_id = stacked_style_item_id
                    .or_else(|| resolve_default_structure_style_item_id(recipe, &item_registry));
                (drop_requirements, style_source_item_id)
            };
            let entity = spawn_structure_model_entity(
                &mut commands,
                &asset_server,
                recipe,
                place_origin,
                key.rotation_quarters,
                key.rotation_steps,
                drop_requirements,
                style_source_item_id,
            );
            runtime.spawned_entities.insert(key.clone(), entity);
            runtime.entity_to_key.insert(entity, key);
        }
    }
}

fn register_structure_in_runtime(
    runtime: &mut StructureRuntimeState,
    structure_entity: Entity,
    recipe: &BuildingStructureRecipe,
    place_origin: IVec3,
    rotation_quarters: u8,
    rotation_steps: u8,
    style_source_item_id: Option<ItemId>,
    drop_requirements: &[BuildingMaterialRequirement],
    item_registry: &ItemRegistry,
    uses_local_save_data: bool,
    ws: Option<&WorldSave>,
    mut region_cache: Option<&mut RegionCache>,
) {
    let (origin_chunk, _) = world_to_chunk_xz(place_origin.x, place_origin.z);
    runtime.loaded_chunks.insert(origin_chunk);
    let key = placed_structure_key(
        origin_chunk,
        recipe.name.clone(),
        place_origin,
        rotation_quarters,
        rotation_steps,
    );
    runtime
        .spawned_entities
        .insert(key.clone(), structure_entity);
    runtime.entity_to_key.insert(structure_entity, key);

    let entries = runtime.records_by_chunk.entry(origin_chunk).or_default();
    let style_item = style_source_item_id
        .and_then(|item_id| item_registry.def_opt(item_id))
        .map(|item| item.localized_name.clone())
        .unwrap_or_default();
    let drop_items = structure_region_drop_items_from_requirements(drop_requirements);
    if let Some(existing_entry) = entries.iter_mut().find(|entry| {
        entry.recipe_name == recipe.name
            && entry.place_origin == [place_origin.x, place_origin.y, place_origin.z]
            && normalize_rotation_quarters(entry.rotation_quarters as i32) == rotation_quarters
            && normalize_rotation_steps(
                entry
                    .rotation_steps
                    .map_or((entry.rotation_quarters as i32) * 2, i32::from),
            ) == rotation_steps
    }) {
        existing_entry.style_item = style_item;
        existing_entry.drop_items = drop_items;
    } else {
        entries.push(StructureRegionEntry {
            recipe_name: recipe.name.clone(),
            place_origin: [place_origin.x, place_origin.y, place_origin.z],
            rotation_quarters,
            rotation_steps: Some(rotation_steps),
            style_item,
            drop_items,
            inventory_slots: Vec::new(),
        });
    }

    if !uses_local_save_data {
        return;
    }
    let (Some(ws), Some(cache)) = (ws, region_cache.as_deref_mut()) else {
        return;
    };
    let _ = persist_structure_records_for_chunk(ws, cache, origin_chunk, entries);
}

#[allow(clippy::too_many_arguments)]
fn try_spawn_runtime_structure_for_registered_block(
    commands: &mut Commands,
    asset_server: &AssetServer,
    structure_recipe_registry: Option<&BuildingStructureRecipeRegistry>,
    block_registry: &BlockRegistry,
    item_registry: &ItemRegistry,
    world_pos: IVec3,
    block_id: u16,
    runtime: &mut StructureRuntimeState,
    uses_local_save_data: bool,
    ws: Option<&WorldSave>,
    region_cache: Option<&mut RegionCache>,
) -> bool {
    let Some(structure_recipe_registry) = structure_recipe_registry else {
        return false;
    };
    let Some((recipe, rotation_quarters, rotation_steps)) =
        recipe_for_registered_runtime_block(structure_recipe_registry, block_registry, block_id)
    else {
        return false;
    };

    let (origin_chunk, _) = world_to_chunk_xz(world_pos.x, world_pos.z);
    let key = placed_structure_key(
        origin_chunk,
        recipe.name.clone(),
        world_pos,
        rotation_quarters,
        rotation_steps,
    );
    if runtime.spawned_entities.contains_key(&key) {
        return true;
    }

    let fallback_entry = StructureRegionEntry {
        recipe_name: recipe.name.clone(),
        place_origin: [world_pos.x, world_pos.y, world_pos.z],
        rotation_quarters,
        rotation_steps: Some(rotation_steps),
        style_item: String::new(),
        drop_items: Vec::new(),
        inventory_slots: Vec::new(),
    };
    let drop_requirements =
        resolve_structure_drop_requirements_for_entry(&fallback_entry, recipe, item_registry);
    let style_source_item_id = resolve_structure_style_source_item_id_for_entry(
        &fallback_entry,
        drop_requirements.as_slice(),
        recipe,
        item_registry,
    );
    let structure_entity = spawn_structure_model_entity(
        commands,
        asset_server,
        recipe,
        world_pos,
        rotation_quarters,
        rotation_steps,
        drop_requirements.clone(),
        style_source_item_id,
    );
    register_structure_in_runtime(
        runtime,
        structure_entity,
        recipe,
        world_pos,
        rotation_quarters,
        rotation_steps,
        style_source_item_id,
        drop_requirements.as_slice(),
        item_registry,
        uses_local_save_data,
        ws,
        region_cache,
    );
    true
}

fn recipe_for_registered_runtime_block<'a>(
    structure_recipe_registry: &'a BuildingStructureRecipeRegistry,
    block_registry: &BlockRegistry,
    block_id: u16,
) -> Option<(&'a BuildingStructureRecipe, u8, u8)> {
    for recipe in &structure_recipe_registry.recipes {
        for rotation_quarters in 0..4u8 {
            let Some(runtime_block_id) =
                structure_runtime_placeholder_block_id(recipe, block_registry, rotation_quarters)
            else {
                continue;
            };
            if runtime_block_id != block_id {
                continue;
            }
            let rotation_steps = normalize_rotation_steps((rotation_quarters as i32) * 2);
            return Some((recipe, rotation_quarters, rotation_steps));
        }
    }
    None
}

fn placed_structure_key(
    origin_chunk: IVec2,
    recipe_name: String,
    place_origin: IVec3,
    rotation_quarters: u8,
    rotation_steps: u8,
) -> PlacedStructureKey {
    PlacedStructureKey {
        origin_chunk,
        recipe_name,
        place_origin,
        rotation_quarters,
        rotation_steps,
    }
}

fn structure_entry_matches_key(entry: &StructureRegionEntry, key: &PlacedStructureKey) -> bool {
    let entry_rotation_steps = normalize_rotation_steps(
        entry
            .rotation_steps
            .map_or((entry.rotation_quarters as i32) * 2, i32::from),
    );
    let entry_rotation_quarters = rotation_steps_to_placement_quarters(entry_rotation_steps);

    entry.recipe_name == key.recipe_name
        && entry.place_origin == [key.place_origin.x, key.place_origin.y, key.place_origin.z]
        && entry_rotation_quarters == key.rotation_quarters
        && entry_rotation_steps == key.rotation_steps
}

fn load_structure_records_for_chunk(
    ws: &WorldSave,
    cache: &mut RegionCache,
    coord: IVec2,
) -> Vec<StructureRegionEntry> {
    let Ok(Some(slot)) = cache.read_chunk(ws, coord) else {
        return Vec::new();
    };
    let Some(payload) = container_find(slot.as_slice(), TAG_STR1) else {
        return Vec::new();
    };
    decode_structure_entries(payload).unwrap_or_default()
}

fn persist_structure_records_for_chunk(
    ws: &WorldSave,
    cache: &mut RegionCache,
    coord: IVec2,
    entries: &[StructureRegionEntry],
) -> std::io::Result<()> {
    let payload = encode_structure_entries(entries);
    let old = cache.read_chunk(ws, coord).ok().flatten();
    let merged = container_upsert(old.as_deref(), TAG_STR1, payload.as_slice());
    cache.write_chunk_replace(ws, coord, merged.as_slice())
}

fn chest_payload_slots_from_region_slots(
    entry: &StructureRegionEntry,
    item_registry: &ItemRegistry,
) -> Vec<ChestInventorySlotPayload> {
    let mut slots = Vec::new();
    let mut occupied = HashSet::new();
    for saved in &entry.inventory_slots {
        if saved.count == 0 || !occupied.insert(saved.slot) {
            continue;
        }
        let item_name = saved.item.trim();
        if item_name.is_empty() {
            continue;
        }
        let Some(item_id) = item_registry.id_opt(item_name) else {
            continue;
        };
        let Some(item_def) = item_registry.def_opt(item_id) else {
            continue;
        };
        slots.push(ChestInventorySlotPayload {
            slot: saved.slot,
            item: item_def.localized_name.clone(),
            count: saved.count.max(1),
        });
    }
    slots.sort_by_key(|slot| slot.slot);
    slots
}

fn chest_region_inventory_slots_from_payload(
    slots: &[ChestInventorySlotPayload],
    item_registry: &ItemRegistry,
) -> Vec<StructureRegionInventorySlot> {
    let mut saved_slots = Vec::new();
    let mut occupied = HashSet::new();
    for slot in slots {
        if slot.count == 0 || !occupied.insert(slot.slot) {
            continue;
        }
        let item_name = slot.item.trim();
        if item_name.is_empty() {
            continue;
        }
        let Some(item_id) = item_registry.id_opt(item_name) else {
            continue;
        };
        let Some(item_def) = item_registry.def_opt(item_id) else {
            continue;
        };
        saved_slots.push(StructureRegionInventorySlot {
            slot: slot.slot,
            item: item_def.localized_name.clone(),
            count: slot.count.max(1),
        });
    }
    saved_slots.sort_by_key(|slot| slot.slot);
    saved_slots
}

fn structure_region_drop_items_from_requirements(
    requirements: &[BuildingMaterialRequirement],
) -> Vec<StructureRegionDropItem> {
    let mut entries = Vec::new();
    for requirement in requirements {
        if requirement.count == 0 {
            continue;
        }
        let BuildingMaterialRequirementSource::Item {
            item_localized_name,
            ..
        } = &requirement.source
        else {
            continue;
        };
        if item_localized_name.trim().is_empty() {
            continue;
        }
        entries.push(StructureRegionDropItem {
            item: item_localized_name.clone(),
            count: requirement.count.max(1),
        });
    }
    entries
}

fn resolve_structure_drop_requirements_for_entry(
    entry: &StructureRegionEntry,
    recipe: &BuildingStructureRecipe,
    item_registry: &ItemRegistry,
) -> Vec<BuildingMaterialRequirement> {
    let mut from_save = Vec::new();
    for drop_item in &entry.drop_items {
        let item_name = drop_item.item.trim();
        if item_name.is_empty() {
            continue;
        }
        let Some(item_id) = item_registry.id_opt(item_name) else {
            continue;
        };
        from_save.push(BuildingMaterialRequirement::item(
            item_id,
            item_name.to_string(),
            drop_item.count.max(1),
        ));
    }
    if !from_save.is_empty() {
        return from_save;
    }

    let mut fallback = Vec::new();
    for requirement in &recipe.requirements {
        if requirement.count == 0 {
            continue;
        }
        match &requirement.source {
            BuildingMaterialRequirementSource::Item {
                item_id,
                item_localized_name,
            } => {
                fallback.push(BuildingMaterialRequirement::item(
                    *item_id,
                    item_localized_name.clone(),
                    requirement.count.max(1),
                ));
            }
            BuildingMaterialRequirementSource::Group { group } => {
                let Some(item_id) = first_item_in_group(item_registry, group.as_str()) else {
                    continue;
                };
                let Some(item_def) = item_registry.def_opt(item_id) else {
                    continue;
                };
                fallback.push(BuildingMaterialRequirement::item(
                    item_id,
                    item_def.localized_name.clone(),
                    requirement.count.max(1),
                ));
            }
        }
    }
    fallback
}

fn resolve_structure_style_source_item_id_for_entry(
    entry: &StructureRegionEntry,
    drop_requirements: &[BuildingMaterialRequirement],
    recipe: &BuildingStructureRecipe,
    item_registry: &ItemRegistry,
) -> Option<ItemId> {
    let style_item_name = entry.style_item.trim();
    if !style_item_name.is_empty()
        && let Some(item_id) = item_registry.id_opt(style_item_name)
    {
        return Some(item_id);
    }
    first_requirement_item_id_in_group(drop_requirements, item_registry, "logs")
        .or_else(|| first_requirement_item_id(drop_requirements))
        .or_else(|| resolve_default_structure_style_item_id(recipe, item_registry))
}

fn resolve_default_structure_style_item_id(
    recipe: &BuildingStructureRecipe,
    item_registry: &ItemRegistry,
) -> Option<ItemId> {
    for requirement in &recipe.requirements {
        let BuildingMaterialRequirementSource::Item { item_id, .. } = &requirement.source else {
            continue;
        };
        if item_registry.has_group(*item_id, "logs") {
            return Some(*item_id);
        }
    }
    for requirement in &recipe.requirements {
        let BuildingMaterialRequirementSource::Group { group } = &requirement.source else {
            continue;
        };
        if group == "logs" {
            return first_item_in_group(item_registry, group.as_str());
        }
    }
    first_requirement_item_id(&recipe.requirements).or_else(|| {
        recipe.requirements.iter().find_map(|requirement| {
            let BuildingMaterialRequirementSource::Group { group } = &requirement.source else {
                return None;
            };
            first_item_in_group(item_registry, group.as_str())
        })
    })
}

fn first_requirement_item_id(requirements: &[BuildingMaterialRequirement]) -> Option<ItemId> {
    requirements.iter().find_map(|requirement| {
        let BuildingMaterialRequirementSource::Item { item_id, .. } = &requirement.source else {
            return None;
        };
        Some(*item_id)
    })
}

fn first_requirement_item_id_in_group(
    requirements: &[BuildingMaterialRequirement],
    item_registry: &ItemRegistry,
    group: &str,
) -> Option<ItemId> {
    requirements.iter().find_map(|requirement| {
        let BuildingMaterialRequirementSource::Item { item_id, .. } = &requirement.source else {
            return None;
        };
        if item_registry.has_group(*item_id, group) {
            Some(*item_id)
        } else {
            None
        }
    })
}

fn first_item_in_group(item_registry: &ItemRegistry, group: &str) -> Option<ItemId> {
    let max_item_id = item_registry.defs.len().saturating_sub(1) as ItemId;
    (1..=max_item_id).find(|item_id| item_registry.has_group(*item_id, group))
}

fn spawn_structure_model_entity(
    commands: &mut Commands,
    asset_server: &AssetServer,
    recipe: &BuildingStructureRecipe,
    place_origin: IVec3,
    rotation_quarters: u8,
    rotation_steps: u8,
    drop_requirements: Vec<BuildingMaterialRequirement>,
    style_source_item_id: Option<ItemId>,
) -> Entity {
    let model_rotation_quarters = normalize_rotation_quarters(
        rotation_quarters as i32 + recipe.model_meta.model_rotation_quarters as i32,
    );
    let model_rotation_steps = normalize_rotation_steps(
        rotation_steps as i32 + (recipe.model_meta.model_rotation_quarters as i32 * 2),
    );
    let model_rotation =
        Quat::from_rotation_y(-(model_rotation_steps as f32) * std::f32::consts::FRAC_PI_4);
    let placement_size_world = rotated_structure_space(recipe.space, rotation_quarters).as_vec3();
    let selection_center_world = Vec3::new(
        (place_origin.x as f32 + placement_size_world.x * 0.5) * VOXEL_SIZE,
        (place_origin.y as f32 + placement_size_world.y * 0.5) * VOXEL_SIZE,
        (place_origin.z as f32 + placement_size_world.z * 0.5) * VOXEL_SIZE,
    );
    let selection_size_world = placement_size_world * VOXEL_SIZE;
    let translation = structure_model_translation(
        recipe,
        place_origin,
        rotation_quarters,
        model_rotation_quarters,
    ) + (model_rotation * recipe.model_meta.model_offset) * VOXEL_SIZE;
    let scene_handle = asset_server.load(recipe.model_asset_path.clone());

    let structure_entity = commands
        .spawn((
            Name::new(format!("Structure:{}", recipe.name)),
            PlacedStructureMetadata {
                recipe_name: recipe.name.clone(),
                model_asset_path: recipe.model_asset_path.clone(),
                model_animated: recipe.model_meta.animated,
                stats: recipe.model_meta.stats.clone(),
                place_origin,
                drop_requirements,
                registration: recipe.model_meta.block_registration.clone(),
                selection_center_world,
                selection_size_world,
            },
            RigidBody::Fixed,
            SceneRoot(scene_handle),
            Transform::from_translation(translation).with_rotation(model_rotation),
            GlobalTransform::default(),
            Visibility::Inherited,
            InheritedVisibility::default(),
            ViewVisibility::default(),
        ))
        .id();
    let mut style_source = None;
    if let Some(style_item_id) = style_source_item_id.filter(|item_id| *item_id != 0) {
        style_source = Some(StructureStyleSourceItem {
            item_id: style_item_id,
        });
    }
    let texture_bindings = if recipe.model_meta.textures.is_empty() {
        None
    } else {
        Some(StructureTextureBindings {
            entries: recipe.model_meta.textures.clone(),
        })
    };
    if style_source.is_some() || texture_bindings.is_some() {
        if let Some(style_source) = style_source {
            commands.entity(structure_entity).insert(style_source);
        }
        if let Some(texture_bindings) = texture_bindings {
            commands.entity(structure_entity).insert(texture_bindings);
        }
        commands
            .entity(structure_entity)
            .insert(StructureStyleMaterialPending);
    }

    match &recipe.model_meta.colliders {
        BuildingStructureColliderSource::Boxes(colliders) => {
            if colliders.is_empty() {
                return structure_entity;
            }
            commands.entity(structure_entity).with_children(|children| {
                for (index, collider) in colliders.iter().enumerate() {
                    if !collider.block_entities {
                        continue;
                    }
                    let half_x = (collider.size_m[0] * 0.5).max(0.005);
                    let half_y = (collider.size_m[1] * 0.5).max(0.005);
                    let half_z = (collider.size_m[2] * 0.5).max(0.005);
                    children.spawn((
                        Name::new(format!("StructureCollider:{}:{}", recipe.name, index)),
                        Collider::cuboid(half_x, half_y, half_z),
                        Transform::from_translation(Vec3::new(
                            collider.offset_m[0],
                            collider.offset_m[1],
                            collider.offset_m[2],
                        )),
                        GlobalTransform::default(),
                    ));
                }
            });
        }
        BuildingStructureColliderSource::Mesh => {
            let mesh_flags = TriMeshFlags::FIX_INTERNAL_EDGES
                | TriMeshFlags::MERGE_DUPLICATE_VERTICES
                | TriMeshFlags::DELETE_DEGENERATE_TRIANGLES;
            commands.entity(structure_entity).insert((
                AsyncSceneCollider {
                    shape: Some(ComputedColliderShape::TriMesh(mesh_flags)),
                    named_shapes: default(),
                },
                StructureMeshColliderNameFilterPending,
                StructureMeshColliderCleanupPending,
            ));
        }
    }
    structure_entity
}
