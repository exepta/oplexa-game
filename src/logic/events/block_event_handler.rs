use crate::core::entities::player::block_selection::SelectionState;
use crate::core::entities::player::inventory::PlayerInventory;
use crate::core::entities::player::{FpsController, GameMode, GameModeState, Player};
use crate::core::events::block::block_player_events::{
    BlockBreakByPlayerEvent, BlockPlaceByPlayerEvent,
};
use crate::core::events::chunk_events::SubChunkNeedRemeshEvent;
use crate::core::events::ui_events::{OpenStructureBuildMenuRequest, OpenWorkbenchMenuRequest};
use crate::core::inventory::items::{
    ItemId, ItemRegistry, block_requirement_for_id, can_drop_from_block, mining_speed_multiplier,
    spawn_world_item_for_block_break, spawn_world_item_with_motion,
};
use crate::core::inventory::recipe::{
    ActiveStructurePlacementState, ActiveStructureRecipeState, BuildingMaterialRequirement,
    BuildingMaterialRequirementSource, BuildingModelAnchor, BuildingStructureBlockRegistration,
    BuildingStructureColliderSource, BuildingStructureRecipe, BuildingStructureRecipeRegistry,
    BuildingStructureTextureBinding, BuildingStructureTextureSource,
};
use crate::core::multiplayer::MultiplayerConnectionState;
use crate::core::states::states::{AppState, InGameStates};
use crate::core::ui::{HotbarSelectionState, UiInteractionState};
use crate::core::world::block::*;
use crate::core::world::chunk::*;
use crate::core::world::chunk_dimension::*;
use crate::core::world::fluid::FluidMap;
use crate::core::world::save::{
    RegionCache, StructureRegionDropItem, StructureRegionEntry, TAG_STR1, WorldSave,
    container_find, container_upsert, decode_structure_entries, encode_structure_entries,
};
use crate::core::world::{mark_dirty_block_and_neighbors, world_access_mut};
use crate::generator::chunk::chunk_utils::safe_despawn_entity;
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::ecs::relationship::RelatedSpawnerCommands;
use bevy::ecs::system::SystemParam;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::math::Affine2;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy_rapier3d::prelude::{
    AsyncSceneCollider, Collider, ComputedColliderShape, QueryFilter, ReadRapierContext, RigidBody,
    TriMeshFlags,
};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

#[derive(Component, Clone, Copy)]
struct StructureStyleSourceItem {
    item_id: ItemId,
}

#[derive(Component, Clone, Default)]
struct StructureTextureBindings {
    entries: Vec<BuildingStructureTextureBinding>,
}

#[derive(Component, Default)]
struct StructureStyleMaterialPending;

#[derive(Component, Default)]
struct StructureMeshColliderNameFilterPending;

#[derive(Component, Default)]
struct StructureMeshColliderCleanupPending;

