use crate::core::config::{GlobalConfig, WorldGenConfig};
use crate::core::debug::{
    BuildInfo, ChunkDebugStats, DebugGridMode, DebugGridState, DebugOverlayState, RuntimePerfStats,
    SysStats, WorldInspectorState,
};
use crate::core::entities::player::block_selection::SelectionState;
use crate::core::entities::player::inventory::{
    InventorySlot, PLAYER_INVENTORY_SLOTS, PLAYER_INVENTORY_STACK_MAX, PlayerInventory,
};
use crate::core::entities::player::{
    FlightState, FpsController, GameMode, GameModeState, Player, PlayerCamera,
};
use crate::core::events::ui_events::{
    ConnectToServerRequest, CraftHandCraftedRequest, CraftWorkTableRequest,
    DisconnectFromServerRequest, DropItemRequest, OpenStructureBuildMenuRequest,
    OpenWorkbenchMenuRequest,
};
use crate::core::inventory::creative_panel::{
    CREATIVE_PANEL_COLUMNS, CREATIVE_PANEL_PAGE_SIZE, CreativePanelState,
};
use crate::core::inventory::items::{
    ItemId, ItemRegistry, block_requirement_for_id, build_block_item_icon_image,
    infer_tool_from_item_key, parse_block_icon_cache_key, player_drop_spawn_motion,
    player_drop_world_location, spawn_player_dropped_item_stack,
};
use crate::core::inventory::recipe::{
    ActiveStructurePlacementState, ActiveStructureRecipeState, BuildingMaterialRequirementSource,
    BuildingStructureRecipe, BuildingStructureRecipeRegistry, HAND_CRAFTED_INPUT_SLOTS,
    HandCraftedState, RecipeRegistry, RecipeTypeRegistry, ResolvedRecipe,
    WORK_TABLE_CRAFTING_INPUT_SLOTS, WorkTableCraftingState,
};
use crate::core::multiplayer::{MultiplayerConnectionPhase, MultiplayerConnectionState};
use crate::core::states::states::{
    AppState, BeforeUiState, InGameStates, LoadingStates, is_state_in_game,
};
use crate::core::states::world_gen::{LoadingPhase, LoadingProgress};
use crate::core::ui::{HOTBAR_SLOTS, HotbarSelectionState, UiInteractionState};
use crate::core::world::biome::func::dominant_biome_at_p_chunks;
use crate::core::world::biome::registry::BiomeRegistry;
use crate::core::world::block::{
    BlockRegistry, MiningState, SelectedBlock, VOXEL_SIZE, mining_progress,
};
use crate::core::world::chunk::{CaveTracker, ChunkMap, LoadCenter};
use crate::core::world::chunk_dimension::{CX, CZ, SEC_COUNT, world_to_chunk_xz};
use crate::core::world::fluid::{FluidMap, WaterMeshIndex};
use crate::core::world::save::{RegionCache, WorldSave, default_saves_root};
use crate::generator::chunk::cave::cave_builder::CaveJobs;
use crate::generator::chunk::chunk_builder::{
    ChunkStageTelemetry, ColliderBacklog, PendingColliderBuild,
};
use crate::generator::chunk::chunk_struct::{MeshBacklog, PendingGen, PendingMesh};
use crate::utils::key_utils::convert;
use api::core::network::config::NetworkSettings;
use api::core::network::discovery::{LanDiscoveryClient, LanServerInfo};
use api::utils::v_ram_utils;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::render::renderer::RenderAdapterInfo;
use bevy::tasks::{AsyncComputeTaskPool, ComputeTaskPool, IoTaskPool};
use bevy::window::{CursorGrabMode, CursorIcon, CursorOptions, PrimaryWindow, SystemCursorIcon};
use bevy_extended_ui::styles::{CssClass, CssID};
use bevy_extended_ui::widgets::{
    BindToID, Button, Div, Img, InputField, InputType, InputValue, Paragraph, ProgressBar,
    Scrollbar, UIGenID, UIWidgetState,
};
use bevy_extended_ui::{ExtendedUiConfiguration, ExtendedUiPlugin, ImageCache};
use lightyear::prelude::{LocalTimeline, Tick};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use sysinfo::{ProcessesToUpdate, get_current_pid};

const WORLD_UNLOAD_HOLD_SECS: f32 = 0.35;
const DOUBLE_CLICK_WINDOW_SECS: f64 = 0.1;
const DEFAULT_SERVER_PORT: u16 = 14191;
const PROBE_INTERVAL_SECS: f32 = 3.0;
const SERVER_STALE_AFTER_SECS: f64 = 10.0;
const WORLD_META_FILE: &str = "world.meta.json";
const MULTIPLAYER_SERVER_FILE: &str = "config/multiplayer_servers.toml";

const MAIN_MENU_SINGLE_PLAYER_ID: &str = "main-menu-single-player";
const MAIN_MENU_MULTI_PLAYER_ID: &str = "main-menu-multi-player";
const MAIN_MENU_SETTINGS_ID: &str = "main-menu-settings";
const MAIN_MENU_QUIT_ID: &str = "main-menu-quit";

const SINGLE_PLAYER_WORLD_CARD_PREFIX: &str = "single-player-world-card-";
const SINGLE_PLAYER_CREATE_WORLD_ID: &str = "single-player-create-world";
const SINGLE_PLAYER_PLAY_WORLD_ID: &str = "single-player-play-world";
const SINGLE_PLAYER_DELETE_WORLD_ID: &str = "single-player-delete-world";
const SINGLE_PLAYER_BACK_ID: &str = "single-player-back";
const SINGLE_PLAYER_DELETE_CONFIRM_ID: &str = "single-player-delete-confirm";
const SINGLE_PLAYER_DELETE_CANCEL_ID: &str = "single-player-delete-cancel";
const SINGLE_PLAYER_DELETE_TEXT_ID: &str = "single-player-delete-text";

