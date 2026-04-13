// shaders/water.wgsl

#import bevy_pbr::mesh_view_bindings::{view, fog}
#import bevy_pbr::pbr_functions::apply_fog
#import bevy_pbr::view_transformations::position_world_to_clip

struct VOut {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv_local: vec2<f32>,
  @location(1) tile_rect: vec4<f32>,
  @location(2) world_pos: vec3<f32>,
  @location(3) normal_ws: vec3<f32>,
  @location(4) flow_dir: vec2<f32>,
};

struct VertexInput {
  @location(0) position: vec3<f32>,
  @location(1) normal: vec3<f32>,
  @location(2) uv_local: vec2<f32>,
  @location(3) flow_dir: vec2<f32>,
  @location(5) tile_rect: vec4<f32>,
  @builtin(instance_index) instance_index: u32,
};

struct WaterParams {
  uv_rect: vec4<f32>,
  flow: vec4<f32>,
  t_misc: vec4<f32>,
  tint: vec4<f32>,
};

struct UvSampleData {
  uv: vec2<f32>,
  duvdx: vec2<f32>,
  duvdy: vec2<f32>,
};
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: WaterParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var atlas_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var atlas_smp: sampler;

fn remap_uv_scroll(
  uv_local: vec2<f32>,
  tile_rect: vec4<f32>,
  world_pos: vec3<f32>,
  normal_ws: vec3<f32>,
  flow_dir: vec2<f32>,
) -> UvSampleData {
  let tile_min = tile_rect.xy;
  let tile_max = tile_rect.zw;
  let t = params.t_misc.x;

  let is_top = normal_ws.y > 0.7;
  let is_side_x = abs(normal_ws.x) >= abs(normal_ws.z);
  let uv_scale = select(1.0, params.t_misc.w, params.t_misc.w > 0.001);
  let uv_base = select(
    select(world_pos.xy, world_pos.zy, is_side_x),
    world_pos.xz,
    is_top
  ) * uv_scale;

  let base_speed = max(length(params.flow.xy), 0.0001);
  let has_local_dir = length(flow_dir) > 0.001;
  let world_dir = normalize(select(vec2<f32>(1.0, 0.0), flow_dir, has_local_dir));
  let top_flow = world_dir * base_speed;
  let side_u = select(world_dir.x, -world_dir.y, is_side_x);
  let side_flow_horizontal = vec2<f32>(side_u * base_speed, 0.0);
  let side_flow_fall = vec2<f32>(0.0, base_speed * 2.8);
  let side_flow = select(side_flow_fall, side_flow_horizontal, has_local_dir);
  let flow = select(side_flow, top_flow, is_top);

  let side_distort_scale = select(0.22, 1.0, is_top);
  let distort = vec2<f32>(
    sin(world_pos.x * 0.85 + t * 1.15) * 0.015
      + cos(world_pos.z * 1.27 - t * 0.97) * 0.010,
    cos(world_pos.z * 0.91 + t * 1.05) * 0.015
      + sin(world_pos.x * 1.11 + t * 0.88) * 0.010
  ) * side_distort_scale;

  let uv_cont = uv_base + flow * t + distort;
  let local_scrolled = fract(uv_cont);

  let atlas_dims = vec2<f32>(textureDimensions(atlas_tex, 0));
  let atlas_texel = 1.0 / max(atlas_dims, vec2<f32>(1.0, 1.0));
  let safe_pad = atlas_texel * 0.5;
  let safe_min = tile_min + safe_pad;
  let safe_max = tile_max - safe_pad;
  let safe_size = max(safe_max - safe_min, atlas_texel);
  let atlas_uv = safe_min + local_scrolled * safe_size;
  let grad_x = dpdx(uv_cont) * safe_size;
  let grad_y = dpdy(uv_cont) * safe_size;
  return UvSampleData(atlas_uv, grad_x, grad_y);
}

@vertex
fn vertex(v: VertexInput) -> VOut {
  var out: VOut;
  let world_from_local = bevy_pbr::mesh_functions::get_world_from_local(v.instance_index);
  let world_pos4 = world_from_local * vec4<f32>(v.position, 1.0);
  let normal_ws = normalize(
    bevy_pbr::mesh_functions::mesh_normal_local_to_world(v.normal, v.instance_index)
  );

  var pos_world = world_pos4.xyz;
  let amp = params.flow.z;
  let freq = params.flow.w;
  let t = params.t_misc.x;
  let wave_raw =
    sin(pos_world.x * freq + t * 1.30) +
    0.62 * sin(pos_world.z * (freq * 1.12) + t * 0.92);
  // Keep displacement non-negative to avoid opening visible air gaps at solid borders.
  let wave = (0.5 + 0.5 * clamp(wave_raw / 1.62, -1.0, 1.0)) * amp;

  let is_top_face = normal_ws.y > 0.95;
  let is_side_top_edge = abs(normal_ws.y) < 0.05 && v.uv_local.y <= 0.001;
  if (is_top_face || is_side_top_edge) {
    pos_world.y = pos_world.y + wave;
  }

  out.clip = position_world_to_clip(pos_world);
  out.uv_local = v.uv_local;
  out.tile_rect = v.tile_rect;
  out.world_pos = pos_world;
  out.normal_ws = normal_ws;
  out.flow_dir = v.flow_dir;
  return out;
}

@fragment
fn fragment(in: VOut) -> @location(0) vec4<f32> {
  let cam_pos = (view.world_from_view * vec4<f32>(0.0, 0.0, 0.0, 1.0)).xyz;
  let V = normalize(cam_pos - in.world_pos);
  let N = normalize(in.normal_ws);

  let uv_sample = remap_uv_scroll(in.uv_local, in.tile_rect, in.world_pos, in.normal_ws, in.flow_dir);
  let tex = textureSampleGrad(atlas_tex, atlas_smp, uv_sample.uv, uv_sample.duvdx, uv_sample.duvdy);

  let fres = pow(1.0 - clamp(dot(N, V), 0.0, 1.0), 3.0);

  let base_rgb = tex.rgb * params.tint.rgb;
  let fres_rgb = mix(base_rgb * 0.90, base_rgb * 1.10, fres);

  let L = normalize(vec3<f32>(0.35, 1.0, 0.2));
  let H = normalize(L + V);
  let spec_p = max(params.t_misc.z, 1.0);
  let spec_i = params.t_misc.y;
  let spec = pow(max(dot(N, H), 0.0), spec_p) * spec_i;

  let rgb_lit = fres_rgb + vec3<f32>(spec);
  let fogged = apply_fog(
    fog,
    vec4<f32>(rgb_lit, params.tint.a),
    in.world_pos,
    view.world_position.xyz
  );

  return vec4<f32>(fogged.rgb, fogged.a);
}
