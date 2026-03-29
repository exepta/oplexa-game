mod game;
mod menu;
mod world_gen_screen;

use crate::graphic::game::GameUiModule;
use crate::graphic::menu::MenuUiModule;
use crate::graphic::world_gen_screen::WorldGenScreenPlugin;
use bevy::prelude::*;

pub struct GraphicModule;

impl Plugin for GraphicModule {
    fn build(&self, app: &mut App) {
        app.add_plugins((WorldGenScreenPlugin, MenuUiModule, GameUiModule));
    }
}
