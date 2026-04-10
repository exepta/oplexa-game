use crate::core::entities::player::inventory::{
    InventorySlot, PLAYER_INVENTORY_SLOTS, PLAYER_INVENTORY_STACK_MAX,
};
use serde::{Deserialize, Serialize};

/// Wire format for one inventory slot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventorySlotState {
    pub item_id: u16,
    pub count: u16,
}

impl From<InventorySlot> for InventorySlotState {
    fn from(slot: InventorySlot) -> Self {
        Self {
            item_id: slot.item_id,
            count: slot.count,
        }
    }
}

impl From<InventorySlotState> for InventorySlot {
    fn from(slot: InventorySlotState) -> Self {
        if slot.item_id == 0 || slot.count == 0 {
            return InventorySlot::default();
        }

        InventorySlot {
            item_id: slot.item_id,
            count: slot.count.min(PLAYER_INVENTORY_STACK_MAX),
        }
    }
}

/// Client to server inventory snapshot.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientInventorySync {
    pub slots: Vec<InventorySlotState>,
}

impl ClientInventorySync {
    /// Creates a snapshot from a fixed inventory array.
    pub fn from_slots(slots: &[InventorySlot; PLAYER_INVENTORY_SLOTS]) -> Self {
        Self {
            slots: slots
                .iter()
                .copied()
                .map(InventorySlotState::from)
                .collect(),
        }
    }

    /// Converts this message into a fixed inventory array.
    pub fn to_slots(&self) -> [InventorySlot; PLAYER_INVENTORY_SLOTS] {
        let mut out = [InventorySlot::default(); PLAYER_INVENTORY_SLOTS];
        for (index, slot) in self
            .slots
            .iter()
            .copied()
            .take(PLAYER_INVENTORY_SLOTS)
            .enumerate()
        {
            out[index] = InventorySlot::from(slot);
        }
        out
    }
}
