use crate::core::entities::player::PlayerCamera;
use crate::core::states::states::{AppState, InGameStates};
use crate::core::world::block::{BlockId, VOXEL_SIZE, get_block_world, id_any};
use crate::core::world::chunk::ChunkMap;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::math::primitives::Rectangle;
use bevy::prelude::*;

const MAX_LEAF_PARTICLES: usize = 140;
const SPAWN_INTERVAL_SECS: f32 = 0.16;
const SPAWN_RADIUS_BLOCKS: i32 = 14;
const SPAWN_ATTEMPTS: usize = 18;
const LEAF_SCAN_DEPTH: i32 = 12;

/// Ambient leaves VFX plugin.
pub struct LeavesAmbientFxPlugin;

impl Plugin for LeavesAmbientFxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LeavesFxAssets>()
            .init_resource::<LeavesFxState>()
            .add_systems(
                Update,
                (
                    ensure_leaves_fx_assets,
                    spawn_falling_leaves,
                    update_falling_leaves,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame(InGameStates::Game))),
            )
            .add_systems(
                OnExit(AppState::InGame(InGameStates::Game)),
                cleanup_falling_leaves,
            );
    }
}

#[derive(Resource, Default)]
struct LeavesFxAssets {
    initialized: bool,
    quad: Handle<Mesh>,
    oak_material: Handle<StandardMaterial>,
    spruce_material: Handle<StandardMaterial>,
    oak_leaves_id: BlockId,
    spruce_leaves_id: BlockId,
}

#[derive(Resource, Default)]
struct LeavesFxState {
    spawn_accumulator: f32,
}

#[derive(Component, Clone, Copy)]
struct FallingLeafParticle {
    velocity: Vec3,
    age: f32,
    lifetime: f32,
    size: f32,
    spin_rate: f32,
    spin_phase: f32,
}

fn ensure_leaves_fx_assets(
    mut fx_assets: ResMut<LeavesFxAssets>,
    reg: Res<crate::core::world::block::BlockRegistry>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if fx_assets.initialized {
        return;
    }

    fx_assets.oak_leaves_id = id_any(&reg, &["oak_leaves_block", "leaves_block"]);
    fx_assets.spruce_leaves_id = id_any(&reg, &["spruce_leaves_block"]);
    fx_assets.quad = meshes.add(Mesh::from(Rectangle::new(0.20, 0.20)));

    let oak_tex: Handle<Image> = asset_server.load("textures/blocks/oak_leaves/oak_leaf_particle.png");
    let spruce_tex: Handle<Image> =
        asset_server.load("textures/blocks/spruce_leaves/spruce_leaf_particle.png");

    fx_assets.oak_material = materials.add(StandardMaterial {
        base_color_texture: Some(oak_tex),
        base_color: Color::srgba(1.0, 1.0, 1.0, 0.95),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        reflectance: 0.0,
        metallic: 0.0,
        perceptual_roughness: 1.0,
        ..default()
    });
    fx_assets.spruce_material = materials.add(StandardMaterial {
        base_color_texture: Some(spruce_tex),
        base_color: Color::srgba(0.93, 0.97, 0.93, 0.95),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        reflectance: 0.0,
        metallic: 0.0,
        perceptual_roughness: 1.0,
        ..default()
    });

    fx_assets.initialized = true;
}

fn spawn_falling_leaves(
    mut commands: Commands,
    time: Res<Time>,
    mut fx_state: ResMut<LeavesFxState>,
    fx_assets: Res<LeavesFxAssets>,
    chunk_map: Res<ChunkMap>,
    q_player_cam: Query<&GlobalTransform, With<PlayerCamera>>,
    q_fallback_cam: Query<&GlobalTransform, (With<Camera3d>, Without<PlayerCamera>)>,
    q_particles: Query<(), With<FallingLeafParticle>>,
) {
    if !fx_assets.initialized {
        return;
    }

    let cam_pos = q_player_cam
        .iter()
        .next()
        .map(GlobalTransform::translation)
        .or_else(|| q_fallback_cam.iter().next().map(GlobalTransform::translation));
    let Some(cam_pos) = cam_pos else {
        return;
    };

    let mut alive = q_particles.iter().len();
    if alive >= MAX_LEAF_PARTICLES {
        return;
    }

    fx_state.spawn_accumulator += time.delta_secs();
    while fx_state.spawn_accumulator >= SPAWN_INTERVAL_SECS {
        fx_state.spawn_accumulator -= SPAWN_INTERVAL_SECS;
        if alive >= MAX_LEAF_PARTICLES {
            break;
        }

        // Keep ambience subtle.
        if rand_f32() > 0.55 {
            continue;
        }

        let Some((spawn_pos, spruce)) =
            find_leaf_spawn(cam_pos, &chunk_map, fx_assets.oak_leaves_id, fx_assets.spruce_leaves_id)
        else {
            continue;
        };

        let velocity = Vec3::new(
            rand_range(-0.22, 0.22),
            rand_range(-0.08, 0.02),
            rand_range(-0.22, 0.22),
        );
        let size = rand_range(0.09, 0.16);
        let spin_phase = rand_range(0.0, std::f32::consts::TAU);
        let spin_rate = rand_range(1.3, 3.2);
        let lifetime = rand_range(4.6, 9.2);
        let material = if spruce {
            fx_assets.spruce_material.clone()
        } else {
            fx_assets.oak_material.clone()
        };

        commands.spawn((
            FallingLeafParticle {
                velocity,
                age: 0.0,
                lifetime,
                size,
                spin_rate,
                spin_phase,
            },
            Mesh3d(fx_assets.quad.clone()),
            MeshMaterial3d(material),
            Transform::from_translation(spawn_pos).with_scale(Vec3::splat(size)),
            Visibility::default(),
            NotShadowCaster,
            NotShadowReceiver,
            Name::new("FallingLeafParticle"),
        ));
        alive += 1;
    }
}

