pub mod block_player_events;

use crate::core::events::block::block_player_events::*;
use bevy::prelude::*;

pub struct BlockEventsModule;

impl Plugin for BlockEventsModule {
    fn build(&self, app: &mut App) {
        app.add_message::<BlockBreakByPlayerEvent>()
            .add_message::<BlockPlaceByPlayerEvent>();
    }
}
