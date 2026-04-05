use crate::core::shader::terrain_shader::{TerrainChunkMatIndex, TerrainChunkMaterial};
use crate::core::states::states::{AppState, LoadingStates};
use crate::core::world::block::BlockRegistry;
use bevy::prelude::*;
use std::collections::HashMap;

pub struct TerrainChunkGfxPlugin;

impl Plugin for TerrainChunkGfxPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<TerrainChunkMaterial>::default())
            .add_systems(
                OnEnter(AppState::Loading(LoadingStates::BaseGen)),
                setup_chunk_terrain_materials,
            );
    }
}

fn setup_chunk_terrain_materials(
    reg: Res<BlockRegistry>,
    mut mats: ResMut<Assets<TerrainChunkMaterial>>,
    mut cmds: Commands,
) {
    let mut index = HashMap::new();

    for id in 1..(reg.defs.len() as u16) {
        let alpha_mode = if reg.is_opaque(id) {
            AlphaMode::Opaque
        } else {
            AlphaMode::Blend
        };

        let handle = mats.add(TerrainChunkMaterial {
            atlas: reg.def(id).image.clone(),
            alpha_mode,
        });
        index.insert(id, handle);
    }

    cmds.insert_resource(TerrainChunkMatIndex(index));
}
