pub mod chunk;

use crate::generator::chunk::ChunkService;
use bevy::prelude::*;

/// Represents generator module used by the `generator` module.
pub struct GeneratorModule;

impl Plugin for GeneratorModule {
    /// Builds this component for the `generator` module.
    fn build(&self, app: &mut App) {
        app.add_plugins(ChunkService);
    }
}
