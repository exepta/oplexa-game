mod world_gen_screen;
mod menu;
mod game;

use crate::graphic::world_gen_screen::WorldGenScreenPlugin;
use bevy::prelude::*;
use crate::graphic::game::GameUiModule;

pub struct GraphicModule;

impl Plugin for GraphicModule {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            WorldGenScreenPlugin,
            GameUiModule,
        ));
    }
}
