use crate::core::config::GlobalConfig;
use crate::core::debug::{DebugGridMode, DebugGridState};
use crate::core::entities::player::{Player, PlayerCamera};
use crate::core::world::block::VOXEL_SIZE;
use crate::core::world::chunk_dimension::{
    CX, CZ, SEC_COUNT, SEC_H, Y_MAX, Y_MIN, world_to_chunk_xz,
};
use crate::utils::key_utils::convert_input;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::math::primitives::Cuboid;
use bevy::mesh::Mesh3d;
use bevy::pbr::MeshMaterial3d;
use bevy::prelude::*;

#[derive(Component)]
pub(super) struct ChunkGridDebugLine;

#[derive(Default)]
pub(super) struct ChunkGridMeshCache {
    last_center: Option<IVec2>,
    last_plane_y: Option<i32>,
    last_mode: Option<DebugGridMode>,
    border_material: Option<Handle<StandardMaterial>>,
    corner_material: Option<Handle<StandardMaterial>>,
    edge_x_mesh: Option<Handle<Mesh>>,
    edge_z_mesh: Option<Handle<Mesh>>,
    edge_y_mesh: Option<Handle<Mesh>>,
}

/// Runs the `toggle_chunk_grid` routine for toggle chunk grid in the `client` module.
pub(super) fn toggle_chunk_grid(
    mut debug_grid: ResMut<DebugGridState>,
    keyboard: Res<ButtonInput<KeyCode>>,
    game_config: Res<GlobalConfig>,
    cam_query: Query<&GlobalTransform, With<PlayerCamera>>,
    player_query: Query<&Transform, With<Player>>,
) {
    let key = convert_input(game_config.input.chunk_grid.as_str()).unwrap_or(KeyCode::F9);

    if keyboard.just_pressed(key) {
        debug_grid.mode = match debug_grid.mode {
            DebugGridMode::Off => DebugGridMode::Chunks,
            DebugGridMode::Chunks => DebugGridMode::AllSubchunks,
            DebugGridMode::AllSubchunks => DebugGridMode::Off,
        };
        debug_grid.show = !matches!(debug_grid.mode, DebugGridMode::Off);
        if debug_grid.show {
            if let Some(cam) = cam_query.iter().next() {
                debug_grid.plane_y = cam.translation().y.floor();
            } else if let Some(player_transform) = player_query.iter().next() {
                debug_grid.plane_y = player_transform.translation.y.floor();
            }
        }
        info!("Chunk Grid: {}", grid_mode_label(debug_grid.mode));
    }

    if debug_grid.show {
        if let Some(cam) = cam_query.iter().next() {
            debug_grid.plane_y = cam.translation().y.floor();
        } else if let Some(player_transform) = player_query.iter().next() {
            debug_grid.plane_y = player_transform.translation.y.floor();
        }
    }
}

