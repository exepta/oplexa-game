pub mod terrain_shader;
pub mod water_shader;

use crate::shader::terrain_shader::TerrainChunkGfxPlugin;
use crate::shader::water_shader::WaterGfxPlugin;
use bevy::prelude::*;

/// Represents world shader service used by the `generator::shader` module.
pub struct WorldShaderService;
impl Plugin for WorldShaderService {
    /// Builds this component for the `generator::shader` module.
    fn build(&self, app: &mut App) {
        app.add_plugins((TerrainChunkGfxPlugin, WaterGfxPlugin));
    }
}
