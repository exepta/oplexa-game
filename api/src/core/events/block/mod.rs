pub mod block_player_events;

use crate::core::events::block::block_player_events::*;
use bevy::prelude::*;

/// Represents block events module used by the `core::events::block` module.
pub struct BlockEventsModule;

impl Plugin for BlockEventsModule {
    /// Builds this component for the `core::events::block` module.
    fn build(&self, app: &mut App) {
        app.add_message::<BlockBreakByPlayerEvent>()
            .add_message::<BlockPlaceByPlayerEvent>();
    }
}
