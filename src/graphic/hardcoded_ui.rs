use crate::core::config::{GlobalConfig, WorldGenConfig};
use crate::core::debug::{
    BuildInfo, DebugGridState, DebugOverlayState, SysStats, WorldInspectorState,
};
use crate::core::entities::player::inventory::{
    InventorySlot, PLAYER_INVENTORY_SLOTS, PLAYER_INVENTORY_STACK_MAX, PlayerInventory,
};
use crate::core::entities::player::{GameMode, GameModeState, Player};
use crate::core::events::ui_events::{
    ConnectToServerRequest, CraftHandCraftedRequest, DisconnectFromServerRequest, DropItemRequest,
    OpenToLanRequest, StopLanHostRequest,
};
use crate::core::inventory::creative_panel::{
    CREATIVE_PANEL_COLUMNS, CREATIVE_PANEL_PAGE_SIZE, CreativePanelState,
};
use crate::core::inventory::items::{
    ItemId, ItemRegistry, build_block_item_icon_image, parse_block_icon_cache_key,
    player_drop_spawn_motion, player_drop_world_location, spawn_player_dropped_item_stack,
};
use crate::core::inventory::recipe::{
    HAND_CRAFTED_INPUT_SLOTS, HandCraftedState, RecipeRegistry, RecipeTypeRegistry, ResolvedRecipe,
};
use crate::core::multiplayer::{MultiplayerConnectionPhase, MultiplayerConnectionState};
use crate::core::states::states::{
    AppState, BeforeUiState, InGameStates, LoadingStates, is_state_in_game,
};
use crate::core::states::world_gen::{LoadingPhase, LoadingProgress};
use crate::core::ui::{HOTBAR_SLOTS, HotbarSelectionState, UiInteractionState};
use crate::core::world::biome::func::dominant_biome_at_p_chunks;
use crate::core::world::biome::registry::BiomeRegistry;
use crate::core::world::block::{BlockRegistry, SelectedBlock};
use crate::core::world::chunk::ChunkMap;
use crate::core::world::chunk_dimension::{CX, CZ};
use crate::core::world::fluid::{FluidMap, WaterMeshIndex};
use crate::core::world::save::{RegionCache, WorldSave, default_saves_root};
use crate::utils::key_utils::convert;
use api::core::network::config::NetworkSettings;
use api::core::network::discovery::{LanDiscoveryClient, LanServerInfo};
use api::utils::v_ram_utils;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::render::renderer::RenderAdapterInfo;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_extended_ui::styles::CssID;
use bevy_extended_ui::widgets::{
    BindToID, Button, Div, Img, InputField, InputType, InputValue, Paragraph, ProgressBar,
    Scrollbar, UIGenID, UIWidgetState,
};
use bevy_extended_ui::{ExtendedUiConfiguration, ExtendedUiPlugin, ImageCache};
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
const SINGLE_PLAYER_DELETE_CONFIRM_ID: &str = "single-player-delete-confirm";
const SINGLE_PLAYER_DELETE_CANCEL_ID: &str = "single-player-delete-cancel";
const SINGLE_PLAYER_DELETE_TEXT_ID: &str = "single-player-delete-text";

const CREATE_WORLD_NAME_INPUT_ID: &str = "create-world-name-input";
const CREATE_WORLD_SEED_INPUT_ID: &str = "create-world-seed-input";
const CREATE_WORLD_CREATE_ID: &str = "create-world-create";
const CREATE_WORLD_ABORT_ID: &str = "create-world-abort";

const MULTIPLAYER_CARD_PREFIX: &str = "multi-player-server-card-";
const MULTIPLAYER_JOIN_ID: &str = "multi-player-join-server";
const MULTIPLAYER_REFRESH_ID: &str = "multi-player-refresh-server-list";
const MULTIPLAYER_ADD_ID: &str = "multi-player-add-server";
const MULTIPLAYER_EDIT_ID: &str = "multi-player-edit-server";
const MULTIPLAYER_DELETE_ID: &str = "multi-player-delete-server";

const MULTIPLAYER_FORM_TITLE_ID: &str = "multi-player-form-title";
const MULTIPLAYER_FORM_NAME_INPUT_ID: &str = "multi-player-form-name-input";
const MULTIPLAYER_FORM_ADDRESS_INPUT_ID: &str = "multi-player-form-address-input";
const MULTIPLAYER_FORM_ADD_ID: &str = "multi-player-form-add";
const MULTIPLAYER_FORM_EDIT_ID: &str = "multi-player-form-edit";
const MULTIPLAYER_FORM_ABORT_ID: &str = "multi-player-form-abort";

