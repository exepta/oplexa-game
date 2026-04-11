mod building;
mod hand_crafted;
mod loader;
mod registry;
mod state;
mod types;

pub use building::{
    BUILDING_SHAPED_RECIPE_KIND, BuildingMaterialRequirement, BuildingModelAnchor,
    BuildingModelRegisterAs, BuildingStructureBlockRegistration, BuildingStructureColliderSource,
    BuildingStructureRecipe, BuildingStructureRecipeRegistry,
    load_building_structure_recipe_registry,
};
pub use hand_crafted::{HAND_CRAFTED_INPUT_SLOTS, HAND_CRAFTED_TYPE_LOCALIZED};
pub use loader::load_recipe_registry;
pub use registry::{RecipeRegistry, RecipeTypeHandler, RecipeTypeRegistry};
pub use state::{ActiveStructurePlacementState, ActiveStructureRecipeState, HandCraftedState};
pub use types::{
    NamespacedKey, RecipeCraftingEntry, RecipeDefinition, RecipeInputRequirement, RecipeResultDef,
    RecipeResultTemplateDef, ResolvedRecipe,
};
