use crate::core::world::block::BlockId;
use bevy::prelude::*;

pub const PLAYER_INVENTORY_SLOTS: usize = 12;
pub const PLAYER_INVENTORY_STACK_MAX: u16 = 128;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InventorySlot {
    pub block_id: BlockId,
    pub count: u16,
}

impl InventorySlot {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.block_id == 0 || self.count == 0
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
    pub fn add_block(&mut self, block_id: BlockId, mut amount: u16) -> u16 {
        if block_id == 0 || amount == 0 {
            return amount;
        }

        for slot in &mut self.slots {
            if slot.block_id != block_id || slot.count >= PLAYER_INVENTORY_STACK_MAX {
                continue;
            }

            let free = PLAYER_INVENTORY_STACK_MAX - slot.count;
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

            let take = PLAYER_INVENTORY_STACK_MAX.min(amount);
            slot.block_id = block_id;
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
