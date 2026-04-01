use crate::core::inventory::items::{DEFAULT_ITEM_STACK_SIZE, ItemId, ItemRegistry};
use bevy::prelude::*;

pub const PLAYER_INVENTORY_SLOTS: usize = 12;
pub const PLAYER_INVENTORY_STACK_MAX: u16 = DEFAULT_ITEM_STACK_SIZE;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InventorySlot {
    pub item_id: ItemId,
    pub count: u16,
}

impl InventorySlot {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.item_id == 0 || self.count == 0
    }
}

#[derive(Resource, Debug)]
pub struct PlayerInventory {
    pub slots: [InventorySlot; PLAYER_INVENTORY_SLOTS],
}

impl Default for PlayerInventory {
    fn default() -> Self {
        Self {
            slots: [InventorySlot::default(); PLAYER_INVENTORY_SLOTS],
        }
    }
}

impl PlayerInventory {
    pub fn add_item(&mut self, item_id: ItemId, mut amount: u16, items: &ItemRegistry) -> u16 {
        if item_id == 0 || amount == 0 {
            return amount;
        }

        let stack_max = items
            .stack_limit(item_id)
            .min(PLAYER_INVENTORY_STACK_MAX)
            .max(1);

        for slot in &mut self.slots {
            if slot.item_id != item_id || slot.count >= stack_max {
                continue;
            }

            let free = stack_max - slot.count;
            let take = free.min(amount);
            slot.count += take;
            amount -= take;

            if amount == 0 {
                return 0;
            }
        }

        for slot in &mut self.slots {
            if !slot.is_empty() {
                continue;
            }

            let take = stack_max.min(amount);
            slot.item_id = item_id;
            slot.count = take;
            amount -= take;

            if amount == 0 {
                return 0;
            }
        }

        amount
    }

    pub fn total_items(&self) -> u32 {
        self.slots.iter().map(|slot| slot.count as u32).sum()
    }
}
