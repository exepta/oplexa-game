use crate::core::events::ui_events::ChestInventorySlotPayload;
use serde::{Deserialize, Serialize};

/// Client request to load one chest inventory from the authoritative server.
#[derive(Clone, Serialize, Deserialize)]
pub struct ClientChestInventoryOpen {
    pub world_pos: [i32; 3],
}

impl ClientChestInventoryOpen {
    /// Creates a new chest-open request.
    pub fn new(world_pos: [i32; 3]) -> Self {
        Self { world_pos }
    }
}

/// Client request to persist one chest inventory on the authoritative server.
#[derive(Clone, Serialize, Deserialize)]
pub struct ClientChestInventoryPersist {
    pub world_pos: [i32; 3],
    pub slots: Vec<ChestInventorySlotPayload>,
}

impl ClientChestInventoryPersist {
    /// Creates a new chest-persist request.
    pub fn new(world_pos: [i32; 3], slots: Vec<ChestInventorySlotPayload>) -> Self {
        Self { world_pos, slots }
    }
}

/// Server response containing the current contents of one chest inventory.
#[derive(Clone, Serialize, Deserialize)]
pub struct ServerChestInventoryContents {
    pub world_pos: [i32; 3],
    pub slots: Vec<ChestInventorySlotPayload>,
}

impl ServerChestInventoryContents {
    /// Creates a new chest-contents response.
    pub fn new(world_pos: [i32; 3], slots: Vec<ChestInventorySlotPayload>) -> Self {
        Self { world_pos, slots }
    }
}
