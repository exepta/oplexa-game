use crate::core::inventory::items::ItemRegistry;
use crate::core::states::states::{AppState, BeforeUiState};
use crate::core::world::block::BlockRegistry;
use bevy::prelude::*;

pub struct BlockInternalRegistry;

impl Plugin for BlockInternalRegistry {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Preload), start_block_registry);
    }
}

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
    commands.insert_resource(block_registry);
    commands.insert_resource(item_registry);
    next.set(AppState::Screen(BeforeUiState::Menu));
}