const CREATE_WORLD_NAME_INPUT_ID: &str = "create-world-name-input";
const CREATE_WORLD_SEED_INPUT_ID: &str = "create-world-seed-input";
const CREATE_WORLD_CREATE_ID: &str = "create-world-create";
const CREATE_WORLD_ABORT_ID: &str = "create-world-abort";
const BENCHMARK_DIALOG_TEXT_ID: &str = "benchmark-dialog-text";
const BENCHMARK_DIALOG_START_ID: &str = "benchmark-dialog-start";
const BENCHMARK_DIALOG_ABORT_ID: &str = "benchmark-dialog-abort";

const MULTIPLAYER_CARD_PREFIX: &str = "multi-player-server-card-";
const MULTIPLAYER_JOIN_ID: &str = "multi-player-join-server";
const MULTIPLAYER_REFRESH_ID: &str = "multi-player-refresh-server-list";
const MULTIPLAYER_ADD_ID: &str = "multi-player-add-server";
const MULTIPLAYER_EDIT_ID: &str = "multi-player-edit-server";
const MULTIPLAYER_DELETE_ID: &str = "multi-player-delete-server";
const MULTIPLAYER_BACK_ID: &str = "multi-player-back";

const MULTIPLAYER_FORM_TITLE_ID: &str = "multi-player-form-title";
const MULTIPLAYER_FORM_NAME_INPUT_ID: &str = "multi-player-form-name-input";
const MULTIPLAYER_FORM_ADDRESS_INPUT_ID: &str = "multi-player-form-address-input";
const MULTIPLAYER_FORM_ADD_ID: &str = "multi-player-form-add";
const MULTIPLAYER_FORM_EDIT_ID: &str = "multi-player-form-edit";
const MULTIPLAYER_FORM_ABORT_ID: &str = "multi-player-form-abort";

const MULTIPLAYER_DELETE_TEXT_ID: &str = "multi-player-delete-text";
const MULTIPLAYER_DELETE_CONFIRM_ID: &str = "multi-player-delete-confirm";
const MULTIPLAYER_DELETE_ABORT_ID: &str = "multi-player-delete-abort";
const MULTIPLAYER_CONNECT_TEXT_ID: &str = "multi-player-connect-text";
const MULTIPLAYER_CONNECT_OK_ID: &str = "multi-player-connect-ok";

const PAUSE_PLAY_ID: &str = "pause-menu-play";
const PAUSE_SETTINGS_ID: &str = "pause-menu-settings";
const PAUSE_CLOSE_ID: &str = "pause-menu-close";

const HUD_SLOT_PREFIX: &str = "hud-hotbar-slot-";
const HUD_SLOT_BADGE_PREFIX: &str = "hud-hotbar-slot-badge-";
const HUD_SELECTED_TOOLTIP_ID: &str = "hud-selected-item-tooltip";

const PLAYER_INVENTORY_TOTAL_ID: &str = "player-inventory-total";
const PLAYER_INVENTORY_FRAME_PREFIX: &str = "player-inventory-frame-";
const PLAYER_INVENTORY_BADGE_PREFIX: &str = "player-inventory-badge-";
const HAND_CRAFTED_FRAME_PREFIX: &str = "hand-crafted-frame-";
const HAND_CRAFTED_BADGE_PREFIX: &str = "hand-crafted-badge-";
const HAND_CRAFTED_RESULT_FRAME_ID: &str = "hand-crafted-result-frame";
const HAND_CRAFTED_RESULT_BADGE_ID: &str = "hand-crafted-result-badge";
const INVENTORY_TOOLTIP_NAME_ID: &str = "inventory-tooltip-name";
const INVENTORY_TOOLTIP_KEY_ID: &str = "inventory-tooltip-key";
const INVENTORY_CURSOR_BADGE_ID: &str = "inventory-cursor-badge";
const INVENTORY_TRASH_BUTTON_ID: &str = "inventory-trash-button";
const RECIPE_PREVIEW_TITLE_ID: &str = "recipe-preview-title";
const RECIPE_PREVIEW_MODE_ID: &str = "recipe-preview-mode";
const RECIPE_PREVIEW_TAB_TOOLTIP_ID: &str = "recipe-preview-tab-tooltip";
const RECIPE_PREVIEW_TAB_PREV_ID: &str = "recipe-preview-tab-prev";
const RECIPE_PREVIEW_TAB_NEXT_ID: &str = "recipe-preview-tab-next";
const RECIPE_PREVIEW_TAB_PREFIX: &str = "recipe-preview-tab-";
const RECIPE_PREVIEW_TAB_ICON_PREFIX: &str = "recipe-preview-tab-icon-";
const RECIPE_PREVIEW_INPUT_FRAME_PREFIX: &str = "recipe-preview-input-frame-";
const RECIPE_PREVIEW_INPUT_BADGE_PREFIX: &str = "recipe-preview-input-badge-";
const RECIPE_PREVIEW_RESULT_FRAME_ID: &str = "recipe-preview-result-frame";
const RECIPE_PREVIEW_RESULT_BADGE_ID: &str = "recipe-preview-result-badge";
const RECIPE_PREVIEW_FILL_ID: &str = "recipe-preview-fill";
const RECIPE_PREVIEW_INPUT_SLOTS: usize = WORK_TABLE_CRAFTING_INPUT_SLOTS;
const RECIPE_PREVIEW_TABS_PER_PAGE: usize = 4;
const STRUCTURE_BUILD_WORKBENCH_ID: &str = "structure-build-workbench";
const STRUCTURE_BUILD_HINT_ID: &str = "structure-build-hint";
const WORKBENCH_RECIPE_TITLE_ID: &str = "workbench-recipe-title";
const WORKBENCH_CRAFT_FRAME_PREFIX: &str = "workbench-craft-frame-";
const WORKBENCH_CRAFT_BADGE_PREFIX: &str = "workbench-craft-badge-";
const WORKBENCH_RESULT_FRAME_ID: &str = "workbench-result-frame";
const WORKBENCH_RESULT_BADGE_ID: &str = "workbench-result-badge";
const WORKBENCH_RESULT_TIME_ID: &str = "workbench-result-time";
const WORKBENCH_RESULT_PROGRESS_ID: &str = "workbench-result-progress";
const WORKBENCH_TOOL_FRAME_PREFIX: &str = "workbench-tool-frame-";
const WORKBENCH_TOOL_BADGE_PREFIX: &str = "workbench-tool-badge-";
const WORKBENCH_PLAYER_INVENTORY_FRAME_PREFIX: &str = "workbench-player-inventory-frame-";
const WORKBENCH_PLAYER_INVENTORY_BADGE_PREFIX: &str = "workbench-player-inventory-badge-";
const WORKBENCH_ITEMS_TOTAL_ID: &str = "workbench-items-total";
const WORKBENCH_ITEMS_PAGE_ID: &str = "workbench-items-page";
const WORKBENCH_ITEMS_PREV_ID: &str = "workbench-items-prev";
const WORKBENCH_ITEMS_NEXT_ID: &str = "workbench-items-next";
const WORKBENCH_ITEMS_SLOT_PREFIX: &str = "workbench-items-slot-";
const WORKBENCH_RECIPE_HINT_ID: &str = "workbench-recipe-hint";
const WORKBENCH_TRASH_BUTTON_ID: &str = "workbench-trash-button";
const CREATIVE_PANEL_TOTAL_ID: &str = "creative-panel-total";
const CREATIVE_PANEL_PAGE_ID: &str = "creative-panel-page";
const CREATIVE_PANEL_PREV_ID: &str = "creative-panel-prev";
const CREATIVE_PANEL_NEXT_ID: &str = "creative-panel-next";
const CREATIVE_PANEL_SLOT_PREFIX: &str = "creative-panel-slot-";
const CREATIVE_RECIPE_HINT_ID: &str = "creative-recipe-hint";

