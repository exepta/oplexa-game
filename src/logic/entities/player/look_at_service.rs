use crate::core::entities::player::PlayerCamera;
use crate::core::entities::player::block_selection::SelectionState;
use crate::core::entities::player::{GameMode, GameModeState};
use crate::core::config::GlobalConfig;
use crate::core::states::states::{AppState, InGameStates};
use crate::core::world::block::{BlockRegistry, SelectedBlock, VOXEL_SIZE};
use crate::core::world::chunk::{ChunkMap, VoxelStage};
use crate::core::world::ray_cast_voxels;
use bevy::camera::visibility::RenderLayers;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;

/// Represents look at service used by the `logic::entities::player::look_at_service` module.
pub struct LookAtService;

/// Represents selection outline root used by the `logic::entities::player::look_at_service` module.
#[derive(Component)]
struct SelectionOutlineRoot;

impl Plugin for LookAtService {
    /// Builds this component for the `logic::entities::player::look_at_service` module.
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_selection_outline);

        app.configure_sets(
            Update,
            (
                VoxelStage::Input,
                VoxelStage::WorldEdit,
                VoxelStage::Meshing,
            )
                .chain(),
        );

        app.add_systems(
            Update,
            (
                update_selection.in_set(VoxelStage::Input),
                sync_selection_outline.in_set(VoxelStage::Input),
                pick_block_from_look,
            )
                .chain()
                .run_if(in_state(AppState::InGame(InGameStates::Game))),
        );
    }
}

/// Spawns selection outline for the `logic::entities::player::look_at_service` module.
fn spawn_selection_outline(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    game_config: Res<GlobalConfig>,
    q_existing: Query<Entity, With<SelectionOutlineRoot>>,
) {
    if !q_existing.is_empty() {
        return;
    }

    let edge_mesh = meshes.add(Mesh::from(Cuboid::new(1.0, 1.0, 1.0)));
    let outline_color = parse_hex_color(
        game_config
            .interface
            .block_selection_border_color
            .as_str(),
    )
    .unwrap_or_else(|| {
        warn!(
            "Invalid interface.block-selection-border-color='{}', using fallback '#111111'",
            game_config.interface.block_selection_border_color
        );
        Color::srgba(17.0 / 255.0, 17.0 / 255.0, 17.0 / 255.0, 1.0)
    });
    let edge_mat = materials.add(StandardMaterial {
        base_color: outline_color,
        unlit: true,
        cull_mode: None,
        ..default()
    });

    let s = VOXEL_SIZE;
    let half = s * 0.5 + 0.008;
    let len = s + 0.016;
    let line_width = game_config.interface.selection_line_width.clamp(0.1, 16.0);
    let t = (s * 0.010 * line_width).max(0.002);

    commands
        .spawn((
            SelectionOutlineRoot,
            Name::new("SelectionOutline"),
            Transform::default(),
            GlobalTransform::default(),
            Visibility::Hidden,
            InheritedVisibility::default(),
            ViewVisibility::default(),
            RenderLayers::layer(0),
        ))
        .with_children(|p| {
            for y in [-half, half] {
                for z in [-half, half] {
                    p.spawn((
                        Mesh3d(edge_mesh.clone()),
                        MeshMaterial3d(edge_mat.clone()),
                        Transform::from_translation(Vec3::new(0.0, y, z))
                            .with_scale(Vec3::new(len, t, t)),
                        RenderLayers::layer(0),
                        NotShadowCaster,
                        NotShadowReceiver,
                    ));
                }
            }
            for x in [-half, half] {
                for z in [-half, half] {
                    p.spawn((
                        Mesh3d(edge_mesh.clone()),
                        MeshMaterial3d(edge_mat.clone()),
                        Transform::from_translation(Vec3::new(x, 0.0, z))
                            .with_scale(Vec3::new(t, len, t)),
                        RenderLayers::layer(0),
                        NotShadowCaster,
                        NotShadowReceiver,
                    ));
                }
            }
            for x in [-half, half] {
                for y in [-half, half] {
                    p.spawn((
                        Mesh3d(edge_mesh.clone()),
                        MeshMaterial3d(edge_mat.clone()),
                        Transform::from_translation(Vec3::new(x, y, 0.0))
                            .with_scale(Vec3::new(t, t, len)),
                        RenderLayers::layer(0),
                        NotShadowCaster,
                        NotShadowReceiver,
                    ));
                }
            }
        });
}

