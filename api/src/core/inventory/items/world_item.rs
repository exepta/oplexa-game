use crate::core::inventory::items::{ItemId, ItemRegistry};
use crate::core::world::block::{BlockId, BlockRegistry, VOXEL_SIZE, build_block_cube_mesh};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::math::primitives::Rectangle;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;

/// Visual size in world units for dropped item entities.
pub const WORLD_ITEM_SIZE: f32 = 0.32;
/// Radius at which items can be collected by the player.
pub const WORLD_ITEM_PICKUP_RADIUS: f32 = 1.35;
/// Radius used by attraction behavior toward the player.
pub const WORLD_ITEM_ATTRACT_RADIUS: f32 = 3.5;
/// Attraction acceleration for item pickup magnet behavior.
pub const WORLD_ITEM_ATTRACT_ACCEL: f32 = 34.0;
/// Speed clamp for attracted items.
pub const WORLD_ITEM_ATTRACT_MAX_SPEED: f32 = 12.0;
/// Downward acceleration for dropped items.
pub const WORLD_ITEM_DROP_GRAVITY: f32 = 12.0;
/// Delay after spawn before the item can be picked up.
pub const WORLD_ITEM_PICKUP_DELAY_SECS: f32 = 0.7;

const DROP_POP_MIN_DIST: f32 = 0.1;
const DROP_POP_MAX_DIST: f32 = 1.0;
const PLAYER_DROP_THROW_DISTANCE: f32 = 2.0;
const PLAYER_DROP_THROW_HEIGHT: f32 = 0.65;
const PLAYER_DROP_THROW_SPEED: f32 = 2.5;
const PLAYER_DROP_THROW_UP_SPEED: f32 = 1.5;

/// World-space item entity state for local drops.
#[derive(Component, Clone, Copy, Debug)]
pub struct WorldItemEntity {
    /// Item type represented by this entity.
    pub item_id: ItemId,
    /// Stack size represented by this entity.
    pub amount: u16,
    /// Whether the item is currently resting on terrain.
    pub resting: bool,
    /// Earliest timestamp when pickup is allowed.
    pub pickup_ready_at: f32,
}

/// Current linear velocity for a world item.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct WorldItemVelocity(pub Vec3);

/// Current angular velocity for a world item.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct WorldItemAngularVelocity(pub Vec3);

/// Spawns a world item drop for a broken block using block→item mapping.
pub fn spawn_world_item_for_block_break(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    block_registry: &BlockRegistry,
    item_registry: &ItemRegistry,
    block_id: u16,
    world_loc: IVec3,
    now: f32,
) {
    let Some(item_id) = item_registry.item_for_block(block_id) else {
        return;
    };
    let center = Vec3::new(
        (world_loc.x as f32 + 0.5) * VOXEL_SIZE,
        (world_loc.y as f32 + 0.5) * VOXEL_SIZE + 0.28,
        (world_loc.z as f32 + 0.5) * VOXEL_SIZE,
    );
    spawn_world_item_with_motion(
        commands,
        meshes,
        block_registry,
        item_registry,
        item_id,
        1,
        center,
        compute_drop_pop_velocity(world_loc, now),
        world_loc,
        now,
    );
}

/// Spawns one or more dropped items from a player throw action.
pub fn spawn_player_dropped_item_stack(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    block_registry: &BlockRegistry,
    item_registry: &ItemRegistry,
    item_id: ItemId,
    amount: u16,
    player_translation: Vec3,
    player_forward: Vec3,
    now: f32,
) {
    if item_id == 0 || amount == 0 {
        return;
    }

    let (base_center, initial_velocity) =
        player_drop_spawn_motion(player_translation, player_forward);
    for i in 0..amount {
        let spawn_now = now + i as f32 * 0.013;
        let center = base_center + Vec3::Y * (i as f32 * 0.015);
        let seed_loc = IVec3::new(
            center.x.floor() as i32,
            center.y.floor() as i32,
            center.z.floor() as i32,
        );
        spawn_world_item_with_motion(
            commands,
            meshes,
            block_registry,
            item_registry,
            item_id,
            1,
            center,
            initial_velocity,
            seed_loc,
            spawn_now,
        );
    }
}

