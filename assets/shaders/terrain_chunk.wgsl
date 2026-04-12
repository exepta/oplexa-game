#import bevy_pbr::mesh_view_bindings::{view, fog}
#import bevy_pbr::pbr_functions::apply_fog
#import bevy_pbr::view_transformations::position_world_to_clip

struct VOut {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv_local: vec2<f32>,
  @location(1) tile_rect: vec4<f32>,
  @location(2) normal_ws: vec3<f32>,
  @location(3) ctm: vec2<f32>,
  @location(4) world_pos: vec3<f32>,
};

struct VertexInput {
  @location(0) position: vec3<f32>,
  @location(1) normal: vec3<f32>,
  @location(2) uv_local: vec2<f32>,
  @location(3) ctm: vec2<f32>,
  @location(5) tile_rect: vec4<f32>,
  @builtin(instance_index) instance_index: u32,
};

struct TerrainParams {
  // x: leaf enabled, y: cutout threshold, z: leaf protrusion strength, w: translucency
  leaf_cfg: vec4<f32>,
  // xyz: leaf tint multiplier, w: per-pixel color variation strength
  leaf_tint: vec4<f32>,
  // x: prop nearest sampling flag, y: wind strength, z: wind frequency, w: time
  material_cfg: vec4<f32>,
  // x: wobble enabled, y: amplitude, z: frequency, w: vertical contribution
  mining_wobble_cfg: vec4<f32>,
  // xyz: mining target block coords (world), w: progress (0..1), <0 when inactive
  mining_target: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: TerrainParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var atlas_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var atlas_smp: sampler;

fn hash12(p: vec2<f32>) -> f32 {
  let h = dot(p, vec2<f32>(127.1, 311.7));
  return fract(sin(h) * 43758.5453);
}

fn hash22(p: vec2<f32>) -> vec2<f32> {
  return vec2<f32>(
    hash12(p + vec2<f32>(31.27, 91.41)),
    hash12(p + vec2<f32>(17.83, 47.77))
  );
}

// Distance to Worley cell borders (small values == near crack-like boundaries).
fn worley_edge(p: vec2<f32>) -> f32 {
  let ip = floor(p);
  let fp = fract(p);
  var d1 = 1e9;
  var d2 = 1e9;

  var oy: i32 = -1;
  loop {
    if (oy > 1) { break; }
    var ox: i32 = -1;
    loop {
      if (ox > 1) { break; }
      let b = vec2<f32>(f32(ox), f32(oy));
      let feature = b + hash22(ip + b);
      let r = feature - fp;
      let d = dot(r, r);
      if (d < d1) {
        d2 = d1;
        d1 = d;
      } else if (d < d2) {
        d2 = d;
      }
      ox = ox + 1;
    }
    oy = oy + 1;
  }

  let e = sqrt(max(d2, 0.0)) - sqrt(max(d1, 0.0));
  return max(e, 0.0);
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

  // Gentle wind sway for props and foliage blocks.
  if (params.material_cfg.y > 0.0001) {
    var bend = 1.0;
    // Props (crossed planes) stay anchored at the bottom and bend more near their tips.
    if (params.material_cfg.x > 0.5) {
      let tip = clamp(fract(v.position.y), 0.0, 1.0);
      bend = tip * tip;
    }
    let t = params.material_cfg.w;
    let f = max(params.material_cfg.z, 0.01);
    let phase = world_pos.x * (0.72 * f) + world_pos.z * (0.54 * f);
    let gust = sin(phase + t * 1.45) + 0.45 * sin(world_pos.x * (1.37 * f) - world_pos.z * (0.93 * f) + t * 2.15);
    let sway = gust * max(params.material_cfg.y, 0.0) * bend;
    world_pos.x = world_pos.x + sway;
    world_pos.z = world_pos.z + sway * 0.58;
  }

  out.clip = position_world_to_clip(world_pos);
  out.normal_ws = normal_ws;
  out.uv_local = v.uv_local;
  out.tile_rect = v.tile_rect;
  out.ctm = v.ctm;
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
  var tiled = fract(in.uv_local);
  var ctm_span = vec2<f32>(1.0, 1.0);
  let ctm_mask = i32(round(in.ctm.x));
  let edge_clip = clamp(in.ctm.y, 0.0, 0.49);
  if (edge_clip > 0.0001 && ctm_mask >= 0) {
    let has_u_pos = (ctm_mask & 1) != 0;
    let has_u_neg = (ctm_mask & 2) != 0;
    let has_v_pos = (ctm_mask & 4) != 0;
    let has_v_neg = (ctm_mask & 8) != 0;

    var u0 = 0.0;
    var u1 = 1.0;
    var v0 = 0.0;
    var v1 = 1.0;
    if (has_u_neg) { u0 = edge_clip; }
    if (has_u_pos) { u1 = 1.0 - edge_clip; }
    if (has_v_neg) { v0 = edge_clip; }
    if (has_v_pos) { v1 = 1.0 - edge_clip; }

    let span_u = max(u1 - u0, 1e-4);
    let span_v = max(v1 - v0, 1e-4);
    ctm_span = vec2<f32>(span_u, span_v);
    tiled = vec2<f32>(u0 + tiled.x * span_u, v0 + tiled.y * span_v);
  }
  let atlas_uv = safe_tile_min + tiled * safe_tile_size;

  // Provide explicit gradients from unwrapped UVs to avoid seam artifacts from fract()-based derivatives.
  let duvdx = dpdx(in.uv_local) * safe_tile_size * ctm_span;
  let duvdy = dpdy(in.uv_local) * safe_tile_size * ctm_span;
  var tex: vec4<f32>;
  if (params.material_cfg.x > 0.5) {
    // Pixel-art props: sample exact texel centers at LOD0 for crisp visuals.
    let px_uv = (floor(atlas_uv * atlas_dims) + vec2<f32>(0.5, 0.5)) / atlas_dims;
    tex = textureSampleLevel(atlas_tex, atlas_smp, px_uv, 0.0);
  } else {
    tex = textureSampleGrad(atlas_tex, atlas_smp, atlas_uv, duvdx, duvdy);
  }

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

  var surface_rgb = tex.rgb;
  if (params.mining_target.w >= 0.0) {
    let sample_cell = floor(in.world_pos - n * 0.001);
    let target_cell = params.mining_target.xyz;
    let on_target = all(abs(sample_cell - target_cell) < vec3<f32>(0.5));
    if (on_target) {
      let progress = clamp(params.mining_target.w, 0.0, 1.0);
      let local = clamp(in.world_pos - target_cell, vec3<f32>(0.0), vec3<f32>(1.0));

      let an = abs(n);
      var crack_uv = vec2<f32>(0.0, 0.0);
      if (an.y >= an.x && an.y >= an.z) {
        crack_uv = local.xz;
      } else if (an.x >= an.z) {
        crack_uv = local.zy;
      } else {
        crack_uv = local.xy;
      }

      // Clean progression: cracks appear at center and spread outward with smooth edge.
      let radial_dist = distance(crack_uv, vec2<f32>(0.5, 0.5));
      let reveal_radius = progress * 0.72;
      let reveal = 1.0 - smoothstep(reveal_radius - 0.10, reveal_radius + 0.02, radial_dist);

      let macro_scale = 8.0;
      let micro_scale = 16.0;
      let edge_macro = worley_edge(crack_uv * macro_scale);
      let edge_micro = worley_edge(crack_uv * micro_scale + vec2<f32>(17.0, 9.0));
      let width_macro = 0.018 + (0.095 - 0.018) * progress;
      let width_micro = 0.010 + (0.045 - 0.010) * progress;
      let crack_macro = 1.0 - smoothstep(0.0, width_macro, edge_macro);
      let crack_micro = 1.0 - smoothstep(0.0, width_micro, edge_micro);
      let crack_mask = clamp((crack_macro * 0.85 + crack_micro * 0.35) * reveal, 0.0, 1.0);

      let crack_darkness = (0.18 + 0.62 * progress) * crack_mask;
      surface_rgb = surface_rgb * (1.0 - crack_darkness);

      // Subtle "bröckeln": darker chips near strong crack regions at high progress.
      let chip_cell = floor(crack_uv * (macro_scale * 1.6));
      let chip_noise = hash12(chip_cell + vec2<f32>(3.0, 11.0));
      let chip_threshold = 0.985 - progress * 0.22;
      let chip = step(chip_threshold, chip_noise) * crack_mask;
      surface_rgb = surface_rgb * (1.0 - chip * (0.08 + 0.18 * progress));
    }
  }

  let diff = 0.35 + 0.65 * max(dot(n, l), 0.0);
  let lit = vec4<f32>(surface_rgb * diff, tex.a);
  return apply_fog(fog, lit, in.world_pos, view.world_position.xyz);
}
