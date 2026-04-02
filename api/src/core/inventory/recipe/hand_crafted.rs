use crate::core::entities::player::inventory::InventorySlot;
use crate::core::inventory::items::ItemRegistry;
use crate::core::inventory::recipe::registry::{RecipeTypeHandler, RecipeTypeRegistry};
use crate::core::inventory::recipe::types::{NamespacedKey, RecipeInputRequirement};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

pub const HAND_CRAFTED_TYPE_LOCALIZED: &str = "oplexa:hand_crafted";
pub const HAND_CRAFTED_INPUT_SLOTS: usize = 2;

#[derive(Deserialize)]
struct HandCraftedDataJson {
    #[serde(default)]
    craft: HashMap<String, HandCraftedEntryJson>,
}

#[derive(Deserialize)]
struct HandCraftedEntryJson {
    item: String,
    #[serde(default = "default_required_count")]
    count: u16,
}

pub fn register_hand_crafted_recipe_type(recipe_type_registry: &mut RecipeTypeRegistry) {
    let Some(recipe_type) = NamespacedKey::parse(HAND_CRAFTED_TYPE_LOCALIZED) else {
        return;
    };
    recipe_type_registry.register_handler(
        recipe_type,
        RecipeTypeHandler {
            matcher: match_hand_crafted_inputs,
        },
    );
}

fn match_hand_crafted_inputs(
    data: &Value,
    input_slots: &[InventorySlot],
    item_registry: &ItemRegistry,
) -> Option<Vec<RecipeInputRequirement>> {
    if input_slots.len() < HAND_CRAFTED_INPUT_SLOTS {
        return None;
    }

    let parsed: HandCraftedDataJson = serde_json::from_value(data.clone()).ok()?;
    if parsed.craft.is_empty() {
        return None;
    }

    let mut required_slots = [false; HAND_CRAFTED_INPUT_SLOTS];
    let mut required_inputs = Vec::with_capacity(parsed.craft.len());

    for (slot_raw, required_entry) in parsed.craft {
        let slot_index = slot_raw.parse::<usize>().ok()?;
        if slot_index >= HAND_CRAFTED_INPUT_SLOTS {
            return None;
        }

        let required_item_id = item_registry.id_opt(required_entry.item.as_str())?;
        let required_count = required_entry.count.max(1);
        let current_slot = input_slots.get(slot_index)?;
        if current_slot.item_id != required_item_id || current_slot.count < required_count {
            return None;
        }

        required_slots[slot_index] = true;
        required_inputs.push(RecipeInputRequirement {
            slot_index,
            item_id: required_item_id,
            count: required_count,
        });
    }

    for (slot_index, slot) in input_slots
        .iter()
        .take(HAND_CRAFTED_INPUT_SLOTS)
        .enumerate()
    {
        if !required_slots[slot_index] && !slot.is_empty() {
            return None;
        }
    }

    required_inputs.sort_by_key(|entry| entry.slot_index);
    Some(required_inputs)
}

#[inline]
fn default_required_count() -> u16 {
    1
}