/// Updates selection for the `logic::entities::player::look_at_service` module.
fn update_selection(
    mut sel: ResMut<SelectionState>,
    game_mode: Res<GameModeState>,
    registry: Res<BlockRegistry>,
    q_player_cam: Query<(&GlobalTransform, &Camera), With<PlayerCamera>>,
    q_fallback_cam: Query<(&GlobalTransform, &Camera), With<Camera3d>>,
    chunk_map: Res<ChunkMap>,
) {
    if matches!(game_mode.0, GameMode::Spectator) {
        sel.hit = None;
        return;
    }

    let cam = q_player_cam
        .iter()
        .next()
        .or_else(|| q_fallback_cam.iter().next());
    let Some((tf, _cam)) = cam else {
        sel.hit = None;
        return;
    };

    let origin_bs = tf.translation() / VOXEL_SIZE;
    let dir_bs: Vec3 = tf.forward().into();
    let max_dist_blocks = 8.0;

    sel.hit = ray_cast_voxels(origin_bs, dir_bs, max_dist_blocks, &chunk_map, &registry);
}

/// Synchronizes selection outline for the `logic::entities::player::look_at_service` module.
fn sync_selection_outline(
    sel: Res<SelectionState>,
    game_mode: Res<GameModeState>,
    registry: Res<BlockRegistry>,
    mut q_outline: Query<(&mut Transform, &mut Visibility), With<SelectionOutlineRoot>>,
) {
    let Ok((mut tf, mut vis)) = q_outline.single_mut() else {
        return;
    };

    if matches!(game_mode.0, GameMode::Spectator) {
        *vis = Visibility::Hidden;
        return;
    }

    if let Some(hit) = sel.hit {
        let id = hit.block_id;
        let Some((size, offset)) = registry.selection_box(id) else {
            *vis = Visibility::Hidden;
            return;
        };
        let s = VOXEL_SIZE;
        tf.translation = Vec3::new(
            (hit.block_pos.x as f32 + 0.5 + offset[0]) * s,
            (hit.block_pos.y as f32 + 0.5 + offset[1]) * s,
            (hit.block_pos.z as f32 + 0.5 + offset[2]) * s,
        );
        tf.scale = Vec3::new(size[0], size[1], size[2]).max(Vec3::splat(0.02));
        *vis = Visibility::Visible;
    } else {
        *vis = Visibility::Hidden;
    }
}

/// Picks block from look for the `logic::entities::player::look_at_service` module.
fn pick_block_from_look(
    buttons: Res<ButtonInput<MouseButton>>,
    game_mode: Res<GameModeState>,
    sel_state: Res<SelectionState>,
    reg: Res<BlockRegistry>,
    mut selected: ResMut<SelectedBlock>,
) {
    if matches!(game_mode.0, GameMode::Spectator) {
        return;
    }
    if !buttons.just_pressed(MouseButton::Middle) {
        return;
    }
    let Some(hit) = sel_state.hit else {
        return;
    };

    let id = hit.block_id;
    if id == 0 {
        return;
    }

    selected.id = id;
    selected.name = reg.display_name_opt(id).unwrap_or("").to_string();
    debug!("Picked block: {} ({})", selected.name, selected.id);
}

fn parse_hex_color(raw: &str) -> Option<Color> {
    let trimmed = raw.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);

    match hex.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(Color::srgba(
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                1.0,
            ))
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            Some(Color::srgba(
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                a as f32 / 255.0,
            ))
        }
        _ => None,
    }
}
