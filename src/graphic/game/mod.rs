mod hud;
mod pause_menu_ui;
mod player_inventory_ui;
mod system_last_ui;

use bevy::prelude::*;
use hud::HudPlugin;
use pause_menu_ui::PauseMenuUiPlugin;
use player_inventory_ui::PlayerInventoryUiPlugin;
use system_last_ui::SystemLastUiPlugin;

pub struct GameUiModule;

impl Plugin for GameUiModule {
    fn build(&self, app: &mut App) {
        app.add_plugins((HudPlugin, PauseMenuUiPlugin, PlayerInventoryUiPlugin, SystemLastUiPlugin));
    }
}