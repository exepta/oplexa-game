mod world_gen_screen;

use crate::graphic::world_gen_screen::WorldGenScreenPlugin;
use bevy::prelude::*;

pub struct GraphicModule;

impl Plugin for GraphicModule {
    fn build(&self, app: &mut App) {
        app.add_plugins(WorldGenScreenPlugin);
    }
}
