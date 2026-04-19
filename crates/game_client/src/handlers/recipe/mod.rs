use crate::core::entities::player::inventory::{InventorySlot, PlayerInventory};
use crate::core::events::ui_events::{CraftHandCraftedRequest, CraftWorkTableRequest};
use crate::core::inventory::items::ItemRegistry;
use crate::core::inventory::recipe::{
    HAND_CRAFTED_TYPE_LOCALIZED, HandCraftedState, RecipeRegistry, RecipeTypeRegistry,
    ResolvedRecipe, WORK_TABLE_CRAFTING_TYPE_LOCALIZED, WorkTableCraftingState,
};
use bevy::prelude::*;

/// Represents recipe handler plugin used by the `handlers::recipe` module.
pub struct RecipeHandlerPlugin;

impl Plugin for RecipeHandlerPlugin {
    /// Builds this component for the `handlers::recipe` module.
    fn build(&self, app: &mut App) {
        app.add_message::<CraftHandCraftedRequest>()
            .add_message::<CraftWorkTableRequest>()
            .add_systems(
                Update,
                (process_hand_crafted_requests, process_work_table_requests),
            );
    }
}

/// Defines the possible hand crafted execution result variants in the `handlers::recipe` module.
#[derive(Clone, Debug)]
pub enum HandCraftedExecutionResult {
    Ignored,
    Crafted {
        recipe: ResolvedRecipe,
        inserted: u16,
    },
    InventoryFull {
        recipe: ResolvedRecipe,
    },
}

/// Defines the possible work table execution result variants in the `handlers::recipe` module.
#[derive(Clone, Debug)]
pub enum WorkTableExecutionResult {
    Ignored,
    Crafted {
        recipe: ResolvedRecipe,
        inserted: u16,
    },
    InventoryFull {
        recipe: ResolvedRecipe,
    },
}

/// Runs the `resolve_hand_crafted_recipe` routine for resolve hand crafted recipe in the `handlers::recipe` module.
pub fn resolve_hand_crafted_recipe(
    hand_crafted: &HandCraftedState,
    recipe_registry: &RecipeRegistry,
    recipe_type_registry: &RecipeTypeRegistry,
    item_registry: &ItemRegistry,
) -> Option<ResolvedRecipe> {
    let resolved = recipe_registry.find_match_for_slots_with_type(
        &hand_crafted.input_slots,
        item_registry,
        recipe_type_registry,
        Some(HAND_CRAFTED_TYPE_LOCALIZED),
    )?;
    Some(resolved)
}

/// Runs the `resolve_work_table_recipe` routine for resolve work table recipe in the `handlers::recipe` module.
pub fn resolve_work_table_recipe(
    work_table: &WorkTableCraftingState,
    recipe_registry: &RecipeRegistry,
    recipe_type_registry: &RecipeTypeRegistry,
    item_registry: &ItemRegistry,
) -> Option<ResolvedRecipe> {
    let resolved = recipe_registry.find_match_for_slots_with_type(
        &work_table.input_slots,
        item_registry,
        recipe_type_registry,
        Some(WORK_TABLE_CRAFTING_TYPE_LOCALIZED),
    )?;
    Some(resolved)
}

/// Runs the `execute_hand_crafted_recipe` routine for execute hand crafted recipe in the `handlers::recipe` module.
pub fn execute_hand_crafted_recipe(
    inventory: &mut PlayerInventory,
    hand_crafted: &mut HandCraftedState,
    recipe_registry: &RecipeRegistry,
    recipe_type_registry: &RecipeTypeRegistry,
    item_registry: &ItemRegistry,
) -> HandCraftedExecutionResult {
    let Some(resolved) = resolve_hand_crafted_recipe(
        hand_crafted,
        recipe_registry,
        recipe_type_registry,
        item_registry,
    ) else {
        return HandCraftedExecutionResult::Ignored;
    };

    debug!(
        "Recipe triggered: format='{}' type='{}' result='{}' x{} from '{}'",
        resolved.recipe_format,
        resolved.recipe_type,
        resolved.result.item_localized_name,
        resolved.result.count,
        resolved.source_path
    );

    if !can_consume_all_inputs(hand_crafted, &resolved) {
        return HandCraftedExecutionResult::Ignored;
    }

    let mut probe_inventory = inventory.clone();
    let leftover = probe_inventory.add_item(
        resolved.result.item_id,
        resolved.result.count,
        item_registry,
    );
    if leftover != 0 {
        return HandCraftedExecutionResult::InventoryFull { recipe: resolved };
    }

    consume_recipe_inputs(hand_crafted, &resolved);
    let leftover_after_consume = inventory.add_item(
        resolved.result.item_id,
        resolved.result.count,
        item_registry,
    );
    let inserted = resolved.result.count.saturating_sub(leftover_after_consume);

    debug!(
        "Craft executed: format='{}' type='{}' result='{}' x{} (inserted={}) from '{}'",
        resolved.recipe_format,
        resolved.recipe_type,
        resolved.result.item_localized_name,
        resolved.result.count,
        inserted,
        resolved.source_path
    );

    HandCraftedExecutionResult::Crafted {
        recipe: resolved,
        inserted,
    }
}

