use bevy::prelude::*;

/// Represents block break by player event used by the `core::events::block::block_player_events` module.
#[derive(Message, Clone, Debug)]
pub struct BlockBreakByPlayerEvent {
    pub chunk_coord: IVec2,
    pub location: IVec3,
    pub chunk_x: u8,
    pub chunk_y: u16,
    pub chunk_z: u8,
    pub block_id: u16,
    pub drop_item_id: u16,
    pub block_name: String,
    pub drops_item: bool,
}

/// Represents an observed block break from world replication.
/// This event is local-only and must not be re-sent over the network.
#[derive(Message, Clone, Debug)]
pub struct BlockBreakObservedEvent {
    pub location: IVec3,
}

/// Represents block place by player event used by the `core::events::block::block_player_events` module.
#[derive(Message, Clone, Debug)]
pub struct BlockPlaceByPlayerEvent {
    pub location: IVec3,
    pub block_id: u16,
    pub stacked_block_id: u16,
    pub block_name: String,
}

/// Represents an observed block placement from world replication.
/// This event is local-only and must not be re-sent over the network.
#[derive(Message, Clone, Debug)]
pub struct BlockPlaceObservedEvent {
    pub location: IVec3,
    pub block_id: u16,
}
