use crate::core::entities::player::inventory::PlayerInventory;
use crate::core::events::ui_events::CraftHandCraftedRequest;
use crate::core::inventory::items::ItemRegistry;
use crate::core::inventory::recipe::{
    HAND_CRAFTED_TYPE_LOCALIZED, HandCraftedState, RecipeRegistry, RecipeTypeRegistry,
    ResolvedRecipe,
};
use bevy::prelude::*;

pub struct RecipeHandlerPlugin;

impl Plugin for RecipeHandlerPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CraftHandCraftedRequest>()
            .add_systems(Update, process_hand_crafted_requests);
    }
}

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

pub fn resolve_hand_crafted_recipe(
    hand_crafted: &HandCraftedState,
    recipe_registry: &RecipeRegistry,
    recipe_type_registry: &RecipeTypeRegistry,
    item_registry: &ItemRegistry,
) -> Option<ResolvedRecipe> {
    let resolved = recipe_registry.find_match_for_slots(
        &hand_crafted.input_slots,
        item_registry,
        recipe_type_registry,
    )?;
    if resolved.recipe_type.localized_name() != HAND_CRAFTED_TYPE_LOCALIZED {
        return None;
    }
    Some(resolved)
}

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
        "Recipe triggered: kind='{}' type='{}' result='{}' x{} from '{}'",
        resolved.recipe_kind,
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
        "Craft executed: kind='{}' type='{}' result='{}' x{} (inserted={}) from '{}'",
        resolved.recipe_kind,
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

fn can_consume_all_inputs(hand_crafted: &HandCraftedState, resolved: &ResolvedRecipe) -> bool {
    resolved.required_inputs.iter().all(|required| {
        hand_crafted
            .input_slots
            .get(required.slot_index)
            .is_some_and(|slot| slot.item_id == required.item_id && slot.count >= required.count)
    })
}

fn consume_recipe_inputs(hand_crafted: &mut HandCraftedState, resolved: &ResolvedRecipe) {
    for required in &resolved.required_inputs {
        let Some(slot) = hand_crafted.input_slots.get_mut(required.slot_index) else {
            continue;
        };
        if slot.count <= required.count {
            *slot = Default::default();
            continue;
        }
        slot.count -= required.count;
    }
}
