pub(crate) mod block_event_handler;

use crate::logic::events::block_event_handler::BlockEventHandler;
use bevy::prelude::*;

pub struct EventsHandler;

impl Plugin for EventsHandler {
    fn build(&self, app: &mut App) {
        app.add_plugins(BlockEventHandler);
    }
}
