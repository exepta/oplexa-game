use crate::core::entities::player::block_selection::SelectionState;
use crate::core::entities::player::inventory::PlayerInventory;
use crate::core::entities::player::{FpsController, GameMode, GameModeState, Player, PlayerCamera};
use crate::core::events::block::block_player_events::{
    BlockBreakByPlayerEvent, BlockPlaceByPlayerEvent,
};
use crate::core::events::chunk_events::SubChunkNeedRemeshEvent;
use crate::core::events::ui_events::{OpenStructureBuildMenuRequest, OpenWorkbenchMenuRequest};
use crate::core::inventory::items::{
    ItemRegistry, block_requirement_for_id, can_drop_from_block, mining_speed_multiplier,
    spawn_world_item_for_block_break, spawn_world_item_with_motion,
};
use crate::core::inventory::recipe::{
    ActiveStructurePlacementState, ActiveStructureRecipeState, BuildingMaterialRequirement,
    BuildingModelAnchor, BuildingStructureBlockRegistration, BuildingStructureColliderSource,
    BuildingStructureRecipe, BuildingStructureRecipeRegistry,
};
use crate::core::multiplayer::MultiplayerConnectionState;
use crate::core::states::states::{AppState, InGameStates};
use crate::core::ui::{HotbarSelectionState, UiInteractionState};
use crate::core::world::block::*;
use crate::core::world::chunk::*;
use crate::core::world::chunk_dimension::*;
use crate::core::world::fluid::FluidMap;
use crate::core::world::save::{
    RegionCache, StructureRegionEntry, TAG_STR1, WorldSave, container_find, container_upsert,
    decode_structure_entries, encode_structure_entries,
};
use crate::core::world::{mark_dirty_block_and_neighbors, world_access_mut};
use crate::generator::chunk::chunk_utils::safe_despawn_entity;
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::ecs::relationship::RelatedSpawnerCommands;
use bevy::ecs::system::SystemParam;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy_rapier3d::prelude::{
    AsyncSceneCollider, Collider, ComputedColliderShape, RigidBody, TriMeshFlags,
};
use std::collections::{HashMap, HashSet};

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

#[derive(Component, Clone)]
#[allow(dead_code)]
pub(crate) struct PlacedStructureMetadata {
    pub recipe_name: String,
    pub stats: BlockStats,
    pub place_origin: IVec3,
    pub rotation_quarters: u8,
    pub origin_chunk: IVec2,
    pub drop_requirements: Vec<BuildingMaterialRequirement>,
    pub registration: Option<BuildingStructureBlockRegistration>,
    pub selection_center_world: Vec3,
    pub selection_size_world: Vec3,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct PlacedStructureKey {
    origin_chunk: IVec2,
    recipe_name: String,
    place_origin: IVec3,
    rotation_quarters: u8,
}

#[derive(Resource, Default)]
struct StructureRuntimeState {
    loaded_chunks: HashSet<IVec2>,
    records_by_chunk: HashMap<IVec2, Vec<StructureRegionEntry>>,
    spawned_entities: HashMap<PlacedStructureKey, Entity>,
    entity_to_key: HashMap<Entity, PlacedStructureKey>,
}

#[derive(Clone, Copy, Debug)]
struct StructureMiningTarget {
    entity: Entity,
    started_at: f32,
    duration: f32,
}

#[derive(Resource, Default)]
struct StructureMiningState {
    target: Option<StructureMiningTarget>,
}

/// Defines the possible axis variants in the `logic::events::block_event_handler` module.
#[derive(Clone, Copy)]
enum Axis {
    XY,
    XZ,
    YZ,
}

/// Represents block event handler used by the `logic::events::block_event_handler` module.
pub struct BlockEventHandler;

#[derive(SystemParam)]
struct StructurePlacementDeps<'w, 's> {
    commands: Commands<'w, 's>,
    asset_server: Res<'w, AssetServer>,
    structure_recipe_registry: Option<Res<'w, BuildingStructureRecipeRegistry>>,
    active_structure_recipe: ResMut<'w, ActiveStructureRecipeState>,
    active_structure_placement: ResMut<'w, ActiveStructurePlacementState>,
    open_structure_menu_requests: MessageWriter<'w, OpenStructureBuildMenuRequest>,
    open_workbench_menu_requests: MessageWriter<'w, OpenWorkbenchMenuRequest>,
}

#[derive(SystemParam)]
struct ActiveStructureCancelState<'w> {
    active_structure_recipe: ResMut<'w, ActiveStructureRecipeState>,
    active_structure_placement: ResMut<'w, ActiveStructurePlacementState>,
}