const WORLD_GEN_PROGRESS_ID: &str = "world-gen-progress";
const WORLD_GEN_CHUNKS_ID: &str = "world-gen-chunks";
const WORLD_GEN_SPINNER_ID: &str = "world-gen-spinner";

const ID_BUILD: &str = "debug-build";
const ID_CPU_NAME: &str = "debug-cpu-name";
const ID_GPU_NAME: &str = "debug-gpu-name";
const ID_GPU_LOAD: &str = "debug-gpu-load";
const ID_GPU_CLOCK: &str = "debug-gpu-clock";
const ID_VRAM: &str = "debug-vram";
const ID_BIOME: &str = "debug-biome";
const ID_BIOME_CLIMATE: &str = "debug-biome-climate";
const ID_GLOBAL_CPU: &str = "debug-global-cpu";
const ID_APP_CPU: &str = "debug-app-cpu";
const ID_APP_MEM: &str = "debug-app-mem";
const ID_FPS: &str = "debug-fps";
const ID_FPS_LOW: &str = "debug-fps-low";
const ID_STREAM_DECODE_QUEUE: &str = "debug-stream-decode-queue";
const ID_STREAM_REMESH_QUEUE: &str = "debug-stream-remesh-queue";
const ID_TICK_SPEED: &str = "debug-tick-speed";
const ID_LOOK_BLOCK: &str = "debug-look-block";
const ID_PLAYER_POS: &str = "debug-player-pos";
const ID_CHUNK_COORD: &str = "debug-chunk-coord";
const ID_GRID: &str = "debug-grid";
const ID_INSPECTOR: &str = "debug-world-inspector";
const ID_OVERLAY: &str = "debug-overlay";

/// Represents hardcoded ui plugin used by the `graphic::hardcoded_ui` module.
pub struct HardcodedUiPlugin;

/// Represents main menu root used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct MainMenuRoot;
/// Represents single player root used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct SinglePlayerRoot;
/// Represents create world root used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct CreateWorldRoot;
/// Represents single player world list used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct SinglePlayerWorldList;
/// Represents list div scroll ready used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct ListDivScrollReady;
/// Represents single player list item used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct SinglePlayerListItem;
/// Represents single player delete dialog used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct SinglePlayerDeleteDialog;
/// Represents single player delete text used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct SinglePlayerDeleteText;
/// Represents multiplayer root used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct MultiplayerRoot;
/// Represents multiplayer server list used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct MultiplayerServerList;
/// Represents multiplayer list item used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct MultiplayerListItem;
/// Represents multiplayer form dialog used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct MultiplayerFormDialog;
/// Represents multiplayer form add button used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct MultiplayerFormAddButton;
/// Represents multiplayer form edit button used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct MultiplayerFormEditButton;
/// Represents multiplayer delete dialog used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct MultiplayerDeleteDialog;
/// Represents multiplayer delete text used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct MultiplayerDeleteText;
/// Represents multiplayer connect dialog used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct MultiplayerConnectDialog;
/// Represents multiplayer connect ok button used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct MultiplayerConnectOkButton;
/// Represents pause menu root used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct PauseMenuRoot;
/// Represents world gen root used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct WorldGenRoot;
/// Represents world unload root used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct WorldUnloadRoot;
/// Represents hud root used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct HudRoot;
/// Represents hotbar selection tooltip text used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct HotbarSelectionTooltipText;
/// Represents looked block hud card used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct HudLookedBlockCard;
/// Represents looked block hud icon used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct HudLookedBlockIcon;
/// Represents looked block hud localized display name used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct HudLookedBlockDisplayName;
/// Represents looked block hud localized id used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct HudLookedBlockLocalizedName;
/// Represents looked block hud mining level text used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct HudLookedBlockLevel;
/// Represents looked block hud mining progress bar used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct HudLookedBlockProgress;
/// Represents player inventory root used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct PlayerInventoryRoot;
/// Represents structure build root used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct StructureBuildRoot;
/// Represents workbench recipe root used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct WorkbenchRecipeRoot;
/// Represents workbench recipe main panel used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct WorkbenchRecipeMainPanel;
/// Represents workbench recipe player inventory panel used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct WorkbenchRecipeInventoryPanel;
/// Represents workbench recipe item grid root used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct WorkbenchRecipeItemGridRoot;
/// Represents inventory main panel used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct InventoryMainPanel;
/// Represents inventory drop zone panel used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct InventoryDropZonePanel;
/// Represents inventory tooltip root used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct InventoryTooltipRoot;
/// Represents inventory cursor item root used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct InventoryCursorItemRoot;
/// Represents inventory cursor item icon used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct InventoryCursorItemIcon;
/// Represents inventory cursor item badge used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct InventoryCursorItemBadge;
/// Represents recipe preview dialog root used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct RecipePreviewDialogRoot;
/// Represents recipe preview dialog panel used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct RecipePreviewDialogPanel;
/// Represents recipe preview input grid used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct RecipePreviewInputGrid;
/// Represents creative panel grid root used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct CreativePanelGridRoot;
/// Represents debug overlay root used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct DebugOverlayRoot;
/// Represents benchmark border root used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct BenchmarkBorderRoot;
/// Represents benchmark menu dialog root used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct BenchmarkMenuDialogRoot;
/// Represents benchmark menu dialog text used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct BenchmarkMenuDialogText;
/// Represents benchmark automation timer root used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct BenchmarkAutomationTimerRoot;
/// Represents benchmark automation timer text used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct BenchmarkAutomationTimerText;

