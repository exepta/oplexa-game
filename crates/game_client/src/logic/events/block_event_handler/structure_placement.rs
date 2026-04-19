/// Structure placement, validation, and requirement-consumption helpers.
fn resolve_structure_place_origin(
    hit: crate::core::entities::player::block_selection::BlockHit,
    chunk_map: &ChunkMap,
    registry: &BlockRegistry,
) -> IVec3 {
    let hit_primary_id = get_block_world(chunk_map, hit.block_pos);
    if !hit.is_stacked && hit_primary_id != 0 && registry.is_overridable(hit_primary_id) {
        hit.block_pos
    } else {
        hit.place_pos
    }
}

fn can_place_structure_recipe_at(
    place_origin: IVec3,
    recipe: &BuildingStructureRecipe,
    rotation_quarters: u8,
    chunk_map: &ChunkMap,
    registry: &BlockRegistry,
) -> bool {
    for y_offset in 0..recipe.space.y as i32 {
        for local_z in 0..recipe.space.z as i32 {
            for local_x in 0..recipe.space.x as i32 {
                let (x_offset, z_offset) = rotated_structure_offset(
                    local_x,
                    local_z,
                    recipe.space.x as i32,
                    recipe.space.z as i32,
                    rotation_quarters,
                );
                let world_pos = place_origin + IVec3::new(x_offset, y_offset, z_offset);
                if !is_structure_cell_placeable(world_pos, chunk_map, registry) {
                    return false;
                }
            }
        }
    }

    for local_z in 0..recipe.space.z as i32 {
        for local_x in 0..recipe.space.x as i32 {
            let (x_offset, z_offset) = rotated_structure_offset(
                local_x,
                local_z,
                recipe.space.x as i32,
                recipe.space.z as i32,
                rotation_quarters,
            );
            let support_pos = place_origin + IVec3::new(x_offset, -1, z_offset);
            if !is_structure_support_cell(support_pos, chunk_map, registry) {
                return false;
            }
        }
    }

    true
}

fn clear_props_within_structure_volume(
    place_origin: IVec3,
    recipe: &BuildingStructureRecipe,
    rotation_quarters: u8,
    chunk_map: &mut ChunkMap,
    registry: &BlockRegistry,
    ev_dirty: &mut MessageWriter<SubChunkNeedRemeshEvent>,
) {
    let mut dirty_positions = Vec::new();
    for y_offset in 0..recipe.space.y as i32 {
        for local_z in 0..recipe.space.z as i32 {
            for local_x in 0..recipe.space.x as i32 {
                let (x_offset, z_offset) = rotated_structure_offset(
                    local_x,
                    local_z,
                    recipe.space.x as i32,
                    recipe.space.z as i32,
                    rotation_quarters,
                );
                let world_pos = place_origin + IVec3::new(x_offset, y_offset, z_offset);
                let Some(mut access) = world_access_mut(chunk_map, world_pos) else {
                    continue;
                };

                let mut changed = false;
                let current = access.get();
                if current != 0 && registry.is_prop(current) {
                    access.set(0);
                    changed = true;
                }
                let stacked = access.get_stacked();
                if stacked != 0 && registry.is_prop(stacked) {
                    access.set_stacked(0);
                    changed = true;
                }
                if changed {
                    dirty_positions.push(world_pos);
                }
            }
        }
    }

    for world_pos in dirty_positions {
        mark_dirty_block_and_neighbors(chunk_map, world_pos, ev_dirty);
    }
}

fn is_structure_cell_placeable(
    world_pos: IVec3,
    chunk_map: &ChunkMap,
    registry: &BlockRegistry,
) -> bool {
    if world_pos.y < Y_MIN || world_pos.y > Y_MAX {
        return false;
    }

    let (chunk_coord, local) = world_to_chunk_xz(world_pos.x, world_pos.z);
    let Some(chunk) = chunk_map.chunks.get(&chunk_coord) else {
        return false;
    };

    let lx = local.x.clamp(0, (CX as i32 - 1) as u32) as usize;
    let lz = local.y.clamp(0, (CZ as i32 - 1) as u32) as usize;
    let ly = (world_pos.y - Y_MIN).clamp(0, CY as i32 - 1) as usize;

    let existing = chunk.get(lx, ly, lz);
    let stacked = chunk.get_stacked(lx, ly, lz);
    (existing == 0 || registry.is_overridable(existing)) && stacked == 0
}

