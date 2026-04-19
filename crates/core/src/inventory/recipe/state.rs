use crate::core::entities::player::inventory::InventorySlot;
use crate::core::inventory::recipe::{HAND_CRAFTED_INPUT_SLOTS, WORK_TABLE_CRAFTING_INPUT_SLOTS};
use bevy::prelude::Resource;

/// Represents hand crafted state used by the `core::inventory::recipe::state` module.
#[derive(Resource, Clone, Debug)]
pub struct HandCraftedState {
    pub input_slots: [InventorySlot; HAND_CRAFTED_INPUT_SLOTS],
}

impl Default for HandCraftedState {
    /// Runs the `default` routine for default in the `core::inventory::recipe::state` module.
    fn default() -> Self {
        Self {
            input_slots: [InventorySlot::default(); HAND_CRAFTED_INPUT_SLOTS],
        }
    }
}

/// Represents work table crafting state used by the `core::inventory::recipe::state` module.
#[derive(Resource, Clone, Debug)]
pub struct WorkTableCraftingState {
    pub input_slots: [InventorySlot; WORK_TABLE_CRAFTING_INPUT_SLOTS],
}

impl Default for WorkTableCraftingState {
    /// Runs the `default` routine for default in the `core::inventory::recipe::state` module.
    fn default() -> Self {
        Self {
            input_slots: [InventorySlot::default(); WORK_TABLE_CRAFTING_INPUT_SLOTS],
        }
    }
}

/// Selected structure recipe that is currently armed for placement.
#[derive(Resource, Clone, Debug, Default)]
pub struct ActiveStructureRecipeState {
    pub selected_recipe_name: Option<String>,
}

/// Runtime placement state for active structure previews.
#[derive(Resource, Clone, Debug, Default)]
pub struct ActiveStructurePlacementState {
    /// Rotation around +Y.
    ///
    /// Stored as 45° units for save/runtime compatibility, but current gameplay
    /// snaps to right angles (90° steps).
    pub rotation_quarters: i32,
}
