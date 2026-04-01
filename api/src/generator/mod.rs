pub mod chunk;
mod shader;

use crate::generator::chunk::ChunkService;
use crate::generator::shader::WorldShaderService;
use bevy::prelude::*;

pub struct GeneratorModule;

impl Plugin for GeneratorModule {
    fn build(&self, app: &mut App) {
        app.add_plugins((ChunkService, WorldShaderService));
    }
}