#[derive(Component, Clone)]
#[allow(dead_code)]
pub(crate) struct PlacedStructureMetadata {
    pub recipe_name: String,
    pub stats: BlockStats,
    pub place_origin: IVec3,
    pub rotation_quarters: u8,
    pub rotation_steps: u8,
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
    rotation_steps: u8,
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

#[derive(Resource, Default)]
struct MultiplayerStructureReconcileQueue {
    pending_chunks: HashSet<IVec2>,
}

const MULTIPLAYER_STRUCTURE_RECONCILE_CHUNKS_PER_FRAME: usize = 2;

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
        app.init_resource::<MultiplayerStructureReconcileQueue>();
        app.add_systems(
            Update,
            (
                sync_structures_for_loaded_chunks.in_set(VoxelStage::WorldEdit),
                enforce_block_texture_nearest_sampler_system.in_set(VoxelStage::WorldEdit),
                configure_structure_mesh_collider_name_filters.in_set(VoxelStage::WorldEdit),
                cleanup_structure_none_mesh_colliders.in_set(VoxelStage::WorldEdit),
                apply_structure_style_material_system.in_set(VoxelStage::WorldEdit),
                (block_break_handler, sync_mining_overlay)
                    .chain()
                    .in_set(VoxelStage::WorldEdit),
                block_place_handler.in_set(VoxelStage::WorldEdit),
                (
                    collect_multiplayer_structure_reconcile_chunks,
                    reconcile_multiplayer_structure_visuals,
                )
                    .chain()
                    .in_set(VoxelStage::Meshing),
            )
                .run_if(in_state(AppState::InGame(InGameStates::Game))),
        );
        app.add_systems(
            OnExit(AppState::InGame(InGameStates::Game)),
            cleanup_structure_runtime_on_exit,
        );
    }
}

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
    rapier_context: ReadRapierContext,
    q_player_controls: Query<&FpsController, With<Player>>,
    q_structures: Query<&PlacedStructureMetadata>,
    q_structure_parents: Query<&ChildOf>,
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
    if let Some(hit) = sel.hit
        && block_has_workbench_ui(get_block_world(&chunk_map, hit.block_pos), &registry)
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
            let rotation_steps =
                normalize_rotation_steps(active_structure_placement.rotation_quarters) & !1;
            let rotation_quarters = rotation_steps_to_placement_quarters(rotation_steps);
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

            let Some(consumed_requirements) = consume_structure_requirements_from_inventory(
                &mut inventory,
                &recipe.requirements,
                &item_registry,
            ) else {
                return;
            };
            let style_source_item_id = consumed_requirements
                .style_source_item_id
                .or_else(|| resolve_default_structure_style_item_id(recipe, &item_registry));
            let style_source_block_id = style_source_item_id
                .and_then(|item_id| item_registry.block_for_item(item_id))
                .filter(|block_id| *block_id != 0);

            if !multiplayer_connection.uses_local_save_data() {
                let Some(registered_block_id) =
                    structure_runtime_placeholder_block_id(recipe, &registry, rotation_quarters)
                else {
                    bevy::log::warn!(
                        "Structure recipe '{}' has no registered block id for rotation {}; cannot place in multiplayer.",
                        recipe.name,
                        rotation_quarters
                    );
                    return;
                };

                let (chunk_coord, local) = world_to_chunk_xz(place_origin.x, place_origin.z);
                let lx = local.x.clamp(0, (CX as i32 - 1) as u32) as usize;
                let lz = local.y.clamp(0, (CZ as i32 - 1) as u32) as usize;
                let ly = (place_origin.y - Y_MIN).clamp(0, CY as i32 - 1) as usize;
                if let Some(fc) = fluids.0.get_mut(&chunk_coord) {
                    fc.set(lx, ly, lz, false);
                }
                if let Some(mut access) = world_access_mut(&mut chunk_map, place_origin) {
                    access.set(registered_block_id);
                    access.set_stacked(style_source_block_id.unwrap_or(0));
                } else {
                    return;
                }
                mark_dirty_block_and_neighbors(&mut chunk_map, place_origin, &mut ev_dirty);

                let name = registry
                    .name_opt(registered_block_id)
                    .unwrap_or("")
                    .to_string();
                place_ev.write(BlockPlaceByPlayerEvent {
                    location: place_origin,
                    block_id: registered_block_id,
                    stacked_block_id: style_source_block_id.unwrap_or(0),
                    block_name: name,
                });

                let structure_entity = spawn_structure_model_entity(
                    &mut commands,
                    &asset_server,
                    recipe,
                    place_origin,
                    rotation_quarters,
                    rotation_steps,
                    consumed_requirements.drop_requirements.clone(),
                    style_source_item_id,
                );
                clear_props_within_structure_volume(
                    place_origin,
                    recipe,
                    rotation_quarters,
                    &mut chunk_map,
                    &registry,
                    &mut ev_dirty,
                );
                register_structure_in_runtime(
                    &mut structure_runtime,
                    structure_entity,
                    recipe,
                    place_origin,
                    rotation_quarters,
                    rotation_steps,
                    style_source_item_id,
                    consumed_requirements.drop_requirements.as_slice(),
                    &item_registry,
                    multiplayer_connection.uses_local_save_data(),
                    ws.as_deref(),
                    region_cache.as_deref_mut(),
                );

                active_structure_recipe.selected_recipe_name = None;
                active_structure_placement.rotation_quarters = 0;
                return;
            }

            let structure_entity = spawn_structure_model_entity(
                &mut commands,
                &asset_server,
                recipe,
                place_origin,
                rotation_quarters,
                rotation_steps,
                consumed_requirements.drop_requirements.clone(),
                style_source_item_id,
            );
            clear_props_within_structure_volume(
                place_origin,
                recipe,
                rotation_quarters,
                &mut chunk_map,
                &registry,
                &mut ev_dirty,
            );
            register_structure_in_runtime(
                &mut structure_runtime,
                structure_entity,
                recipe,
                place_origin,
                rotation_quarters,
                rotation_steps,
                style_source_item_id,
                consumed_requirements.drop_requirements.as_slice(),
                &item_registry,
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
        let Some(hit) = build_structure_surface_hit(structure_hit, &q_structures) else {
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
    if world_cell_intersects_structure(
        world_pos,
        &rapier_context,
        &q_structures,
        &q_structure_parents,
    ) {
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

    let existing_primary_id = get_block_world(&chunk_map, world_pos);
    let (network_block_id, network_stacked_block_id) = if place_into_stacked {
        (existing_primary_id, place_id)
    } else {
        (place_id, 0)
    };

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
        block_id: network_block_id,
        stacked_block_id: network_stacked_block_id,
        block_name: name,
    });
}

