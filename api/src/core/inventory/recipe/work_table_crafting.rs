use crate::core::entities::player::inventory::InventorySlot;
use crate::core::inventory::items::{ItemId, ItemRegistry};
use crate::core::inventory::recipe::registry::{RecipeTypeHandler, RecipeTypeRegistry};
use crate::core::inventory::recipe::types::{NamespacedKey, RecipeInputRequirement};
use crate::core::inventory::recipe::{CRAFTING_SHAPED_RECIPE_KIND, CRAFTING_SHAPELESS_RECIPE_KIND};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

pub const WORK_TABLE_CRAFTING_TYPE_LOCALIZED: &str = "oplexa:work_table_crafting";
pub const WORK_TABLE_CRAFTING_INPUT_SLOTS: usize = 8;

/// Represents work table shaped recipe data used by the `core::inventory::recipe::work_table_crafting` module.
#[derive(Deserialize)]
struct WorkTableShapedDataJson {
    #[serde(default)]
    craft: HashMap<String, WorkTableEntryJson>,
}

/// Represents work table shapeless recipe data used by the `core::inventory::recipe::work_table_crafting` module.
#[derive(Deserialize)]
struct WorkTableShapelessDataJson {
    #[serde(default)]
    ingredients: Vec<WorkTableEntryJson>,
}

/// Represents work table recipe entry used by the `core::inventory::recipe::work_table_crafting` module.
#[derive(Deserialize, Clone)]
struct WorkTableEntryJson {
    #[serde(default)]
    item: String,
    #[serde(default)]
    group: String,
    #[serde(default = "default_required_count")]
    count: u16,
}

/// Registers work table recipe type for the `core::inventory::recipe::work_table_crafting` module.
pub fn register_work_table_recipe_type(recipe_type_registry: &mut RecipeTypeRegistry) {
    let Some(recipe_type) = NamespacedKey::parse(WORK_TABLE_CRAFTING_TYPE_LOCALIZED) else {
        return;
    };

    recipe_type_registry.register_handler(
        recipe_type,
        RecipeTypeHandler {
            matcher: match_work_table_inputs,
        },
    );
}

/// Matches work table crafting inputs for shaped or shapeless recipes.
fn match_work_table_inputs(
    recipe_format: &str,
    data: &Value,
    input_slots: &[InventorySlot],
    item_registry: &ItemRegistry,
) -> Option<Vec<RecipeInputRequirement>> {
    if input_slots.len() < WORK_TABLE_CRAFTING_INPUT_SLOTS {
        return None;
    }

    if recipe_format
        .trim()
        .eq_ignore_ascii_case(CRAFTING_SHAPED_RECIPE_KIND)
    {
        return match_shaped_work_table_inputs(data, input_slots, item_registry);
    }

    if recipe_format
        .trim()
        .eq_ignore_ascii_case(CRAFTING_SHAPELESS_RECIPE_KIND)
    {
        return match_shapeless_work_table_inputs(data, input_slots, item_registry);
    }

    None
}

fn match_shaped_work_table_inputs(
    data: &Value,
    input_slots: &[InventorySlot],
    item_registry: &ItemRegistry,
) -> Option<Vec<RecipeInputRequirement>> {
    let parsed: WorkTableShapedDataJson = serde_json::from_value(data.clone()).ok()?;
    if parsed.craft.is_empty() {
        return None;
    }

    let mut required_slots = [false; WORK_TABLE_CRAFTING_INPUT_SLOTS];
    let mut required_inputs = Vec::with_capacity(parsed.craft.len());

    for (slot_raw, required_entry) in parsed.craft {
        let slot_index = slot_raw.parse::<usize>().ok()?;
        if slot_index >= WORK_TABLE_CRAFTING_INPUT_SLOTS {
            return None;
        }

        let required_count = required_entry.count.max(1);
        let current_slot = input_slots.get(slot_index)?;
        if current_slot.count < required_count {
            return None;
        }

        let required_item_id =
            resolve_required_item_id(&required_entry, current_slot, item_registry)?;

        required_slots[slot_index] = true;
        required_inputs.push(RecipeInputRequirement {
            slot_index,
            item_id: required_item_id,
            count: required_count,
        });
    }

    for (slot_index, slot) in input_slots
        .iter()
        .take(WORK_TABLE_CRAFTING_INPUT_SLOTS)
        .enumerate()
    {
        if !required_slots[slot_index] && !slot.is_empty() {
            return None;
        }
    }

    required_inputs.sort_by_key(|entry| entry.slot_index);
    Some(required_inputs)
}