const MULTIPLAYER_DELETE_TEXT_ID: &str = "multi-player-delete-text";
const MULTIPLAYER_DELETE_CONFIRM_ID: &str = "multi-player-delete-confirm";
const MULTIPLAYER_DELETE_ABORT_ID: &str = "multi-player-delete-abort";

const PAUSE_PLAY_ID: &str = "pause-menu-play";
const PAUSE_CONNECT_ID: &str = "pause-menu-connect";
const PAUSE_SETTINGS_ID: &str = "pause-menu-settings";
const PAUSE_CLOSE_ID: &str = "pause-menu-close";

const HUD_SLOT_PREFIX: &str = "hud-hotbar-slot-";
const HUD_SLOT_BADGE_PREFIX: &str = "hud-hotbar-slot-badge-";

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
const RECIPE_PREVIEW_TITLE_ID: &str = "recipe-preview-title";
const RECIPE_PREVIEW_INPUT_FRAME_PREFIX: &str = "recipe-preview-input-frame-";
const RECIPE_PREVIEW_INPUT_BADGE_PREFIX: &str = "recipe-preview-input-badge-";
const RECIPE_PREVIEW_RESULT_FRAME_ID: &str = "recipe-preview-result-frame";
const RECIPE_PREVIEW_RESULT_BADGE_ID: &str = "recipe-preview-result-badge";
const RECIPE_PREVIEW_FILL_ID: &str = "recipe-preview-fill";
const CREATIVE_PANEL_TOTAL_ID: &str = "creative-panel-total";
const CREATIVE_PANEL_PAGE_ID: &str = "creative-panel-page";
const CREATIVE_PANEL_PREV_ID: &str = "creative-panel-prev";
const CREATIVE_PANEL_NEXT_ID: &str = "creative-panel-next";
const CREATIVE_PANEL_SLOT_PREFIX: &str = "creative-panel-slot-";
const CREATIVE_RECIPE_HINT_ID: &str = "creative-recipe-hint";

const WORLD_GEN_PROGRESS_ID: &str = "world-gen-progress";

const ID_BUILD: &str = "debug-build";
const ID_CPU_NAME: &str = "debug-cpu-name";
const ID_GPU_NAME: &str = "debug-gpu-name";
const ID_VRAM: &str = "debug-vram";
const ID_BIOME: &str = "debug-biome";
const ID_GLOBAL_CPU: &str = "debug-global-cpu";
const ID_APP_CPU: &str = "debug-app-cpu";
const ID_APP_MEM: &str = "debug-app-mem";
const ID_PLAYER_POS: &str = "debug-player-pos";
const ID_GRID: &str = "debug-grid";
const ID_INSPECTOR: &str = "debug-world-inspector";
const ID_OVERLAY: &str = "debug-overlay";

pub struct HardcodedUiPlugin;