include!("block_event_handler/placement.rs");

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
        if uses_local_save_data {
            runtime.records_by_chunk.remove(&coord);
        }

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
    registry: Res<BlockRegistry>,
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

    let mut recipe_by_block_id: HashMap<u16, (&BuildingStructureRecipe, u8, u8)> = HashMap::new();
    for recipe in &structure_recipe_registry.recipes {
        for rotation_quarters in 0..4u8 {
            let Some(block_id) =
                structure_runtime_placeholder_block_id(recipe, &registry, rotation_quarters)
            else {
                continue;
            };
            let rotation_steps = normalize_rotation_steps((rotation_quarters as i32) * 2);
            recipe_by_block_id.insert(block_id, (recipe, rotation_quarters, rotation_steps));
        }
    }
    if recipe_by_block_id.is_empty() {
        queue.pending_chunks.clear();
        return;
    }

    let process_limit = MULTIPLAYER_STRUCTURE_RECONCILE_CHUNKS_PER_FRAME.max(1);
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
        let Some(chunk) = chunk_map.chunks.get(&coord) else {
            continue;
        };

        let mut expected_keys: HashMap<
            PlacedStructureKey,
            (&BuildingStructureRecipe, IVec3, Option<ItemId>),
        > = HashMap::new();
        for y in 0..CY {
            for z in 0..CZ {
                for x in 0..CX {
                    let block_id = chunk.get(x, y, z);
                    let Some((recipe, rotation_quarters, rotation_steps)) =
                        recipe_by_block_id.get(&block_id).copied()
                    else {
                        continue;
                    };

                    let place_origin = IVec3::new(
                        coord.x * CX as i32 + x as i32,
                        Y_MIN + y as i32,
                        coord.y * CZ as i32 + z as i32,
                    );
                    let key = placed_structure_key(
                        coord,
                        recipe.name.clone(),
                        place_origin,
                        rotation_quarters,
                        rotation_steps,
                    );
                    let style_source_item_id = item_registry
                        .item_for_block(chunk.get_stacked(x, y, z))
                        .filter(|item_id| *item_id != 0);
                    expected_keys.entry(key).or_insert((
                        recipe,
                        place_origin,
                        style_source_item_id,
                    ));
                }
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
    let (origin_chunk, _) = world_to_chunk_xz(place_origin.x, place_origin.z);

    let structure_entity = commands
        .spawn((
            Name::new(format!("Structure:{}", recipe.name)),
            PlacedStructureMetadata {
                recipe_name: recipe.name.clone(),
                stats: recipe.model_meta.stats.clone(),
                place_origin,
                rotation_quarters,
                rotation_steps,
                origin_chunk,
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

fn configure_structure_mesh_collider_name_filters(
    mut commands: Commands,
    scene_spawner: Res<bevy::scene::SceneSpawner>,
    children: Query<&Children>,
    mesh_names: Query<&Name, With<Mesh3d>>,
    mut q_pending: Query<
        (Entity, &bevy::scene::SceneInstance, &mut AsyncSceneCollider),
        With<StructureMeshColliderNameFilterPending>,
    >,
) {
    for (structure_entity, scene_instance, mut async_scene_collider) in &mut q_pending {
        if !scene_spawner.instance_is_ready(**scene_instance) {
            continue;
        }

        for child_entity in children.iter_descendants(structure_entity) {
            let Ok(name) = mesh_names.get(child_entity) else {
                continue;
            };
            if !name.as_str().to_ascii_lowercase().contains("none") {
                continue;
            }
            async_scene_collider
                .named_shapes
                .insert(name.as_str().to_string(), None);
        }

        commands
            .entity(structure_entity)
            .remove::<StructureMeshColliderNameFilterPending>();
    }
}

fn cleanup_structure_none_mesh_colliders(
    mut commands: Commands,
    scene_spawner: Res<bevy::scene::SceneSpawner>,
    q_children: Query<&Children>,
    q_names: Query<&Name>,
    q_parents: Query<&ChildOf>,
    q_colliders: Query<(), With<Collider>>,
    q_pending: Query<
        (Entity, &bevy::scene::SceneInstance, Has<AsyncSceneCollider>),
        With<StructureMeshColliderCleanupPending>,
    >,
) {
    for (structure_entity, scene_instance, has_async_scene_collider) in &q_pending {
        if !scene_spawner.instance_is_ready(**scene_instance) {
            continue;
        }
        // Wait until Rapier has finished generating child colliders.
        if has_async_scene_collider {
            continue;
        }

        for child_entity in q_children.iter_descendants(structure_entity) {
            if q_colliders.get(child_entity).is_err() {
                continue;
            }
            if !entity_or_ancestor_name_contains_none(
                child_entity,
                structure_entity,
                &q_names,
                &q_parents,
            ) {
                continue;
            }
            commands.entity(child_entity).remove::<Collider>();
        }

        commands
            .entity(structure_entity)
            .remove::<StructureMeshColliderCleanupPending>();
    }
}

fn entity_or_ancestor_name_contains_none(
    entity: Entity,
    root_entity: Entity,
    q_names: &Query<&Name>,
    q_parents: &Query<&ChildOf>,
) -> bool {
    let mut current = entity;
    loop {
        if q_names
            .get(current)
            .is_ok_and(|name| name.as_str().to_ascii_lowercase().contains("none"))
        {
            return true;
        }
        if current == root_entity {
            return false;
        }
        let Ok(parent) = q_parents.get(current) else {
            return false;
        };
        current = parent.parent();
    }
}

fn apply_structure_style_material_system(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    meshes: Res<Assets<Mesh>>,
    item_registry: Res<ItemRegistry>,
    block_registry: Res<BlockRegistry>,
    q_pending: Query<
        (
            Entity,
            Option<&StructureStyleSourceItem>,
            Option<&StructureTextureBindings>,
        ),
        With<StructureStyleMaterialPending>,
    >,
    q_children: Query<&Children>,
    q_names: Query<&Name>,
    q_parents: Query<&ChildOf>,
    q_meshes: Query<&Mesh3d>,
    mut q_mesh_materials: Query<&mut MeshMaterial3d<StandardMaterial>>,
) {
    for (structure_entity, style_source, texture_bindings) in &q_pending {
        let style_source_item_id = style_source
            .map(|style_source| style_source.item_id)
            .filter(|item_id| *item_id != 0);
        let style_material = style_source_item_id.and_then(|item_id| {
            resolve_structure_style_material_handle(item_id, &item_registry, &block_registry)
        });
        let apply_stats = apply_materials_to_structure_descendants(
            structure_entity,
            style_source_item_id,
            style_material.as_ref(),
            texture_bindings.map(|bindings| bindings.entries.as_slice()),
            &asset_server,
            &mut materials,
            &mut images,
            &meshes,
            &item_registry,
            &block_registry,
            &q_children,
            &q_names,
            &q_parents,
            &q_meshes,
            &mut q_mesh_materials,
        );

        if apply_stats.mesh_count > 0 || (style_material.is_none() && texture_bindings.is_none()) {
            commands
                .entity(structure_entity)
                .remove::<StructureStyleMaterialPending>();
        }
    }
}

fn resolve_structure_style_material_handle(
    style_source_item_id: ItemId,
    item_registry: &ItemRegistry,
    block_registry: &BlockRegistry,
) -> Option<Handle<StandardMaterial>> {
    if style_source_item_id == 0 {
        return None;
    }
    let style_item_id = item_registry
        .related_item_in_group(style_source_item_id, "planks")
        .unwrap_or(style_source_item_id);
    let block_id = item_registry
        .block_for_item(style_item_id)
        .or_else(|| item_registry.block_for_item(style_source_item_id))?;
    block_registry
        .def_opt(block_id)
        .map(|block| block.material.clone())
}

#[derive(Clone, Copy, Debug, Default)]
struct StructureMaterialApplyStats {
    mesh_count: usize,
    changed: usize,
}

fn apply_materials_to_structure_descendants(
    structure_entity: Entity,
    style_source_item_id: Option<ItemId>,
    style_material: Option<&Handle<StandardMaterial>>,
    texture_bindings: Option<&[BuildingStructureTextureBinding]>,
    asset_server: &AssetServer,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    meshes: &Assets<Mesh>,
    item_registry: &ItemRegistry,
    block_registry: &BlockRegistry,
    q_children: &Query<&Children>,
    q_names: &Query<&Name>,
    q_parents: &Query<&ChildOf>,
    q_meshes: &Query<&Mesh3d>,
    q_mesh_materials: &mut Query<&mut MeshMaterial3d<StandardMaterial>>,
) -> StructureMaterialApplyStats {
    let mut stats = StructureMaterialApplyStats::default();
    let mut stack: Vec<Entity> = Vec::new();
    let mut uv_bounds_cache: HashMap<AssetId<Mesh>, Option<[[f32; 2]; 2]>> = HashMap::new();
    if let Ok(children) = q_children.get(structure_entity) {
        stack.extend(children.iter());
    }

    while let Some(entity) = stack.pop() {
        if let Ok(mut mesh_material) = q_mesh_materials.get_mut(entity) {
            stats.mesh_count += 1;
            let mesh_name_haystack =
                mesh_name_haystack(entity, structure_entity, q_names, q_parents);
            let uv_bounds =
                mesh_uv_bounds_for_entity(entity, q_meshes, meshes, &mut uv_bounds_cache);
            let target_material = texture_bindings.and_then(|bindings| {
                resolve_texture_binding_material_for_mesh(
                    bindings,
                    mesh_name_haystack.as_str(),
                    style_source_item_id,
                    style_material,
                    asset_server,
                    materials,
                    images,
                    item_registry,
                    block_registry,
                    uv_bounds,
                )
            });
            if let Some(target_material) = target_material.or_else(|| style_material.cloned())
                && mesh_material.0 != target_material
            {
                mesh_material.0 = target_material;
                stats.changed += 1;
            }
        }
        if let Ok(children) = q_children.get(entity) {
            stack.extend(children.iter());
        }
    }

    stats
}

fn resolve_texture_binding_material_for_mesh(
    texture_bindings: &[BuildingStructureTextureBinding],
    mesh_name_haystack: &str,
    style_source_item_id: Option<ItemId>,
    style_material: Option<&Handle<StandardMaterial>>,
    asset_server: &AssetServer,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    item_registry: &ItemRegistry,
    block_registry: &BlockRegistry,
    uv_bounds: Option<[[f32; 2]; 2]>,
) -> Option<Handle<StandardMaterial>> {
    let binding = texture_bindings
        .iter()
        .find(|binding| mesh_name_haystack.contains(binding.mesh_name_contains.as_str()))?;

    let resolved = match &binding.source {
        BuildingStructureTextureSource::Group { group, tile } => resolve_group_texture_material(
            style_source_item_id,
            style_material,
            group.as_str(),
            *tile,
            binding.uv_repeat,
            materials,
            images,
            item_registry,
            block_registry,
            uv_bounds,
        )?,
        BuildingStructureTextureSource::DirectPath { asset_path } => {
            let mut material = style_material
                .and_then(|handle| materials.get(handle))
                .cloned()
                .unwrap_or(StandardMaterial {
                    metallic: 0.0,
                    perceptual_roughness: 1.0,
                    reflectance: 0.0,
                    ..default()
                });
            let texture_handle: Handle<Image> = asset_server.load(asset_path.clone());
            apply_nearest_sampler_to_texture_handle(images, &texture_handle, true);
            material.base_color_texture = Some(texture_handle);
            material.uv_transform =
                build_uv_transform([0.0, 0.0], [1.0, 1.0], binding.uv_repeat, uv_bounds);
            materials.add(material)
        }
    };
    Some(resolved)
}

fn mesh_name_haystack(
    entity: Entity,
    root_entity: Entity,
    q_names: &Query<&Name>,
    q_parents: &Query<&ChildOf>,
) -> String {
    let mut names = Vec::<String>::new();
    let mut current = entity;
    loop {
        if let Ok(name) = q_names.get(current) {
            names.push(name.as_str().to_ascii_lowercase());
        }
        if current == root_entity {
            break;
        }
        let Ok(parent) = q_parents.get(current) else {
            break;
        };
        current = parent.parent();
    }
    names.join(" > ")
}

fn resolve_group_texture_material(
    style_source_item_id: Option<ItemId>,
    style_material: Option<&Handle<StandardMaterial>>,
    group: &str,
    tile: Option<[u32; 2]>,
    uv_repeat: Option<[f32; 2]>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    item_registry: &ItemRegistry,
    block_registry: &BlockRegistry,
    uv_bounds: Option<[[f32; 2]; 2]>,
) -> Option<Handle<StandardMaterial>> {
    let style_item_id = resolve_item_for_group(style_source_item_id, group, item_registry)?;
    let block_id = item_registry
        .block_for_item(style_item_id)
        .or_else(|| style_source_item_id.and_then(|source| item_registry.block_for_item(source)))?;
    let block_def = block_registry.def_opt(block_id)?;
    apply_nearest_sampler_to_texture_handle(images, &block_def.image, tile.is_none());

    let mut material = materials
        .get(&block_def.material)
        .cloned()
        .or_else(|| {
            style_material
                .and_then(|handle| materials.get(handle))
                .cloned()
        })
        .unwrap_or(StandardMaterial {
            metallic: 0.0,
            perceptual_roughness: 1.0,
            reflectance: 0.0,
            ..default()
        });

    if let Some([tile_x, tile_y]) = tile {
        let (tile_offset, tile_scale) =
            uv_rect_for_block_tile(block_def.localized_name.as_str(), tile_x, tile_y)?;
        material.uv_transform = build_uv_transform(tile_offset, tile_scale, uv_repeat, uv_bounds);
    } else {
        material.uv_transform = build_uv_transform([0.0, 0.0], [1.0, 1.0], uv_repeat, uv_bounds);
    }

    Some(materials.add(material))
}

#[inline]
fn mesh_uv_bounds_for_entity(
    entity: Entity,
    q_meshes: &Query<&Mesh3d>,
    meshes: &Assets<Mesh>,
    cache: &mut HashMap<AssetId<Mesh>, Option<[[f32; 2]; 2]>>,
) -> Option<[[f32; 2]; 2]> {
    let mesh_handle = q_meshes.get(entity).ok()?;
    let mesh_id = mesh_handle.0.id();
    if let Some(cached) = cache.get(&mesh_id) {
        return *cached;
    }
    let bounds = meshes
        .get(&mesh_handle.0)
        .and_then(mesh_uv_bounds)
        .map(|(min, max)| [min, max]);
    cache.insert(mesh_id, bounds);
    bounds
}

fn mesh_uv_bounds(mesh: &Mesh) -> Option<([f32; 2], [f32; 2])> {
    let values = mesh.attribute(Mesh::ATTRIBUTE_UV_0)?;
    let VertexAttributeValues::Float32x2(uvs) = values else {
        return None;
    };
    let first = uvs.first()?;
    let mut min_u = first[0];
    let mut max_u = first[0];
    let mut min_v = first[1];
    let mut max_v = first[1];
    for uv in uvs.iter().skip(1) {
        min_u = min_u.min(uv[0]);
        max_u = max_u.max(uv[0]);
        min_v = min_v.min(uv[1]);
        max_v = max_v.max(uv[1]);
    }
    Some(([min_u, min_v], [max_u, max_v]))
}

fn build_uv_transform(
    base_offset: [f32; 2],
    base_scale: [f32; 2],
    uv_repeat: Option<[f32; 2]>,
    uv_bounds: Option<[[f32; 2]; 2]>,
) -> Affine2 {
    let repeat = uv_repeat.unwrap_or([1.0, 1.0]);
    let bounds = uv_bounds.unwrap_or([[0.0, 0.0], [1.0, 1.0]]);
    let min_u = bounds[0][0];
    let min_v = bounds[0][1];
    let range_u = (bounds[1][0] - bounds[0][0]).abs().max(0.000_01);
    let range_v = (bounds[1][1] - bounds[0][1]).abs().max(0.000_01);

    let scale_u = base_scale[0] * repeat[0] / range_u;
    let scale_v = base_scale[1] * repeat[1] / range_v;
    let translate_u = base_offset[0] - (min_u * scale_u);
    let translate_v = base_offset[1] - (min_v * scale_v);

    Affine2::from_scale_angle_translation(
        Vec2::new(scale_u, scale_v),
        0.0,
        Vec2::new(translate_u, translate_v),
    )
}

#[inline]
fn apply_nearest_sampler_to_image(
    images: &mut Assets<Image>,
    image_id: AssetId<Image>,
    repeat: bool,
) {
    let Some(image) = images.get_mut(image_id) else {
        return;
    };
    let address_mode = if repeat {
        bevy::image::ImageAddressMode::Repeat
    } else {
        bevy::image::ImageAddressMode::ClampToEdge
    };
    image.sampler = bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
        address_mode_u: address_mode,
        address_mode_v: address_mode,
        address_mode_w: address_mode,
        mag_filter: bevy::image::ImageFilterMode::Nearest,
        min_filter: bevy::image::ImageFilterMode::Nearest,
        mipmap_filter: bevy::image::ImageFilterMode::Nearest,
        anisotropy_clamp: 1,
        ..default()
    });
}

#[inline]
fn apply_nearest_sampler_to_texture_handle(
    images: &mut Assets<Image>,
    texture: &Handle<Image>,
    repeat: bool,
) {
    apply_nearest_sampler_to_image(images, texture.id(), repeat);
}

fn resolve_item_for_group(
    style_source_item_id: Option<ItemId>,
    group: &str,
    item_registry: &ItemRegistry,
) -> Option<ItemId> {
    if let Some(style_source_item_id) = style_source_item_id {
        if let Some(related_item_id) =
            item_registry.related_item_in_group(style_source_item_id, group)
        {
            return Some(related_item_id);
        }
        if item_registry.has_group(style_source_item_id, group) {
            return Some(style_source_item_id);
        }
    }
    first_item_in_group(item_registry, group)
}

#[derive(Deserialize)]
struct StructureTextureDirJson {
    #[serde(default)]
    texture_dir: Option<String>,
}

#[derive(Deserialize)]
struct StructureTilesetJson {
    #[serde(default)]
    tile_size: u32,
    columns: u32,
    rows: u32,
}

fn uv_rect_for_block_tile(
    block_localized_name: &str,
    tile_x: u32,
    tile_y: u32,
) -> Option<([f32; 2], [f32; 2])> {
    const ATLAS_PAD_PX: f32 = 0.5;
    let texture_dir = resolve_texture_dir_for_block(block_localized_name);
    let tileset_path = format!("assets/{texture_dir}/data.json");
    let raw = fs::read_to_string(tileset_path).ok()?;
    let tileset = serde_json::from_str::<StructureTilesetJson>(raw.as_str()).ok()?;
    if tileset.columns == 0 || tileset.rows == 0 {
        return None;
    }
    if tile_x >= tileset.columns || tile_y >= tileset.rows {
        return None;
    }
    let tile_size = tileset.tile_size.max(1);
    let image_w = tileset.columns as f32 * tile_size as f32;
    let image_h = tileset.rows as f32 * tile_size as f32;
    let tile_w = image_w / tileset.columns as f32;
    let tile_h = image_h / tileset.rows as f32;

    let u0 = (tile_x as f32 * tile_w + ATLAS_PAD_PX) / image_w;
    let v0 = (tile_y as f32 * tile_h + ATLAS_PAD_PX) / image_h;
    let u1 = ((tile_x as f32 + 1.0) * tile_w - ATLAS_PAD_PX) / image_w;
    let v1 = ((tile_y as f32 + 1.0) * tile_h - ATLAS_PAD_PX) / image_h;

    let scale_x = (u1 - u0).max(0.000_01);
    let scale_y = (v1 - v0).max(0.000_01);
    Some(([u0, v0], [scale_x, scale_y]))
}

fn resolve_texture_dir_for_block(block_localized_name: &str) -> String {
    let block_file = format!("assets/blocks/{block_localized_name}.json");
    if let Ok(raw) = fs::read_to_string(block_file.as_str())
        && let Ok(parsed) = serde_json::from_str::<StructureTextureDirJson>(raw.as_str())
        && let Some(texture_dir) = parsed.texture_dir
    {
        let normalized = normalize_asset_path(texture_dir.as_str());
        if !normalized.is_empty() {
            return normalized;
        }
    }
    let base = block_localized_name
        .trim()
        .strip_suffix("_block")
        .unwrap_or(block_localized_name)
        .trim_matches('/');
    format!("textures/blocks/{base}")
}

#[inline]
fn normalize_asset_path(raw: &str) -> String {
    let mut value = raw.trim().replace('\\', "/");
    if let Some(stripped) = value.strip_prefix("assets/") {
        value = stripped.to_string();
    }
    if let Some(stripped) = value.strip_prefix("./") {
        value = stripped.to_string();
    }
    if Path::new(value.as_str()).as_os_str().is_empty() {
        return String::new();
    }
    value
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
fn normalize_rotation_steps(raw: i32) -> u8 {
    raw.rem_euclid(8) as u8
}

#[inline]
fn rotation_steps_to_placement_quarters(rotation_steps: u8) -> u8 {
    normalize_rotation_quarters((rotation_steps as i32) / 2)
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

include!("block_event_handler/overlay.rs");
