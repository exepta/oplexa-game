use crate::core::entities::player::inventory::InventorySlot;
use crate::core::inventory::items::ItemRegistry;
use crate::core::inventory::recipe::hand_crafted::register_hand_crafted_recipe_type;
use crate::core::inventory::recipe::types::{
    NamespacedKey, RecipeDefinition, RecipeInputRequirement, ResolvedRecipe,
};
use bevy::prelude::Resource;
use serde_json::Value;
use std::collections::HashMap;

/// Type alias for recipe matcher fn used by the `core::inventory::recipe::registry` module.
pub type RecipeMatcherFn =
    fn(&Value, &[InventorySlot], &ItemRegistry) -> Option<Vec<RecipeInputRequirement>>;

/// Represents recipe type handler used by the `core::inventory::recipe::registry` module.
#[derive(Clone)]
pub struct RecipeTypeHandler {
    pub matcher: RecipeMatcherFn,
}

/// Represents recipe type registry used by the `core::inventory::recipe::registry` module.
#[derive(Resource, Default, Clone)]
pub struct RecipeTypeRegistry {
    handlers: HashMap<NamespacedKey, RecipeTypeHandler>,
}

impl RecipeTypeRegistry {
    /// Runs the `with_defaults` routine for with defaults in the `core::inventory::recipe::registry` module.
    pub fn with_defaults() -> Self {
        let mut registry = Self::default();
        registry.register_default_types();
        registry
    }

    /// Registers default types for the `core::inventory::recipe::registry` module.
    pub fn register_default_types(&mut self) {
        register_hand_crafted_recipe_type(self);
    }

    /// Registers handler for the `core::inventory::recipe::registry` module.
    pub fn register_handler(&mut self, recipe_type: NamespacedKey, handler: RecipeTypeHandler) {
        self.handlers.insert(recipe_type, handler);
    }

    /// Checks whether handler in the `core::inventory::recipe::registry` module.
    #[inline]
    pub fn has_handler(&self, recipe_type: &NamespacedKey) -> bool {
        self.handlers.contains_key(recipe_type)
    }

    /// Runs the `try_match` routine for try match in the `core::inventory::recipe::registry` module.
    pub fn try_match(
        &self,
        recipe_type: &NamespacedKey,
        data: &Value,
        input_slots: &[InventorySlot],
        item_registry: &ItemRegistry,
    ) -> Option<Vec<RecipeInputRequirement>> {
        let handler = self.handlers.get(recipe_type)?;
        (handler.matcher)(data, input_slots, item_registry)
    }
}

/// Represents recipe registry used by the `core::inventory::recipe::registry` module.
#[derive(Resource, Default, Clone, Debug)]
pub struct RecipeRegistry {
    pub recipes: Vec<RecipeDefinition>,
}

impl RecipeRegistry {
    /// Finds match for slots for the `core::inventory::recipe::registry` module.
    pub fn find_match_for_slots(
        &self,
        input_slots: &[InventorySlot],
        item_registry: &ItemRegistry,
        recipe_type_registry: &RecipeTypeRegistry,
    ) -> Option<ResolvedRecipe> {
        for recipe in &self.recipes {
            for crafting in &recipe.crafting {
                let Some(required_inputs) = recipe_type_registry.try_match(
                    &crafting.recipe_type,
                    &crafting.data,
                    input_slots,
                    item_registry,
                ) else {
                    continue;
                };

                return Some(ResolvedRecipe {
                    source_path: recipe.source_path.clone(),
                    recipe_kind: recipe.recipe_kind.clone(),
                    recipe_type: crafting.recipe_type.clone(),
                    required_inputs,
                    result: recipe.result.clone(),
                });
            }
        }
        None
    }
}