/// Defines the possible ui button kind variants in the `graphic::hardcoded_ui` module.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum UiButtonKind {
    Action,
    ActionRow,
    RecipeTab,
    Card,
    InventorySlot,
    InventoryResultSlot,
}

/// Defines the possible ui button tone variants in the `graphic::hardcoded_ui` module.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum UiButtonTone {
    Normal,
    Accent,
}

/// Defines the possible ui text tone variants in the `graphic::hardcoded_ui` module.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum UiTextTone {
    Heading,
    CardName,
    CardPing,
    Normal,
    Darker,
    HotbarTooltip,
    TooltipName,
    TooltipKey,
}

/// Represents ui button layout applied used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct UiButtonLayoutApplied;

/// Represents ui input layout applied used by the `graphic::hardcoded_ui` module.
#[derive(Component)]
struct UiInputLayoutApplied;

/// Represents ui entities used by the `graphic::hardcoded_ui` module.
#[derive(Resource, Debug, Clone, Copy)]
struct UiEntities {
    single_player_world_list: Entity,
    multiplayer_server_list: Entity,
}

/// Represents world gen ui animation used by the `graphic::hardcoded_ui` module.
#[derive(Resource, Debug, Clone)]
struct WorldGenUiAnimation {
    displayed_pct: f32,
}

impl Default for WorldGenUiAnimation {
    /// Runs the `default` routine for default in the `graphic::hardcoded_ui` module.
    fn default() -> Self {
        Self { displayed_pct: 0.0 }
    }
}

/// Represents world generation progress log state used by the `graphic::hardcoded_ui` module.
#[derive(Resource, Debug, Clone)]
struct WorldGenProgressLogState {
    world_sequence: u32,
    last_logged_percent: Option<u8>,
    last_phase: LoadingPhase,
    phase_peak_percent: f32,
    phase_peak_chunks: usize,
    timer: Timer,
}

impl Default for WorldGenProgressLogState {
    /// Runs the `default` routine for default in the `graphic::hardcoded_ui` module.
    fn default() -> Self {
        Self {
            world_sequence: 0,
            last_logged_percent: None,
            last_phase: LoadingPhase::BaseGen,
            phase_peak_percent: 0.0,
            phase_peak_chunks: 0,
            timer: Timer::from_seconds(0.5, TimerMode::Repeating),
        }
    }
}

/// Represents hotbar selection tooltip state used by the `graphic::hardcoded_ui` module.
#[derive(Resource, Debug, Clone)]
struct HotbarSelectionTooltipState {
    visible: bool,
    text: String,
    last_selected_index: usize,
    timer: Timer,
}

impl Default for HotbarSelectionTooltipState {
    /// Runs the `default` routine for default in the `graphic::hardcoded_ui` module.
    fn default() -> Self {
        Self {
            visible: false,
            text: String::new(),
            last_selected_index: 0,
            timer: Timer::from_seconds(1.3, TimerMode::Once),
        }
    }
}

/// Represents runtime tick sampling state used by the `graphic::hardcoded_ui` module.
#[derive(Resource, Debug, Clone, Default)]
struct RuntimePerfSampleState {
    last_local_tick: Option<Tick>,
    last_sample_real_secs: Option<f64>,
    fps_window_secs: f32,
    fps_window_sum: f32,
    fps_window_count: u32,
    low_window_secs: f32,
    low_window_fps_samples: Vec<f32>,
}

/// Represents world unload ui state used by the `graphic::hardcoded_ui` module.
#[derive(Resource, Debug, Clone)]
struct WorldUnloadUiState {
    active: bool,
    timer: Timer,
}

impl Default for WorldUnloadUiState {
    /// Runs the `default` routine for default in the `graphic::hardcoded_ui` module.
    fn default() -> Self {
        Self {
            active: false,
            timer: Timer::from_seconds(WORLD_UNLOAD_HOLD_SECS, TimerMode::Once),
        }
    }
}

/// Represents pause menu state used by the `graphic::hardcoded_ui` module.
#[derive(Resource, Debug, Default, Clone, Copy)]
struct PauseMenuState {
    open: bool,
}

