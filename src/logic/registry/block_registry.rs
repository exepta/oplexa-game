use crate::core::inventory::items::ItemRegistry;
use crate::core::inventory::recipe::{RecipeTypeRegistry, load_recipe_registry};
use crate::core::states::states::{AppState, BeforeUiState};
use crate::core::world::block::BlockRegistry;
use bevy::prelude::*;

/// Represents block internal registry used by the `logic::registry::block_registry` module.
pub struct BlockInternalRegistry;

impl Plugin for BlockInternalRegistry {
    /// Builds this component for the `logic::registry::block_registry` module.
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Preload), start_block_registry);
    }
}

/// Starts block registry for the `logic::registry::block_registry` module.
fn start_block_registry(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut next: ResMut<NextState<AppState>>,
) {
    let block_registry = BlockRegistry::load_all(&asset_server, &mut materials, "assets/blocks");
    let item_registry = ItemRegistry::load_all(
        &asset_server,
        &mut materials,
        "assets/items",
        &block_registry,
    );
    let recipe_type_registry = RecipeTypeRegistry::with_defaults();
    let recipe_registry =
        load_recipe_registry("assets/recipes", &item_registry, &recipe_type_registry);

    info!(
        "Loaded {} block(s) from assets/blocks",
        block_registry.defs.len().saturating_sub(1)
    );
    info!(
        "Loaded {} item(s) from assets/items",
        item_registry.defs.len().saturating_sub(1)
    );

    commands.insert_resource(block_registry);
    commands.insert_resource(item_registry);
    commands.insert_resource(recipe_type_registry);
    commands.insert_resource(recipe_registry);
    next.set(AppState::Screen(BeforeUiState::Menu));
}
