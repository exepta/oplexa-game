use super::chunk_debug_grid::{sync_chunk_grid_meshes, toggle_chunk_grid};
use crate::core::CoreModule;
use crate::core::config::GlobalConfig;
use crate::core::debug::{BlockColliderGizmoState, WorldInspectorState};
use crate::core::entities::player::PlayerCamera;
use crate::core::world::block::{BlockRegistry, VOXEL_SIZE, get_block_world};
use crate::core::world::chunk::ChunkMap;
use crate::generator::GeneratorModule;
use crate::graphic::GraphicModule;
use crate::logic::LogicModule;
use crate::shader::WorldShaderService;
use crate::utils::key_utils::convert;
use bevy::gizmos::config::{DefaultGizmoConfigGroup, GizmoConfigStore};
use bevy::light::DirectionalLightShadowMap;
use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

/// Represents manager plugin used by the `client` module.
pub struct ManagerPlugin;

#[derive(Clone, Copy, Debug)]
struct BlockColliderDebugSample {
    pos: IVec3,
    faces_mask: u8,
}

#[derive(Resource, Default)]
struct BlockColliderDebugCache {
    last_anchor: Option<IVec3>,
    samples: Vec<BlockColliderDebugSample>,
}

impl Plugin for ManagerPlugin {
    /// Builds this component for the `client` module.
    fn build(&self, app: &mut App) {
        app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default());
        app.add_plugins((
            CoreModule,
            LogicModule,
            GeneratorModule,
            WorldShaderService,
            GraphicModule,
        ));
        app.init_resource::<BlockColliderDebugCache>();
        app.add_systems(
            Startup,
            (setup_shadow_map, setup_block_collider_gizmo_style),
        );
        app.add_systems(
            Update,
            (
                toggle_world_inspector,
                toggle_chunk_grid,
                sync_chunk_grid_meshes,
                toggle_block_collider_gizmos,
                rebuild_block_collider_debug_cache,
                draw_block_collider_debug_lines,
            ),
        );
    }
}

/// Runs the `setup_shadow_map` routine for setup shadow map in the `client` module.
fn setup_shadow_map(mut commands: Commands) {
    commands.insert_resource(DirectionalLightShadowMap { size: 1024 });
}

/// Runs the `setup_block_collider_gizmo_style` routine for setup block collider gizmo style in the `client` module.
fn setup_block_collider_gizmo_style(mut gizmo_config_store: ResMut<GizmoConfigStore>) {
    let (config, _) = gizmo_config_store.config_mut::<DefaultGizmoConfigGroup>();
    config.depth_bias = -0.97;
    // Default is 2px, make it 1px thicker.
    config.line.width = 3.0;
    config.line.perspective = false;
}

/// Runs the `toggle_world_inspector` routine for toggle world inspector in the `client` module.
fn toggle_world_inspector(
    mut debug_context: ResMut<WorldInspectorState>,
    keyboard: Res<ButtonInput<KeyCode>>,
    game_config: Res<GlobalConfig>,
) {
    let key = convert(game_config.input.world_inspector.as_str())
        .expect("Invalid key for world inspector");
    if keyboard.just_pressed(key) {
        debug_context.0 = !debug_context.0;
        info!("World Inspector: {}", debug_context.0);
    }
}

/// Runs the `toggle_block_collider_gizmos` routine for toggle block collider gizmos in the `client` module.
fn toggle_block_collider_gizmos(
    keyboard: Res<ButtonInput<KeyCode>>,
    game_config: Res<GlobalConfig>,
    mut gizmo_state: ResMut<BlockColliderGizmoState>,
) {
    let debug_key =
        convert(game_config.input.collider_debug.as_str()).expect("Invalid key for collider debug");

    if keyboard.just_pressed(debug_key) {
        gizmo_state.show = !gizmo_state.show;
        info!("Block Collider Gizmos: {}", gizmo_state.show);
    }
}

