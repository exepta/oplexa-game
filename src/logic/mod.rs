mod entities;
mod events;
mod registry;
mod world;

use crate::logic::entities::EntitiesHandler;
use crate::logic::events::EventsHandler;
use crate::logic::registry::RegistryHandler;
use crate::logic::world::WorldHandler;
use bevy::prelude::*;

pub struct LogicModule;

impl Plugin for LogicModule {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            EventsHandler,
            EntitiesHandler,
            RegistryHandler,
            WorldHandler,
        ));
    }
}
