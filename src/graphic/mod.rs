mod game;
mod menu;
mod world_gen_screen;
pub(crate) mod world_unload_ui;

use crate::graphic::game::GameUiModule;
use crate::graphic::menu::MenuUiModule;
use crate::graphic::world_gen_screen::WorldGenScreenPlugin;
use crate::graphic::world_unload_ui::WorldUnloadUiPlugin;
use bevy::prelude::*;

pub struct GraphicModule;

impl Plugin for GraphicModule {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            WorldGenScreenPlugin,
            WorldUnloadUiPlugin,
            MenuUiModule,
            GameUiModule,
        ));
    }
}
