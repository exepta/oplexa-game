pub(crate) mod block_event_handler;

use crate::logic::events::block_event_handler::BlockEventHandler;
use crate::handlers::items::WorldItemHandlerPlugin;
use crate::handlers::recipe::RecipeHandlerPlugin;
use bevy::prelude::*;

/// Represents events handler used by the `logic::events` module.
pub struct EventsHandler;

impl Plugin for EventsHandler {
    /// Builds this component for the `logic::events` module.
    fn build(&self, app: &mut App) {
        app.add_plugins((
            BlockEventHandler,
            WorldItemHandlerPlugin,
            RecipeHandlerPlugin,
        ));
    }
}
