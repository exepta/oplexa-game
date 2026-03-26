mod biome_registry;
mod block_registry;

use crate::logic::registry::biome_registry::BiomeInternalRegistry;
use crate::logic::registry::block_registry::BlockInternalRegistry;
use bevy::prelude::*;

pub struct RegistryHandler;

impl Plugin for RegistryHandler {
    fn build(&self, app: &mut App) {
        app.add_plugins((BlockInternalRegistry, BiomeInternalRegistry));
    }
}