/// Runs the `rebuild_block_collider_debug_cache` routine for rebuild block collider debug cache in the `client` module.
fn rebuild_block_collider_debug_cache(
    gizmo_state: Res<BlockColliderGizmoState>,
    mut cache: ResMut<BlockColliderDebugCache>,
    chunk_map: Option<Res<ChunkMap>>,
    block_registry: Option<Res<BlockRegistry>>,
    q_player_cam: Query<&GlobalTransform, With<PlayerCamera>>,
    q_any_cam: Query<&GlobalTransform, (With<Camera3d>, Without<PlayerCamera>)>,
) {
    if !gizmo_state.show {
        cache.last_anchor = None;
        cache.samples.clear();
        return;
    }

    let (Some(chunk_map), Some(block_registry)) = (chunk_map, block_registry) else {
        cache.last_anchor = None;
        cache.samples.clear();
        return;
    };

    let cam = q_player_cam
        .iter()
        .next()
        .or_else(|| q_any_cam.iter().next());
    let Some(cam) = cam else {
        cache.last_anchor = None;
        cache.samples.clear();
        return;
    };

    let center = cam.translation() / VOXEL_SIZE;
    let center_block = IVec3::new(
        center.x.floor() as i32,
        center.y.floor() as i32,
        center.z.floor() as i32,
    );

    const REBUILD_STEP_BLOCKS: i32 = 4;
    let anchor = IVec3::new(
        center_block.x.div_euclid(REBUILD_STEP_BLOCKS),
        center_block.y.div_euclid(REBUILD_STEP_BLOCKS),
        center_block.z.div_euclid(REBUILD_STEP_BLOCKS),
    );
    if cache.last_anchor == Some(anchor) && !cache.samples.is_empty() {
        return;
    }

    const DEBUG_RADIUS_BLOCKS: i32 = 50;
    const DEBUG_RADIUS_SQ: i32 = DEBUG_RADIUS_BLOCKS * DEBUG_RADIUS_BLOCKS;
    const MAX_DEBUG_BLOCKS: usize = 6000;

    let mut picked: Vec<(i32, BlockColliderDebugSample)> = Vec::new();

    for y in (center_block.y - DEBUG_RADIUS_BLOCKS)..=(center_block.y + DEBUG_RADIUS_BLOCKS) {
        for z in (center_block.z - DEBUG_RADIUS_BLOCKS)..=(center_block.z + DEBUG_RADIUS_BLOCKS) {
            for x in (center_block.x - DEBUG_RADIUS_BLOCKS)..=(center_block.x + DEBUG_RADIUS_BLOCKS)
            {
                let dx = x - center_block.x;
                let dy = y - center_block.y;
                let dz = z - center_block.z;
                let dist_sq = dx * dx + dy * dy + dz * dz;
                if dist_sq > DEBUG_RADIUS_SQ {
                    continue;
                }

                let p = IVec3::new(x, y, z);
                let id = get_block_world(&chunk_map, p);
                if !block_registry.is_solid_for_collision(id) {
                    continue;
                }

                let faces_mask = exposed_faces_mask(p, &chunk_map, &block_registry);
                if faces_mask == 0 {
                    continue;
                }

                picked.push((dist_sq, BlockColliderDebugSample { pos: p, faces_mask }));
            }
        }
    }

    picked.sort_unstable_by_key(|(dist_sq, _)| *dist_sq);
    cache.samples.clear();
    cache.samples.extend(
        picked
            .into_iter()
            .take(MAX_DEBUG_BLOCKS)
            .map(|(_, sample)| sample),
    );
    cache.last_anchor = Some(anchor);
}

