pub mod chunk;
mod shader;

use crate::generator::chunk::ChunkService;
use crate::generator::shader::WorldShaderService;
use bevy::prelude::*;

/// Represents generator module used by the `generator` module.
pub struct GeneratorModule;

impl Plugin for GeneratorModule {
    /// Builds this component for the `generator` module.
    fn build(&self, app: &mut App) {
        app.add_plugins((ChunkService, WorldShaderService));
    }
}