fn is_structure_support_cell(
    world_pos: IVec3,
    chunk_map: &ChunkMap,
    registry: &BlockRegistry,
) -> bool {
    if world_pos.y < Y_MIN || world_pos.y > Y_MAX {
        return false;
    }

    let (chunk_coord, local) = world_to_chunk_xz(world_pos.x, world_pos.z);
    let Some(chunk) = chunk_map.chunks.get(&chunk_coord) else {
        return false;
    };

    let lx = local.x.clamp(0, (CX as i32 - 1) as u32) as usize;
    let lz = local.y.clamp(0, (CZ as i32 - 1) as u32) as usize;
    let ly = (world_pos.y - Y_MIN).clamp(0, CY as i32 - 1) as usize;

    let existing = chunk.get(lx, ly, lz);
    let stacked = chunk.get_stacked(lx, ly, lz);
    (existing != 0 && !registry.is_overridable(existing))
        || (stacked != 0 && !registry.is_overridable(stacked))
}

fn world_cell_intersects_structure(
    world_pos: IVec3,
    rapier_context: &ReadRapierContext,
    q_structures: &Query<&PlacedStructureMetadata>,
    q_structure_parents: &Query<&ChildOf>,
) -> bool {
    let Ok(ctx) = rapier_context.single() else {
        return false;
    };
    // Slightly shrink probe so touching at a face/edge is still placeable,
    // while true overlap with structure colliders stays blocked.
    let cell_half = (VOXEL_SIZE * 0.5 - 0.02).max(0.01);
    let cell_center_world = Vec3::new(
        (world_pos.x as f32 + 0.5) * VOXEL_SIZE,
        (world_pos.y as f32 + 0.5) * VOXEL_SIZE,
        (world_pos.z as f32 + 0.5) * VOXEL_SIZE,
    );
    let probe = Collider::cuboid(cell_half, cell_half, cell_half);
    let mut intersects_structure = false;
    let structure_filter = |entity: Entity| -> bool {
        is_structure_collider_entity(entity, q_structures, q_structure_parents)
    };

    ctx.intersect_shape(
        cell_center_world,
        Quat::IDENTITY,
        (&probe).into(),
        QueryFilter::default()
            .exclude_sensors()
            .predicate(&structure_filter),
        |_| {
            intersects_structure = true;
            false
        },
    );

    intersects_structure
}

#[inline]
fn structure_has_workbench_ui(meta: &PlacedStructureMetadata) -> bool {
    if meta.recipe_name.eq_ignore_ascii_case("work_table") {
        return true;
    }
    meta.registration.as_ref().is_some_and(|registration| {
        registration
            .localized_name
            .eq_ignore_ascii_case("workbench_block")
    })
}

#[inline]
fn structure_has_chest_ui(meta: &PlacedStructureMetadata) -> bool {
    if meta.recipe_name.eq_ignore_ascii_case("chest") {
        return true;
    }
    meta.registration.as_ref().is_some_and(|registration| {
        registration
            .localized_name
            .eq_ignore_ascii_case("chest_block")
    })
}

#[inline]
fn block_has_workbench_ui(block_id: u16, registry: &BlockRegistry) -> bool {
    if block_id == 0 {
        return false;
    }
    registry.def_opt(block_id).is_some_and(|def| {
        let localized = def.localized_name.to_ascii_lowercase();
        let key = def.name.to_ascii_uppercase();
        localized == "workbench_block"
            || localized.starts_with("workbench_block_r")
            || key == "KEY_WORKBENCH_BLOCK"
            || key.starts_with("KEY_WORKBENCH_BLOCK_R")
    })
}

#[inline]
fn block_has_chest_ui(block_id: u16, registry: &BlockRegistry) -> bool {
    if block_id == 0 {
        return false;
    }
    registry.def_opt(block_id).is_some_and(|def| {
        let localized = def.localized_name.to_ascii_lowercase();
        let key = def.name.to_ascii_uppercase();
        localized == "chest_block"
            || localized.starts_with("chest_block_r")
            || key == "KEY_CHEST_BLOCK"
            || key.starts_with("KEY_CHEST_BLOCK_R")
    })
}