/// Represents player inventory ui state used by the `graphic::hardcoded_ui` module.
#[derive(Resource, Debug, Default)]
struct PlayerInventoryUiState {
    open: bool,
}

/// Represents structure build menu state used by the `graphic::hardcoded_ui` module.
#[derive(Resource, Debug, Default, Clone, Copy)]
struct StructureBuildMenuState {
    open: bool,
}

/// Represents workbench recipe menu state used by the `graphic::hardcoded_ui` module.
#[derive(Resource, Debug, Default, Clone, Copy)]
struct WorkbenchRecipeMenuState {
    open: bool,
}

/// Represents workbench crafting progress state used by the `graphic::hardcoded_ui` module.
#[derive(Resource, Debug, Clone)]
struct WorkbenchCraftProgressState {
    active: bool,
    elapsed_secs: f32,
    duration_secs: f32,
    recipe_source_path: String,
}

impl Default for WorkbenchCraftProgressState {
    /// Runs the `default` routine for default in the `graphic::hardcoded_ui` module.
    fn default() -> Self {
        Self {
            active: false,
            elapsed_secs: 0.0,
            duration_secs: 0.0,
            recipe_source_path: String::new(),
        }
    }
}

/// Represents workbench tool slots state used by the `graphic::hardcoded_ui` module.
#[derive(Resource, Debug, Clone, Copy)]
struct WorkbenchToolSlotsState {
    slots: [InventorySlot; 5],
}

impl Default for WorkbenchToolSlotsState {
    /// Runs the `default` routine for default in the `graphic::hardcoded_ui` module.
    fn default() -> Self {
        Self {
            slots: [InventorySlot::default(); 5],
        }
    }
}

/// Represents inventory cursor item state used by the `graphic::hardcoded_ui` module.
#[derive(Resource, Debug, Default, Clone, Copy)]
struct InventoryCursorItemState {
    slot: InventorySlot,
}

/// Represents recipe preview dialog state used by the `graphic::hardcoded_ui` module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecipePreviewCraftingType {
    HandCrafted,
    WorkTable,
}

/// Represents recipe preview dialog state used by the `graphic::hardcoded_ui` module.
#[derive(Debug, Clone)]
struct RecipePreviewVariant {
    crafting_type: RecipePreviewCraftingType,
    input_slot_count: usize,
    input_slots: [InventorySlot; RECIPE_PREVIEW_INPUT_SLOTS],
    input_slot_alternatives: [Vec<ItemId>; RECIPE_PREVIEW_INPUT_SLOTS],
    result_slot: InventorySlot,
}

/// Represents recipe preview dialog state used by the `graphic::hardcoded_ui` module.
#[derive(Resource, Debug, Clone)]
struct RecipePreviewDialogState {
    open: bool,
    variants: Vec<RecipePreviewVariant>,
    selected_variant_index: usize,
    tab_page: usize,
    crafting_type: Option<RecipePreviewCraftingType>,
    input_slot_count: usize,
    input_slots: [InventorySlot; RECIPE_PREVIEW_INPUT_SLOTS],
    result_slot: InventorySlot,
}

impl Default for RecipePreviewDialogState {
    /// Runs the `default` routine for default in the `graphic::hardcoded_ui` module.
    fn default() -> Self {
        Self {
            open: false,
            variants: Vec::new(),
            selected_variant_index: 0,
            tab_page: 0,
            crafting_type: None,
            input_slot_count: 0,
            input_slots: [InventorySlot::default(); RECIPE_PREVIEW_INPUT_SLOTS],
            result_slot: InventorySlot::default(),
        }
    }
}

/// Represents creative panel ui state used by the `graphic::hardcoded_ui` module.
#[derive(Resource, Debug, Default, Clone, Copy)]
struct CreativePanelUiState {
    synced_once: bool,
}

/// Represents debug vram state used by the `graphic::hardcoded_ui` module.
#[derive(Resource, Debug, Default, Clone, Copy)]
struct DebugVramState {
    bytes: Option<u64>,
    total_bytes: Option<u64>,
    source: Option<&'static str>,
    scope: Option<&'static str>,
}

/// Represents debug gpu load state used by the `graphic::hardcoded_ui` module.
#[derive(Resource, Debug, Default, Clone, Copy)]
struct DebugGpuLoadState {
    percent: Option<f32>,
    source: Option<&'static str>,
    scope: Option<&'static str>,
}

/// Represents debug gpu clock state used by the `graphic::hardcoded_ui` module.
#[derive(Resource, Debug, Default, Clone, Copy)]
struct DebugGpuClockState {
    hz: Option<u64>,
    source: Option<&'static str>,
    scope: Option<&'static str>,
}

/// Represents saved world entry used by the `graphic::hardcoded_ui` module.
#[derive(Clone, Debug)]
struct SavedWorldEntry {
    folder_name: String,
    seed: i32,
    path: PathBuf,
}

/// Defines the possible single player page variants in the `graphic::hardcoded_ui` module.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SinglePlayerPage {
    #[default]
    List,
    CreateWorld,
}

/// Represents single player ui state used by the `graphic::hardcoded_ui` module.
#[derive(Resource, Default)]
struct SinglePlayerUiState {
    page: SinglePlayerPage,
    worlds: Vec<SavedWorldEntry>,
    selected_index: Option<usize>,
    pending_delete_index: Option<usize>,
    last_card_click: Option<(usize, f64)>,
    closing_for_world_load: bool,
}

/// Represents benchmark automation runtime state used by the `graphic::hardcoded_ui` module.
#[derive(Resource, Debug, Default)]
struct BenchmarkAutomationState {
    dialog_open: bool,
    active_world: Option<SavedWorldEntry>,
    session_started_elapsed_secs: Option<f64>,
    measure_started_elapsed_secs: Option<f64>,
    warmup_duration_secs: f64,
    run_duration_secs: f64,
    abort_requested: bool,
    cleanup_pending_world_path: Option<PathBuf>,
}

