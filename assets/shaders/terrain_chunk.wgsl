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
  // x: leaf enabled, y: cutout threshold, z: leaf protrusion strength, w: translucency
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
  let base_pos4 = world_from_local * vec4<f32>(v.position, 1.0);
  let normal_ws = normalize(bevy_pbr::mesh_functions::mesh_normal_local_to_world(v.normal, v.instance_index));
  var world_pos = base_pos4.xyz;

  // Puff foliage a bit outward to break rigid block silhouettes.
  if (params.leaf_cfg.x > 0.5) {
    let cell = floor(world_pos.xz * 2.2 + world_pos.y * 0.25);
    let puff = hash12(cell + vec2<f32>(5.3, 17.9));
    // Requested overhang: approx. 0.20..0.40 block units.
    let push = (0.20 + 0.20 * puff) * max(params.leaf_cfg.z, 0.0);
    world_pos = world_pos + normal_ws * push;
  }

  out.clip = position_world_to_clip(world_pos);
  out.normal_ws = normal_ws;
  out.uv_local = v.uv_local;
  out.tile_rect = v.tile_rect;
  out.world_pos = world_pos;
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

  let n = normalize(in.normal_ws);
  let l = normalize(vec3<f32>(0.35, 1.0, 0.2));

  if (params.leaf_cfg.x > 0.5) {
    // Pixel-center sampling for foliage avoids bright halos from bilinear blending at cutout edges.
    let leaf_px = (floor(atlas_uv * atlas_dims) + vec2<f32>(0.5, 0.5)) / atlas_dims;
    let leaf_tex = textureSampleLevel(atlas_tex, atlas_smp, leaf_px, 0.0);

    // Procedural foliage mask so "leaf tufts" appear on every face, even with plain green textures.
    let an = abs(n);
    var map_uv = vec2<f32>(0.0, 0.0);
    if (an.y >= an.x && an.y >= an.z) {
      map_uv = in.world_pos.xz;
    } else if (an.x >= an.z) {
      map_uv = in.world_pos.zy;
    } else {
      map_uv = in.world_pos.xy;
    }

    let p = map_uv * 11.0;
    let c = floor(p);
    let f = fract(p) - vec2<f32>(0.5, 0.5);
    let radius = 0.22 + 0.22 * hash12(c + vec2<f32>(13.1, 37.7));
    let in_blob = step(length(f), radius);
    let hole = step(0.86, hash12(c.yx + vec2<f32>(3.7, 19.3)));
    let mask = in_blob * (1.0 - hole);

    let alpha_cut = clamp(params.leaf_cfg.y, 0.05, 0.95);
    if (mask < alpha_cut) {
      discard;
    }

    let leaf_rgb = leaf_tex.rgb * params.leaf_tint.xyz;

    let front = 0.42 + 0.58 * max(dot(n, l), 0.0);
    let back = max(dot(-n, l), 0.0) * params.leaf_cfg.w;
    let lit_leaf = vec4<f32>(leaf_rgb * (0.32 + front * 0.68 + back * 0.36), 1.0);
    return apply_fog(fog, lit_leaf, in.world_pos, view.world_position.xyz);
  }

  if (tex.a <= 0.001) {
    discard;
  }

  let diff = 0.35 + 0.65 * max(dot(n, l), 0.0);
  let lit = vec4<f32>(tex.rgb * diff, tex.a);
  return apply_fog(fog, lit, in.world_pos, view.world_position.xyz);
}
