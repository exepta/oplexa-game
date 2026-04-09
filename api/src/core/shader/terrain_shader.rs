use crate::core::world::block::BlockId;
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::render::render_resource::ShaderType;
use bevy::shader::ShaderRef;
use std::collections::HashMap;

/// Per-material terrain shader params.
#[derive(Clone, Copy, Default, ShaderType, Debug)]
pub struct TerrainChunkParams {
    pub leaf_cfg: Vec4,
    pub leaf_tint: Vec4,
    pub material_cfg: Vec4,
}

/// Material for greedy terrain chunk meshes using atlas-local tiling in shader.
#[derive(AsBindGroup, Asset, TypePath, Clone, Debug)]
pub struct TerrainChunkMaterial {
    #[uniform(0, visibility = "VertexFragment")]
    pub params: TerrainChunkParams,

    #[texture(1)]
    #[sampler(2)]
    pub atlas: Handle<Image>,

    pub alpha_mode: AlphaMode,
}

impl Material for TerrainChunkMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Path("shaders/terrain_chunk.wgsl".into())
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path("shaders/terrain_chunk.wgsl".into())
    }

    fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }
}

/// Lookup from block id to chunk terrain material handle.
#[derive(Resource, Default, Clone)]
pub struct TerrainChunkMatIndex(pub HashMap<BlockId, Handle<TerrainChunkMaterial>>);
