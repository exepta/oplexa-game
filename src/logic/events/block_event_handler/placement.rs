/// Resolved block placement target for one place action.
#[derive(Clone, Copy)]
pub(crate) struct PlacementResolution {
    pub world_pos: IVec3,
    pub block_id: BlockId,
    pub place_into_stacked: bool,
}

pub(crate) fn resolve_placement_for_selected(
    selected_id: BlockId,
    hit: crate::core::entities::player::block_selection::BlockHit,
    player_yaw: f32,
    player_pitch: f32,
    chunk_map: &ChunkMap,
    registry: &BlockRegistry,
) -> PlacementResolution {
    let (adjacent_place_id, same_voxel_place_id, _slab_mode) =
        resolve_placement_block_id(selected_id, hit, player_yaw, player_pitch, registry);
    let mut world_pos = hit.place_pos;
    let mut place_id = adjacent_place_id;
    let mut place_into_stacked = false;

    if let Some((stack_pos, use_same_voxel_id)) = try_resolve_slab_stack(
        selected_id,
        adjacent_place_id,
        same_voxel_place_id,
        hit,
        world_pos,
        chunk_map,
        registry,
    ) {
        world_pos = stack_pos;
        place_id = if use_same_voxel_id {
            same_voxel_place_id
        } else {
            adjacent_place_id
        };
        place_into_stacked = true;
    }

    PlacementResolution {
        world_pos,
        block_id: place_id,
        place_into_stacked,
    }
}

#[inline]
fn resolve_placement_block_id(
    requested_id: BlockId,
    hit: crate::core::entities::player::block_selection::BlockHit,
    player_yaw: f32,
    _player_pitch: f32,
    registry: &BlockRegistry,
) -> (BlockId, BlockId, Option<SlabPlacementMode>) {
    let Some(name) = registry.name_opt(requested_id) else {
        return (requested_id, requested_id, None);
    };
    let Some(prefix) = slab_family_prefix(name) else {
        return (requested_id, requested_id, None);
    };

    let mode = resolve_slab_mode_for_click(hit.face, hit.hit_local);
    let adjacent_variant = resolve_slab_variant_for_click(hit, mode, player_yaw, false);
    let same_voxel_variant = resolve_slab_variant_for_click(hit, mode, player_yaw, true);
    let adjacent_id =
        slab_block_id_for_variant(prefix, adjacent_variant, registry).unwrap_or(requested_id);
    let same_voxel_id =
        slab_block_id_for_variant(prefix, same_voxel_variant, registry).unwrap_or(adjacent_id);
    (adjacent_id, same_voxel_id, Some(mode))
}

#[inline]
fn slab_family_prefix(name: &str) -> Option<&str> {
    const SUFFIXES: [&str; 6] = [
        "_slab_block",
        "_slab_top_block",
        "_slab_north_block",
        "_slab_south_block",
        "_slab_east_block",
        "_slab_west_block",
    ];

    SUFFIXES.iter().find_map(|suffix| name.strip_suffix(suffix))
}

fn try_resolve_slab_stack(
    selected_id: BlockId,
    adjacent_stack_id: BlockId,
    same_voxel_stack_id: BlockId,
    hit: crate::core::entities::player::block_selection::BlockHit,
    place_pos: IVec3,
    chunk_map: &ChunkMap,
    registry: &BlockRegistry,
) -> Option<(IVec3, bool)> {
    let selected_name = registry.name_opt(selected_id)?;
    slab_family_prefix(selected_name)?;

    // Rule: slab on slab should share one voxel slot whenever possible.
    let hit_existing_id = get_block_world(chunk_map, hit.block_pos);
    let hit_stacked_id = get_stacked_block_world(chunk_map, hit.block_pos);
    if slab_cell_accepts_second_slab_in_waterlogged_cell(
        hit_existing_id,
        hit_stacked_id,
        same_voxel_stack_id,
        registry,
    ) {
        return Some((hit.block_pos, true));
    }
    if slab_cell_accepts_second_slab_for_incoming(
        hit_existing_id,
        hit_stacked_id,
        same_voxel_stack_id,
        registry,
    ) {
        return Some((hit.block_pos, true));
    }

    let place_existing_id = get_block_world(chunk_map, place_pos);
    let place_stacked_id = get_stacked_block_world(chunk_map, place_pos);
    if slab_cell_accepts_second_slab_in_waterlogged_cell(
        place_existing_id,
        place_stacked_id,
        adjacent_stack_id,
        registry,
    ) {
        return Some((place_pos, false));
    }
    if slab_cell_accepts_second_slab_for_incoming(
        place_existing_id,
        place_stacked_id,
        adjacent_stack_id,
        registry,
    ) {
        return Some((place_pos, false));
    }

    None
}