fn update_falling_leaves(
    mut commands: Commands,
    time: Res<Time>,
    chunk_map: Res<ChunkMap>,
    q_player_cam: Query<&GlobalTransform, With<PlayerCamera>>,
    q_fallback_cam: Query<&GlobalTransform, (With<Camera3d>, Without<PlayerCamera>)>,
    mut q_particles: Query<(Entity, &mut Transform, &mut FallingLeafParticle)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    let cam_pos = q_player_cam
        .iter()
        .next()
        .map(GlobalTransform::translation)
        .or_else(|| q_fallback_cam.iter().next().map(GlobalTransform::translation));

    for (entity, mut tf, mut leaf) in &mut q_particles {
        leaf.age += dt;
        if leaf.age >= leaf.lifetime {
            commands.entity(entity).despawn();
            continue;
        }

        let gust = Vec2::new(
            (leaf.spin_phase + leaf.age * 1.10).sin(),
            (leaf.spin_phase + leaf.age * 0.83).cos(),
        ) * 0.36;
        leaf.velocity.x += gust.x * dt * 0.62;
        leaf.velocity.z += gust.y * dt * 0.62;
        leaf.velocity.y -= (0.78 + 0.34 * (leaf.spin_phase + leaf.age * 2.3).sin().abs()) * dt;
        leaf.velocity *= 1.0 - (0.10 * dt).clamp(0.0, 0.10);

        tf.translation += leaf.velocity * dt;

        let wp = IVec3::new(
            (tf.translation.x / VOXEL_SIZE).floor() as i32,
            (tf.translation.y / VOXEL_SIZE).floor() as i32,
            (tf.translation.z / VOXEL_SIZE).floor() as i32,
        );
        if get_block_world(&chunk_map, wp) != 0 || tf.translation.y < -64.0 {
            commands.entity(entity).despawn();
            continue;
        }

        let spin = Quat::from_axis_angle(Vec3::Y, leaf.spin_rate * leaf.age)
            * Quat::from_axis_angle(Vec3::X, (leaf.spin_phase + leaf.age * 3.2).sin() * 0.40);
        if let Some(cam) = cam_pos {
            let to_cam = cam - tf.translation;
            if to_cam.length_squared() > 1e-5 {
                let billboard = Quat::from_rotation_arc(Vec3::Z, to_cam.normalize());
                tf.rotation = billboard * spin;
            } else {
                tf.rotation = spin;
            }
        } else {
            tf.rotation = spin;
        }

        let t = 1.0 - (leaf.age / leaf.lifetime);
        tf.scale = Vec3::splat(leaf.size * (0.85 + 0.25 * t));
    }
}

fn cleanup_falling_leaves(
    mut commands: Commands,
    q_particles: Query<Entity, With<FallingLeafParticle>>,
) {
    for entity in &q_particles {
        commands.entity(entity).despawn();
    }
}

fn find_leaf_spawn(
    cam_pos: Vec3,
    chunk_map: &ChunkMap,
    oak_id: BlockId,
    spruce_id: BlockId,
) -> Option<(Vec3, bool)> {
    if oak_id == 0 && spruce_id == 0 {
        return None;
    }

    let base = IVec3::new(
        (cam_pos.x / VOXEL_SIZE).floor() as i32,
        (cam_pos.y / VOXEL_SIZE).floor() as i32,
        (cam_pos.z / VOXEL_SIZE).floor() as i32,
    );

    for _ in 0..SPAWN_ATTEMPTS {
        let sx = base.x + rand_i32(-SPAWN_RADIUS_BLOCKS, SPAWN_RADIUS_BLOCKS);
        let sz = base.z + rand_i32(-SPAWN_RADIUS_BLOCKS, SPAWN_RADIUS_BLOCKS);
        let start_y = base.y + rand_i32(2, 16);

        for d in 0..LEAF_SCAN_DEPTH {
            let sy = start_y - d;
            let id = get_block_world(chunk_map, IVec3::new(sx, sy, sz));
            let is_oak = oak_id != 0 && id == oak_id;
            let is_spruce = spruce_id != 0 && id == spruce_id;
            if !is_oak && !is_spruce {
                continue;
            }

            let spawn_pos = Vec3::new(
                (sx as f32 + rand_range(0.12, 0.88)) * VOXEL_SIZE,
                (sy as f32 + rand_range(0.18, 0.95)) * VOXEL_SIZE,
                (sz as f32 + rand_range(0.12, 0.88)) * VOXEL_SIZE,
            );
            return Some((spawn_pos, is_spruce));
        }
    }

    None
}

#[inline]
fn rand_f32() -> f32 {
    rand::random::<f32>()
}

#[inline]
fn rand_range(min: f32, max: f32) -> f32 {
    min + (max - min) * rand_f32()
}

#[inline]
fn rand_i32(min: i32, max: i32) -> i32 {
    if min >= max {
        return min;
    }
    let span = (max - min + 1) as u32;
    min + (rand::random::<u32>() % span) as i32
}
