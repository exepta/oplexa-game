use crate::core::entities::player::block_selection::SelectionState;
use crate::core::entities::player::inventory::PlayerInventory;
use crate::core::entities::player::{FpsController, GameMode, GameModeState, Player};
use crate::core::events::block::block_player_events::{
    BlockBreakByPlayerEvent, BlockPlaceByPlayerEvent,
};
use crate::core::events::chunk_events::SubChunkNeedRemeshEvent;
use crate::core::inventory::items::{
    ItemRegistry, block_requirement_for_id, can_drop_from_block, mining_speed_multiplier,
    spawn_world_item_for_block_break,
};
use crate::core::multiplayer::MultiplayerConnectionState;
use crate::core::states::states::{AppState, InGameStates};
use crate::core::ui::{HotbarSelectionState, UiInteractionState};
use crate::core::world::block::*;
use crate::core::world::chunk::*;
use crate::core::world::chunk_dimension::*;
use crate::core::world::fluid::FluidMap;
use crate::core::world::{mark_dirty_block_and_neighbors, world_access_mut};
use crate::generator::chunk::chunk_utils::safe_despawn_entity;
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::ecs::relationship::RelatedSpawnerCommands;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;

/// Resolved block placement target for one place action.
#[derive(Clone, Copy)]
pub(crate) struct PlacementResolution {
    pub world_pos: IVec3,
    pub block_id: BlockId,
    pub place_into_stacked: bool,
}

/// Represents mining overlay used by the `logic::events::block_event_handler` module.
#[derive(Component)]
struct MiningOverlay;

/// Represents mining overlay face used by the `logic::events::block_event_handler` module.
#[derive(Component)]
struct MiningOverlayFace;

/// Defines the possible axis variants in the `logic::events::block_event_handler` module.
#[derive(Clone, Copy)]
enum Axis {
    XY,
    XZ,
    YZ,
}

/// Represents block event handler used by the `logic::events::block_event_handler` module.
pub struct BlockEventHandler;

impl Plugin for BlockEventHandler {
    /// Builds this component for the `logic::events::block_event_handler` module.
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                (block_break_handler, sync_mining_overlay)
                    .chain()
                    .in_set(VoxelStage::WorldEdit),
                block_place_handler.in_set(VoxelStage::WorldEdit),
            )
                .run_if(in_state(AppState::InGame(InGameStates::Game))),
        );
    }
}

