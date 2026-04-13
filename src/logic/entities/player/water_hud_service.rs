use crate::core::entities::player::PlayerCamera;
use crate::core::states::states::{AppState, InGameStates};
use crate::core::ui::UiInteractionState;
use crate::core::world::block::{BlockRegistry, VOXEL_SIZE, fluid_at_world, get_block_world};
use crate::core::world::chunk::ChunkMap;
use crate::core::world::chunk::SEA_LEVEL;
use crate::core::world::chunk_dimension::world_to_chunk_xz;
use crate::core::world::fluid::FluidMap;
use crate::generator::chunk::chunk_utils::safe_despawn_entity;
use bevy::prelude::*;

const UW_VIS_GOOD_BLOCKS: f32 = 30.0;
const UW_VIS_MEDIUM_END_BLOCKS: f32 = 45.0;
const UW_VIS_WEAK_END_SHALLOW_BLOCKS: f32 = 54.0;
const UW_VIS_WEAK_END_DEEP_BLOCKS: f32 = 46.0;
const UW_OVERLAY_MAX_ALPHA: f32 = 0.18;
const UW_STATE_HOLD_SECONDS: f32 = 0.28;
const UW_FOG_SMOOTH_SPEED: f32 = 8.0;

/// Represents underwater fx plugin used by the `logic::entities::player::water_hud_service` module.
pub struct UnderwaterFxPlugin;

impl Plugin for UnderwaterFxPlugin {
    /// Builds this component for the `logic::entities::player::water_hud_service` module.
    fn build(&self, app: &mut App) {
        app.init_resource::<UnderwaterFxState>().add_systems(
            Update,
            update_underwater_fx.run_if(in_state(AppState::InGame(InGameStates::Game))),
        );
    }
}

// --- State & Marker -----------------------------------------------

/// Represents underwater fx state used by the `logic::entities::player::water_hud_service` module.
#[derive(Resource)]
struct UnderwaterFxState {
    overlay: Option<Entity>,
    was_underwater: bool,
    last_underwater_seen_at: f32,
    fog_camera: Option<Entity>,
    base_fog: Option<DistanceFog>,
    base_clear_color: Option<Color>,
    smoothed_fog: Option<UnderwaterFogParams>,
}

impl Default for UnderwaterFxState {
    fn default() -> Self {
        Self {
            overlay: None,
            was_underwater: false,
            last_underwater_seen_at: -10_000.0,
            fog_camera: None,
            base_fog: None,
            base_clear_color: None,
            smoothed_fog: None,
        }
    }
}

/// Represents underwater overlay used by the `logic::entities::player::water_hud_service` module.
#[derive(Component)]
struct UnderwaterOverlay;

#[derive(Clone, Copy)]
struct UnderwaterFogParams {
    color: Vec3,
    start: f32,
    end: f32,
}

// --- System -------------------------------------------------------

