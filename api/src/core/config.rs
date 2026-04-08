use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Top‐level global configuration resource.
///
/// This resource aggregates all configurable subsystems of the game,
/// including graphics, input, and audio settings. It can be
/// deserialized from and serialized to external configuration files
/// and is registered as a Bevy `Resource`.
#[derive(Resource, Deserialize, Serialize, Debug, Clone)]
pub struct GlobalConfig {
    /// Settings related to rendering and display.
    pub graphics: GraphicsConfig,

    /// Settings related to gameplay behavior.
    pub gameplay: GameplayConfig,

    /// Settings related to user input mappings and sensitivities.
    pub input: InputConfig,

    /// Settings related to user interface behavior.
    pub interface: InterfaceConfig,
}

impl Default for GlobalConfig {
    /// Runs the `default` routine for default in the `core::config` module.
    fn default() -> Self {
        Self {
            graphics: GraphicsConfig::default(),
            gameplay: GameplayConfig::default(),
            input: InputConfig::default(),
            interface: InterfaceConfig::default(),
        }
    }
}

impl GlobalConfig {
    /// Runs the `ensure_config_files_exist` routine for ensure config files exist in the `core::config` module.
    pub fn ensure_config_files_exist() {
        Self::ensure_default_config_file("config/graphics.toml", &GraphicsConfig::default());
        Self::ensure_default_config_file("config/gameplay.toml", &GameplayConfig::default());
        Self::ensure_default_config_file("config/input.toml", &InputConfig::default());
        Self::ensure_default_config_file("config/interface.toml", &InterfaceConfig::default());
    }

    /// Runs the `ensure_default_config_file` routine for ensure default config file in the `core::config` module.
    fn ensure_default_config_file<T: Serialize>(path: &str, default: &T) {
        let config_path = Path::new(path);

        if config_path.exists() {
            return;
        }

        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).expect("Failed to create config directory");
        }

        Self::save(default, path);
    }

    /// Loads a configuration file and deserializes it into the specified type.
    ///
    /// # Arguments
    /// - `path`: The file path of the configuration file to load.
    ///
    /// # Panics
    /// This function will panic if the file cannot be read or parsed correctly.
    ///
    /// # Returns
    /// - `T`: The deserialized configuration data.
    pub fn load<T: for<'de> Deserialize<'de>>(path: &str) -> T {
        let content = fs::read_to_string(Path::new(path)).expect("Failed to read config file");
        toml::from_str(&content).expect("Failed to parse toml file")
    }

    /// Creates a new `GlobalConfig` instance and loads all configuration files.
    ///
    ///
    /// # Returns
    /// - `GlobalConfig`: A new instance with loaded configurations for game, graphics, input, and audio.
    pub fn new() -> Self {
        Self::ensure_config_files_exist();

        Self {
            graphics: Self::load("config/graphics.toml"),
            gameplay: Self::load("config/gameplay.toml"),
            input: Self::load("config/input.toml"),
            interface: Self::load("config/interface.toml"),
        }
    }

    /// Saves the requested data for the `core::config` module.
    fn save<T: Serialize>(data: &T, path: &str) {
        let toml_string = toml::to_string_pretty(data).expect("Failed to serialize to TOML");
        fs::write(Path::new(path), toml_string).expect("Failed to write config file");
    }

    /// Saves all for the `core::config` module.
    pub fn save_all(&self) {
        Self::ensure_config_files_exist();
        Self::save(&self.graphics, "config/graphics.toml");
        Self::save(&self.gameplay, "config/gameplay.toml");
        Self::save(&self.input, "config/input.toml");
        Self::save(&self.interface, "config/interface.toml");
    }
}

// =======================================================
//                         Interface
// =======================================================

/// Configuration settings for in-game interface behavior.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct InterfaceConfig {
    /// Maximum number of chat lines kept in local history.
    #[serde(default = "default_chat_max_space")]
    pub chat_max_space: usize,
    /// Max block distance used by local `/locate` biome search.
    #[serde(
        default = "default_locate_search_radius",
        rename = "locate-search-radius",
        alias = "locate_search_radius"
    )]
    pub locate_search_radius: i32,
}

impl Default for InterfaceConfig {
    /// Runs the `default` routine for default in the `core::config` module.
    fn default() -> Self {
        Self {
            chat_max_space: default_chat_max_space(),
            locate_search_radius: default_locate_search_radius(),
        }
    }
}

