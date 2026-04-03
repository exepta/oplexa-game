mod entities;
pub(crate) mod events;
mod registry;
mod world;

use crate::logic::entities::EntitiesHandler;
use crate::logic::events::EventsHandler;
use crate::logic::registry::RegistryHandler;
use crate::logic::world::WorldHandler;
use bevy::prelude::*;

/// Represents logic module used by the `logic` module.
pub struct LogicModule;

impl Plugin for LogicModule {
    /// Builds this component for the `logic` module.
    fn build(&self, app: &mut App) {
        app.add_plugins((
            EventsHandler,
            EntitiesHandler,
            RegistryHandler,
            WorldHandler,
        ));
    }
}