#[inline]
fn slab_cell_accepts_second_slab(
    existing_id: BlockId,
    existing_stacked_id: BlockId,
    registry: &BlockRegistry,
) -> bool {
    if existing_id == 0 {
        return false;
    }
    if existing_stacked_id != 0 {
        return false;
    }
    is_any_slab_variant(existing_id, registry)
}

#[inline]
fn slab_cell_accepts_second_slab_for_incoming(
    existing_id: BlockId,
    existing_stacked_id: BlockId,
    incoming_id: BlockId,
    registry: &BlockRegistry,
) -> bool {
    if !slab_cell_accepts_second_slab(existing_id, existing_stacked_id, registry) {
        return false;
    }
    let Some(existing_variant) = slab_variant_from_block_id(existing_id, registry) else {
        return false;
    };
    let Some(incoming_variant) = slab_variant_from_block_id(incoming_id, registry) else {
        return false;
    };
    slabs_are_complementary(existing_variant, incoming_variant)
}

#[inline]
fn slab_cell_accepts_second_slab_in_waterlogged_cell(
    existing_id: BlockId,
    existing_stacked_id: BlockId,
    incoming_id: BlockId,
    registry: &BlockRegistry,
) -> bool {
    if !registry.is_fluid(existing_id) || existing_stacked_id == 0 {
        return false;
    }
    slab_cell_accepts_second_slab_for_incoming(existing_stacked_id, 0, incoming_id, registry)
}

#[inline]
fn slab_variant_from_block_id(block_id: BlockId, registry: &BlockRegistry) -> Option<SlabVariant> {
    let name = registry.name_opt(block_id)?;
    slab_variant_from_name(name)
}

fn resolve_slab_variant_for_click(
    hit: crate::core::entities::player::block_selection::BlockHit,
    mode: SlabPlacementMode,
    player_yaw: f32,
    for_same_voxel: bool,
) -> SlabVariant {
    match mode {
        SlabPlacementMode::Horizontal => {
            resolve_horizontal_half_variant_for_face(hit.face, hit.hit_local.y, for_same_voxel)
        }
        SlabPlacementMode::Vertical => resolve_vertical_side_variant_for_face(
            hit.face,
            hit.hit_local,
            player_yaw,
            for_same_voxel,
        ),
    }
}

#[inline]
fn resolve_slab_mode_for_click(face: Face, local: Vec3) -> SlabPlacementMode {
    // Requested rule: edge => vertical, center => horizontal.
    const EDGE_THRESHOLD: f32 = 0.30;
    let edge_metric = edge_metric_for_face(face, local);
    if edge_metric >= EDGE_THRESHOLD {
        SlabPlacementMode::Vertical
    } else {
        SlabPlacementMode::Horizontal
    }
}

#[inline]
fn edge_metric_for_face(face: Face, local: Vec3) -> f32 {
    let l = local.clamp(Vec3::ZERO, Vec3::ONE);
    match face {
        Face::Top | Face::Bottom => (l.x - 0.5).abs().max((l.z - 0.5).abs()),
        Face::East | Face::West => (l.y - 0.5).abs().max((l.z - 0.5).abs()),
        Face::North | Face::South => (l.x - 0.5).abs().max((l.y - 0.5).abs()),
    }
}