// =======================================================
//                          Graphics
// =======================================================

/// Configuration settings for the graphics subsystem.
///
/// This struct defines window dimensions, display modes, and rendering backend
/// preferences. It can be serialized to or deserialized from external configuration
/// files to customize the game’s graphical behavior.
#[derive(Deserialize, Serialize, Debug, Clone)]
#[allow(dead_code)]
pub struct GraphicsConfig {
    /// The width of the application window (in logical pixels or units).
    #[serde(default = "default_window_width")]
    pub window_width: u32,

    /// The height of the application window (in logical pixels or units).
    #[serde(default = "default_window_height")]
    pub window_height: u32,

    /// Whether the application should run in fullscreen mode.
    #[serde(default = "default_fullscreen")]
    pub fullscreen: bool,

    /// Whether vertical synchronization (vsync) is enabled.
    #[serde(default = "default_vsync")]
    pub vsync: bool,

    /// Identifier for the graphics backend to use (e.g., "wgpu", "OpenGL", "Vulkan").
    #[serde(default = "default_graphic_backend")]
    pub graphic_backend: String,

    /// The number of chunk generating ranges. 2 means 2 chunks in each direction.
    /// Note this build a cube around the player.
    #[serde(default = "default_chunk_range")]
    pub chunk_range: i32,

    /// Number of chunk generation tasks submitted per frame while in-game.
    #[serde(default = "default_chunk_gen_submit_per_frame")]
    pub chunk_gen_submit_per_frame: usize,

    /// Max simultaneous chunk generation tasks while in-game.
    #[serde(default = "default_chunk_gen_max_inflight")]
    pub chunk_gen_max_inflight: usize,

    /// Max simultaneous chunk meshing tasks while in-game.
    #[serde(default = "default_chunk_mesh_max_inflight")]
    pub chunk_mesh_max_inflight: usize,

    /// Max finished chunk meshes applied per frame while in-game.
    #[serde(default = "default_chunk_mesh_apply_per_frame")]
    pub chunk_mesh_apply_per_frame: usize,

    /// Max simultaneous collider build tasks while in-game.
    #[serde(default = "default_chunk_collider_max_inflight")]
    pub chunk_collider_max_inflight: usize,

    /// Max finished colliders applied per frame while in-game.
    #[serde(default = "default_chunk_collider_apply_per_frame")]
    pub chunk_collider_apply_per_frame: usize,

    /// Activation radius for chunk colliders around entities (in blocks).
    #[serde(default = "default_chunk_collider_activation_radius_blocks")]
    pub chunk_collider_activation_radius_blocks: i32,

    /// Maximum number of chunks unloaded per frame.
    #[serde(default = "default_chunk_unload_budget_per_frame")]
    pub chunk_unload_budget_per_frame: usize,

    /// Enables or disables world fog.
    #[serde(default = "default_fog_enabled")]
    pub fog_enabled: bool,

    /// Fog color as RGB in 0.0..1.0.
    #[serde(default = "default_fog_color")]
    pub fog_color: [f32; 3],

    /// Fog start as factor of loaded world radius.
    #[serde(default = "default_fog_start_factor")]
    pub fog_start_factor: f32,

    /// Fog end as factor of loaded world radius.
    #[serde(default = "default_fog_end_factor")]
    pub fog_end_factor: f32,

    /// Additional distance added behind fog end for camera far clip.
    #[serde(default = "default_far_clip_extra")]
    pub far_clip_extra: f32,
}

impl Default for GraphicsConfig {
    /// Runs the `default` routine for default in the `core::config` module.
    fn default() -> Self {
        Self {
            window_width: default_window_width(),
            window_height: default_window_height(),
            fullscreen: default_fullscreen(),
            vsync: default_vsync(),
            graphic_backend: default_graphic_backend(),
            chunk_range: default_chunk_range(),
            chunk_gen_submit_per_frame: default_chunk_gen_submit_per_frame(),
            chunk_gen_max_inflight: default_chunk_gen_max_inflight(),
            chunk_mesh_max_inflight: default_chunk_mesh_max_inflight(),
            chunk_mesh_apply_per_frame: default_chunk_mesh_apply_per_frame(),
            chunk_collider_max_inflight: default_chunk_collider_max_inflight(),
            chunk_collider_apply_per_frame: default_chunk_collider_apply_per_frame(),
            chunk_collider_activation_radius_blocks:
                default_chunk_collider_activation_radius_blocks(),
            chunk_unload_budget_per_frame: default_chunk_unload_budget_per_frame(),
            fog_enabled: default_fog_enabled(),
            fog_color: default_fog_color(),
            fog_start_factor: default_fog_start_factor(),
            fog_end_factor: default_fog_end_factor(),
            far_clip_extra: default_far_clip_extra(),
        }
    }
}

