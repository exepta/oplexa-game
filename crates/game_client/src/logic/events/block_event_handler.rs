use crate::core::config::GlobalConfig;
use crate::core::entities::player::block_selection::SelectionState;
use crate::core::entities::player::inventory::PlayerInventory;
use crate::core::entities::player::{FpsController, GameMode, GameModeState, Player};
use crate::core::events::block::block_player_events::{
    BlockBreakByPlayerEvent, BlockBreakObservedEvent, BlockPlaceByPlayerEvent,
    BlockPlaceObservedEvent,
};
use crate::core::events::chunk_events::SubChunkNeedRemeshEvent;
use crate::core::events::ui_events::{
    ChestInventoryContentsSync, ChestInventoryPersistRequest, ChestInventorySlotPayload,
    ChestInventorySnapshotRequest, ChestInventoryUiClosed, ChestInventoryUiOpened,
    OpenChestInventoryMenuRequest, OpenStructureBuildMenuRequest, OpenWorkbenchMenuRequest,
};
use crate::core::inventory::items::{
    block_requirement_for_id, can_drop_from_block, mining_speed_multiplier,
    spawn_world_item_for_block_break, spawn_world_item_with_motion, ItemId, ItemRegistry,
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
use crate::core::world::fluid::{FluidChunk, FluidMap};
use crate::core::world::save::{
    container_find, container_upsert, decode_structure_entries, encode_structure_entries,
    RegionCache, StructureRegionDropItem, StructureRegionEntry, StructureRegionInventorySlot,
    WorldSave, TAG_STR1,
};
use crate::core::world::{mark_dirty_block_and_neighbors, world_access_mut};
use crate::generator::chunk::chunk_meshing::safe_despawn_entity;
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::ecs::relationship::RelatedSpawnerCommands;
use bevy::ecs::system::SystemParam;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::math::Affine2;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;
use bevy_rapier3d::prelude::{
    AsyncSceneCollider, Collider, ComputedColliderShape, QueryFilter, ReadRapierContext, RigidBody,
    TriMeshFlags,
};
use serde::Deserialize;
use std::collections::{HashMap, HashSet, VecDeque};
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
/// Runtime metadata attached to a spawned structure entity.
pub(crate) struct PlacedStructureMetadata {
    pub recipe_name: String,
    pub model_asset_path: String,
    pub model_animated: bool,
    pub stats: BlockStats,
    pub place_origin: IVec3,
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
pub(crate) struct StructureRuntimeState {
    loaded_chunks: HashSet<IVec2>,
    pub(crate) records_by_chunk: HashMap<IVec2, Vec<StructureRegionEntry>>,
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
pub(crate) struct MultiplayerStructureReconcileQueue {
    pub(crate) pending_chunks: HashSet<IVec2>,
}

const MULTIPLAYER_STRUCTURE_RECONCILE_MIN_CHUNKS_PER_FRAME: usize = 8;
const MULTIPLAYER_STRUCTURE_RECONCILE_MAX_CHUNKS_PER_FRAME: usize = 48;
const MINING_PARTICLE_INTERVAL_SECS: f32 = 0.045;
const MINING_RUBBLE_RADIUS_MAX_METERS: f32 = 1.0;
const MINING_DEBRIS_FALLBACK_LIFETIME_SECS: f32 = 3.0;
const MINING_DEBRIS_MIN_LIFETIME_SECS: f32 = 0.05;
const WATER_FLOW_SOURCE_LEVEL: u8 = 10;
const WATER_FALL_VERTICAL_LEVEL: u8 = WATER_FLOW_SOURCE_LEVEL / 2;
const WATER_FLOW_DEFAULT_STEP_MS: f32 = 750.0;
const WATER_FLOW_MAX_STEPS_PER_FRAME: usize = 4;
const WATER_FLOW_MAX_CELLS_PER_TICK: usize = 384;

#[derive(Resource, Default)]
struct MiningDebrisFxAssets {
    initialized: bool,
    cube: Handle<Mesh>,
}

#[derive(Resource, Default)]
struct MiningDebrisEmitterState {
    next_emit_at: f32,
    active_target: Option<(IVec3, BlockId)>,
}

#[derive(Resource, Default)]
struct WaterFlowIds {
    initialized: bool,
    source_id: BlockId,
    settled_id: BlockId,
    by_level: [BlockId; 11],
}

impl WaterFlowIds {
    #[inline]
    fn contains(&self, id: BlockId) -> bool {
        id != 0 && (id == self.settled_id || self.by_level.contains(&id))
    }

    #[inline]
    fn id_for_level(&self, level: u8) -> BlockId {
        self.by_level[level.clamp(1, WATER_FLOW_SOURCE_LEVEL) as usize]
    }

    #[inline]
    fn level_for_id(&self, id: BlockId) -> u8 {
        if id == 0 {
            return 0;
        }
        if id == self.settled_id {
            return WATER_FLOW_SOURCE_LEVEL;
        }
        if id == self.source_id {
            return WATER_FLOW_SOURCE_LEVEL;
        }
        for level in 1..WATER_FLOW_SOURCE_LEVEL {
            if self.by_level[level as usize] == id {
                return level;
            }
        }
        0
    }
}

#[derive(Resource)]
struct WaterFlowState {
    sources: HashSet<IVec3>,
    frontier: VecDeque<IVec3>,
    queued: HashSet<IVec3>,
    pending_per_subchunk: HashMap<(IVec2, usize), u32>,
    sleeping_subchunks: HashSet<(IVec2, usize)>,
    step_ms: f32,
    step_secs: f32,
    accumulator_secs: f32,
    tick_cell_budget: usize,
}

impl Default for WaterFlowState {
    fn default() -> Self {
        Self {
            sources: HashSet::new(),
            frontier: VecDeque::new(),
            queued: HashSet::new(),
            pending_per_subchunk: HashMap::new(),
            sleeping_subchunks: HashSet::new(),
            step_ms: WATER_FLOW_DEFAULT_STEP_MS,
            step_secs: WATER_FLOW_DEFAULT_STEP_MS / 1000.0,
            accumulator_secs: 0.0,
            tick_cell_budget: WATER_FLOW_MAX_CELLS_PER_TICK,
        }
    }
}

#[derive(Component)]
struct MiningDebrisVisual;

#[derive(Component)]
struct MiningDebrisLifetime {
    age: f32,
    lifetime: f32,
}

#[derive(Component)]
struct MiningDebrisMotion {
    velocity: Vec3,
    angular_velocity: Vec3,
    resting: bool,
}

#[derive(Component, Clone, Copy)]
struct MiningRubblePiece {
    origin: Vec3,
    max_radius: f32,
}

/// Plugin that owns block break/place handling, water-flow simulation, and structure runtime sync.
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
    open_chest_menu_requests: MessageWriter<'w, OpenChestInventoryMenuRequest>,
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
    global_config: Res<'w, GlobalConfig>,
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
    /// Registers resources and systems for in-game block interactions.
    fn build(&self, app: &mut App) {
        app.init_resource::<StructureRuntimeState>();
        app.init_resource::<StructureMiningState>();
        app.init_resource::<MultiplayerStructureReconcileQueue>();
        app.init_resource::<MiningDebrisFxAssets>();
        app.init_resource::<MiningDebrisEmitterState>();
        app.init_resource::<WaterFlowIds>();
        app.init_resource::<WaterFlowState>();
        app.add_systems(
            Update,
            (
                init_water_flow_ids.in_set(VoxelStage::WorldEdit),
                sync_structures_for_loaded_chunks.in_set(VoxelStage::WorldEdit),
                enforce_block_texture_nearest_sampler_system.in_set(VoxelStage::WorldEdit),
                ensure_mining_debris_fx_assets.in_set(VoxelStage::WorldEdit),
                configure_structure_mesh_collider_name_filters.in_set(VoxelStage::WorldEdit),
                cleanup_structure_none_mesh_colliders.in_set(VoxelStage::WorldEdit),
                apply_structure_style_material_system.in_set(VoxelStage::WorldEdit),
                mark_chest_structures_for_animation.in_set(VoxelStage::WorldEdit),
                bind_chest_animation_players.in_set(VoxelStage::WorldEdit),
                sync_chest_inventory_contents_for_opened_ui.in_set(VoxelStage::WorldEdit),
                sync_chest_inventory_snapshot_requests.in_set(VoxelStage::WorldEdit),
                persist_chest_inventory_from_ui_requests.in_set(VoxelStage::WorldEdit),
                apply_chest_ui_animation_requests.in_set(VoxelStage::WorldEdit),
                update_chest_animation_playback.in_set(VoxelStage::WorldEdit),
                (block_break_handler, sync_mining_overlay)
                    .chain()
                    .in_set(VoxelStage::WorldEdit),
                update_mining_debris_fx.in_set(VoxelStage::WorldEdit),
                (
                    block_place_handler,
                    track_water_flow_sources_from_block_events,
                    run_water_flow_simulation,
                )
                    .chain()
                    .in_set(VoxelStage::WorldEdit),
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
            (cleanup_structure_runtime_on_exit, cleanup_mining_debris_fx).chain(),
        );
    }
}

// Water-flow simulation systems and helpers.
include!("block_event_handler/water_flow.rs");

// Cleanup, mining FX, and block-break systems.
include!("block_event_handler/mining_and_cleanup.rs");

// Block placement entry system.
include!("block_event_handler/block_place_handler.rs");

// Shared block-placement helpers (slab logic and inventory helpers).
include!("block_event_handler/placement.rs");

// Structure placement validation and inventory-consumption helpers.
include!("block_event_handler/structure_placement.rs");

// Structure runtime persistence and multiplayer reconciliation.
include!("block_event_handler/structure_runtime.rs");

// Structure material, texture, and collider post-processing.
include!("block_event_handler/structure_materials.rs");

// Chest structure animation and UI open/close linkage.
include!("block_event_handler/chest_animation.rs");

// Structure transform and randomization helpers.
include!("block_event_handler/structure_math.rs");

// Mining overlay rendering helpers.
include!("block_event_handler/overlay.rs");
