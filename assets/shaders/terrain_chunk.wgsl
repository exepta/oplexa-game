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

struct TerrainParams {
  // x: leaf enabled, y: cutout threshold, z: edge/noise strength, w: translucency
  leaf_cfg: vec4<f32>,
  // xyz: leaf tint multiplier, w: per-pixel color variation strength
  leaf_tint: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: TerrainParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var atlas_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var atlas_smp: sampler;

fn hash12(p: vec2<f32>) -> f32 {
  let h = dot(p, vec2<f32>(127.1, 311.7));
  return fract(sin(h) * 43758.5453);
}

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

  // Avoid atlas bleeding at tile borders by sampling slightly inset from tile edges.
  let atlas_dims_u = textureDimensions(atlas_tex);
  let atlas_dims = vec2<f32>(f32(atlas_dims_u.x), f32(atlas_dims_u.y));
  let atlas_texel = 1.0 / max(atlas_dims, vec2<f32>(1.0, 1.0));
  let inset = min(tile_size * 0.25, atlas_texel);
  let safe_tile_min = tile_min + inset;
  let safe_tile_max = tile_max - inset;
  let safe_tile_size = max(safe_tile_max - safe_tile_min, vec2<f32>(1e-6, 1e-6));

  // Repeat inside one atlas tile, independent of greedy quad size.
  let tiled = fract(in.uv_local);
  let atlas_uv = safe_tile_min + tiled * safe_tile_size;

  // Provide explicit gradients from unwrapped UVs to avoid seam artifacts from fract()-based derivatives.
  let duvdx = dpdx(in.uv_local) * safe_tile_size;
  let duvdy = dpdy(in.uv_local) * safe_tile_size;
  let tex = textureSampleGrad(atlas_tex, atlas_smp, atlas_uv, duvdx, duvdy);

  if (tex.a <= 0.001) {
    discard;
  }

  let n = normalize(in.normal_ws);
  let l = normalize(vec3<f32>(0.35, 1.0, 0.2));

  if (params.leaf_cfg.x > 0.5) {
    let n0 = hash12(tiled * 31.7 + in.world_pos.xz * 0.21);
    let n1 = hash12((tiled.yx + vec2<f32>(7.9, 13.3)) * 23.1 + in.world_pos.zy * 0.17);
    let leaf_noise = 0.5 * (n0 + n1);

    let edge_dist = min(min(tiled.x, tiled.y), min(1.0 - tiled.x, 1.0 - tiled.y));
    let edge_factor = 1.0 - smoothstep(0.08, 0.28, edge_dist);
    let cutout = params.leaf_cfg.y
      + (leaf_noise - 0.5) * params.leaf_cfg.z
      + edge_factor * params.leaf_cfg.z * 0.45;

    let alpha_from_color = clamp(tex.g * 1.20 - tex.r * 0.25 - tex.b * 0.25, 0.0, 1.0);
    let alpha_src = max(tex.a, alpha_from_color);
    if (alpha_src < cutout) {
      discard;
    }

    let variation = ((leaf_noise - 0.5) * 2.0) * params.leaf_tint.w;
    let tint = params.leaf_tint.xyz * (1.0 + variation);
    let leaf_rgb = tex.rgb * tint;

    let front = 0.35 + 0.65 * max(dot(n, l), 0.0);
    let back = max(dot(-n, l), 0.0) * params.leaf_cfg.w;
    let lit_leaf = vec4<f32>(leaf_rgb * (front + back * 0.55), 1.0);
    return apply_fog(fog, lit_leaf, in.world_pos, view.world_position.xyz);
  }

  let diff = 0.35 + 0.65 * max(dot(n, l), 0.0);
  let lit = vec4<f32>(tex.rgb * diff, tex.a);
  return apply_fog(fog, lit, in.world_pos, view.world_position.xyz);
}
