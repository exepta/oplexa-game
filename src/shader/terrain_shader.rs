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
        app.init_resource::<TerrainChunkMatIndex>()
            .add_plugins(MaterialPlugin::<TerrainChunkMaterial>::default())
            .add_systems(
                OnEnter(AppState::Loading(LoadingStates::BaseGen)),
                setup_chunk_terrain_materials,
            )
            .add_systems(
                Update,
                ensure_chunk_terrain_materials
                    .run_if(resource_exists::<BlockRegistry>)
                    .run_if(resource_exists::<State<AppState>>),
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

        let params = TerrainChunkParams {
            leaf_cfg: Vec4::ZERO,
            leaf_tint: Vec4::new(1.0, 1.0, 1.0, 0.0),
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

fn ensure_chunk_terrain_materials(
    reg: Res<BlockRegistry>,
    mut mats: ResMut<Assets<TerrainChunkMaterial>>,
    mut index_res: ResMut<TerrainChunkMatIndex>,
    app_state: Res<State<AppState>>,
) {
    if !matches!(
        app_state.get(),
        AppState::Loading(LoadingStates::BaseGen)
            | AppState::Loading(LoadingStates::WaterGen)
            | AppState::InGame(_)
    ) {
        return;
    }

    if index_res.0.len() >= reg.defs.len().saturating_sub(1) {
        return;
    }

    let mut index = HashMap::new();
    for id in 1..(reg.defs.len() as u16) {
        let alpha_mode = if reg.is_opaque(id) {
            AlphaMode::Opaque
        } else {
            AlphaMode::Blend
        };

        let params = TerrainChunkParams {
            leaf_cfg: Vec4::ZERO,
            leaf_tint: Vec4::new(1.0, 1.0, 1.0, 0.0),
        };

        let handle = mats.add(TerrainChunkMaterial {
            params,
            atlas: reg.def(id).image.clone(),
            alpha_mode,
        });
        index.insert(id, handle);
    }

    index_res.0 = index;
}