/// Calculates spawn center and initial velocity for a player-thrown item.
pub fn player_drop_spawn_motion(player_translation: Vec3, player_forward: Vec3) -> (Vec3, Vec3) {
    let throw_dir = player_drop_throw_direction(player_forward);
    let center = player_drop_spawn_center(player_translation, player_forward);
    let velocity = throw_dir * PLAYER_DROP_THROW_SPEED + Vec3::Y * PLAYER_DROP_THROW_UP_SPEED;
    (center, velocity)
}

/// Calculates the integer world location around the player where a drop starts.
pub fn player_drop_world_location(player_translation: Vec3, player_forward: Vec3) -> IVec3 {
    let center = player_drop_spawn_center(player_translation, player_forward);
    IVec3::new(
        center.x.floor() as i32,
        center.y.floor() as i32,
        center.z.floor() as i32,
    )
}

/// Builds a mesh + material pair for rendering a dropped world item.
pub fn build_world_item_drop_visual(
    block_registry: &BlockRegistry,
    item_registry: &ItemRegistry,
    item_id: ItemId,
    size: f32,
) -> Option<(Mesh, Handle<StandardMaterial>, Vec3)> {
    if item_id == 0 {
        return None;
    }

    if let Some(block_id) = resolve_drop_block_id(block_registry, item_registry, item_id) {
        let mut mesh = build_block_cube_mesh(block_registry, block_id, size);
        center_mesh_vertices(&mut mesh, size * 0.5);
        return Some((mesh, block_registry.material(block_id), Vec3::ONE));
    }

    let mesh = Mesh::from(Rectangle::new(size * 0.95, size * 0.95));
    let material = item_registry.def_opt(item_id)?.material.clone();
    Some((mesh, material, Vec3::ONE))
}

/// Resolves the block id that should be used for a dropped-item block mesh.
///
/// The direct item→block mapping is preferred. A fallback is applied for
/// block-items that were not explicitly bound, to avoid flat rectangle drops.
fn resolve_drop_block_id(
    block_registry: &BlockRegistry,
    item_registry: &ItemRegistry,
    item_id: ItemId,
) -> Option<BlockId> {
    if let Some(block_id) = item_registry.block_for_item(item_id) {
        return Some(block_id);
    }

    let item = item_registry.def_opt(item_id)?;
    if !item.block_item {
        return None;
    }

    block_registry.id_opt(item.key.as_str()).or_else(|| {
        guess_block_name_from_item_key(item.key.as_str())
            .and_then(|name| block_registry.id_opt(name.as_str()))
    })
}