#[inline]
fn structure_runtime_placeholder_localized_name(
    base_localized_name: &str,
    rotation_quarters: u8,
) -> String {
    let normalized = normalize_rotation_quarters(rotation_quarters as i32);
    if normalized == 0 {
        base_localized_name.to_string()
    } else {
        format!("{base_localized_name}_r{normalized}")
    }
}

fn structure_runtime_placeholder_block_id(
    recipe: &BuildingStructureRecipe,
    registry: &BlockRegistry,
    rotation_quarters: u8,
) -> Option<u16> {
    let registration = recipe.model_meta.block_registration.as_ref()?;
    let normalized = normalize_rotation_quarters(rotation_quarters as i32);
    if normalized == 0 {
        return registration.block_id.filter(|block_id| *block_id != 0);
    }
    let localized = structure_runtime_placeholder_localized_name(
        registration.localized_name.as_str(),
        normalized,
    );
    registry
        .id_opt(localized.as_str())
        .or_else(|| registration.block_id.filter(|block_id| *block_id != 0))
}

fn is_structure_collider_entity(
    entity: Entity,
    q_structures: &Query<&PlacedStructureMetadata>,
    q_structure_parents: &Query<&ChildOf>,
) -> bool {
    let mut current = entity;
    loop {
        if q_structures.get(current).is_ok() {
            return true;
        }
        let Ok(parent) = q_structure_parents.get(current) else {
            return false;
        };
        current = parent.parent();
    }
}

fn build_structure_surface_hit(
    structure_hit: crate::core::entities::player::block_selection::StructureHit,
    q_structures: &Query<&PlacedStructureMetadata>,
) -> Option<crate::core::entities::player::block_selection::BlockHit> {
    let meta = q_structures.get(structure_hit.entity).ok()?;
    let face = face_from_normal(structure_hit.hit_normal_world);
    let inward_probe = structure_hit.hit_world - structure_hit.hit_normal_world * 0.02;

    let block_pos = IVec3::new(
        inward_probe.x.floor() as i32,
        inward_probe.y.floor() as i32,
        inward_probe.z.floor() as i32,
    );
    let place_pos = block_pos + face_to_block_offset(face);
    let hit_local = Vec3::new(
        structure_hit.hit_world.x - block_pos.x as f32,
        structure_hit.hit_world.y - block_pos.y as f32,
        structure_hit.hit_world.z - block_pos.z as f32,
    )
    .clamp(Vec3::splat(0.0), Vec3::splat(0.999));

    let block_id = meta
        .registration
        .as_ref()
        .and_then(|registration| registration.block_id)
        .unwrap_or(1);

    Some(crate::core::entities::player::block_selection::BlockHit {
        block_pos,
        block_id,
        is_stacked: false,
        face,
        hit_local,
        place_pos,
    })
}

#[inline]
fn face_to_block_offset(face: Face) -> IVec3 {
    match face {
        Face::Top => IVec3::new(0, 1, 0),
        Face::Bottom => IVec3::new(0, -1, 0),
        Face::North => IVec3::new(0, 0, -1),
        Face::South => IVec3::new(0, 0, 1),
        Face::East => IVec3::new(1, 0, 0),
        Face::West => IVec3::new(-1, 0, 0),
    }
}

#[inline]
fn face_from_normal(normal: Vec3) -> Face {
    let axis = normal.abs();
    if axis.x >= axis.y && axis.x >= axis.z {
        if normal.x >= 0.0 {
            Face::East
        } else {
            Face::West
        }
    } else if axis.y >= axis.z {
        if normal.y >= 0.0 {
            Face::Top
        } else {
            Face::Bottom
        }
    } else if normal.z >= 0.0 {
        Face::South
    } else {
        Face::North
    }
}

#[derive(Clone, Copy)]
struct PlannedStructureConsumption {
    slot_index: usize,
    requirement_index: usize,
    item_id: ItemId,
    count: u16,
}