#[inline]
fn resolve_vertical_side_variant_for_face(
    face: Face,
    local: Vec3,
    player_yaw: f32,
    for_same_voxel: bool,
) -> SlabVariant {
    match face {
        Face::East => {
            if for_same_voxel {
                SlabVariant::East
            } else {
                SlabVariant::West
            }
        }
        Face::West => {
            if for_same_voxel {
                SlabVariant::West
            } else {
                SlabVariant::East
            }
        }
        Face::South => {
            if for_same_voxel {
                SlabVariant::South
            } else {
                SlabVariant::North
            }
        }
        Face::North => {
            if for_same_voxel {
                SlabVariant::North
            } else {
                SlabVariant::South
            }
        }
        Face::Top | Face::Bottom => {
            let l = local.clamp(Vec3::ZERO, Vec3::ONE);
            let dx = l.x - 0.5;
            let dz = l.z - 0.5;
            if (dx.abs() - dz.abs()).abs() <= 0.05 {
                return yaw_to_horizontal_variant(player_yaw);
            }
            if dx.abs() >= dz.abs() {
                if dx >= 0.0 {
                    SlabVariant::East
                } else {
                    SlabVariant::West
                }
            } else if dz >= 0.0 {
                SlabVariant::South
            } else {
                SlabVariant::North
            }
        }
    }
}

#[inline]
fn resolve_horizontal_half_variant_for_face(
    face: Face,
    local_y: f32,
    for_same_voxel: bool,
) -> SlabVariant {
    match face {
        Face::Top => {
            if for_same_voxel {
                SlabVariant::Top
            } else {
                SlabVariant::Bottom
            }
        }
        Face::Bottom => {
            if for_same_voxel {
                SlabVariant::Bottom
            } else {
                SlabVariant::Top
            }
        }
        Face::East | Face::West | Face::North | Face::South => {
            if local_y >= 0.5 {
                SlabVariant::Top
            } else {
                SlabVariant::Bottom
            }
        }
    }
}

