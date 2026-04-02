mod hand_crafted;
mod loader;
mod registry;
mod state;
mod types;

pub use hand_crafted::{HAND_CRAFTED_INPUT_SLOTS, HAND_CRAFTED_TYPE_LOCALIZED};
pub use loader::load_recipe_registry;
pub use registry::{RecipeRegistry, RecipeTypeHandler, RecipeTypeRegistry};
pub use state::HandCraftedState;
pub use types::{
    NamespacedKey, RecipeCraftingEntry, RecipeDefinition, RecipeInputRequirement, RecipeResultDef,
    ResolvedRecipe,
};
