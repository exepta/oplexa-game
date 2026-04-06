use crate::core::shader::terrain_shader::{
    TerrainChunkMatIndex, TerrainChunkMaterial, TerrainChunkParams,
};
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
        let block_name = reg.def(id).name.as_str();
        let is_leaf = block_name.ends_with("_leaves_block");

        let alpha_mode = if is_leaf {
            AlphaMode::Mask(0.5)
        } else if reg.is_opaque(id) {
            AlphaMode::Opaque
        } else {
            AlphaMode::Blend
        };

        let params = if is_leaf {
            let leaf_tint = if block_name.starts_with("spruce_") {
                Vec4::new(0.88, 0.96, 0.90, 0.12)
            } else {
                Vec4::new(1.02, 1.08, 0.98, 0.10)
            };

            TerrainChunkParams {
                // x: enabled, y: cutout threshold, z: edge/noise strength, w: translucency
                leaf_cfg: Vec4::new(1.0, 0.45, 0.18, 0.45),
                // xyz: tint multiplier, w: per-pixel color variation strength
                leaf_tint,
            }
        } else {
            TerrainChunkParams {
                leaf_cfg: Vec4::ZERO,
                leaf_tint: Vec4::new(1.0, 1.0, 1.0, 0.0),
            }
        };

        let handle = mats.add(TerrainChunkMaterial {
            params,
            atlas: reg.def(id).image.clone(),
            alpha_mode,
        });
        index.insert(id, handle);
    }

    cmds.insert_resource(TerrainChunkMatIndex(index));
}
