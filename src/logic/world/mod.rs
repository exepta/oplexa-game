mod save_service;

use crate::logic::world::save_service::WorldSaveService;
use bevy::prelude::*;

pub struct WorldHandler;

impl Plugin for WorldHandler {
    fn build(&self, app: &mut App) {
        app.add_plugins(WorldSaveService);
    }
}
