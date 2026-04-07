pub mod base;
pub mod cave;
pub mod fluid;
pub mod trees;

pub use base::{chunk_builder, chunk_gen, chunk_struct, chunk_utils, river_utils};
pub use cave::{cave_builder, cave_utils};
pub use fluid::{water_builder, water_utils};
pub use trees::{registry as tree_registry, tree_gen};

use crate::core::world::chunk::ChunkMap;
use crate::generator::chunk::chunk_builder::ChunkBuilder;
use crate::generator::chunk::water_builder::WaterBuilder;
use bevy::prelude::*;

/// Represents chunk service used by the `generator::chunk` module.
pub struct ChunkService;

impl Plugin for ChunkService {
    /// Builds this component for the `generator::chunk` module.
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkMap>();
        app.add_plugins((ChunkBuilder, WaterBuilder));
    }
}