#[derive(Component)]
struct MainMenuRoot;
#[derive(Component)]
struct SinglePlayerRoot;
#[derive(Component)]
struct CreateWorldRoot;
#[derive(Component)]
struct SinglePlayerWorldList;
#[derive(Component)]
struct ListDivScrollReady;
#[derive(Component)]
struct SinglePlayerListItem;
#[derive(Component)]
struct SinglePlayerDeleteDialog;
#[derive(Component)]
struct SinglePlayerDeleteText;
#[derive(Component)]
struct MultiplayerRoot;
#[derive(Component)]
struct MultiplayerServerList;
#[derive(Component)]
struct MultiplayerListItem;
#[derive(Component)]
struct MultiplayerFormDialog;
#[derive(Component)]
struct MultiplayerFormAddButton;
#[derive(Component)]
struct MultiplayerFormEditButton;
#[derive(Component)]
struct MultiplayerDeleteDialog;
#[derive(Component)]
struct MultiplayerDeleteText;
#[derive(Component)]
struct MultiplayerConnectDialog;
#[derive(Component)]
struct PauseMenuRoot;
#[derive(Component)]
struct WorldGenRoot;
#[derive(Component)]
struct WorldUnloadRoot;
#[derive(Component)]
struct HudRoot;
#[derive(Component)]
struct PlayerInventoryRoot;
#[derive(Component)]
struct InventoryMainPanel;
#[derive(Component)]
struct InventoryDropZonePanel;
#[derive(Component)]
struct InventoryTooltipRoot;
#[derive(Component)]
struct InventoryCursorItemRoot;
#[derive(Component)]
struct InventoryCursorItemIcon;
#[derive(Component)]
struct InventoryCursorItemBadge;
#[derive(Component)]
struct RecipePreviewDialogRoot;
#[derive(Component)]
struct RecipePreviewDialogPanel;
#[derive(Component)]
struct CreativePanelGridRoot;
#[derive(Component)]
struct DebugOverlayRoot;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum UiButtonKind {
    Action,
    ActionRow,
    Card,
    InventorySlot,
    InventoryResultSlot,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum UiButtonTone {
    Normal,
    Accent,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum UiTextTone {
    Heading,
    CardName,
    CardPing,
    Normal,
    Darker,
    TooltipName,
    TooltipKey,
}

#[derive(Component)]
struct UiButtonLayoutApplied;

#[derive(Component)]
struct UiInputLayoutApplied;

#[derive(Resource, Debug, Clone, Copy)]
struct UiEntities {
    single_player_world_list: Entity,
    multiplayer_server_list: Entity,
}

#[derive(Resource, Debug, Clone)]
struct WorldGenUiAnimation {
    displayed_pct: f32,
}

impl Default for WorldGenUiAnimation {
    fn default() -> Self {
        Self { displayed_pct: 0.0 }
    }
}

#[derive(Resource, Debug, Clone)]
struct WorldUnloadUiState {
    active: bool,
    timer: Timer,
}

impl Default for WorldUnloadUiState {
    fn default() -> Self {
        Self {
            active: false,
            timer: Timer::from_seconds(WORLD_UNLOAD_HOLD_SECS, TimerMode::Once),
        }
    }
}

#[derive(Resource, Debug, Default, Clone, Copy)]
struct PauseMenuState {
    open: bool,
}

#[derive(Resource, Debug, Default)]
struct PlayerInventoryUiState {
    open: bool,
}

#[derive(Resource, Debug, Default, Clone, Copy)]
struct InventoryCursorItemState {
    slot: InventorySlot,
}

#[derive(Resource, Debug, Default, Clone, Copy)]
struct InventoryLeftHoldState {
    source_slot: Option<InventoryHoldSource>,
    next_pull_at_secs: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InventoryHoldSource {
    Player(usize),
    HandCrafted(usize),
}

#[derive(Resource, Debug, Clone, Copy)]
struct RecipePreviewDialogState {
    open: bool,
    input_slots: [InventorySlot; HAND_CRAFTED_INPUT_SLOTS],
    result_slot: InventorySlot,
}

impl Default for RecipePreviewDialogState {
    fn default() -> Self {
        Self {
            open: false,
            input_slots: [InventorySlot::default(); HAND_CRAFTED_INPUT_SLOTS],
            result_slot: InventorySlot::default(),
        }
    }
}

#[derive(Resource, Debug, Default, Clone, Copy)]
struct CreativePanelUiState {
    synced_once: bool,
}

#[derive(Resource, Debug, Default, Clone, Copy)]
struct DebugVramState {
    bytes: Option<u64>,
    source: Option<&'static str>,
    scope: Option<&'static str>,
}

#[derive(Clone, Debug)]
struct SavedWorldEntry {
    folder_name: String,
    seed: i32,
    path: PathBuf,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SinglePlayerPage {
    #[default]
    List,
    CreateWorld,
}

#[derive(Resource, Default)]
struct SinglePlayerUiState {
    page: SinglePlayerPage,
    worlds: Vec<SavedWorldEntry>,
    selected_index: Option<usize>,
    pending_delete_index: Option<usize>,
    last_card_click: Option<(usize, f64)>,
    closing_for_world_load: bool,
}

#[derive(Serialize, Deserialize)]
struct WorldMeta {
    seed: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SinglePlayerAction {
    SelectWorld(usize),
    OpenCreateWorld,
    PlayWorld,
    DeleteWorld,
    ConfirmDelete,
    CancelDelete,
    CreateWorldSubmit,
    CreateWorldAbort,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct SavedServerEntry {
    server_name: String,
    host: String,
    port: u16,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct SavedServerConfig {
    #[serde(default)]
    servers: Vec<SavedServerEntry>,
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ServerFormMode {
    Add,
    Edit,
}

#[derive(Clone, Debug)]
struct ServerFormDialogState {
    mode: ServerFormMode,
    editing_saved_index: Option<usize>,
}

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
    fn selected_server(&self) -> Option<&DisplayServerEntry> {
        let key = self.selected_key.as_ref()?;
        self.display_servers.iter().find(|entry| &entry.key == key)
    }
}

#[derive(Default)]
struct ServerProbeRuntime {
    client: Option<LanDiscoveryClient>,
    probe_timer: Timer,
    last_broadcast_sent_at: Option<f64>,
    pending_direct_probes: HashMap<String, f64>,
}

impl ServerProbeRuntime {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MultiplayerAction {
    SelectServer(usize),
    JoinServer,
    RefreshServers,
    OpenAddServer,
    OpenEditServer,
    OpenDeleteServer,
    ConfirmDelete,
    AbortDelete,
    SubmitAdd,
    SubmitEdit,
    AbortForm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PauseMenuAction {
    BackToGame,
    OpenToLan,
    Settings,
    ExitToMenu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MainMenuAction {
    SinglePlayer,
    MultiPlayer,
    Settings,
    QuitGame,
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum InGameInventoryUiSet {
    Input,
    Sync,
}

impl Plugin for HardcodedUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadingProgress>()
            .init_resource::<WorldGenUiAnimation>()
            .init_resource::<WorldUnloadUiState>()
            .init_resource::<PauseMenuState>()
            .init_resource::<PlayerInventoryUiState>()
            .init_resource::<InventoryCursorItemState>()
            .init_resource::<InventoryLeftHoldState>()
            .init_resource::<RecipePreviewDialogState>()
            .init_resource::<CreativePanelUiState>()
            .init_resource::<CreativePanelState>()
            .init_resource::<DebugVramState>()
            .init_resource::<SinglePlayerUiState>()
            .init_resource::<MultiplayerUiState>()
            .init_resource::<DebugOverlayState>()
            .init_resource::<DebugGridState>()
            .init_resource::<SysStats>()
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
            .add_systems(PostUpdate, style_button_icons)
            .add_systems(PostUpdate, suppress_stale_scrollbars)
            .add_systems(Last, style_scroll_div_contents)
            .add_systems(
                OnEnter(AppState::Screen(BeforeUiState::Menu)),
                (show_main_menu, set_menu_cursor),
            )
            .add_systems(
                Update,
                (set_menu_cursor, handle_main_menu_buttons)
                    .chain()
                    .run_if(in_state(AppState::Screen(BeforeUiState::Menu))),
            )
            .add_systems(
                OnExit(AppState::Screen(BeforeUiState::Menu)),
                hide_main_menu,
            )
            .add_systems(
                OnEnter(AppState::Screen(BeforeUiState::SinglePlayer)),
                enter_single_player_screen,
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
                enter_multiplayer_screen,
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
                (reset_world_gen_ui_animation, show_world_gen_ui),
            )
            .add_systems(
                OnExit(AppState::Loading(LoadingStates::WaterGen)),
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
                    drop_selected_hotbar_item,
                    sync_hotbar_selected_block,
                    sync_hud_hotbar_ui,
                )
                    .run_if(in_state(AppState::InGame(InGameStates::Game))),
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
                (
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
                close_player_inventory_ui,
            )
            .add_systems(
                Update,
                (
                    toggle_system_last_ui,
                    refresh_sys_stats,
                    sync_system_last_ui,
                )
                    .run_if(in_state(AppState::InGame(InGameStates::Game))),
            );
        app.add_systems(
            OnExit(AppState::InGame(InGameStates::Game)),
            close_system_last_ui,
        );
    }
}

include!("components/theme.rs");
include!("components/spawn.rs");
include!("components/main_menu.rs");
include!("components/single_player.rs");
include!("components/multiplayer.rs");
include!("components/world_flow.rs");
include!("components/hud.rs");
include!("components/pause_menu.rs");
include!("components/inventory.rs");
include!("components/inventory_creative.rs");
include!("components/ui_interaction_sync.rs");
include!("components/debug_overlay.rs");

#[inline]
fn bytes_to_mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

#[inline]
fn bool_label(value: bool) -> &'static str {
    if value { "ON" } else { "OFF" }
}

impl SavedServerEntry {
    fn key(&self) -> String {
        server_key(self.host.as_str(), self.port)
    }

    fn session_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }
}
