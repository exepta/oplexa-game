pub mod block;
pub mod chunk_events;
pub mod ui_events;

use crate::core::events::block::BlockEventsModule;
use crate::core::events::chunk_events::*;
use crate::core::events::ui_events::{
    ChatSubmitRequest, ConnectToServerRequest, DisconnectFromServerRequest, DropItemRequest,
};
use bevy::prelude::*;

/// Represents event module used by the `core::events` module.
pub struct EventModule;

impl Plugin for EventModule {
    /// Builds this component for the `core::events` module.
    fn build(&self, app: &mut App) {
        app.add_plugins(BlockEventsModule)
            .add_message::<ChunkUnloadEvent>()
            .add_message::<ChunkGeneratedEvent>()
            .add_message::<SubChunkNeedRemeshEvent>()
            .add_message::<ConnectToServerRequest>()
            .add_message::<DisconnectFromServerRequest>()
            .add_message::<ChatSubmitRequest>()
            .add_message::<DropItemRequest>();
    }
}
