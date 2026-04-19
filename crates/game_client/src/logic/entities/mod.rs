pub mod player;

use crate::logic::entities::player::PlayerServices;
use bevy::prelude::*;

/// Represents entities handler used by the `logic::entities` module.
pub struct EntitiesHandler;

impl Plugin for EntitiesHandler {
    /// Builds this component for the `logic::entities` module.
    fn build(&self, app: &mut App) {
        app.add_plugins(PlayerServices);
    }
}
