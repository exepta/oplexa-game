use bevy::prelude::*;

/// Represents connect to server request used by the `core::events::ui_events` module.
#[derive(Message, Clone, Debug, Default)]
pub struct ConnectToServerRequest {
    pub session_url: String,
    pub server_name: String,
}

/// Represents disconnect from server request used by the `core::events::ui_events` module.
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct DisconnectFromServerRequest;

/// Represents open to lan request used by the `core::events::ui_events` module.
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct OpenToLanRequest;

/// Represents stop lan host request used by the `core::events::ui_events` module.
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct StopLanHostRequest;

/// Represents chat submit request used by the `core::events::ui_events` module.
#[derive(Message, Clone, Debug, Default)]
pub struct ChatSubmitRequest {
    pub text: String,
}

/// Represents craft hand crafted request used by the `core::events::ui_events` module.
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct CraftHandCraftedRequest;

/// Represents drop item request used by the `core::events::ui_events` module.
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct DropItemRequest {
    pub item_id: u16,
    pub amount: u16,
    pub location: [i32; 3],
    pub spawn_translation: [f32; 3],
    pub initial_velocity: [f32; 3],
}

impl DropItemRequest {
    /// Creates a new instance for the `core::events::ui_events` module.
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
