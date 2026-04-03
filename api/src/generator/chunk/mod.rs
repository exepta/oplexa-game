pub mod cave_builder;
pub mod cave_utils;
pub mod chunk_builder;
pub mod chunk_gen;
pub mod chunk_struct;
pub mod chunk_utils;
pub mod river_utils;
pub mod water_builder;
pub mod water_utils;

use crate::core::world::chunk::ChunkMap;
use crate::generator::chunk::cave_builder::CaveBuilder;
use crate::generator::chunk::chunk_builder::ChunkBuilder;
use crate::generator::chunk::water_builder::WaterBuilder;
use bevy::prelude::*;

/// Represents chunk service used by the `generator::chunk` module.
pub struct ChunkService;

impl Plugin for ChunkService {
    /// Builds this component for the `generator::chunk` module.
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkMap>();
        app.add_plugins((ChunkBuilder, WaterBuilder, CaveBuilder));
    }
}
