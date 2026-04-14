use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::*;
use bevy::shader::ShaderRef;
pub use water_types::WaterParams;

mod water_types {
    #![allow(dead_code)]
    use bevy::prelude::*;
    use bevy::render::render_resource::ShaderType;

    /// Represents water params used by the `core::shader::water_shader` module.
    #[derive(Clone, Copy, Default, ShaderType, Debug)]
    pub struct WaterParams {
        pub uv_rect: Vec4,
        pub flow: Vec4,
        pub t_misc: Vec4,
        pub tint: Vec4,
    }
}

/// Represents water material used by the `core::shader::water_shader` module.
#[derive(AsBindGroup, Asset, TypePath, Clone, Debug)]
pub struct WaterMaterial {
    #[uniform(0, visibility = "VertexFragment")]
    pub params: WaterParams,

    #[texture(1)]
    #[sampler(2)]
    pub atlas: Handle<Image>,
}

/// Represents water mat handle used by the `core::shader::water_shader` module.
#[derive(Resource, Clone)]
pub struct WaterMatHandle(pub Handle<WaterMaterial>);

impl Material for WaterMaterial {
    /// Runs the `vertex_shader` routine for vertex shader in the `core::shader::water_shader` module.
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Path("shaders/water.wgsl".into())
    }
    /// Runs the `fragment_shader` routine for fragment shader in the `core::shader::water_shader` module.
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path("shaders/water.wgsl".into())
    }
    /// Runs the `alpha_mode` routine for alpha mode in the `core::shader::water_shader` module.
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    /// Runs the `specialize` routine for specialize in the `core::shader::water_shader` module.
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        if let Some(ds) = descriptor.depth_stencil.as_mut() {
            // Write depth for the nearest water layer so back layers/sides do not
            // shine through and produce noisy stacked transparency artifacts.
            ds.depth_write_enabled = true;
        }
        // Cull back-faces to avoid rendering both sides of the same water plane.
        descriptor.primitive.cull_mode = Some(Face::Back);
        if let Some(fragment) = descriptor.fragment.as_mut() {
            if let Some(Some(tgt)) = fragment.targets.get_mut(0) {
                tgt.blend = Some(BlendState::ALPHA_BLENDING);
            }
        }
        Ok(())
    }
}