/// Represents world meta used by the `graphic::hardcoded_ui` module.
#[derive(Serialize, Deserialize)]
struct WorldMeta {
    seed: i32,
    #[serde(default)]
    spawn_translation: Option<[f32; 3]>,
}

/// Defines the possible single player action variants in the `graphic::hardcoded_ui` module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SinglePlayerAction {
    SelectWorld(usize),
    OpenCreateWorld,
    PlayWorld,
    DeleteWorld,
    BackToMenu,
    ConfirmDelete,
    CancelDelete,
    CreateWorldSubmit,
    CreateWorldAbort,
}

/// Represents saved server entry used by the `graphic::hardcoded_ui` module.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct SavedServerEntry {
    server_name: String,
    host: String,
    port: u16,
}

/// Represents saved server config used by the `graphic::hardcoded_ui` module.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct SavedServerConfig {
    #[serde(default)]
    servers: Vec<SavedServerEntry>,
}

/// Represents probed server status used by the `graphic::hardcoded_ui` module.
#[derive(Clone, Debug)]
struct ProbedServerStatus {
    session_url: String,
    observed_host: Option<String>,
    matched_saved_key: Option<String>,
    server_name: String,
    motd: String,
    current_players: usize,
    max_players: usize,
    ping_ms: Option<u32>,
    last_seen_at: f64,
}

/// Represents display server entry used by the `graphic::hardcoded_ui` module.
#[derive(Clone, Debug)]
struct DisplayServerEntry {
    key: String,
    saved_index: Option<usize>,
    server_name: String,
    host: String,
    port: u16,
    motd: String,
    current_players: Option<usize>,
    max_players: Option<usize>,
    ping_ms: Option<u32>,
    online: bool,
    waiting_for_response: bool,
    session_url: String,
}

/// Defines the possible server form mode variants in the `graphic::hardcoded_ui` module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ServerFormMode {
    Add,
    Edit,
}

/// Represents server form dialog state used by the `graphic::hardcoded_ui` module.
#[derive(Clone, Debug)]
struct ServerFormDialogState {
    mode: ServerFormMode,
    editing_saved_index: Option<usize>,
}

/// Represents multiplayer ui state used by the `graphic::hardcoded_ui` module.
#[derive(Resource, Default)]
struct MultiplayerUiState {
    saved_servers: Vec<SavedServerEntry>,
    probed_servers: HashMap<String, ProbedServerStatus>,
    probe_started_at: HashMap<String, f64>,
    dismissed_server_keys: HashSet<String>,
    display_servers: Vec<DisplayServerEntry>,
    rendered_keys: Vec<String>,
    selected_key: Option<String>,
    form_dialog: Option<ServerFormDialogState>,
    pending_delete_key: Option<String>,
    joining_key: Option<String>,
}

impl MultiplayerUiState {
    /// Runs the `selected_server` routine for selected server in the `graphic::hardcoded_ui` module.
    fn selected_server(&self) -> Option<&DisplayServerEntry> {
        let key = self.selected_key.as_ref()?;
        self.display_servers.iter().find(|entry| &entry.key == key)
    }
}

/// Represents server probe runtime used by the `graphic::hardcoded_ui` module.
#[derive(Default)]
struct ServerProbeRuntime {
    client: Option<LanDiscoveryClient>,
    probe_timer: Timer,
    last_broadcast_sent_at: Option<f64>,
    pending_direct_probes: HashMap<String, f64>,
}

impl ServerProbeRuntime {
    /// Runs the `configure` routine for configure in the `graphic::hardcoded_ui` module.
    fn configure(&mut self) {
        let settings = NetworkSettings::load_or_create("config/network.toml");
        self.client = if settings.client.lan_discovery {
            LanDiscoveryClient::bind(settings.client.lan_discovery_port).ok()
        } else {
            None
        };
        self.probe_timer = Timer::from_seconds(PROBE_INTERVAL_SECS, TimerMode::Repeating);
        self.last_broadcast_sent_at = None;
        self.pending_direct_probes.clear();
    }
}

/// Defines the possible multiplayer action variants in the `graphic::hardcoded_ui` module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MultiplayerAction {
    SelectServer(usize),
    JoinServer,
    RefreshServers,
    BackToMenu,
    DismissConnectError,
    OpenAddServer,
    OpenEditServer,
    OpenDeleteServer,
    ConfirmDelete,
    AbortDelete,
    SubmitAdd,
    SubmitEdit,
    AbortForm,
}

/// Defines the possible pause menu action variants in the `graphic::hardcoded_ui` module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PauseMenuAction {
    BackToGame,
    Settings,
    ExitToMenu,
}

/// Defines the possible main menu action variants in the `graphic::hardcoded_ui` module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MainMenuAction {
    SinglePlayer,
    MultiPlayer,
    Settings,
    QuitGame,
}

/// Defines the possible in game inventory ui set variants in the `graphic::hardcoded_ui` module.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum InGameInventoryUiSet {
    Input,
    Sync,
}

