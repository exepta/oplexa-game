mod player_inventory_ui;
mod system_last_ui;
mod world_gen_screen;

use crate::graphic::player_inventory_ui::PlayerInventoryUiPlugin;
use crate::graphic::system_last_ui::SystemLastUiPlugin;
use crate::graphic::world_gen_screen::WorldGenScreenPlugin;
use bevy::prelude::*;

pub struct GraphicModule;

impl Plugin for GraphicModule {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            WorldGenScreenPlugin,
            SystemLastUiPlugin,
            PlayerInventoryUiPlugin,
        ));
    }
}
