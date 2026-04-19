pub mod runtime;

pub use runtime::{chunk_meshing, chunk_runtime, chunk_runtime_types};

use crate::core::world::chunk::ChunkMap;
use crate::core::world::fluid::{FluidMap, WaterMeshIndex};
use crate::generator::chunk::chunk_runtime::ChunkRuntimePlugin;
use bevy::prelude::*;

/// Registers streamed chunk state and runtime systems on the client.
pub struct ChunkRuntimeService;

impl Plugin for ChunkRuntimeService {
    /// Builds this component for the `generator::chunk` module.
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkMap>()
            .init_resource::<FluidMap>()
            .init_resource::<WaterMeshIndex>();
        app.add_plugins(ChunkRuntimePlugin);
    }
}
