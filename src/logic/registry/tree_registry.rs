use crate::core::states::states::AppState;
use crate::generator::chunk::trees::registry::TreeRegistry;
use bevy::prelude::*;

/// Represents tree internal registry used by the `logic::registry::tree_registry` module.
pub struct TreeInternalRegistry;

impl Plugin for TreeInternalRegistry {
    /// Builds this component for the `logic::registry::tree_registry` module.
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Preload), load_trees_from_folder);
    }
}

/// Loads trees from folder for the `logic::registry::tree_registry` module.
fn load_trees_from_folder(mut commands: Commands) {
    let registry = TreeRegistry::load_from_folder("assets/data/trees");
    info!(
        "Loaded {} tree family/families ({} variant(s)) from assets/data/trees",
        registry.family_count(),
        registry.variant_count()
    );
    commands.insert_resource(registry);
}
