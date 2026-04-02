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
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            graphics: GraphicsConfig::default(),
            gameplay: GameplayConfig::default(),
            input: InputConfig::default(),
        }
    }
}

impl GlobalConfig {
    pub fn ensure_config_files_exist() {
        Self::ensure_default_config_file("config/graphics.toml", &GraphicsConfig::default());
        Self::ensure_default_config_file("config/gameplay.toml", &GameplayConfig::default());
        Self::ensure_default_config_file("config/input.toml", &InputConfig::default());
    }

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
        }
    }

    fn save<T: Serialize>(data: &T, path: &str) {
        let toml_string = toml::to_string_pretty(data).expect("Failed to serialize to TOML");
        fs::write(Path::new(path), toml_string).expect("Failed to write config file");
    }

    pub fn save_all(&self) {
        Self::ensure_config_files_exist();
        Self::save(&self.graphics, "config/graphics.toml");
        Self::save(&self.gameplay, "config/gameplay.toml");
        Self::save(&self.input, "config/input.toml");
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
    pub window_width: u32,

    /// The height of the application window (in logical pixels or units).
    pub window_height: u32,

    /// Whether the application should run in fullscreen mode.
    pub fullscreen: bool,

    /// Whether vertical synchronization (vsync) is enabled.
    pub vsync: bool,

    /// Identifier for the graphics backend to use (e.g., "wgpu", "OpenGL", "Vulkan").
    pub graphic_backend: String,

    /// The number of chunk generating ranges. 2 means 2 chunks in each direction.
    /// Note this build a cube around the player.
    pub chunk_range: i32,
}

impl Default for GraphicsConfig {
    fn default() -> Self {
        Self {
            window_width: 1270,
            window_height: 720,
            fullscreen: false,
            vsync: true,
            graphic_backend: String::from("AUTO"),
            chunk_range: 8,
        }
    }
}

// =======================================================
//                         Gameplay
// =======================================================

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GameplayConfig {
    /// Vertical sensitivity multiplier for look input.
    pub mouse_sensitivity_vertical: f32,

    /// Horizontal sensitivity multiplier for look input.
    pub mouse_sensitivity_horizontal: f32,
}

impl Default for GameplayConfig {
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
            inventory_recipe_open: default_inventory_recipe_open_key(),

            debug_overlay: String::from("F3"),
            chunk_grid: String::from("F9"),
            world_inspector: String::from("F1"),
        }
    }
}

fn default_drop_item_key() -> String {
    String::from("Q")
}

fn default_inventory_recipe_open_key() -> String {
    String::from("R")
}

// =======================================================
//                         Crosshair
// =======================================================

#[derive(Resource)]
pub struct CrosshairConfig {
    pub radius: f32,
    pub thickness: f32,
    pub segments: usize,
    pub color: Color,
    pub visible_when_unlocked: bool,
}

impl Default for CrosshairConfig {
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

#[derive(Resource, Clone)]
pub struct WorldGenConfig {
    pub seed: i32,
}

impl Default for WorldGenConfig {
    fn default() -> Self {
        Self { seed: 1337 }
    }
}