/// Updates underwater fx for the `logic::entities::player::water_hud_service` module.
fn update_underwater_fx(
    mut commands: Commands,
    mut state: ResMut<UnderwaterFxState>,
    time: Res<Time>,
    ui_interaction: Res<UiInteractionState>,
    fluids: Option<Res<FluidMap>>,
    chunk_map: Option<Res<ChunkMap>>,
    blocks: Option<Res<BlockRegistry>>,
    mut q_player_cam: Query<
        (
            Entity,
            &GlobalTransform,
            Option<&mut DistanceFog>,
            Option<&mut Camera>,
        ),
        With<PlayerCamera>,
    >,
    mut q_fallback_cam: Query<
        (
            Entity,
            &GlobalTransform,
            Option<&mut DistanceFog>,
            Option<&mut Camera>,
        ),
        (With<Camera3d>, Without<PlayerCamera>),
    >,
    mut overlay_q: Query<&mut BackgroundColor, With<UnderwaterOverlay>>,
) {
    let cam = q_player_cam
        .iter_mut()
        .next()
        .or_else(|| q_fallback_cam.iter_mut().next());
    let Some((cam_entity, xf, fog_opt, cam_opt)) = cam else {
        return;
    };
    let eye = xf.translation();

    let wx = (eye.x / VOXEL_SIZE).floor() as i32;
    let wy = (eye.y / VOXEL_SIZE).floor() as i32;
    let wz = (eye.z / VOXEL_SIZE).floor() as i32;
    let (eye_chunk, _) = world_to_chunk_xz(wx, wz);
    let eye_chunk_loaded = chunk_map
        .as_ref()
        .is_some_and(|cm| cm.chunks.contains_key(&eye_chunk));

    let underwater_fluid_map = fluids
        .as_ref()
        .is_some_and(|f| fluid_at_world(f, wx, wy, wz));
    let underwater_block_id = match (chunk_map.as_ref(), blocks.as_ref()) {
        (Some(cm), Some(reg)) => reg.is_fluid(get_block_world(cm, IVec3::new(wx, wy, wz))),
        _ => false,
    };
    let fallback_by_sea_height = eye.y <= ((SEA_LEVEL as f32 + 0.20) * VOXEL_SIZE);
    let needs_height_fallback = fluids.is_none() && (chunk_map.is_none() || blocks.is_none());
    let mut underwater_sampled = underwater_fluid_map
        || underwater_block_id
        || (needs_height_fallback && fallback_by_sea_height);
    let sampling_reliable = eye_chunk_loaded || needs_height_fallback;
    if !sampling_reliable {
        // Avoid fog/overlay flicker while the player's current chunk is still streaming in.
        underwater_sampled = state.was_underwater;
    }
    if underwater_sampled {
        state.last_underwater_seen_at = time.elapsed_secs();
    }
    let underwater = underwater_sampled
        || (time.elapsed_secs() - state.last_underwater_seen_at) <= UW_STATE_HOLD_SECONDS;

    // --- Enter / Leave -------------------------------------------
    if underwater && !state.was_underwater {
        info!("Entering underwater");
        if state.overlay.is_none() {
            let e = commands
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        right: Val::Px(0.0),
                        top: Val::Px(0.0),
                        bottom: Val::Px(0.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.08, 0.20, 0.30, 0.06)),
                    GlobalZIndex(-10),
                    UnderwaterOverlay,
                ))
                .id();
            state.overlay = Some(e);
        }
    } else if !underwater && state.was_underwater {
        if let Some(e) = state.overlay.take() {
            safe_despawn_entity(&mut commands, e);
        }
    }

    if state.fog_camera != Some(cam_entity) {
        state.fog_camera = Some(cam_entity);
        state.base_fog = fog_opt.as_ref().map(|fog| (**fog).clone());
        state.base_clear_color = cam_opt.as_ref().and_then(|cam| {
            if let ClearColorConfig::Custom(color) = cam.clear_color {
                Some(color)
            } else {
                None
            }
        });
        state.smoothed_fog = None;
    }

    if underwater {
        if state.base_fog.is_none() {
            state.base_fog = fog_opt.as_ref().map(|fog| (**fog).clone());
        }
        if state.base_clear_color.is_none() {
            state.base_clear_color = cam_opt.as_ref().and_then(|cam| {
                if let ClearColorConfig::Custom(color) = cam.clear_color {
                    Some(color)
                } else {
                    None
                }
            });
        }
    }

    let underwater_target = if underwater {
        let depth = ((SEA_LEVEL as f32 * VOXEL_SIZE) - eye.y).max(0.0);
        let depth_factor = (depth / 9.0).clamp(0.0, 1.0);
        let look_up = xf.forward().dot(Vec3::Y).clamp(0.0, 1.0);

        let mut r = 0.10 + (0.045 - 0.10) * depth_factor;
        let mut g = 0.25 + (0.16 - 0.25) * depth_factor;
        let mut b = 0.34 + (0.24 - 0.34) * depth_factor;

        // Looking upwards should feel slightly brighter/greener (towards surface light).
        r += 0.018 * look_up;
        g += 0.034 * look_up;
        b += 0.010 * look_up;

        let fog_start = UW_VIS_GOOD_BLOCKS * VOXEL_SIZE;
        let fog_end_blocks = (UW_VIS_WEAK_END_SHALLOW_BLOCKS
            + (UW_VIS_WEAK_END_DEEP_BLOCKS - UW_VIS_WEAK_END_SHALLOW_BLOCKS) * depth_factor)
            .max(UW_VIS_MEDIUM_END_BLOCKS + 1.0);
        let fog_end = fog_end_blocks * VOXEL_SIZE;

        Some(UnderwaterFogParams {
            color: Vec3::new(r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0)),
            start: fog_start,
            end: fog_end,
        })
    } else {
        None
    };

    let smoothed_underwater = if let Some(target) = underwater_target {
        let dt = time.delta_secs().max(0.0);
        let t = (1.0 - (-UW_FOG_SMOOTH_SPEED * dt).exp()).clamp(0.0, 1.0);
        let next = if let Some(prev) = state.smoothed_fog {
            UnderwaterFogParams {
                color: prev.color.lerp(target.color, t),
                start: prev.start + (target.start - prev.start) * t,
                end: prev.end + (target.end - prev.end) * t,
            }
        } else {
            target
        };
        state.smoothed_fog = Some(next);
        Some(next)
    } else {
        state.smoothed_fog = None;
        None
    };

    let mut remove_underwater_only_fog = false;
    if let Some(mut fog) = fog_opt {
        if state.base_fog.is_none() && !underwater {
            state.base_fog = Some((*fog).clone());
        }

        if let Some(uw) = smoothed_underwater {
            fog.color = Color::srgb(uw.color.x, uw.color.y, uw.color.z);
            fog.falloff = FogFalloff::Linear {
                start: uw.start,
                end: uw.end,
            };
        } else if let Some(base) = state.base_fog.clone() {
            *fog = base;
        } else {
            remove_underwater_only_fog = true;
        }
    } else if let Some(uw) = smoothed_underwater {
        commands.entity(cam_entity).insert(DistanceFog {
            color: Color::srgb(uw.color.x, uw.color.y, uw.color.z),
            falloff: FogFalloff::Linear {
                start: uw.start,
                end: uw.end,
            },
            ..default()
        });
    }
    if remove_underwater_only_fog && !underwater {
        commands.entity(cam_entity).remove::<DistanceFog>();
    }

    if let Some(mut cam) = cam_opt {
        if let Some(uw) = smoothed_underwater {
            cam.clear_color =
                ClearColorConfig::Custom(Color::srgb(uw.color.x, uw.color.y, uw.color.z));
        } else if let Some(base_color) = state.base_clear_color {
            cam.clear_color = ClearColorConfig::Custom(base_color);
        }
    }

    if underwater {
        let view_dir = xf.forward();
        let up = Vec3::Y;

        let t = view_dir.dot(up).clamp(0.0, 1.0);

        let depth = (SEA_LEVEL as f32 - eye.y).max(0.0);
        let depth_factor = (depth / 6.0).clamp(0.0, 1.0);

        // Keep HUD overlay minimal; fog does most of the underwater look.
        let base = 0.02;
        let extra_look = 0.03 * t;
        let extra_depth = 0.08 * depth_factor;
        let alpha = if ui_interaction.blocks_game_input() {
            0.0
        } else {
            (base + extra_look + extra_depth).clamp(0.0, UW_OVERLAY_MAX_ALPHA)
        };

        if let Some(mut bg) = overlay_q.iter_mut().next() {
            let mut c = bg.0.to_linear();
            c.set_alpha(alpha);
            bg.0 = c.into();
        }
    }

    state.was_underwater = underwater;
}
