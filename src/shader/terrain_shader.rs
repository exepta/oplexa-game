use crate::core::shader::terrain_shader::{
    TerrainChunkMatIndex, TerrainChunkMaterial, TerrainChunkParams,
};
use crate::core::states::states::{AppState, LoadingStates};
use crate::core::world::block::BlockRegistry;
use bevy::prelude::*;
use std::collections::HashMap;

pub struct TerrainChunkGfxPlugin;
const PROP_WIND_STRENGTH: f32 = 0.055;
const PROP_WIND_FREQUENCY: f32 = 1.75;
const LEAF_WIND_STRENGTH: f32 = 0.040;
const LEAF_WIND_FREQUENCY: f32 = 1.25;
const CUTOUT_ALPHA_THRESHOLD: f32 = 0.5;

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
            )
            .add_systems(
                Update,
                tick_terrain_material_time
                    .run_if(resource_exists::<TerrainChunkMatIndex>)
                    .run_if(resource_exists::<State<AppState>>),
            );
    }
}

#[inline]
fn material_cfg_for_block(reg: &BlockRegistry, id: u16) -> Vec4 {
    if reg.is_prop(id) {
        return Vec4::new(1.0, PROP_WIND_STRENGTH, PROP_WIND_FREQUENCY, 0.0);
    }

    if reg.stats(id).foliage {
        return Vec4::new(0.0, LEAF_WIND_STRENGTH, LEAF_WIND_FREQUENCY, 0.0);
    }

    Vec4::ZERO
}

#[inline]
fn terrain_alpha_mode_for_block(reg: &BlockRegistry, id: u16) -> AlphaMode {
    if reg.is_opaque(id) {
        return AlphaMode::Opaque;
    }

    if reg.stats(id).foliage || reg.is_prop(id) {
        // Cutout rendering keeps depth stable for dense overlapping leaves/props.
        return AlphaMode::Mask(CUTOUT_ALPHA_THRESHOLD);
    }

    AlphaMode::Blend
}

fn setup_chunk_terrain_materials(
    reg: Res<BlockRegistry>,
    mut mats: ResMut<Assets<TerrainChunkMaterial>>,
    mut cmds: Commands,
) {
    let mut index = HashMap::new();

    for id in 1..(reg.defs.len() as u16) {
        let alpha_mode = terrain_alpha_mode_for_block(&reg, id);

        let params = TerrainChunkParams {
            leaf_cfg: Vec4::ZERO,
            leaf_tint: Vec4::new(1.0, 1.0, 1.0, 0.0),
            material_cfg: material_cfg_for_block(&reg, id),
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
        let alpha_mode = terrain_alpha_mode_for_block(&reg, id);

        let params = TerrainChunkParams {
            leaf_cfg: Vec4::ZERO,
            leaf_tint: Vec4::new(1.0, 1.0, 1.0, 0.0),
            material_cfg: material_cfg_for_block(&reg, id),
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

fn tick_terrain_material_time(
    time: Res<Time>,
    index: Res<TerrainChunkMatIndex>,
    mut mats: ResMut<Assets<TerrainChunkMaterial>>,
) {
    let now = time.elapsed_secs();
    for handle in index.0.values() {
        if let Some(material) = mats.get_mut(handle) {
            material.params.material_cfg.w = now;
        }
    }
}
