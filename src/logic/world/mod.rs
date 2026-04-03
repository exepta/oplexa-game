mod save_service;

use crate::logic::world::save_service::WorldSaveService;
use bevy::prelude::*;

/// Represents world handler used by the `logic::world` module.
pub struct WorldHandler;

impl Plugin for WorldHandler {
    /// Builds this component for the `logic::world` module.
    fn build(&self, app: &mut App) {
        app.add_plugins(WorldSaveService);
    }
}