#[inline]
fn yaw_to_horizontal_variant(player_yaw: f32) -> SlabVariant {
    let look = Quat::from_rotation_y(player_yaw) * Vec3::NEG_Z;
    if look.x.abs() >= look.z.abs() {
        if look.x >= 0.0 {
            SlabVariant::East
        } else {
            SlabVariant::West
        }
    } else if look.z >= 0.0 {
        SlabVariant::South
    } else {
        SlabVariant::North
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum SlabVariant {
    Bottom,
    Top,
    North,
    South,
    East,
    West,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum SlabPlacementMode {
    Horizontal,
    Vertical,
}

#[inline]
fn slab_variant_from_name(name: &str) -> Option<SlabVariant> {
    if name.ends_with("_slab_block") {
        return Some(SlabVariant::Bottom);
    }
    if name.ends_with("_slab_top_block") {
        return Some(SlabVariant::Top);
    }
    if name.ends_with("_slab_north_block") {
        return Some(SlabVariant::North);
    }
    if name.ends_with("_slab_south_block") {
        return Some(SlabVariant::South);
    }
    if name.ends_with("_slab_east_block") {
        return Some(SlabVariant::East);
    }
    if name.ends_with("_slab_west_block") {
        return Some(SlabVariant::West);
    }
    None
}

#[inline]
fn is_any_slab_variant(block_id: BlockId, registry: &BlockRegistry) -> bool {
    let Some(name) = registry.name_opt(block_id) else {
        return false;
    };
    slab_variant_from_name(name).is_some()
}

#[inline]
fn slab_block_id_for_variant(
    slab_prefix: &str,
    variant: SlabVariant,
    registry: &BlockRegistry,
) -> Option<BlockId> {
    let name = match variant {
        SlabVariant::Bottom => format!("{slab_prefix}_slab_block"),
        SlabVariant::Top => format!("{slab_prefix}_slab_top_block"),
        SlabVariant::North => format!("{slab_prefix}_slab_north_block"),
        SlabVariant::South => format!("{slab_prefix}_slab_south_block"),
        SlabVariant::East => format!("{slab_prefix}_slab_east_block"),
        SlabVariant::West => format!("{slab_prefix}_slab_west_block"),
    };
    registry.id_opt(name.as_str())
}

#[inline]
fn slabs_are_complementary(a: SlabVariant, b: SlabVariant) -> bool {
    matches!(
        (a, b),
        (SlabVariant::Bottom, SlabVariant::Top)
            | (SlabVariant::Top, SlabVariant::Bottom)
            | (SlabVariant::North, SlabVariant::South)
            | (SlabVariant::South, SlabVariant::North)
            | (SlabVariant::East, SlabVariant::West)
            | (SlabVariant::West, SlabVariant::East)
    )
}

fn remove_hit_block_occupant(
    chunk_map: &mut ChunkMap,
    world_loc: IVec3,
    hit_id: BlockId,
    hit_is_stacked: bool,
) -> bool {
    let Some(mut access) = world_access_mut(chunk_map, world_loc) else {
        return false;
    };

    let primary = access.get();
    let stacked = access.get_stacked();

    if hit_is_stacked {
        if stacked == 0 {
            return false;
        }
        access.set_stacked(0);
        return true;
    }

    if primary != hit_id {
        if stacked == hit_id {
            access.set_stacked(0);
            return true;
        }
        return false;
    }

    if stacked != 0 {
        access.set(stacked);
        access.set_stacked(0);
    } else {
        access.set(0);
    }
    true
}

/// Returns whether the selected inventory source can place `block_id`.
fn can_place_from_selected_slot(
    inventory: &PlayerInventory,
    hotbar_selection: Option<&HotbarSelectionState>,
    block_id: BlockId,
    item_registry: &ItemRegistry,
    registry: &BlockRegistry,
) -> bool {
    let canonical_block_id = canonical_inventory_match_block_id(block_id, registry);

    if let Some(index) = hotbar_selection.map(|selection| selection.selected_index) {
        let Some(slot) = inventory.slots.get(index) else {
            return false;
        };
        return !slot.is_empty()
            && item_registry
                .block_for_item(slot.item_id)
                .is_some_and(|item_block| {
                    item_block == block_id || item_block == canonical_block_id
                })
            && slot.count > 0;
    }

    inventory.slots.iter().any(|slot| {
        !slot.is_empty()
            && item_registry
                .block_for_item(slot.item_id)
                .is_some_and(|item_block| {
                    item_block == block_id || item_block == canonical_block_id
                })
            && slot.count > 0
    })
}

/// Consumes one placeable item for `block_id` from the selected source.
fn consume_from_selected_slot(
    inventory: &mut PlayerInventory,
    hotbar_selection: Option<&HotbarSelectionState>,
    block_id: BlockId,
    item_registry: &ItemRegistry,
    registry: &BlockRegistry,
) -> bool {
    let canonical_block_id = canonical_inventory_match_block_id(block_id, registry);

    if let Some(index) = hotbar_selection.map(|selection| selection.selected_index) {
        let Some(slot) = inventory.slots.get_mut(index) else {
            return false;
        };
        if slot.is_empty()
            || !item_registry
                .block_for_item(slot.item_id)
                .is_some_and(|item_block| {
                    item_block == block_id || item_block == canonical_block_id
                })
            || slot.count == 0
        {
            return false;
        }

        slot.count -= 1;
        if slot.count == 0 {
            slot.item_id = 0;
        }
        return true;
    }

    for slot in &mut inventory.slots {
        if slot.is_empty()
            || !item_registry
                .block_for_item(slot.item_id)
                .is_some_and(|item_block| {
                    item_block == block_id || item_block == canonical_block_id
                })
            || slot.count == 0
        {
            continue;
        }
        slot.count -= 1;
        if slot.count == 0 {
            slot.item_id = 0;
        }
        return true;
    }

    false
}

#[inline]
fn canonical_inventory_match_block_id(block_id: BlockId, registry: &BlockRegistry) -> BlockId {
    let Some(name) = registry.name_opt(block_id) else {
        return block_id;
    };
    let Some(prefix) = slab_family_prefix(name) else {
        return block_id;
    };
    registry
        .id_opt(format!("{prefix}_slab_block").as_str())
        .unwrap_or(block_id)
}

fn selected_hotbar_item_id(
    inventory: &PlayerInventory,
    hotbar_selection: Option<&HotbarSelectionState>,
) -> Option<u16> {
    let index = hotbar_selection
        .map(|selection| selection.selected_index)
        .unwrap_or(0);
    let slot = inventory.slots.get(index)?;
    if slot.is_empty() {
        return None;
    }
    Some(slot.item_id)
}

/// Resolves the currently selected hotbar tool, if any.
fn selected_hotbar_tool(
    inventory: &PlayerInventory,
    hotbar_selection: Option<&HotbarSelectionState>,
    item_registry: &ItemRegistry,
) -> Option<crate::core::inventory::items::ToolDef> {
    let item_id = selected_hotbar_item_id(inventory, hotbar_selection)?;
    item_registry.tool_for_item(item_id)
}
