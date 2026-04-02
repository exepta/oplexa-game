use crate::core::entities::player::inventory::InventorySlot;
use crate::core::inventory::recipe::HAND_CRAFTED_INPUT_SLOTS;
use bevy::prelude::Resource;

#[derive(Resource, Clone, Debug)]
pub struct HandCraftedState {
    pub input_slots: [InventorySlot; HAND_CRAFTED_INPUT_SLOTS],
}

impl Default for HandCraftedState {
    fn default() -> Self {
        Self {
            input_slots: [InventorySlot::default(); HAND_CRAFTED_INPUT_SLOTS],
        }
    }
}