/// Spawns one dropped world item with custom initial motion.
#[allow(clippy::too_many_arguments)]
pub fn spawn_world_item_with_motion(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    block_registry: &BlockRegistry,
    item_registry: &ItemRegistry,
    item_id: ItemId,
    amount: u16,
    center: Vec3,
    initial_velocity: Vec3,
    seed_loc: IVec3,
    now: f32,
) {
    let Some((mesh, material, visual_scale)) =
        build_world_item_drop_visual(block_registry, item_registry, item_id, WORLD_ITEM_SIZE)
    else {
        return;
    };

    commands.spawn((
        WorldItemEntity {
            item_id,
            amount: amount.max(1),
            resting: false,
            pickup_ready_at: now + WORLD_ITEM_PICKUP_DELAY_SECS,
        },
        WorldItemVelocity(initial_velocity),
        WorldItemAngularVelocity(compute_drop_angular_velocity(seed_loc, now)),
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(material),
        Transform {
            translation: center,
            rotation: compute_drop_initial_rotation(seed_loc, now),
            scale: visual_scale,
        },
        Visibility::default(),
        Name::new(format!(
            "WorldItem::{}",
            item_registry.name_opt(item_id).unwrap_or("unknown")
        )),
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

fn player_drop_throw_direction(player_forward: Vec3) -> Vec3 {
    let horizontal_forward = Vec3::new(player_forward.x, 0.0, player_forward.z);
    horizontal_forward.try_normalize().unwrap_or(Vec3::Z)
}

fn player_drop_spawn_center(player_translation: Vec3, player_forward: Vec3) -> Vec3 {
    player_translation
        + player_drop_throw_direction(player_forward) * PLAYER_DROP_THROW_DISTANCE
        + Vec3::Y * PLAYER_DROP_THROW_HEIGHT
}

fn compute_drop_pop_velocity(world_loc: IVec3, now: f32) -> Vec3 {
    let seed_base = hash01(
        world_loc.x.wrapping_mul(31) ^ world_loc.y.wrapping_mul(47) ^ world_loc.z.wrapping_mul(73),
        now,
    );
    let seed_angle = hash01(world_loc.x ^ world_loc.z ^ 0x51, now * 0.77);
    let seed_dist = hash01(world_loc.y ^ 0x2A, now * 1.37);

    let angle = seed_angle * std::f32::consts::TAU;
    let distance = DROP_POP_MIN_DIST + (DROP_POP_MAX_DIST - DROP_POP_MIN_DIST) * seed_dist;
    let flight_time = 0.35 + seed_base * 0.25;
    let horizontal_speed = (distance / flight_time).max(0.2);

    Vec3::new(
        angle.cos() * horizontal_speed,
        2.8 + seed_base * 1.2,
        angle.sin() * horizontal_speed,
    )
}

fn compute_drop_angular_velocity(world_loc: IVec3, now: f32) -> Vec3 {
    let seed_base = hash01(
        world_loc.x.wrapping_mul(13) ^ world_loc.y.wrapping_mul(29) ^ world_loc.z.wrapping_mul(61),
        now * 0.91,
    );
    let seed_x = hash01(world_loc.x ^ 0x41, now * 1.13);
    let seed_y = hash01(world_loc.y ^ 0x52, now * 1.31);
    let seed_z = hash01(world_loc.z ^ 0x63, now * 1.49);

    Vec3::new(
        -10.0 + seed_x * 20.0,
        -13.0 + seed_y * 26.0 + seed_base * 4.0,
        -10.0 + seed_z * 20.0,
    )
}

fn compute_drop_initial_rotation(world_loc: IVec3, now: f32) -> Quat {
    let rx = hash01(world_loc.x ^ world_loc.y ^ 0x71, now * 0.47) * std::f32::consts::TAU;
    let ry = hash01(world_loc.y ^ world_loc.z ^ 0x82, now * 0.63) * std::f32::consts::TAU;
    let rz = hash01(world_loc.z ^ world_loc.x ^ 0x93, now * 0.79) * std::f32::consts::TAU;
    Quat::from_euler(EulerRot::XYZ, rx, ry, rz)
}

fn hash01(input: i32, time_factor: f32) -> f32 {
    let x = input as f32 * 12.9898 + time_factor * 78.233;
    (x.sin() * 43_758.547).fract().abs()
}

fn center_mesh_vertices(mesh: &mut Mesh, half_extent: f32) {
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    else {
        return;
    };

    for position in positions.iter_mut() {
        position[0] -= half_extent;
        position[1] -= half_extent;
        position[2] -= half_extent;
    }
}

/// Guesses a block registry key from an item key using common naming patterns.
fn guess_block_name_from_item_key(item_key: &str) -> Option<String> {
    let key = item_key.trim();
    if key.is_empty() {
        return None;
    }
    if key.ends_with("_block") {
        return Some(key.to_string());
    }
    if let Some(base) = key.strip_suffix("_item") {
        return Some(format!("{base}_block"));
    }
    Some(format!("{key}_block"))
}
