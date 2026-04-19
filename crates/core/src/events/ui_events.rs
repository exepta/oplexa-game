use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Represents connect to server request used by the `core::events::ui_events` module.
#[derive(Message, Clone, Debug, Default)]
pub struct ConnectToServerRequest {
    pub session_url: String,
    pub server_name: String,
}

/// Represents disconnect from server request used by the `core::events::ui_events` module.
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct DisconnectFromServerRequest;

/// Represents chat submit request used by the `core::events::ui_events` module.
#[derive(Message, Clone, Debug, Default)]
pub struct ChatSubmitRequest {
    pub text: String,
}

/// Represents craft hand crafted request used by the `core::events::ui_events` module.
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct CraftHandCraftedRequest;

/// Represents craft work table request used by the `core::events::ui_events` module.
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct CraftWorkTableRequest;

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

/// Represents request to open structure build menu (hammer).
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct OpenStructureBuildMenuRequest;

/// Represents request to open workbench recipe menu.
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct OpenWorkbenchMenuRequest {
    pub world_pos: [i32; 3],
}

/// Represents request to open chest inventory UI for one chest block.
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct OpenChestInventoryMenuRequest {
    pub world_pos: [i32; 3],
}

/// Represents event that chest inventory UI has been opened for one chest block.
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct ChestInventoryUiOpened {
    pub world_pos: [i32; 3],
}

/// Requests a chest/container snapshot for HUD preview or non-modal UI reads.
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct ChestInventorySnapshotRequest {
    pub world_pos: [i32; 3],
}

/// Represents event that chest inventory UI has been closed for one chest block.
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct ChestInventoryUiClosed {
    pub world_pos: [i32; 3],
}

/// One serialized chest-slot payload used for UI sync and persistence.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChestInventorySlotPayload {
    pub slot: u16,
    pub item: String,
    pub count: u16,
}

/// Synchronizes chest inventory contents from gameplay layer into UI.
#[derive(Message, Clone, Debug, Default)]
pub struct ChestInventoryContentsSync {
    pub world_pos: [i32; 3],
    pub slots: Vec<ChestInventorySlotPayload>,
}

/// Persists UI-edited chest inventory contents back into gameplay layer.
#[derive(Message, Clone, Debug, Default)]
pub struct ChestInventoryPersistRequest {
    pub world_pos: [i32; 3],
    pub slots: Vec<ChestInventorySlotPayload>,
}
