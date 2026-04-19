pub mod chunk;

use crate::generator::chunk::ChunkRuntimeService;
use bevy::prelude::*;

/// Wires up the client-side streamed chunk runtime.
pub struct ChunkRuntimeModule;

impl Plugin for ChunkRuntimeModule {
    /// Builds this component for the `generator` module.
    fn build(&self, app: &mut App) {
        app.add_plugins(ChunkRuntimeService);
    }
}
