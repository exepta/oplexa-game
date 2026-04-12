use api::core::commands::GameModeKind;
use api::core::entities::player::inventory::{InventorySlot, PLAYER_INVENTORY_SLOTS};
use bevy::math::IVec2;
use std::collections::HashSet;
use std::time::Instant;

/// Represents hosted player used by the `models` module.
pub struct HostedPlayer {
    pub player_id: u64,
    pub username: String,
    pub client_uuid: String,
    pub game_mode: GameModeKind,
    pub translation: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub inventory_slots: [InventorySlot; PLAYER_INVENTORY_SLOTS],
    pub last_seen: Instant,
    pub streamed_chunks: HashSet<IVec2>,
}

/// Represents hosted drop used by the `models` module.
pub struct HostedDrop {
    pub drop_id: u64,
    pub location: [i32; 3],
    pub item_id: u16,
    pub block_id: u16,
    pub has_motion: bool,
    pub spawn_translation: [f32; 3],
    pub initial_velocity: [f32; 3],
}