fn match_shapeless_work_table_inputs(
    data: &Value,
    input_slots: &[InventorySlot],
    item_registry: &ItemRegistry,
) -> Option<Vec<RecipeInputRequirement>> {
    let parsed = serde_json::from_value::<WorkTableShapelessDataJson>(data.clone()).ok();
    let ingredients = if let Some(parsed) = parsed {
        if !parsed.ingredients.is_empty() {
            parsed.ingredients
        } else {
            let fallback: WorkTableShapedDataJson = serde_json::from_value(data.clone()).ok()?;
            if fallback.craft.is_empty() {
                return None;
            }
            fallback.craft.into_values().collect()
        }
    } else {
        let fallback: WorkTableShapedDataJson = serde_json::from_value(data.clone()).ok()?;
        if fallback.craft.is_empty() {
            return None;
        }
        fallback.craft.into_values().collect()
    };

    let mut remaining_counts = [0u16; WORK_TABLE_CRAFTING_INPUT_SLOTS];
    let mut slot_item_ids = [0u16; WORK_TABLE_CRAFTING_INPUT_SLOTS];
    let mut consumed_counts = [0u16; WORK_TABLE_CRAFTING_INPUT_SLOTS];

    for slot_index in 0..WORK_TABLE_CRAFTING_INPUT_SLOTS {
        let slot = input_slots.get(slot_index)?;
        remaining_counts[slot_index] = slot.count;
        slot_item_ids[slot_index] = slot.item_id;
    }

    for ingredient in ingredients {
        let mut required_left = ingredient.count.max(1);
        let explicit_item_id = if ingredient.item.trim().is_empty() {
            None
        } else {
            Some(item_registry.id_opt(ingredient.item.as_str())?)
        };
        let group = normalize_recipe_group(ingredient.group.as_str());
        if explicit_item_id.is_none() && group.is_empty() {
            return None;
        }

        for slot_index in 0..WORK_TABLE_CRAFTING_INPUT_SLOTS {
            if required_left == 0 {
                break;
            }

            let available = remaining_counts[slot_index];
            if available == 0 {
                continue;
            }

            let slot_item_id = slot_item_ids[slot_index];
            if let Some(required_item_id) = explicit_item_id {
                if slot_item_id != required_item_id {
                    continue;
                }
            } else if !item_registry.has_group(slot_item_id, group.as_str()) {
                continue;
            }

            let consumed = available.min(required_left);
            remaining_counts[slot_index] -= consumed;
            consumed_counts[slot_index] += consumed;
            required_left -= consumed;
        }

        if required_left != 0 {
            return None;
        }
    }

    // Shapeless still requires that no unrelated occupied slot exists.
    // Extra amount in one used stack is allowed (minimum-count semantics),
    // but additional occupied slots that were not consumed are not.
    for slot_index in 0..WORK_TABLE_CRAFTING_INPUT_SLOTS {
        let slot = input_slots.get(slot_index)?;
        if !slot.is_empty() && consumed_counts[slot_index] == 0 {
            return None;
        }
    }

    let mut required_inputs = Vec::new();
    for slot_index in 0..WORK_TABLE_CRAFTING_INPUT_SLOTS {
        let consumed = consumed_counts[slot_index];
        if consumed == 0 {
            continue;
        }
        required_inputs.push(RecipeInputRequirement {
            slot_index,
            item_id: slot_item_ids[slot_index],
            count: consumed,
        });
    }

    required_inputs.sort_by_key(|entry| entry.slot_index);
    Some(required_inputs)
}

fn resolve_required_item_id(
    entry: &WorkTableEntryJson,
    current_slot: &InventorySlot,
    item_registry: &ItemRegistry,
) -> Option<ItemId> {
    if !entry.item.trim().is_empty() {
        let required_item_id = item_registry.id_opt(entry.item.as_str())?;
        if current_slot.item_id != required_item_id {
            return None;
        }
        return Some(required_item_id);
    }

    let required_group = normalize_recipe_group(entry.group.as_str());
    if required_group.is_empty() {
        return None;
    }
    if !item_registry.has_group(current_slot.item_id, required_group.as_str()) {
        return None;
    }
    Some(current_slot.item_id)
}

#[inline]
fn default_required_count() -> u16 {
    1
}

#[inline]
fn normalize_recipe_group(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}