/// Synchronizes chunk-grid debug meshes for the `client` module.
pub(super) fn sync_chunk_grid_meshes(
    mut commands: Commands,
    debug_grid: Res<DebugGridState>,
    cam_query: Query<&GlobalTransform, With<PlayerCamera>>,
    player_query: Query<&Transform, With<Player>>,
    existing_lines: Query<Entity, With<ChunkGridDebugLine>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cache: Local<ChunkGridMeshCache>,
) {
    let existing: Vec<Entity> = existing_lines.iter().collect();
    if !debug_grid.show || matches!(debug_grid.mode, DebugGridMode::Off) {
        for entity in existing {
            commands.entity(entity).despawn();
        }
        cache.last_center = None;
        cache.last_plane_y = None;
        cache.last_mode = None;
        return;
    }

    let cam_pos = if let Some(cam) = cam_query.iter().next() {
        cam.translation()
    } else if let Some(player) = player_query.iter().next() {
        player.translation
    } else {
        for entity in existing {
            commands.entity(entity).despawn();
        }
        cache.last_center = None;
        cache.last_plane_y = None;
        cache.last_mode = None;
        return;
    };

    let world_x = (cam_pos.x / VOXEL_SIZE).floor() as i32;
    let world_z = (cam_pos.z / VOXEL_SIZE).floor() as i32;
    let (center_chunk, _) = world_to_chunk_xz(world_x, world_z);
    let plane_y_i = debug_grid.plane_y.floor() as i32;

    let needs_rebuild = cache.last_center != Some(center_chunk)
        || cache.last_plane_y != Some(plane_y_i)
        || cache.last_mode != Some(debug_grid.mode)
        || existing.is_empty();
    if !needs_rebuild {
        return;
    }

    for entity in existing {
        commands.entity(entity).despawn();
    }

    let s = VOXEL_SIZE;
    let chunk_w = CX as f32 * s;
    let chunk_d = CZ as f32 * s;
    let y_min = Y_MIN as f32 * s;
    let y_max = (Y_MAX as f32 + 1.0) * s;
    let y_len = (y_max - y_min).max(0.05);
    let y_center = (y_min + y_max) * 0.5;
    let line_thickness = (0.06 * s).max(0.03);
    let plane_y = debug_grid.plane_y + line_thickness * 0.5;

    let border_material = cache
        .border_material
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: Color::srgb(0.96, 0.54, 0.12),
                emissive: Color::srgb(0.96, 0.54, 0.12).into(),
                unlit: true,
                cull_mode: None,
                ..default()
            })
        })
        .clone();
    let corner_material = cache
        .corner_material
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: Color::srgb(0.98, 0.88, 0.16),
                emissive: Color::srgb(0.98, 0.88, 0.16).into(),
                unlit: true,
                cull_mode: None,
                ..default()
            })
        })
        .clone();

    let edge_x_mesh = cache
        .edge_x_mesh
        .get_or_insert_with(|| {
            meshes.add(Mesh::from(Cuboid::new(
                chunk_w,
                line_thickness,
                line_thickness,
            )))
        })
        .clone();
    let edge_z_mesh = cache
        .edge_z_mesh
        .get_or_insert_with(|| {
            meshes.add(Mesh::from(Cuboid::new(
                line_thickness,
                line_thickness,
                chunk_d,
            )))
        })
        .clone();
    let edge_y_mesh = cache
        .edge_y_mesh
        .get_or_insert_with(|| {
            meshes.add(Mesh::from(Cuboid::new(
                line_thickness,
                y_len,
                line_thickness,
            )))
        })
        .clone();

    let range = 2;
    for dz in -range..=range {
        for dx in -range..=range {
            let chunk = center_chunk + IVec2::new(dx, dz);
            let x0 = chunk.x as f32 * chunk_w;
            let z0 = chunk.y as f32 * chunk_d;
            let x1 = x0 + chunk_w;
            let z1 = z0 + chunk_d;

            if matches!(debug_grid.mode, DebugGridMode::Chunks) {
                let border_segments = [
                    (Vec3::new((x0 + x1) * 0.5, plane_y, z0), edge_x_mesh.clone()),
                    (Vec3::new((x0 + x1) * 0.5, plane_y, z1), edge_x_mesh.clone()),
                    (Vec3::new(x0, plane_y, (z0 + z1) * 0.5), edge_z_mesh.clone()),
                    (Vec3::new(x1, plane_y, (z0 + z1) * 0.5), edge_z_mesh.clone()),
                ];
                for (translation, mesh) in border_segments {
                    commands.spawn((
                        Mesh3d(mesh),
                        MeshMaterial3d(border_material.clone()),
                        Transform::from_translation(translation),
                        ChunkGridDebugLine,
                        NotShadowCaster,
                        NotShadowReceiver,
                    ));
                }
            } else {
                for section in 0..=SEC_COUNT {
                    let wy = Y_MIN + (section * SEC_H) as i32;
                    let y = (wy as f32) * s + line_thickness * 0.5;
                    let border_segments = [
                        (Vec3::new((x0 + x1) * 0.5, y, z0), edge_x_mesh.clone()),
                        (Vec3::new((x0 + x1) * 0.5, y, z1), edge_x_mesh.clone()),
                        (Vec3::new(x0, y, (z0 + z1) * 0.5), edge_z_mesh.clone()),
                        (Vec3::new(x1, y, (z0 + z1) * 0.5), edge_z_mesh.clone()),
                    ];
                    for (translation, mesh) in border_segments {
                        commands.spawn((
                            Mesh3d(mesh),
                            MeshMaterial3d(border_material.clone()),
                            Transform::from_translation(translation),
                            ChunkGridDebugLine,
                            NotShadowCaster,
                            NotShadowReceiver,
                        ));
                    }
                }
            }

            let corner_segments = [
                Vec3::new(x0, y_center, z0),
                Vec3::new(x1, y_center, z0),
                Vec3::new(x1, y_center, z1),
                Vec3::new(x0, y_center, z1),
            ];
            for translation in corner_segments {
                commands.spawn((
                    Mesh3d(edge_y_mesh.clone()),
                    MeshMaterial3d(corner_material.clone()),
                    Transform::from_translation(translation),
                    ChunkGridDebugLine,
                    NotShadowCaster,
                    NotShadowReceiver,
                ));
            }
        }
    }

    cache.last_center = Some(center_chunk);
    cache.last_plane_y = Some(plane_y_i);
    cache.last_mode = Some(debug_grid.mode);
}

fn grid_mode_label(mode: DebugGridMode) -> &'static str {
    match mode {
        DebugGridMode::Off => "Off",
        DebugGridMode::Chunks => "On",
        DebugGridMode::AllSubchunks => "All",
    }
}
