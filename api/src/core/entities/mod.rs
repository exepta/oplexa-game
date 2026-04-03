pub mod player;

use crate::core::entities::player::PlayerModule;
use bevy::prelude::*;

/// Represents entities module used by the `core::entities` module.
pub struct EntitiesModule;

impl Plugin for EntitiesModule {
    /// Builds this component for the `core::entities` module.
    fn build(&self, app: &mut App) {
        app.add_plugins(PlayerModule);
    }
}
