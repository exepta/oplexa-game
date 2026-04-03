mod biome_registry;
mod block_registry;

use crate::logic::registry::biome_registry::BiomeInternalRegistry;
use crate::logic::registry::block_registry::BlockInternalRegistry;
use bevy::prelude::*;

/// Represents registry handler used by the `logic::registry` module.
pub struct RegistryHandler;

impl Plugin for RegistryHandler {
    /// Builds this component for the `logic::registry` module.
    fn build(&self, app: &mut App) {
        app.add_plugins((BlockInternalRegistry, BiomeInternalRegistry));
    }
}