#[derive(SystemParam)]
struct BreakInputContext<'w> {
    multiplayer_connection: Option<Res<'w, MultiplayerConnectionState>>,
    ui_state: Option<Res<'w, UiInteractionState>>,
}

#[derive(SystemParam)]
struct StructureBreakDeps<'w, 's> {
    structure_runtime: ResMut<'w, StructureRuntimeState>,
    structure_mining: ResMut<'w, StructureMiningState>,
    q_structure_meta: Query<'w, 's, &'static PlacedStructureMetadata>,
    ws: Option<Res<'w, WorldSave>>,
    region_cache: Option<ResMut<'w, RegionCache>>,
}

#[derive(SystemParam)]
struct BreakWorldMut<'w> {
    state: ResMut<'w, MiningState>,
    chunk_map: ResMut<'w, ChunkMap>,
    ev_dirty: MessageWriter<'w, SubChunkNeedRemeshEvent>,
    break_ev: MessageWriter<'w, BlockBreakByPlayerEvent>,
}

#[derive(SystemParam)]
struct PlacementWorldDeps<'w> {
    inventory: ResMut<'w, PlayerInventory>,
    fluids: ResMut<'w, FluidMap>,
    chunk_map: ResMut<'w, ChunkMap>,
    multiplayer_connection: Res<'w, MultiplayerConnectionState>,
    ws: Option<Res<'w, WorldSave>>,
    region_cache: Option<ResMut<'w, RegionCache>>,
    structure_runtime: ResMut<'w, StructureRuntimeState>,
    ev_dirty: MessageWriter<'w, SubChunkNeedRemeshEvent>,
    place_ev: MessageWriter<'w, BlockPlaceByPlayerEvent>,
}

impl Plugin for BlockEventHandler {
    /// Builds this component for the `logic::events::block_event_handler` module.
    fn build(&self, app: &mut App) {
        app.init_resource::<StructureRuntimeState>();
        app.init_resource::<StructureMiningState>();
        app.add_systems(
            Update,
            (
                sync_structures_for_loaded_chunks.in_set(VoxelStage::WorldEdit),
                (block_break_handler, sync_mining_overlay)
                    .chain()
                    .in_set(VoxelStage::WorldEdit),
                block_place_handler.in_set(VoxelStage::WorldEdit),
            )
                .run_if(in_state(AppState::InGame(InGameStates::Game))),
        );
        app.add_systems(
            OnExit(AppState::InGame(InGameStates::Game)),
            cleanup_structure_runtime_on_exit,
        );
    }
}

