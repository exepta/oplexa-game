use bevy::prelude::*;

#[derive(Message, Clone, Debug, Default)]
pub struct ConnectToServerRequest {
    pub session_url: String,
    pub server_name: String,
}

#[derive(Message, Clone, Copy, Debug, Default)]
pub struct DisconnectFromServerRequest;

#[derive(Message, Clone, Copy, Debug, Default)]
pub struct OpenToLanRequest;

#[derive(Message, Clone, Copy, Debug, Default)]
pub struct StopLanHostRequest;

#[derive(Message, Clone, Copy, Debug, Default)]
pub struct DropItemRequest {
    pub item_id: u16,
    pub amount: u16,
    pub location: [i32; 3],
    pub spawn_translation: [f32; 3],
    pub initial_velocity: [f32; 3],
}

impl DropItemRequest {
    pub fn new(
        item_id: u16,
        amount: u16,
        location: [i32; 3],
        spawn_translation: [f32; 3],
        initial_velocity: [f32; 3],
    ) -> Self {
        Self {
            item_id,
            amount,
            location,
            spawn_translation,
            initial_velocity,
        }
    }
}
