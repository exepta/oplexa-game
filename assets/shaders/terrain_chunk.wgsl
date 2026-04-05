#import bevy_pbr::mesh_view_bindings::{view, fog}
#import bevy_pbr::pbr_functions::apply_fog
#import bevy_pbr::view_transformations::position_world_to_clip

struct VOut {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv_local: vec2<f32>,
  @location(1) tile_rect: vec4<f32>,
  @location(2) normal_ws: vec3<f32>,
  @location(3) world_pos: vec3<f32>,
};

struct VertexInput {
  @location(0) position: vec3<f32>,
  @location(1) normal: vec3<f32>,
  @location(2) uv_local: vec2<f32>,
  @location(5) tile_rect: vec4<f32>,
  @builtin(instance_index) instance_index: u32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var atlas_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var atlas_smp: sampler;

@vertex
fn vertex(v: VertexInput) -> VOut {
  var out: VOut;
  let world_from_local = bevy_pbr::mesh_functions::get_world_from_local(v.instance_index);
  let world_pos4 = world_from_local * vec4<f32>(v.position, 1.0);
  out.clip = position_world_to_clip(world_pos4.xyz);
  out.normal_ws = normalize(bevy_pbr::mesh_functions::mesh_normal_local_to_world(v.normal, v.instance_index));
  out.uv_local = v.uv_local;
  out.tile_rect = v.tile_rect;
  out.world_pos = world_pos4.xyz;
  return out;
}

@fragment
fn fragment(in: VOut) -> @location(0) vec4<f32> {
  let tile_min = in.tile_rect.xy;
  let tile_max = in.tile_rect.zw;
  let tile_size = max(tile_max - tile_min, vec2<f32>(1e-6, 1e-6));

  // Repeat inside one atlas tile, independent of greedy quad size.
  let tiled = fract(in.uv_local);
  let atlas_uv = tile_min + tiled * tile_size;
  let tex = textureSample(atlas_tex, atlas_smp, atlas_uv);

  if (tex.a <= 0.001) {
    discard;
  }

  let n = normalize(in.normal_ws);
  let l = normalize(vec3<f32>(0.35, 1.0, 0.2));
  let diff = 0.35 + 0.65 * max(dot(n, l), 0.0);
  let lit = vec4<f32>(tex.rgb * diff, tex.a);
  return apply_fog(fog, lit, in.world_pos, view.world_position.xyz);
}
