use crate::core::inventory::items::{ItemId, ItemRegistry};
use crate::core::world::block::{
    BlockId, BlockRegistry, VOXEL_SIZE, build_block_cube_mesh, get_block_world,
};
use crate::core::world::chunk::ChunkMap;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use image::imageops::FilterType;
use image::{RgbaImage, imageops};
use std::path::Path;

/// Visual size in world units for dropped item entities.
pub const WORLD_ITEM_SIZE: f32 = 0.55;
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
const WORLD_BLOCK_DROP_SIZE: f32 = 0.35;

const DROP_POP_MIN_DIST: f32 = 0.1;
const DROP_POP_MAX_DIST: f32 = 1.0;
const PLAYER_DROP_THROW_DISTANCE: f32 = 2.0;
const PLAYER_DROP_THROW_HEIGHT: f32 = 0.65;
const PLAYER_DROP_THROW_SPEED: f32 = 2.5;
const PLAYER_DROP_THROW_UP_SPEED: f32 = 1.5;
const ITEM_DROP_EXTRUDE_THICKNESS_FACTOR: f32 = 0.14;
const ITEM_DROP_ALPHA_THRESHOLD: u8 = 8;
const ITEM_DROP_MAX_ICON_DIM: u32 = 32;

/// World-space item entity state for local drops.
#[derive(Component, Clone, Copy, Debug)]
pub struct WorldItemEntity {
    /// Item type represented by this entity.
    pub item_id: ItemId,
    /// Stack size represented by this entity.
    pub amount: u16,
    /// True when this drop uses the block mesh visual.
    pub block_visual: bool,
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
    chunk_map: &ChunkMap,
    item_registry: &ItemRegistry,
    block_id: u16,
    world_loc: IVec3,
    now: f32,
) {
    let Some(item_id) = item_registry.item_for_block(block_id) else {
        return;
    };
    let (spawn_cell, center) =
        resolve_block_drop_spawn_center(block_registry, chunk_map, world_loc);
    spawn_world_item_with_motion(
        commands,
        meshes,
        block_registry,
        item_registry,
        item_id,
        1,
        center,
        compute_drop_pop_velocity(world_loc, now),
        spawn_cell,
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
) -> Option<(Mesh, Handle<StandardMaterial>, Vec3, bool)> {
    if item_id == 0 {
        return None;
    }

    if let Some(block_id) = resolve_drop_block_id(block_registry, item_registry, item_id) {
        let block_size = size.min(WORLD_BLOCK_DROP_SIZE).max(0.10);
        let mut mesh = build_block_cube_mesh(block_registry, block_id, block_size);
        center_mesh_vertices(&mut mesh, block_size * 0.5);
        let visual_scale = block_registry
            .collision_box(block_id)
            .map(|(size_m, _)| {
                Vec3::new(
                    size_m[0].clamp(0.1, 1.0),
                    size_m[1].clamp(0.1, 1.0),
                    size_m[2].clamp(0.1, 1.0),
                )
            })
            .unwrap_or(Vec3::ONE);
        return Some((mesh, block_registry.material(block_id), visual_scale, true));
    }

    let item = item_registry.def_opt(item_id)?;
    let mesh = build_extruded_item_drop_mesh_from_texture(item.texture_path.as_str(), size * 0.95)
        .unwrap_or_else(|| build_crossed_item_drop_mesh(size * 0.95));
    let material = item.material.clone();
    Some((mesh, material, Vec3::ONE, false))
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
        if !block_registry.is_prop(block_id) {
            return Some(block_id);
        }
        return None;
    }

    let item = item_registry.def_opt(item_id)?;
    if !item.block_item {
        return None;
    }

    block_registry
        .id_opt(item.key.as_str())
        .or_else(|| {
            guess_block_name_from_item_key(item.key.as_str())
                .and_then(|name| block_registry.id_opt(name.as_str()))
        })
        .and_then(|block_id| (!block_registry.is_prop(block_id)).then_some(block_id))
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
    let Some((mesh, material, visual_scale, block_visual)) =
        build_world_item_drop_visual(block_registry, item_registry, item_id, WORLD_ITEM_SIZE)
    else {
        return;
    };

    commands.spawn((
        WorldItemEntity {
            item_id,
            amount: amount.max(1),
            block_visual,
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

/// Runs the `player_drop_throw_direction` routine for player drop throw direction in the `core::inventory::items::world_item` module.
fn player_drop_throw_direction(player_forward: Vec3) -> Vec3 {
    player_forward.try_normalize().unwrap_or(Vec3::Z)
}

/// Runs the `player_drop_spawn_center` routine for player drop spawn center in the `core::inventory::items::world_item` module.
fn player_drop_spawn_center(player_translation: Vec3, player_forward: Vec3) -> Vec3 {
    player_translation
        + player_drop_throw_direction(player_forward) * PLAYER_DROP_THROW_DISTANCE
        + Vec3::Y * PLAYER_DROP_THROW_HEIGHT
}

/// Computes drop pop velocity for the `core::inventory::items::world_item` module.
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

/// Computes drop angular velocity for the `core::inventory::items::world_item` module.
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

/// Computes drop initial rotation for the `core::inventory::items::world_item` module.
fn compute_drop_initial_rotation(world_loc: IVec3, now: f32) -> Quat {
    let rx = hash01(world_loc.x ^ world_loc.y ^ 0x71, now * 0.47) * std::f32::consts::TAU;
    let ry = hash01(world_loc.y ^ world_loc.z ^ 0x82, now * 0.63) * std::f32::consts::TAU;
    let rz = hash01(world_loc.z ^ world_loc.x ^ 0x93, now * 0.79) * std::f32::consts::TAU;
    Quat::from_euler(EulerRot::XYZ, rx, ry, rz)
}

/// Runs the `hash01` routine for hash01 in the `core::inventory::items::world_item` module.
fn hash01(input: i32, time_factor: f32) -> f32 {
    let x = input as f32 * 12.9898 + time_factor * 78.233;
    (x.sin() * 43_758.547).fract().abs()
}

/// Runs the `center_mesh_vertices` routine for center mesh vertices in the `core::inventory::items::world_item` module.
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

/// Builds a crossed-quad mesh for non-block dropped items.
///
/// This keeps item-icon alpha silhouettes while avoiding pure "paper-flat"
/// look from side views.
fn build_crossed_item_drop_mesh(plane_size: f32) -> Mesh {
    let half = plane_size * 0.5;
    let positions = vec![
        // Quad A (XY plane, facing +Z)
        [-half, -half, 0.0],
        [half, -half, 0.0],
        [half, half, 0.0],
        [-half, half, 0.0],
        // Quad B (ZY plane, facing +X)
        [0.0, -half, -half],
        [0.0, -half, half],
        [0.0, half, half],
        [0.0, half, -half],
    ];
    let normals = vec![
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
    ];
    let uvs = vec![
        [0.0, 1.0],
        [1.0, 1.0],
        [1.0, 0.0],
        [0.0, 0.0],
        [0.0, 1.0],
        [1.0, 1.0],
        [1.0, 0.0],
        [0.0, 0.0],
    ];
    let indices = vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7];

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn build_extruded_item_drop_mesh_from_texture(texture_path: &str, plane_size: f32) -> Option<Mesh> {
    if texture_path.trim().is_empty()
        || texture_path.starts_with("block-icon://")
        || !texture_path.to_ascii_lowercase().ends_with(".png")
    {
        return None;
    }

    let rel_path = texture_path
        .trim()
        .strip_prefix("assets/")
        .unwrap_or(texture_path.trim());
    let fs_path = Path::new("assets").join(rel_path);
    let mut image = image::open(fs_path).ok()?.to_rgba8();
    if image.width() == 0 || image.height() == 0 {
        return None;
    }
    if image.width() > ITEM_DROP_MAX_ICON_DIM || image.height() > ITEM_DROP_MAX_ICON_DIM {
        image = imageops::resize(
            &image,
            ITEM_DROP_MAX_ICON_DIM,
            ITEM_DROP_MAX_ICON_DIM,
            FilterType::Nearest,
        );
    }

    build_extruded_item_drop_mesh_from_rgba(image, plane_size)
}

fn build_extruded_item_drop_mesh_from_rgba(image: RgbaImage, plane_size: f32) -> Option<Mesh> {
    let width = image.width();
    let height = image.height();
    if width == 0 || height == 0 {
        return None;
    }

    let px_w = plane_size / width as f32;
    let px_h = plane_size / height as f32;
    let half = plane_size * 0.5;
    let half_thickness = (plane_size * ITEM_DROP_EXTRUDE_THICKNESS_FACTOR * 0.5).max(0.002);

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    #[inline]
    fn is_opaque(image: &RgbaImage, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 || x >= image.width() as i32 || y >= image.height() as i32 {
            return false;
        }
        image.get_pixel(x as u32, y as u32)[3] >= ITEM_DROP_ALPHA_THRESHOLD
    }

    #[inline]
    fn push_quad(
        positions: &mut Vec<[f32; 3]>,
        normals: &mut Vec<[f32; 3]>,
        uvs: &mut Vec<[f32; 2]>,
        indices: &mut Vec<u32>,
        p0: [f32; 3],
        p1: [f32; 3],
        p2: [f32; 3],
        p3: [f32; 3],
        normal: [f32; 3],
        uv0: [f32; 2],
        uv1: [f32; 2],
        uv2: [f32; 2],
        uv3: [f32; 2],
    ) {
        let base = positions.len() as u32;
        positions.extend_from_slice(&[p0, p1, p2, p3]);
        normals.extend_from_slice(&[normal, normal, normal, normal]);
        uvs.extend_from_slice(&[uv0, uv1, uv2, uv3]);
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    for y in 0..height {
        for x in 0..width {
            if image.get_pixel(x, y)[3] < ITEM_DROP_ALPHA_THRESHOLD {
                continue;
            }

            let x0 = -half + x as f32 * px_w;
            let x1 = x0 + px_w;
            let y_top = half - y as f32 * px_h;
            let y_bottom = y_top - px_h;
            let z_front = half_thickness;
            let z_back = -half_thickness;

            let u0 = x as f32 / width as f32;
            let u1 = (x + 1) as f32 / width as f32;
            let v0 = y as f32 / height as f32;
            let v1 = (y + 1) as f32 / height as f32;
            let uc = (u0 + u1) * 0.5;
            let vc = (v0 + v1) * 0.5;

            // Front (+Z).
            push_quad(
                &mut positions,
                &mut normals,
                &mut uvs,
                &mut indices,
                [x0, y_bottom, z_front],
                [x1, y_bottom, z_front],
                [x1, y_top, z_front],
                [x0, y_top, z_front],
                [0.0, 0.0, 1.0],
                [u0, v1],
                [u1, v1],
                [u1, v0],
                [u0, v0],
            );

            // Back (-Z).
            push_quad(
                &mut positions,
                &mut normals,
                &mut uvs,
                &mut indices,
                [x1, y_bottom, z_back],
                [x0, y_bottom, z_back],
                [x0, y_top, z_back],
                [x1, y_top, z_back],
                [0.0, 0.0, -1.0],
                [u0, v1],
                [u1, v1],
                [u1, v0],
                [u0, v0],
            );

            // Left edge (-X).
            if !is_opaque(&image, x as i32 - 1, y as i32) {
                push_quad(
                    &mut positions,
                    &mut normals,
                    &mut uvs,
                    &mut indices,
                    [x0, y_bottom, z_back],
                    [x0, y_bottom, z_front],
                    [x0, y_top, z_front],
                    [x0, y_top, z_back],
                    [-1.0, 0.0, 0.0],
                    [uc, vc],
                    [uc, vc],
                    [uc, vc],
                    [uc, vc],
                );
            }

            // Right edge (+X).
            if !is_opaque(&image, x as i32 + 1, y as i32) {
                push_quad(
                    &mut positions,
                    &mut normals,
                    &mut uvs,
                    &mut indices,
                    [x1, y_bottom, z_front],
                    [x1, y_bottom, z_back],
                    [x1, y_top, z_back],
                    [x1, y_top, z_front],
                    [1.0, 0.0, 0.0],
                    [uc, vc],
                    [uc, vc],
                    [uc, vc],
                    [uc, vc],
                );
            }

            // Top edge (+Y).
            if !is_opaque(&image, x as i32, y as i32 - 1) {
                push_quad(
                    &mut positions,
                    &mut normals,
                    &mut uvs,
                    &mut indices,
                    [x0, y_top, z_front],
                    [x1, y_top, z_front],
                    [x1, y_top, z_back],
                    [x0, y_top, z_back],
                    [0.0, 1.0, 0.0],
                    [uc, vc],
                    [uc, vc],
                    [uc, vc],
                    [uc, vc],
                );
            }

            // Bottom edge (-Y).
            if !is_opaque(&image, x as i32, y as i32 + 1) {
                push_quad(
                    &mut positions,
                    &mut normals,
                    &mut uvs,
                    &mut indices,
                    [x0, y_bottom, z_back],
                    [x1, y_bottom, z_back],
                    [x1, y_bottom, z_front],
                    [x0, y_bottom, z_front],
                    [0.0, -1.0, 0.0],
                    [uc, vc],
                    [uc, vc],
                    [uc, vc],
                    [uc, vc],
                );
            }
        }
    }

    if positions.is_empty() {
        return None;
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
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

fn resolve_block_drop_spawn_center(
    block_registry: &BlockRegistry,
    chunk_map: &ChunkMap,
    world_loc: IVec3,
) -> (IVec3, Vec3) {
    const OFFSETS: [IVec3; 16] = [
        IVec3::new(0, 0, 0),
        IVec3::new(0, 1, 0),
        IVec3::new(0, 2, 0),
        IVec3::new(1, 0, 0),
        IVec3::new(-1, 0, 0),
        IVec3::new(0, 0, 1),
        IVec3::new(0, 0, -1),
        IVec3::new(1, 1, 0),
        IVec3::new(-1, 1, 0),
        IVec3::new(0, 1, 1),
        IVec3::new(0, 1, -1),
        IVec3::new(1, 0, 1),
        IVec3::new(1, 0, -1),
        IVec3::new(-1, 0, 1),
        IVec3::new(-1, 0, -1),
        IVec3::new(0, 3, 0),
    ];

    for offset in OFFSETS {
        let cell = world_loc + offset;
        if can_spawn_drop_in_cell(block_registry, chunk_map, cell) {
            let center = Vec3::new(
                (cell.x as f32 + 0.5) * VOXEL_SIZE,
                (cell.y as f32) * VOXEL_SIZE + 0.36 * VOXEL_SIZE,
                (cell.z as f32 + 0.5) * VOXEL_SIZE,
            );
            return (cell, center);
        }
    }

    (
        world_loc,
        Vec3::new(
            (world_loc.x as f32 + 0.5) * VOXEL_SIZE,
            (world_loc.y as f32) * VOXEL_SIZE + 0.36 * VOXEL_SIZE,
            (world_loc.z as f32 + 0.5) * VOXEL_SIZE,
        ),
    )
}

#[inline]
fn can_spawn_drop_in_cell(
    block_registry: &BlockRegistry,
    chunk_map: &ChunkMap,
    cell: IVec3,
) -> bool {
    let id = get_block_world(chunk_map, cell);
    id == 0 || !block_registry.stats(id).solid
}