struct ConsumedStructureRequirements {
    drop_requirements: Vec<BuildingMaterialRequirement>,
    style_source_item_id: Option<ItemId>,
}

fn consume_structure_requirements_from_inventory(
    inventory: &mut PlayerInventory,
    requirements: &[BuildingMaterialRequirement],
    item_registry: &ItemRegistry,
) -> Option<ConsumedStructureRequirements> {
    let plan = plan_structure_requirement_consumption(inventory, requirements, item_registry)?;

    let mut consumed_totals: Vec<(ItemId, u16)> = Vec::new();
    let mut logs_style_candidate: Option<ItemId> = None;
    let mut first_consumed_item: Option<ItemId> = None;

    for planned in &plan {
        let Some(slot) = inventory.slots.get_mut(planned.slot_index) else {
            return None;
        };
        if slot.is_empty() || slot.item_id != planned.item_id || slot.count < planned.count {
            return None;
        }
        slot.count -= planned.count;
        if slot.count == 0 {
            slot.item_id = 0;
        }

        if first_consumed_item.is_none() {
            first_consumed_item = Some(planned.item_id);
        }
        if logs_style_candidate.is_none()
            && requirements
                .get(planned.requirement_index)
                .is_some_and(is_logs_requirement)
        {
            logs_style_candidate = Some(planned.item_id);
        }

        if let Some((_, total)) = consumed_totals
            .iter_mut()
            .find(|(item_id, _)| *item_id == planned.item_id)
        {
            *total = total.saturating_add(planned.count);
        } else {
            consumed_totals.push((planned.item_id, planned.count));
        }
    }

    let mut drop_requirements = Vec::with_capacity(consumed_totals.len());
    for (item_id, count) in consumed_totals {
        let Some(item_def) = item_registry.def_opt(item_id) else {
            continue;
        };
        drop_requirements.push(BuildingMaterialRequirement::item(
            item_id,
            item_def.localized_name.clone(),
            count.max(1),
        ));
    }

    Some(ConsumedStructureRequirements {
        drop_requirements,
        style_source_item_id: logs_style_candidate.or(first_consumed_item),
    })
}

fn plan_structure_requirement_consumption(
    inventory: &PlayerInventory,
    requirements: &[BuildingMaterialRequirement],
    item_registry: &ItemRegistry,
) -> Option<Vec<PlannedStructureConsumption>> {
    let mut remaining_per_slot: Vec<u16> = inventory
        .slots
        .iter()
        .map(|slot| if slot.is_empty() { 0 } else { slot.count })
        .collect();
    let mut plan = Vec::new();

    for (requirement_index, required) in requirements.iter().enumerate() {
        let mut missing = required.count;
        if missing == 0 {
            continue;
        }
        for (slot_index, slot) in inventory.slots.iter().enumerate() {
            if missing == 0 {
                break;
            }
            let available = *remaining_per_slot.get(slot_index).unwrap_or(&0);
            if available == 0 || slot.is_empty() {
                continue;
            }
            if !structure_requirement_matches_item(required, slot.item_id, item_registry) {
                continue;
            }

            let take = available.min(missing);
            if let Some(remaining_slot_count) = remaining_per_slot.get_mut(slot_index) {
                *remaining_slot_count -= take;
            }
            missing -= take;
            plan.push(PlannedStructureConsumption {
                slot_index,
                requirement_index,
                item_id: slot.item_id,
                count: take,
            });
        }
        if missing > 0 {
            return None;
        }
    }

    Some(plan)
}

fn structure_requirement_matches_item(
    requirement: &BuildingMaterialRequirement,
    item_id: ItemId,
    item_registry: &ItemRegistry,
) -> bool {
    match &requirement.source {
        BuildingMaterialRequirementSource::Item {
            item_id: required_item_id,
            ..
        } => *required_item_id == item_id,
        BuildingMaterialRequirementSource::Group { group } => {
            item_registry.has_group(item_id, group.as_str())
        }
    }
}

fn is_logs_requirement(requirement: &BuildingMaterialRequirement) -> bool {
    matches!(
        &requirement.source,
        BuildingMaterialRequirementSource::Group { group } if group == "logs"
    )
}
