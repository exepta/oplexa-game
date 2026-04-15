pub mod base;
pub mod cave;
pub mod fluid;
pub mod trees;
pub mod vegetation;

pub use base::{chunk_builder, chunk_gen, chunk_struct, chunk_utils, river_utils};
pub use cave::{cave_builder, cave_utils};
pub use fluid::fluid_gen;
pub use trees::{registry as tree_registry, tree_gen};
pub use vegetation::prop_gen as vegetation_prop_gen;

use crate::core::world::chunk::ChunkMap;
use crate::core::world::fluid::{FluidMap, WaterMeshIndex};
use crate::generator::chunk::chunk_builder::ChunkBuilder;
use bevy::prelude::*;

/// Represents chunk service used by the `generator::chunk` module.
pub struct ChunkService;

impl Plugin for ChunkService {
    /// Builds this component for the `generator::chunk` module.
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkMap>()
            .init_resource::<FluidMap>()
            .init_resource::<WaterMeshIndex>();
        app.add_plugins(ChunkBuilder);
    }
}
