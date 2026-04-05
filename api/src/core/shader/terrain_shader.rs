use crate::core::world::block::BlockId;
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use std::collections::HashMap;

/// Material for greedy terrain chunk meshes using atlas-local tiling in shader.
#[derive(AsBindGroup, Asset, TypePath, Clone, Debug)]
pub struct TerrainChunkMaterial {
    #[texture(0)]
    #[sampler(1)]
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