// =======================================================
//                         Gameplay
// =======================================================

/// Represents gameplay config used by the `core::config` module.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GameplayConfig {
    /// Vertical sensitivity multiplier for look input.
    pub mouse_sensitivity_vertical: f32,

    /// Horizontal sensitivity multiplier for look input.
    pub mouse_sensitivity_horizontal: f32,
}

impl Default for GameplayConfig {
    /// Runs the `default` routine for default in the `core::config` module.
    fn default() -> Self {
        Self {
            mouse_sensitivity_vertical: 1.0,
            mouse_sensitivity_horizontal: 1.0,
        }
    }
}

// =======================================================
//                          Input
// =======================================================

/// Configuration settings for user input and control mappings.
///
/// This struct defines sensitivity parameters for camera controls,
/// keybindings for player actions, character swapping, world combat,
/// and UI navigation. It can be deserialized from and serialized to
/// external configuration files to allow users to customize their
/// control scheme.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct InputConfig {
    // Player
    /// Key or button mapping for moving the player character upward.
    pub move_up: String,

    /// Key or button mapping for moving the player character downward.
    pub move_down: String,

    /// Key or button mapping for moving the player character to the left.
    pub move_left: String,

    /// Key or button mapping for moving the player character to the right.
    pub move_right: String,

    /// Key or button mapping for making the player character jump.
    pub jump: String,

    /// Key or button mapping for making the player character sprint.
    pub sprint: String,

    /// Key or button mapping for interacting with objects or NPCs.
    pub interact: String,

    /// Key or button mapping for performing a standard world attack.
    pub attack: String,

    /// Key or button mapping for dropping one item from the active slot.
    #[serde(default = "default_drop_item_key")]
    pub drop_item: String,

    /// Is only used for testing. Remove by finishing the game.
    pub toggle_game_mode: String,

    // UI
    /// Key or button mapping to open or toggle the in‐game menu.
    pub ui_menu: String,

    /// Key or button mapping to open or toggle the inventory screen.
    pub ui_inventory: String,

    /// Key or button mapping to close UI dialogs or go back in menus.
    pub ui_close_back: String,

    /// Key or button mapping to open the in-game chat input.
    #[serde(default = "default_open_chat_key")]
    pub open_chat: String,

    /// Key to open a recipe dialog for the currently hovered inventory item.
    #[serde(default = "default_inventory_recipe_open_key")]
    pub inventory_recipe_open: String,

    // Debug
    /// Shows system stats
    pub debug_overlay: String,

    /// Toggle chunk grid.
    pub chunk_grid: String,

    /// Toggle world inspector.
    pub world_inspector: String,
}

impl Default for InputConfig {
    /// Runs the `default` routine for default in the `core::config` module.
    fn default() -> Self {
        Self {
            move_up: String::from("W"),
            move_down: String::from("S"),
            move_left: String::from("A"),
            move_right: String::from("D"),
            jump: String::from("Space"),
            sprint: String::from("ShiftLeft"),
            interact: String::from("E"),
            attack: String::from("MouseLeft"),
            drop_item: default_drop_item_key(),
            toggle_game_mode: String::from("F2"),

            ui_menu: String::from("Enter"),
            ui_inventory: String::from("Tab"),
            ui_close_back: String::from("Escape"),
            open_chat: default_open_chat_key(),
            inventory_recipe_open: default_inventory_recipe_open_key(),

            debug_overlay: String::from("F3"),
            chunk_grid: String::from("F9"),
            world_inspector: String::from("F1"),
        }
    }
}

/// Runs the `default_drop_item_key` routine for default drop item key in the `core::config` module.
fn default_drop_item_key() -> String {
    String::from("Q")
}

/// Runs the `default_open_chat_key` routine for default open chat key in the `core::config` module.
fn default_open_chat_key() -> String {
    String::from("C")
}

/// Runs the `default_inventory_recipe_open_key` routine for default inventory recipe open key in the `core::config` module.
fn default_inventory_recipe_open_key() -> String {
    String::from("R")
}

