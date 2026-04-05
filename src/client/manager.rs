use super::chunk_debug_grid::{sync_chunk_grid_meshes, toggle_chunk_grid};
use crate::core::CoreModule;
use crate::core::config::GlobalConfig;
use crate::core::debug::WorldInspectorState;
use crate::generator::GeneratorModule;
use crate::graphic::GraphicModule;
use crate::logic::LogicModule;
use crate::shader::WorldShaderService;
use crate::utils::key_utils::convert;
use bevy::light::DirectionalLightShadowMap;
use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

/// Represents manager plugin used by the `client` module.
pub struct ManagerPlugin;

impl Plugin for ManagerPlugin {
    /// Builds this component for the `client` module.
    fn build(&self, app: &mut App) {
        app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default());
        app.add_plugins((
            CoreModule,
            LogicModule,
            GeneratorModule,
            WorldShaderService,
            GraphicModule,
        ));
        app.add_systems(Startup, setup_shadow_map);
        app.add_systems(
            Update,
            (
                toggle_world_inspector,
                toggle_chunk_grid,
                sync_chunk_grid_meshes,
            ),
        );
    }
}

/// Runs the `setup_shadow_map` routine for setup shadow map in the `client` module.
fn setup_shadow_map(mut commands: Commands) {
    commands.insert_resource(DirectionalLightShadowMap { size: 1024 });
}

/// Runs the `toggle_world_inspector` routine for toggle world inspector in the `client` module.
fn toggle_world_inspector(
    mut debug_context: ResMut<WorldInspectorState>,
    keyboard: Res<ButtonInput<KeyCode>>,
    game_config: Res<GlobalConfig>,
) {
    let key = convert(game_config.input.world_inspector.as_str())
        .expect("Invalid key for world inspector");
    if keyboard.just_pressed(key) {
        debug_context.0 = !debug_context.0;
        info!("World Inspector: {}", debug_context.0);
    }
}