/// Runs the `block_break_handler` routine for block break handler in the `logic::events::block_event_handler` module.
fn block_break_handler(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    time: Res<Time>,
    buttons: Res<ButtonInput<MouseButton>>,
    selection: Res<SelectionState>,
    registry: Res<BlockRegistry>,
    item_registry: Res<ItemRegistry>,
    game_mode: Res<GameModeState>,
    inventory: Res<PlayerInventory>,
    hotbar_selection: Option<Res<HotbarSelectionState>>,

    mut state: ResMut<MiningState>,
    mut chunk_map: ResMut<ChunkMap>,
    mut ev_dirty: MessageWriter<SubChunkNeedRemeshEvent>,
    mut break_ev: MessageWriter<BlockBreakByPlayerEvent>,
    multiplayer_connection: Option<Res<MultiplayerConnectionState>>,
    ui_state: Option<Res<UiInteractionState>>,
) {
    if ui_state
        .as_ref()
        .is_some_and(|state| state.blocks_game_input())
    {
        state.target = None;
        return;
    }

    let multiplayer_connected = multiplayer_connection
        .as_ref()
        .is_some_and(|state| state.connected);

    if game_mode.0.eq(&GameMode::Spectator) {
        return;
    }
    if !buttons.pressed(MouseButton::Left) {
        state.target = None;
        return;
    }

    let Some(hit) = selection.hit else {
        state.target = None;
        return;
    };

    let id_now = hit.block_id;
    if id_now == 0 {
        state.target = None;
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
    } else if prop_block {
        // Props (e.g. tall grass) break instantly in survival.
        state.target = None;
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
            return;
        }

        if let Some(target) = state.target {
            if mining_progress(now, &target) < 1.0 {
                return;
            }
        } else {
            return;
        }
    }

    let world_loc = hit.block_pos;
    if !remove_hit_block_occupant(&mut chunk_map, world_loc, id_now, hit.is_stacked) {
        state.target = None;
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
            &item_registry,
            id_now,
            world_loc,
            now,
        );
    }

    state.target = None;
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
        let prop_id = get_block_world(chunk_map, world_loc);
        if prop_id == 0 || !registry.is_prop(prop_id) {
            break;
        }

        let below_id = get_block_world(chunk_map, world_loc + IVec3::NEG_Y);
        if registry.prop_allows_ground(prop_id, below_id) {
            break;
        }

        if let Some(mut access) = world_access_mut(chunk_map, world_loc) {
            access.set(0);
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

/// Runs the `block_place_handler` routine for block place handler in the `logic::events::block_event_handler` module.
fn block_place_handler(
    buttons: Res<ButtonInput<MouseButton>>,
    sel: Res<SelectionState>,
    selected: Res<SelectedBlock>,
    registry: Res<BlockRegistry>,
    item_registry: Res<ItemRegistry>,
    game_mode: Res<GameModeState>,
    hotbar_selection: Option<Res<HotbarSelectionState>>,
    ui_state: Option<Res<UiInteractionState>>,
    q_player_controls: Query<&FpsController, With<Player>>,

    mut inventory: ResMut<PlayerInventory>,
    mut fluids: ResMut<FluidMap>,
    mut chunk_map: ResMut<ChunkMap>,
    mut ev_dirty: MessageWriter<SubChunkNeedRemeshEvent>,
    mut place_ev: MessageWriter<BlockPlaceByPlayerEvent>,
) {
    if ui_state
        .as_ref()
        .is_some_and(|state| state.blocks_game_input())
    {
        return;
    }

    if game_mode.0.eq(&GameMode::Spectator) {
        return;
    }
    if !buttons.just_pressed(MouseButton::Right) {
        return;
    }
    let id = selected.id;
    if id == 0 {
        return;
    }
    let creative_mode = matches!(game_mode.0, GameMode::Creative);
    if !creative_mode
        && !can_place_from_selected_slot(
            &inventory,
            hotbar_selection.as_deref(),
            id,
            &item_registry,
            &registry,
        )
    {
        return;
    }
    let Some(hit) = sel.hit else {
        return;
    };
    let (player_yaw, player_pitch) = q_player_controls
        .iter()
        .next()
        .map(|ctrl| (ctrl.yaw, ctrl.pitch))
        .unwrap_or((0.0, 0.0));
    let placement =
        resolve_placement_for_selected(id, hit, player_yaw, player_pitch, &chunk_map, &registry);
    let place_id = placement.block_id;
    let world_pos = placement.world_pos;
    let place_into_stacked = placement.place_into_stacked;
    let (chunk_coord, l) = world_to_chunk_xz(world_pos.x, world_pos.z);
    let lx = l.x.clamp(0, (CX as i32 - 1) as u32) as usize;
    let lz = l.y.clamp(0, (CZ as i32 - 1) as u32) as usize;
    let ly = (world_pos.y - Y_MIN).clamp(0, CY as i32 - 1) as usize;

    let can_place = chunk_map
        .chunks
        .get(&chunk_coord)
        .map(|ch| {
            let current = ch.get(lx, ly, lz);
            if place_into_stacked {
                current != 0 && ch.get_stacked(lx, ly, lz) == 0
            } else {
                current == 0
            }
        })
        .unwrap_or(false);
    if !can_place {
        return;
    }

    if registry.is_prop(place_id) {
        let ground_pos = world_pos + IVec3::NEG_Y;
        let ground_id = get_block_world(&chunk_map, ground_pos);
        if !registry.prop_allows_ground(place_id, ground_id) {
            return;
        }
    }

    if let Some(fc) = fluids.0.get_mut(&chunk_coord) {
        fc.set(lx, ly, lz, false);
    }

    if let Some(mut access) = world_access_mut(&mut chunk_map, world_pos) {
        if place_into_stacked {
            access.set_stacked(place_id);
        } else {
            access.set(place_id);
        }
    }

    if !creative_mode {
        let _ = consume_from_selected_slot(
            &mut inventory,
            hotbar_selection.as_deref(),
            id,
            &item_registry,
            &registry,
        );
    }

    mark_dirty_block_and_neighbors(&mut chunk_map, world_pos, &mut ev_dirty);

    let name = registry.name_opt(place_id).unwrap_or("").to_string();
    place_ev.write(BlockPlaceByPlayerEvent {
        location: world_pos,
        block_id: place_id,
        block_name: name,
    });
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
    let adjacent_id = slab_block_id_for_variant(prefix, adjacent_variant, registry)
        .unwrap_or(requested_id);
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
        SlabPlacementMode::Horizontal =>
            resolve_horizontal_half_variant_for_face(hit.face, hit.hit_local.y, for_same_voxel),
        SlabPlacementMode::Vertical =>
            resolve_vertical_side_variant_for_face(hit.face, hit.hit_local, player_yaw, for_same_voxel),
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

/// Checks whether place from selected slot in the `logic::events::block_event_handler` module.
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

/// Runs the `consume_from_selected_slot` routine for consume from selected slot in the `logic::events::block_event_handler` module.
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

/// Runs the `selected_hotbar_tool` routine for selected hotbar tool in the `logic::events::block_event_handler` module.
fn selected_hotbar_tool(
    inventory: &PlayerInventory,
    hotbar_selection: Option<&HotbarSelectionState>,
    item_registry: &ItemRegistry,
) -> Option<crate::core::inventory::items::ToolDef> {
    let index = hotbar_selection
        .map(|selection| selection.selected_index)
        .unwrap_or(0);

    let slot = inventory.slots.get(index)?;
    if slot.is_empty() {
        return None;
    }

    item_registry.tool_for_item(slot.item_id)
}

/// Synchronizes mining overlay for the `logic::events::block_event_handler` module.
fn sync_mining_overlay(
    mut commands: Commands,
    mut root: ResMut<MiningOverlayRoot>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    time: Res<Time>,
    state: Res<MiningState>,
    mut q_faces: Query<
        (&mut Transform, &MiningOverlayFace),
        (With<MiningOverlayFace>, Without<MiningOverlay>),
    >,
    mut q_parent_tf: Query<&mut Transform, (With<MiningOverlay>, Without<MiningOverlayFace>)>,
) {
    let Some(target) = state.target else {
        if let Some(e) = root.0.take() {
            safe_despawn_entity(&mut commands, e);
        }
        return;
    };

    let now = time.elapsed_secs();
    let progress = ((now - target.started_at) / target.duration).clamp(0.0, 1.0);

    let s = VOXEL_SIZE;
    let center = Vec3::new(
        (target.loc.x as f32 + 0.5) * s,
        (target.loc.y as f32 + 0.5) * s,
        (target.loc.z as f32 + 0.5) * s,
    );

    let parent_e = if let Some(e) = root.0 {
        e
    } else {
        let e = spawn_overlay_at(
            &mut commands,
            &mut meshes,
            &mut mats,
            center,
            Some(RenderLayers::layer(2)),
            progress,
        );
        root.0 = Some(e);
        e
    };

    if let Ok(mut tf) = q_parent_tf.get_mut(parent_e) {
        tf.translation = center;
    }

    let max_scale = 0.98 * s;
    let size = max_scale * progress;
    let face_scale = Vec3::new(size, size, 1.0);

    for (mut tf, _) in q_faces.iter_mut() {
        tf.scale = face_scale;
    }

    if progress >= 1.0 {
        if let Some(e) = root.0.take() {
            safe_despawn_entity(&mut commands, e);
        }
    }
}

/// Spawns overlay at for the `logic::events::block_event_handler` module.
#[inline]
fn spawn_overlay_at(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    world_center: Vec3,
    layer: Option<RenderLayers>,
    initial_progress: f32,
) -> Entity {
    let quad = meshes.add(unit_centered_quad());
    let mat = mats.add(StandardMaterial {
        base_color: Color::srgba(0.9, 0.9, 0.9, 0.02),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        perceptual_roughness: 1.0,
        ..default()
    });

    let s = VOXEL_SIZE;
    let half = 0.5 * s;
    let eps = 0.003 * s;

    let max_scale = 0.98 * s;
    let init_scale = (initial_progress.clamp(0.0, 1.0).max(0.001)) * max_scale;
    let init_vec = Vec3::new(init_scale, init_scale, 1.0);

    let mut parent = commands.spawn((
        MiningOverlay,
        Visibility::default(),
        NoFrustumCulling,
        Transform::from_translation(world_center),
        GlobalTransform::default(),
        NotShadowCaster,
        NotShadowReceiver,
        Name::new("MiningOverlay"),
    ));
    if let Some(l) = layer.as_ref() {
        parent.insert(l.clone());
    }
    let parent_id = parent.id();

    let spawn_face = |c: &mut RelatedSpawnerCommands<ChildOf>, _: Axis, tf: Transform| {
        let mut e = c.spawn((
            MiningOverlayFace,
            Visibility::default(),
            Mesh3d(quad.clone()),
            MeshMaterial3d(mat.clone()),
            tf.with_scale(init_vec),
            GlobalTransform::default(),
            NotShadowCaster,
            NotShadowReceiver,
            Name::new("MiningOverlayFace"),
        ));
        if let Some(l) = layer.as_ref() {
            e.insert(l.clone());
        }
    };

    commands.entity(parent_id).with_children(|c| {
        // +Z / -Z (XY)
        spawn_face(
            c,
            Axis::XY,
            Transform::from_translation(Vec3::new(0.0, 0.0, half + eps)),
        );
        spawn_face(
            c,
            Axis::XY,
            Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::PI))
                .with_translation(Vec3::new(0.0, 0.0, -half - eps)),
        );

        // +Y / -Y (XZ)
        spawn_face(
            c,
            Axis::XZ,
            Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
                .with_translation(Vec3::new(0.0, half + eps, 0.0)),
        );
        spawn_face(
            c,
            Axis::XZ,
            Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
                .with_translation(Vec3::new(0.0, -half - eps, 0.0)),
        );

        // +X / -X (YZ)
        spawn_face(
            c,
            Axis::YZ,
            Transform::from_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2))
                .with_translation(Vec3::new(half + eps, 0.0, 0.0)),
        );
        spawn_face(
            c,
            Axis::YZ,
            Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2))
                .with_translation(Vec3::new(-half - eps, 0.0, 0.0)),
        );
    });

    parent_id
}

/// Runs the `unit_centered_quad` routine for unit centered quad in the `logic::events::block_event_handler` module.
#[inline]
fn unit_centered_quad() -> Mesh {
    use bevy::mesh::{Indices, PrimitiveTopology};
    use bevy::prelude::Mesh;
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, Default::default());
    m.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [-0.5, -0.5, 0.0],
            [0.5, -0.5, 0.0],
            [0.5, 0.5, 0.0],
            [-0.5, 0.5, 0.0],
        ],
    );
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 4]);
    m.insert_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
    );
    m.insert_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]));
    m
}
