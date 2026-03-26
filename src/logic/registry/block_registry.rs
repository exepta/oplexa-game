use crate::core::states::states::{AppState, LoadingStates};
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
    let registry = BlockRegistry::load_all(&asset_server, &mut materials, "assets/blocks");
    commands.insert_resource(registry);
    next.set(AppState::Loading(LoadingStates::BaseGen));
}