impl Plugin for HardcodedUiPlugin {
    /// Builds this component for the `graphic::hardcoded_ui` module.
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadingProgress>()
            .init_resource::<WorldGenUiAnimation>()
            .init_resource::<WorldUnloadUiState>()
            .init_resource::<PauseMenuState>()
            .init_resource::<PlayerInventoryUiState>()
            .init_resource::<StructureBuildMenuState>()
            .init_resource::<WorkbenchRecipeMenuState>()
            .init_resource::<WorkbenchCraftProgressState>()
            .init_resource::<WorkbenchToolSlotsState>()
            .init_resource::<InventoryCursorItemState>()
            .init_resource::<RecipePreviewDialogState>()
            .init_resource::<CreativePanelUiState>()
            .init_resource::<CreativePanelState>()
            .init_resource::<ChatUiState>()
            .init_resource::<DebugVramState>()
            .init_resource::<DebugGpuLoadState>()
            .init_resource::<DebugGpuClockState>()
            .init_resource::<SinglePlayerUiState>()
            .init_resource::<MultiplayerUiState>()
            .init_resource::<BenchmarkAutomationState>()
            .init_resource::<DebugOverlayState>()
            .init_resource::<DebugGridState>()
            .init_resource::<SysStats>()
            .init_resource::<RuntimePerfStats>()
            .init_resource::<RuntimePerfSampleState>()
            .init_resource::<HotbarSelectionTooltipState>()
            .init_resource::<ClientLanguageState>()
            .init_resource::<ChunkDebugStats>()
            .init_resource::<WorldGenProgressLogState>()
            .init_resource::<ActiveInventorySavePath>()
            .init_resource::<BenchmarkRuntime>()
            .insert_non_send_resource(ServerProbeRuntime::default())
            .add_plugins(ExtendedUiPlugin)
            .configure_sets(
                Update,
                (InGameInventoryUiSet::Input, InGameInventoryUiSet::Sync).chain(),
            )
            .add_systems(
                Startup,
                (configure_extended_ui, spawn_hardcoded_ui, prime_sys_stats),
            )
            .add_systems(
                PreUpdate,
                (
                    layout_buttons_once,
                    layout_inputs_once,
                    style_buttons,
                    style_inputs,
                    style_paragraphs,
                    style_pause_menu_button_texts,
                    style_images,
                    style_scroll_div_lists,
                    style_div_scrollbars,
                    style_progress_bars,
                    style_slot_count_badges,
                ),
            )
            .add_systems(
                PostUpdate,
                (
                    style_button_icons,
                    style_input_placeholder_and_cursor,
                    enforce_text_cursor_for_input_hover,
                ),
            )
            .add_systems(PostUpdate, suppress_stale_scrollbars)
            .add_systems(Last, style_scroll_div_contents)
            .add_systems(
                OnEnter(AppState::Screen(BeforeUiState::Menu)),
                (
                    clear_inventory_context_when_entering_screen,
                    show_main_menu,
                    set_menu_cursor,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    set_menu_cursor,
                    toggle_benchmark_menu_dialog,
                    handle_benchmark_menu_dialog_buttons,
                    sync_benchmark_menu_dialog,
                    cleanup_benchmark_temp_world_if_needed,
                    handle_main_menu_buttons,
                )
                    .chain()
                    .run_if(in_state(AppState::Screen(BeforeUiState::Menu))),
            )
            .add_systems(
                OnExit(AppState::Screen(BeforeUiState::Menu)),
                hide_main_menu,
            )
            .add_systems(
                OnEnter(AppState::Screen(BeforeUiState::SinglePlayer)),
                (
                    clear_inventory_context_when_entering_screen,
                    enter_single_player_screen,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    set_single_player_interaction,
                    handle_single_player_back_navigation,
                    handle_single_player_actions,
                    sync_single_player_visibility,
                    sync_single_player_delete_dialog,
                    sync_single_player_card_style,
                )
                    .chain()
                    .run_if(in_state(AppState::Screen(BeforeUiState::SinglePlayer))),
            )
            .add_systems(
                OnExit(AppState::Screen(BeforeUiState::SinglePlayer)),
                exit_single_player_screen,
            )
            .add_systems(
                OnEnter(AppState::Screen(BeforeUiState::MultiPlayer)),
                (
                    clear_inventory_context_when_entering_screen,
                    enter_multiplayer_screen,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    set_multiplayer_interaction,
                    handle_multiplayer_back_navigation,
                    poll_multiplayer_servers,
                    handle_multiplayer_actions,
                    sync_multiplayer_dialogs,
                    sync_multiplayer_card_text,
                    sync_multiplayer_card_style,
                )
                    .chain()
                    .run_if(in_state(AppState::Screen(BeforeUiState::MultiPlayer))),
            )
            .add_systems(
                OnExit(AppState::Screen(BeforeUiState::MultiPlayer)),
                exit_multiplayer_screen,
            )
            .add_systems(
                OnEnter(AppState::Loading(LoadingStates::BaseGen)),
                (
                    load_inventory_for_world_entry,
                    reset_world_gen_ui_animation,
                    show_world_gen_ui,
                    log_task_pool_worker_counts_on_world_start,
                )
                    .chain(),
            )
            .add_systems(
                OnExit(AppState::Loading(LoadingStates::CaveGen)),
                hide_world_gen_ui,
            )
            .add_systems(
                OnEnter(AppState::InGame(InGameStates::Game)),
                hide_world_gen_ui,
            )
            .add_systems(
                OnEnter(AppState::InGame(InGameStates::Game)),
                hide_menu_roots_for_ingame,
            )
            .add_systems(
                OnEnter(AppState::Loading(LoadingStates::BaseGen)),
                hide_menu_roots_for_ingame,
            )
            .add_systems(Update, sync_world_gen_progress.run_if(is_loading_state))
            .add_systems(
                OnExit(AppState::InGame(InGameStates::Game)),
                trigger_world_unload_ui,
            )
            .add_systems(
                Update,
                tick_world_unload_ui.run_if(world_unload_ui_should_tick),
            )
            .add_systems(
                OnEnter(AppState::Loading(LoadingStates::BaseGen)),
                reset_world_unload_ui,
            )
            .add_systems(
                OnEnter(AppState::InGame(InGameStates::Game)),
                reset_world_unload_ui,
            )
            .add_systems(
                OnEnter(AppState::InGame(InGameStates::Game)),
                show_hud_hotbar_ui,
            )
            .add_systems(
                Update,
                (
                    cycle_hotbar_with_scroll,
                    select_hotbar_with_number_keys,
                    drop_selected_hotbar_item,
                    sync_hotbar_selected_block,
                    track_hotbar_selection_tooltip,
                    sync_hud_hotbar_ui,
                )
                    .run_if(in_state(AppState::InGame(InGameStates::Game))),
            )
            .add_systems(
                Update,
                sync_hud_looked_block_card.run_if(in_state(AppState::InGame(InGameStates::Game))),
            )
            .add_systems(
                Update,
                update_chat_ui_state
                    .before(sync_ingame_ui_interaction_state)
                    .run_if(in_state(AppState::InGame(InGameStates::Game))),
            )
            .add_systems(
                bevy_inspector_egui::bevy_egui::EguiPrimaryContextPass,
                render_chat_overlay.run_if(in_state(AppState::InGame(InGameStates::Game))),
            )
            .add_systems(
                OnExit(AppState::InGame(InGameStates::Game)),
                hide_hud_hotbar_ui,
            )
            .add_systems(
                Update,
                (
                    toggle_pause_menu,
                    enforce_pause_menu_visibility,
                    handle_pause_menu_buttons,
                    sync_pause_menu_labels,
                    sync_pause_time,
                )
                    .chain()
                    .run_if(is_state_in_game),
            )
            .add_systems(
                OnExit(AppState::InGame(InGameStates::Game)),
                close_pause_menu,
            )
            .add_systems(
                Update,
                (
                    handle_open_structure_build_menu_request.in_set(InGameInventoryUiSet::Input),
                    handle_open_workbench_recipe_menu_request.in_set(InGameInventoryUiSet::Input),
                    handle_structure_build_menu_input.in_set(InGameInventoryUiSet::Input),
                    handle_workbench_recipe_menu_input.in_set(InGameInventoryUiSet::Input),
                    handle_workbench_recipe_menu_navigation.in_set(InGameInventoryUiSet::Input),
                    handle_workbench_recipe_menu_item_clicks.in_set(InGameInventoryUiSet::Input),
                    tick_workbench_craft_progress.in_set(InGameInventoryUiSet::Input),
                    rotate_structure_preview_with_scroll.in_set(InGameInventoryUiSet::Input),
                    toggle_player_inventory_ui.in_set(InGameInventoryUiSet::Input),
                    sync_creative_panel_state_from_registry.in_set(InGameInventoryUiSet::Input),
                    handle_creative_panel_navigation.in_set(InGameInventoryUiSet::Input),
                    handle_creative_panel_clicks.in_set(InGameInventoryUiSet::Input),
                    handle_inventory_drag_and_drop.in_set(InGameInventoryUiSet::Input),
                )
                    .run_if(in_state(AppState::InGame(InGameStates::Game))),
            )
            .add_systems(
                Update,
                persist_inventory_on_change.run_if(in_state(AppState::InGame(InGameStates::Game))),
            )
            .add_systems(
                Update,
                (
                    sync_structure_build_menu_ui.in_set(InGameInventoryUiSet::Sync),
                    sync_workbench_recipe_menu_ui.in_set(InGameInventoryUiSet::Sync),
                    sync_player_inventory_ui.in_set(InGameInventoryUiSet::Sync),
                    sync_creative_panel_ui.in_set(InGameInventoryUiSet::Sync),
                    sync_inventory_cursor_item_ui.in_set(InGameInventoryUiSet::Sync),
                    sync_inventory_tooltip_ui.in_set(InGameInventoryUiSet::Sync),
                )
                    .run_if(in_state(AppState::InGame(InGameStates::Game))),
            )
            .add_systems(
                Update,
                sync_ingame_ui_interaction_state
                    .after(sync_pause_time)
                    .after(InGameInventoryUiSet::Sync)
                    .run_if(is_state_in_game),
            )
            .add_systems(
                OnExit(AppState::InGame(InGameStates::Game)),
                (
                    close_chat_ui_on_exit,
                    close_player_inventory_ui,
                    close_structure_build_menu_ui,
                    close_workbench_recipe_menu_ui,
                    persist_inventory_on_world_exit,
                    clear_inventory_after_world_exit,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    run_benchmark_automation,
                    toggle_benchmark,
                    sample_benchmark_runtime,
                    sync_benchmark_border,
                    sync_benchmark_automation_timer,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame(InGameStates::Game))),
            )
            .add_systems(
                Update,
                (
                    toggle_system_last_ui,
                    sample_runtime_perf_stats,
                    sample_chunk_debug_stats,
                    refresh_sys_stats,
                    sync_system_last_ui,
                )
                    .run_if(in_state(AppState::InGame(InGameStates::Game))),
            );
        app.add_systems(
            OnExit(AppState::InGame(InGameStates::Game)),
            close_system_last_ui,
        );
        app.add_systems(
            OnExit(AppState::InGame(InGameStates::Game)),
            force_stop_benchmark_on_game_exit,
        );
        app.add_systems(
            OnExit(AppState::InGame(InGameStates::Game)),
            reset_benchmark_automation_on_world_exit,
        );
    }
}

include!("components/theme.rs");
include!("components/language.rs");
include!("components/spawn.rs");
include!("components/main_menu.rs");
include!("components/single_player.rs");
include!("components/multiplayer.rs");
include!("components/world_flow.rs");
include!("components/hud.rs");
include!("components/chat.rs");
include!("components/pause_menu.rs");
include!("components/inventory.rs");
include!("components/inventory_creative.rs");
include!("components/structure_builder.rs");
include!("components/workbench.rs");
include!("components/inventory_persistence.rs");
include!("components/ui_interaction_sync.rs");
include!("components/debug_overlay.rs");
include!("components/benchmark_auto.rs");
include!("components/benchmark.rs");

/// Runs the `bytes_to_mib` routine for bytes to mib in the `graphic::hardcoded_ui` module.
#[inline]
fn bytes_to_mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

impl SavedServerEntry {
    /// Runs the `key` routine for key in the `graphic::hardcoded_ui` module.
    fn key(&self) -> String {
        server_key(self.host.as_str(), self.port)
    }

    /// Runs the `session_url` routine for session url in the `graphic::hardcoded_ui` module.
    fn session_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }
}