#[inline]
fn exposed_faces_mask(pos: IVec3, chunk_map: &ChunkMap, block_registry: &BlockRegistry) -> u8 {
    const FACE_POS_X: u8 = 1 << 0;
    const FACE_NEG_X: u8 = 1 << 1;
    const FACE_POS_Y: u8 = 1 << 2;
    const FACE_NEG_Y: u8 = 1 << 3;
    const FACE_POS_Z: u8 = 1 << 4;
    const FACE_NEG_Z: u8 = 1 << 5;

    let mut mask = 0u8;
    let is_open = |p: IVec3| !block_registry.is_solid_for_collision(get_block_world(chunk_map, p));

    if is_open(pos + IVec3::new(1, 0, 0)) {
        mask |= FACE_POS_X;
    }
    if is_open(pos + IVec3::new(-1, 0, 0)) {
        mask |= FACE_NEG_X;
    }
    if is_open(pos + IVec3::new(0, 1, 0)) {
        mask |= FACE_POS_Y;
    }
    if is_open(pos + IVec3::new(0, -1, 0)) {
        mask |= FACE_NEG_Y;
    }
    if is_open(pos + IVec3::new(0, 0, 1)) {
        mask |= FACE_POS_Z;
    }
    if is_open(pos + IVec3::new(0, 0, -1)) {
        mask |= FACE_NEG_Z;
    }

    mask
}

/// Runs the `draw_block_collider_debug_lines` routine for draw block collider debug lines in the `client` module.
fn draw_block_collider_debug_lines(
    gizmo_state: Res<BlockColliderGizmoState>,
    cache: Res<BlockColliderDebugCache>,
    mut gizmos: Gizmos,
) {
    if !gizmo_state.show {
        return;
    }

    const FACE_POS_X: u8 = 1 << 0;
    const FACE_NEG_X: u8 = 1 << 1;
    const FACE_POS_Y: u8 = 1 << 2;
    const FACE_NEG_Y: u8 = 1 << 3;
    const FACE_POS_Z: u8 = 1 << 4;
    const FACE_NEG_Z: u8 = 1 << 5;

    let color = Color::srgb(1.0, 0.0, 0.0);
    let s = VOXEL_SIZE;

    for sample in &cache.samples {
        let x0 = sample.pos.x as f32 * s;
        let y0 = sample.pos.y as f32 * s;
        let z0 = sample.pos.z as f32 * s;
        let x1 = x0 + s;
        let y1 = y0 + s;
        let z1 = z0 + s;

        if (sample.faces_mask & FACE_POS_X) != 0 {
            draw_rect(
                &mut gizmos,
                Vec3::new(x1, y0, z0),
                Vec3::new(x1, y0, z1),
                Vec3::new(x1, y1, z1),
                Vec3::new(x1, y1, z0),
                color,
            );
        }
        if (sample.faces_mask & FACE_NEG_X) != 0 {
            draw_rect(
                &mut gizmos,
                Vec3::new(x0, y0, z1),
                Vec3::new(x0, y0, z0),
                Vec3::new(x0, y1, z0),
                Vec3::new(x0, y1, z1),
                color,
            );
        }
        if (sample.faces_mask & FACE_POS_Y) != 0 {
            draw_rect(
                &mut gizmos,
                Vec3::new(x0, y1, z1),
                Vec3::new(x1, y1, z1),
                Vec3::new(x1, y1, z0),
                Vec3::new(x0, y1, z0),
                color,
            );
        }
        if (sample.faces_mask & FACE_NEG_Y) != 0 {
            draw_rect(
                &mut gizmos,
                Vec3::new(x0, y0, z0),
                Vec3::new(x1, y0, z0),
                Vec3::new(x1, y0, z1),
                Vec3::new(x0, y0, z1),
                color,
            );
        }
        if (sample.faces_mask & FACE_POS_Z) != 0 {
            draw_rect(
                &mut gizmos,
                Vec3::new(x0, y0, z1),
                Vec3::new(x1, y0, z1),
                Vec3::new(x1, y1, z1),
                Vec3::new(x0, y1, z1),
                color,
            );
        }
        if (sample.faces_mask & FACE_NEG_Z) != 0 {
            draw_rect(
                &mut gizmos,
                Vec3::new(x1, y0, z0),
                Vec3::new(x0, y0, z0),
                Vec3::new(x0, y1, z0),
                Vec3::new(x1, y1, z0),
                color,
            );
        }
    }
}

#[inline]
fn draw_rect(gizmos: &mut Gizmos, a: Vec3, b: Vec3, c: Vec3, d: Vec3, color: Color) {
    gizmos.line(a, b, color);
    gizmos.line(b, c, color);
    gizmos.line(c, d, color);
    gizmos.line(d, a, color);
}