/// Runs the `execute_work_table_recipe` routine for execute work table recipe in the `handlers::recipe` module.
pub fn execute_work_table_recipe(
    inventory: &mut PlayerInventory,
    work_table: &mut WorkTableCraftingState,
    recipe_registry: &RecipeRegistry,
    recipe_type_registry: &RecipeTypeRegistry,
    item_registry: &ItemRegistry,
) -> WorkTableExecutionResult {
    let Some(resolved) = resolve_work_table_recipe(
        work_table,
        recipe_registry,
        recipe_type_registry,
        item_registry,
    ) else {
        return WorkTableExecutionResult::Ignored;
    };

    debug!(
        "Recipe triggered: format='{}' type='{}' result='{}' x{} from '{}'",
        resolved.recipe_format,
        resolved.recipe_type,
        resolved.result.item_localized_name,
        resolved.result.count,
        resolved.source_path
    );

    if !can_consume_all_inputs_for_slots(&work_table.input_slots, &resolved) {
        return WorkTableExecutionResult::Ignored;
    }

    let mut probe_inventory = inventory.clone();
    let leftover = probe_inventory.add_item(
        resolved.result.item_id,
        resolved.result.count,
        item_registry,
    );
    if leftover != 0 {
        return WorkTableExecutionResult::InventoryFull { recipe: resolved };
    }

    consume_recipe_inputs_from_slots(&mut work_table.input_slots, &resolved);
    let leftover_after_consume = inventory.add_item(
        resolved.result.item_id,
        resolved.result.count,
        item_registry,
    );
    let inserted = resolved.result.count.saturating_sub(leftover_after_consume);

    debug!(
        "Craft executed: format='{}' type='{}' result='{}' x{} (inserted={}) from '{}'",
        resolved.recipe_format,
        resolved.recipe_type,
        resolved.result.item_localized_name,
        resolved.result.count,
        inserted,
        resolved.source_path
    );

    WorkTableExecutionResult::Crafted {
        recipe: resolved,
        inserted,
    }
}

/// Processes hand crafted requests for the `handlers::recipe` module.
fn process_hand_crafted_requests(
    mut requests: MessageReader<CraftHandCraftedRequest>,
    mut inventory: ResMut<PlayerInventory>,
    mut hand_crafted: ResMut<HandCraftedState>,
    recipe_registry: Option<Res<RecipeRegistry>>,
    recipe_type_registry: Option<Res<RecipeTypeRegistry>>,
    item_registry: Option<Res<ItemRegistry>>,
) {
    let request_count = requests.read().count();
    if request_count == 0 {
        return;
    }

    let (Some(recipe_registry), Some(recipe_type_registry), Some(item_registry)) =
        (recipe_registry, recipe_type_registry, item_registry)
    else {
        return;
    };

    for _ in 0..request_count {
        let result = execute_hand_crafted_recipe(
            &mut inventory,
            &mut hand_crafted,
            &recipe_registry,
            &recipe_type_registry,
            &item_registry,
        );
        if let HandCraftedExecutionResult::InventoryFull { recipe } = result {
            debug!(
                "Craft blocked (inventory full): result='{}' x{} from '{}'",
                recipe.result.item_localized_name, recipe.result.count, recipe.source_path
            );
        }
    }
}

/// Processes work table requests for the `handlers::recipe` module.
fn process_work_table_requests(
    mut requests: MessageReader<CraftWorkTableRequest>,
    mut inventory: ResMut<PlayerInventory>,
    mut work_table: ResMut<WorkTableCraftingState>,
    recipe_registry: Option<Res<RecipeRegistry>>,
    recipe_type_registry: Option<Res<RecipeTypeRegistry>>,
    item_registry: Option<Res<ItemRegistry>>,
) {
    let request_count = requests.read().count();
    if request_count == 0 {
        return;
    }

    let (Some(recipe_registry), Some(recipe_type_registry), Some(item_registry)) =
        (recipe_registry, recipe_type_registry, item_registry)
    else {
        return;
    };

    for _ in 0..request_count {
        let result = execute_work_table_recipe(
            &mut inventory,
            &mut work_table,
            &recipe_registry,
            &recipe_type_registry,
            &item_registry,
        );
        if let WorkTableExecutionResult::InventoryFull { recipe } = result {
            debug!(
                "Craft blocked (inventory full): result='{}' x{} from '{}'",
                recipe.result.item_localized_name, recipe.result.count, recipe.source_path
            );
        }
    }
}

/// Checks whether consume all inputs in the `handlers::recipe` module.
fn can_consume_all_inputs(hand_crafted: &HandCraftedState, resolved: &ResolvedRecipe) -> bool {
    can_consume_all_inputs_for_slots(&hand_crafted.input_slots, resolved)
}

/// Checks whether consume all inputs for arbitrary slots in the `handlers::recipe` module.
fn can_consume_all_inputs_for_slots(
    input_slots: &[InventorySlot],
    resolved: &ResolvedRecipe,
) -> bool {
    resolved.required_inputs.iter().all(|required| {
        input_slots
            .get(required.slot_index)
            .is_some_and(|slot| slot.item_id == required.item_id && slot.count >= required.count)
    })
}

/// Runs the `consume_recipe_inputs` routine for consume recipe inputs in the `handlers::recipe` module.
fn consume_recipe_inputs(hand_crafted: &mut HandCraftedState, resolved: &ResolvedRecipe) {
    consume_recipe_inputs_from_slots(&mut hand_crafted.input_slots, resolved);
}

/// Runs the `consume_recipe_inputs_from_slots` routine for consume recipe inputs from arbitrary slots in the `handlers::recipe` module.
fn consume_recipe_inputs_from_slots(input_slots: &mut [InventorySlot], resolved: &ResolvedRecipe) {
    for required in &resolved.required_inputs {
        let Some(slot) = input_slots.get_mut(required.slot_index) else {
            continue;
        };
        if slot.count <= required.count {
            *slot = Default::default();
            continue;
        }
        slot.count -= required.count;
    }
}
