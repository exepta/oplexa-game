use bevy::prelude::*;

#[derive(Message, Clone, Copy, Debug, Default)]
pub struct ConnectToServerRequest;

#[derive(Message, Clone, Copy, Debug, Default)]
pub struct DropItemRequest {
    pub block_id: u16,
    pub amount: u16,
    pub location: [i32; 3],
    pub spawn_translation: [f32; 3],
    pub initial_velocity: [f32; 3],
}

impl DropItemRequest {
    pub fn new(
        block_id: u16,
        amount: u16,
        location: [i32; 3],
        spawn_translation: [f32; 3],
        initial_velocity: [f32; 3],
    ) -> Self {
        Self {
            block_id,
            amount,
            location,
            spawn_translation,
            initial_velocity,
        }
    }
}