fn cleanup_structure_runtime_on_exit(
    mut commands: Commands,
    mut runtime: ResMut<StructureRuntimeState>,
    mut structure_mining: ResMut<StructureMiningState>,
) {
    for (_, entity) in runtime.spawned_entities.drain() {
        safe_despawn_entity(&mut commands, entity);
    }
    runtime.entity_to_key.clear();
    runtime.records_by_chunk.clear();
    runtime.loaded_chunks.clear();
    structure_mining.target = None;
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
    structure_cancel: ActiveStructureCancelState,
    structure_break_deps: StructureBreakDeps,
    break_world: BreakWorldMut,
    break_input_context: BreakInputContext,
) {
    let BreakInputContext {
        multiplayer_connection,
        ui_state,
    } = break_input_context;
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
        return;
    }

    let multiplayer_connected = multiplayer_connection
        .as_ref()
        .is_some_and(|state| state.connected);

    if game_mode.0.eq(&GameMode::Spectator) {
        structure_mining.target = None;
        return;
    }
    if !buttons.pressed(MouseButton::Left) {
        state.target = None;
        structure_mining.target = None;
        return;
    }

    if let Some(structure_hit) = selection.structure_hit {
        state.target = None;
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
        if requirement.item_id == 0 || requirement.count == 0 {
            continue;
        }
        spawn_world_item_with_motion(
            commands,
            meshes,
            registry,
            item_registry,
            requirement.item_id,
            requirement.count,
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
            && normalize_rotation_quarters(entry.rotation_quarters as i32) == key.rotation_quarters)
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
    structure_deps: StructurePlacementDeps,
    buttons: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    sel: Res<SelectionState>,
    selected: Res<SelectedBlock>,
    registry: Res<BlockRegistry>,
    item_registry: Res<ItemRegistry>,
    game_mode: Res<GameModeState>,
    hotbar_selection: Option<Res<HotbarSelectionState>>,
    ui_state: Option<Res<UiInteractionState>>,
    q_player_controls: Query<&FpsController, With<Player>>,
    q_player_cam: Query<&GlobalTransform, With<PlayerCamera>>,
    q_fallback_cam: Query<&GlobalTransform, (With<Camera3d>, Without<PlayerCamera>)>,
    q_structures: Query<&PlacedStructureMetadata>,
    world_deps: PlacementWorldDeps,
) {
    let StructurePlacementDeps {
        mut commands,
        asset_server,
        structure_recipe_registry,
        mut active_structure_recipe,
        mut active_structure_placement,
        mut open_structure_menu_requests,
        mut open_workbench_menu_requests,
    } = structure_deps;
    let PlacementWorldDeps {
        mut inventory,
        mut fluids,
        mut chunk_map,
        multiplayer_connection,
        ws,
        mut region_cache,
        mut structure_runtime,
        mut ev_dirty,
        mut place_ev,
    } = world_deps;

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

    let shift_held = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
    if let Some(structure_hit) = sel.structure_hit
        && let Ok(meta) = q_structures.get(structure_hit.entity)
        && structure_has_workbench_ui(meta)
        && !shift_held
    {
        // Block interaction UI has priority over hammer right-click interaction.
        active_structure_recipe.selected_recipe_name = None;
        active_structure_placement.rotation_quarters = 0;
        open_workbench_menu_requests.write(OpenWorkbenchMenuRequest);
        return;
    }

    let held_item_id = selected_hotbar_item_id(&inventory, hotbar_selection.as_deref());
    let holding_hammer = held_item_id
        .and_then(|item_id| item_registry.def_opt(item_id))
        .is_some_and(|item| item.localized_name == "oplexa:hammer" || item.key == "hammer");
    if holding_hammer {
        let Some(structure_recipe_registry) = structure_recipe_registry.as_ref() else {
            return;
        };

        if let Some(active_recipe_name) = active_structure_recipe.selected_recipe_name.as_deref() {
            let Some(recipe) = structure_recipe_registry.recipe_by_name(active_recipe_name) else {
                active_structure_recipe.selected_recipe_name = None;
                active_structure_placement.rotation_quarters = 0;
                return;
            };
            let Some(hit) = sel.hit else {
                return;
            };
            let rotation_quarters =
                normalize_rotation_quarters(active_structure_placement.rotation_quarters);
            let place_origin = resolve_structure_place_origin(hit, &chunk_map, &registry);
            if !can_place_structure_recipe_at(
                place_origin,
                recipe,
                rotation_quarters,
                &chunk_map,
                &registry,
            ) {
                return;
            }

            if !inventory_has_structure_requirements(&inventory, &recipe.requirements) {
                return;
            }
            if !consume_structure_requirements_from_inventory(&mut inventory, &recipe.requirements)
            {
                return;
            }

            let structure_entity = spawn_structure_model_entity(
                &mut commands,
                &asset_server,
                recipe,
                place_origin,
                rotation_quarters,
            );
            register_structure_in_runtime(
                &mut structure_runtime,
                structure_entity,
                recipe,
                place_origin,
                rotation_quarters,
                multiplayer_connection.uses_local_save_data(),
                ws.as_deref(),
                region_cache.as_deref_mut(),
            );
            active_structure_recipe.selected_recipe_name = None;
            active_structure_placement.rotation_quarters = 0;
            return;
        }

        active_structure_placement.rotation_quarters = 0;
        open_structure_menu_requests.write(OpenStructureBuildMenuRequest);
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
    let hit = if let Some(hit) = sel.hit {
        hit
    } else if let Some(structure_hit) = sel.structure_hit {
        let cam = q_player_cam
            .iter()
            .next()
            .or_else(|| q_fallback_cam.iter().next());
        let Some(cam_tf) = cam else {
            return;
        };
        let Some(hit) = build_structure_surface_hit(structure_hit.entity, cam_tf, &q_structures)
        else {
            return;
        };
        hit
    } else {
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
    let mut world_pos = placement.world_pos;
    let mut place_into_stacked = placement.place_into_stacked;
    let hit_primary_id = get_block_world(&chunk_map, hit.block_pos);
    if hit_primary_id != 0 && registry.is_overridable(hit_primary_id) {
        world_pos = hit.block_pos;
        place_into_stacked = false;
    }
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
                current != 0 && !registry.is_overridable(current) && ch.get_stacked(lx, ly, lz) == 0
            } else {
                current == 0 || registry.is_overridable(current)
            }
        })
        .unwrap_or(false);
    if !can_place {
        return;
    }
    if world_cell_intersects_structure(world_pos, &q_structures) {
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
            access.set_stacked(0);
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

/// Runs the `selected_hotbar_tool` routine for selected hotbar tool in the `logic::events::block_event_handler` module.
fn selected_hotbar_tool(
    inventory: &PlayerInventory,
    hotbar_selection: Option<&HotbarSelectionState>,
    item_registry: &ItemRegistry,
) -> Option<crate::core::inventory::items::ToolDef> {
    let item_id = selected_hotbar_item_id(inventory, hotbar_selection)?;
    item_registry.tool_for_item(item_id)
}

fn resolve_structure_place_origin(
    hit: crate::core::entities::player::block_selection::BlockHit,
    chunk_map: &ChunkMap,
    registry: &BlockRegistry,
) -> IVec3 {
    let hit_primary_id = get_block_world(chunk_map, hit.block_pos);
    if hit_primary_id != 0 && registry.is_overridable(hit_primary_id) {
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
    q_structures: &Query<&PlacedStructureMetadata>,
) -> bool {
    let cell_min = world_pos.as_vec3() * VOXEL_SIZE;
    let cell_max = cell_min + Vec3::splat(VOXEL_SIZE);
    const EPS: f32 = 0.0001;

    q_structures.iter().any(|meta| {
        let half = meta.selection_size_world * 0.5;
        let structure_min = meta.selection_center_world - half;
        let structure_max = meta.selection_center_world + half;

        cell_min.x < structure_max.x - EPS
            && cell_max.x > structure_min.x + EPS
            && cell_min.y < structure_max.y - EPS
            && cell_max.y > structure_min.y + EPS
            && cell_min.z < structure_max.z - EPS
            && cell_max.z > structure_min.z + EPS
    })
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

fn build_structure_surface_hit(
    structure_entity: Entity,
    cam_tf: &GlobalTransform,
    q_structures: &Query<&PlacedStructureMetadata>,
) -> Option<crate::core::entities::player::block_selection::BlockHit> {
    let meta = q_structures.get(structure_entity).ok()?;
    let half = meta.selection_size_world * 0.5;
    let bounds_min = meta.selection_center_world - half;
    let bounds_max = meta.selection_center_world + half;

    let origin = cam_tf.translation();
    let direction: Vec3 = cam_tf.forward().into();
    let (hit_t, hit_normal) = ray_hit_aabb_with_normal(origin, direction, bounds_min, bounds_max)?;
    let hit_world = origin + direction * hit_t;
    let inward_probe = hit_world - hit_normal * 0.002;
    let outward_probe = hit_world + hit_normal * 0.002;

    let block_pos = IVec3::new(
        inward_probe.x.floor() as i32,
        inward_probe.y.floor() as i32,
        inward_probe.z.floor() as i32,
    );
    let place_pos = IVec3::new(
        outward_probe.x.floor() as i32,
        outward_probe.y.floor() as i32,
        outward_probe.z.floor() as i32,
    );
    let hit_local = Vec3::new(
        hit_world.x - block_pos.x as f32,
        hit_world.y - block_pos.y as f32,
        hit_world.z - block_pos.z as f32,
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
        face: face_from_normal(hit_normal),
        hit_local,
        place_pos,
    })
}

fn ray_hit_aabb_with_normal(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<(f32, Vec3)> {
    const EPS: f32 = 1e-6;
    let axes = [
        (origin.x, dir.x, min.x, max.x, Vec3::X),
        (origin.y, dir.y, min.y, max.y, Vec3::Y),
        (origin.z, dir.z, min.z, max.z, Vec3::Z),
    ];

    let mut t_min = f32::NEG_INFINITY;
    let mut t_max = f32::INFINITY;
    let mut near_normal = Vec3::ZERO;
    let mut far_normal = Vec3::ZERO;

    for (o, d, min_a, max_a, axis) in axes {
        if d.abs() < EPS {
            if o < min_a || o > max_a {
                return None;
            }
            continue;
        }
        let inv = 1.0 / d;
        let mut t1 = (min_a - o) * inv;
        let mut t2 = (max_a - o) * inv;
        let mut n1 = -axis;
        let mut n2 = axis;
        if t1 > t2 {
            std::mem::swap(&mut t1, &mut t2);
            std::mem::swap(&mut n1, &mut n2);
        }
        if t1 > t_min {
            t_min = t1;
            near_normal = n1;
        }
        if t2 < t_max {
            t_max = t2;
            far_normal = n2;
        }
        if t_min > t_max {
            return None;
        }
    }

    if t_max < 0.0 {
        return None;
    }
    if t_min >= 0.0 {
        Some((t_min, near_normal))
    } else {
        Some((t_max, far_normal))
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

fn inventory_has_structure_requirements(
    inventory: &PlayerInventory,
    requirements: &[BuildingMaterialRequirement],
) -> bool {
    requirements.iter().all(|required| {
        let mut available = 0u32;
        for slot in &inventory.slots {
            if slot.is_empty() || slot.item_id != required.item_id {
                continue;
            }
            available = available.saturating_add(slot.count as u32);
        }
        available >= required.count as u32
    })
}

fn consume_structure_requirements_from_inventory(
    inventory: &mut PlayerInventory,
    requirements: &[BuildingMaterialRequirement],
) -> bool {
    if !inventory_has_structure_requirements(inventory, requirements) {
        return false;
    }

    for required in requirements {
        let mut missing = required.count;
        for slot in &mut inventory.slots {
            if missing == 0 {
                break;
            }
            if slot.is_empty() || slot.item_id != required.item_id {
                continue;
            }
            let take = slot.count.min(missing);
            slot.count -= take;
            missing -= take;
            if slot.count == 0 {
                slot.item_id = 0;
            }
        }
    }

    true
}

fn sync_structures_for_loaded_chunks(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    chunk_map: Res<ChunkMap>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
    structure_recipe_registry: Option<Res<BuildingStructureRecipeRegistry>>,
    ws: Option<Res<WorldSave>>,
    mut region_cache: Option<ResMut<RegionCache>>,
    mut runtime: ResMut<StructureRuntimeState>,
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
        runtime.records_by_chunk.insert(coord, entries.clone());

        for entry in entries {
            let place_origin = IVec3::new(
                entry.place_origin[0],
                entry.place_origin[1],
                entry.place_origin[2],
            );
            let rotation_quarters = normalize_rotation_quarters(entry.rotation_quarters as i32);
            let Some(recipe) = structure_recipe_registry.recipe_by_name(entry.recipe_name.as_str())
            else {
                continue;
            };
            let key =
                placed_structure_key(coord, recipe.name.clone(), place_origin, rotation_quarters);
            if runtime.spawned_entities.contains_key(&key) {
                continue;
            }
            let entity = spawn_structure_model_entity(
                &mut commands,
                &asset_server,
                recipe,
                place_origin,
                rotation_quarters,
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

fn register_structure_in_runtime(
    runtime: &mut StructureRuntimeState,
    structure_entity: Entity,
    recipe: &BuildingStructureRecipe,
    place_origin: IVec3,
    rotation_quarters: u8,
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
    );
    runtime
        .spawned_entities
        .insert(key.clone(), structure_entity);
    runtime.entity_to_key.insert(structure_entity, key);

    let entries = runtime.records_by_chunk.entry(origin_chunk).or_default();
    if !entries.iter().any(|entry| {
        entry.recipe_name == recipe.name
            && entry.place_origin == [place_origin.x, place_origin.y, place_origin.z]
            && normalize_rotation_quarters(entry.rotation_quarters as i32) == rotation_quarters
    }) {
        entries.push(StructureRegionEntry {
            recipe_name: recipe.name.clone(),
            place_origin: [place_origin.x, place_origin.y, place_origin.z],
            rotation_quarters,
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

fn placed_structure_key(
    origin_chunk: IVec2,
    recipe_name: String,
    place_origin: IVec3,
    rotation_quarters: u8,
) -> PlacedStructureKey {
    PlacedStructureKey {
        origin_chunk,
        recipe_name,
        place_origin,
        rotation_quarters,
    }
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

fn spawn_structure_model_entity(
    commands: &mut Commands,
    asset_server: &AssetServer,
    recipe: &BuildingStructureRecipe,
    place_origin: IVec3,
    rotation_quarters: u8,
) -> Entity {
    let model_rotation_quarters = normalize_rotation_quarters(
        rotation_quarters as i32 + recipe.model_meta.model_rotation_quarters as i32,
    );
    let model_rotation =
        Quat::from_rotation_y(-(model_rotation_quarters as f32) * std::f32::consts::FRAC_PI_2);
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
    let (origin_chunk, _) = world_to_chunk_xz(place_origin.x, place_origin.z);

    let structure_entity = commands
        .spawn((
            Name::new(format!("Structure:{}", recipe.name)),
            PlacedStructureMetadata {
                recipe_name: recipe.name.clone(),
                stats: recipe.model_meta.stats.clone(),
                place_origin,
                rotation_quarters,
                origin_chunk,
                drop_requirements: recipe.requirements.clone(),
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
            commands
                .entity(structure_entity)
                .insert(AsyncSceneCollider {
                    shape: Some(ComputedColliderShape::TriMesh(mesh_flags)),
                    named_shapes: default(),
                });
        }
    }
    structure_entity
}

fn structure_model_translation(
    recipe: &BuildingStructureRecipe,
    place_origin: IVec3,
    placement_rotation_quarters: u8,
    model_rotation_quarters: u8,
) -> Vec3 {
    match recipe.model_meta.model_anchor {
        BuildingModelAnchor::Center => {
            // Center anchor follows occupied recipe space (player preview / placement logic).
            let recipe_space =
                rotated_structure_space(recipe.space, placement_rotation_quarters).as_vec3();
            Vec3::new(
                (place_origin.x as f32 + recipe_space.x * 0.5) * VOXEL_SIZE,
                (place_origin.y as f32 + recipe_space.y * 0.5) * VOXEL_SIZE,
                (place_origin.z as f32 + recipe_space.z * 0.5) * VOXEL_SIZE,
            )
        }
        BuildingModelAnchor::MinCorner => {
            let offset = rotated_model_corner_offset(recipe.space, model_rotation_quarters);
            Vec3::new(
                (place_origin.x as f32 + offset.x) * VOXEL_SIZE,
                (place_origin.y as f32) * VOXEL_SIZE,
                (place_origin.z as f32 + offset.z) * VOXEL_SIZE,
            )
        }
    }
}

#[inline]
fn rotated_model_corner_offset(space: UVec3, rotation_quarters: u8) -> Vec3 {
    match rotation_quarters % 4 {
        0 => Vec3::ZERO,
        1 => Vec3::new(space.z as f32, 0.0, 0.0),
        2 => Vec3::new(space.x as f32, 0.0, space.z as f32),
        _ => Vec3::new(0.0, 0.0, space.x as f32),
    }
}

#[inline]
fn normalize_rotation_quarters(raw: i32) -> u8 {
    raw.rem_euclid(4) as u8
}

#[inline]
fn rotated_structure_space(space: UVec3, rotation_quarters: u8) -> UVec3 {
    if rotation_quarters % 2 == 0 {
        space
    } else {
        UVec3::new(space.z, space.y, space.x)
    }
}

#[inline]
fn rotated_structure_offset(
    local_x: i32,
    local_z: i32,
    size_x: i32,
    size_z: i32,
    rotation_quarters: u8,
) -> (i32, i32) {
    match rotation_quarters % 4 {
        0 => (local_x, local_z),
        1 => (local_z, size_x - 1 - local_x),
        2 => (size_x - 1 - local_x, size_z - 1 - local_z),
        _ => (size_z - 1 - local_z, local_x),
    }
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
