mod building;
mod hand_crafted;
mod loader;
mod registry;
mod state;
mod types;
mod work_table_crafting;
mod helpers;

pub const CRAFTING_SHAPED_RECIPE_KIND: &str = "shaped";
pub const CRAFTING_SHAPELESS_RECIPE_KIND: &str = "shapeless";

pub use building::{
    BUILDING_SHAPED_RECIPE_KIND, BuildingMaterialRequirement, BuildingMaterialRequirementSource,
    BuildingModelAnchor, BuildingModelRegisterAs, BuildingStructureBlockRegistration,
    BuildingStructureColliderSource, BuildingStructureRecipe, BuildingStructureRecipeRegistry,
    BuildingStructureTextureBinding, BuildingStructureTextureSource,
    load_building_structure_recipe_registry,
};
pub use hand_crafted::{HAND_CRAFTED_INPUT_SLOTS, HAND_CRAFTED_TYPE_LOCALIZED};
pub use loader::load_recipe_registry;
pub use registry::{RecipeRegistry, RecipeTypeHandler, RecipeTypeRegistry};
pub use state::{
    ActiveStructurePlacementState, ActiveStructureRecipeState, HandCraftedState,
    WorkTableCraftingState,
};
pub use types::{
    NamespacedKey, RecipeCraftingEntry, RecipeDefinition, RecipeInputRequirement, RecipeResultDef,
    RecipeResultTemplateDef, ResolvedRecipe,
};
pub use work_table_crafting::{
    WORK_TABLE_CRAFTING_INPUT_SLOTS, WORK_TABLE_CRAFTING_TYPE_LOCALIZED,
};