/// Runs the `default_window_width` routine for default window width in the `core::config` module.
fn default_window_width() -> u32 {
    1270
}

/// Runs the `default_window_height` routine for default window height in the `core::config` module.
fn default_window_height() -> u32 {
    720
}

/// Runs the `default_fullscreen` routine for default fullscreen in the `core::config` module.
fn default_fullscreen() -> bool {
    false
}

/// Runs the `default_vsync` routine for default vsync in the `core::config` module.
fn default_vsync() -> bool {
    true
}

/// Runs the `default_graphic_backend` routine for default graphic backend in the `core::config` module.
fn default_graphic_backend() -> String {
    String::from("AUTO")
}

/// Runs the `default_chunk_range` routine for default chunk range in the `core::config` module.
fn default_chunk_range() -> i32 {
    8
}

/// Runs the `default_chunk_gen_submit_per_frame` routine for default chunk gen submit per frame in the `core::config` module.
fn default_chunk_gen_submit_per_frame() -> usize {
    14
}

/// Runs the `default_chunk_gen_max_inflight` routine for default chunk gen max inflight in the `core::config` module.
fn default_chunk_gen_max_inflight() -> usize {
    64
}

/// Runs the `default_chunk_mesh_max_inflight` routine for default chunk mesh max inflight in the `core::config` module.
fn default_chunk_mesh_max_inflight() -> usize {
    64
}

/// Runs the `default_chunk_mesh_apply_per_frame` routine for default chunk mesh apply per frame in the `core::config` module.
fn default_chunk_mesh_apply_per_frame() -> usize {
    28
}

/// Runs the `default_chunk_collider_max_inflight` routine for default chunk collider max inflight in the `core::config` module.
fn default_chunk_collider_max_inflight() -> usize {
    24
}

/// Runs the `default_chunk_collider_apply_per_frame` routine for default chunk collider apply per frame in the `core::config` module.
fn default_chunk_collider_apply_per_frame() -> usize {
    12
}

/// Runs the `default_chunk_collider_activation_radius_blocks` routine for default chunk collider activation radius blocks in the `core::config` module.
fn default_chunk_collider_activation_radius_blocks() -> i32 {
    50
}

/// Runs the `default_chunk_unload_budget_per_frame` routine for default chunk unload budget per frame in the `core::config` module.
fn default_chunk_unload_budget_per_frame() -> usize {
    10
}

/// Runs the `default_fog_enabled` routine for default fog enabled in the `core::config` module.
fn default_fog_enabled() -> bool {
    true
}

/// Runs the `default_fog_color` routine for default fog color in the `core::config` module.
fn default_fog_color() -> [f32; 3] {
    [0.62, 0.72, 0.85]
}

/// Runs the `default_fog_start_factor` routine for default fog start factor in the `core::config` module.
fn default_fog_start_factor() -> f32 {
    0.72
}

/// Runs the `default_fog_end_factor` routine for default fog end factor in the `core::config` module.
fn default_fog_end_factor() -> f32 {
    0.96
}

/// Runs the `default_far_clip_extra` routine for default far clip extra in the `core::config` module.
fn default_far_clip_extra() -> f32 {
    10.0
}

/// Runs the `default_chat_max_space` routine for default chat max space in the `core::config` module.
fn default_chat_max_space() -> usize {
    140
}

/// Runs the `default_locate_search_radius` routine for default locate search radius in the `core::config` module.
fn default_locate_search_radius() -> i32 {
    1000
}

// =======================================================
//                         Crosshair
// =======================================================

/// Represents crosshair config used by the `core::config` module.
#[derive(Resource)]
pub struct CrosshairConfig {
    pub radius: f32,
    pub thickness: f32,
    pub segments: usize,
    pub color: Color,
    pub visible_when_unlocked: bool,
}

impl Default for CrosshairConfig {
    /// Runs the `default` routine for default in the `core::config` module.
    fn default() -> Self {
        Self {
            radius: 8.0,
            thickness: 2.0,
            segments: 48,
            color: Color::WHITE,
            visible_when_unlocked: false,
        }
    }
}

// =======================================================
//                         WorldGen
// =======================================================

/// Represents world gen config used by the `core::config` module.
#[derive(Resource, Clone)]
pub struct WorldGenConfig {
    pub seed: i32,
}

impl Default for WorldGenConfig {
    /// Runs the `default` routine for default in the `core::config` module.
    fn default() -> Self {
        Self { seed: 1337 }
    }
}
