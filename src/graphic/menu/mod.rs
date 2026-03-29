mod main_ui;
mod single_player_ui;

use bevy::prelude::*;
use main_ui::MainMenuPlugin;
use single_player_ui::SinglePlayerUiPlugin;

pub struct MenuUiModule;

impl Plugin for MenuUiModule {
    fn build(&self, app: &mut App) {
        app.add_plugins((MainMenuPlugin, SinglePlayerUiPlugin));
    }
}
