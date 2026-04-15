use crate::core::inventory::items::ItemRegistry;
use crate::core::inventory::recipe::{
    RecipeTypeRegistry, load_building_structure_recipe_registry, load_recipe_registry,
};
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
    let mut block_registry =
        BlockRegistry::load_all(&asset_server, &mut materials, "assets/blocks");
    let mut item_registry = ItemRegistry::load_all(
        &asset_server,
        &mut materials,
        "assets/items",
        &block_registry,
    );
    let recipe_type_registry = RecipeTypeRegistry::with_defaults();
    let recipe_registry =
        load_recipe_registry("assets/recipes", &item_registry, &recipe_type_registry);
    let mut structure_recipe_registry =
        load_building_structure_recipe_registry("assets/recipes/structures", &item_registry);
    for recipe in &mut structure_recipe_registry.recipes {
        let Some(registration) = recipe.model_meta.block_registration.as_mut() else {
            continue;
        };
        let block_id = block_registry.ensure_runtime_block(
            &asset_server,
            &mut materials,
            registration.localized_name.as_str(),
            registration.name.as_str(),
            recipe.model_meta.stats.clone(),
        );
        let mut runtime_block_ids = vec![block_id];
        for rotation_quarters in 1..4u8 {
            let localized_name = format!("{}_r{}", registration.localized_name, rotation_quarters);
            let name_key = format!("{}_R{}", registration.name, rotation_quarters);
            let rotated_block_id = block_registry.ensure_runtime_block(
                &asset_server,
                &mut materials,
                localized_name.as_str(),
                name_key.as_str(),
                recipe.model_meta.stats.clone(),
            );
            runtime_block_ids.push(rotated_block_id);
        }
        // Structure runtime blocks are persisted placeholders (multiplayer/server-side state).
        // Their visible mesh comes from the GLB scene, so keep the voxel block itself invisible.
        let placeholder_material = materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 1.0, 1.0, 0.0),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        });
        for runtime_block_id in runtime_block_ids {
            if let Some(def) = block_registry.defs.get_mut(runtime_block_id as usize) {
                def.material = placeholder_material.clone();
            }
        }
        let item_id = if registration.item_view {
            item_registry.ensure_runtime_block_item(&asset_server, &block_registry, block_id)
        } else {
            None
        };
        registration.block_id = Some(block_id);
        registration.item_id = item_id;
    }

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
    commands.insert_resource(structure_recipe_registry);
    next.set(AppState::Screen(BeforeUiState::Menu));
}
