use crate::core::config::GlobalConfig;
use crate::core::entities::player::{
    FlightState, FpsController, GameMode, GameModeState, Player, PlayerCamera,
};
use crate::core::multiplayer::MultiplayerConnectionState;
use crate::core::states::states::{AppState, InGameStates};
use crate::core::ui::UiInteractionState;
use crate::core::world::block::{BlockRegistry, VOXEL_SIZE};
use crate::core::world::chunk::{ChunkMap, SEA_LEVEL};
use crate::core::world::chunk_dimension::{
    CX, CY, Y_MAX, Y_MIN, local_y_to_world, world_to_chunk_xz, world_y_to_local,
};
use crate::generator::chunk::chunk_meshing::safe_despawn_entity;
use crate::utils::key_utils::convert_input;
use bevy::camera::visibility::RenderLayers;
use bevy::input::mouse::MouseMotion;
use bevy::light::{CascadeShadowConfigBuilder, GlobalAmbientLight};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_rapier3d::prelude::*;

const EYE_HEIGHT: f32 = 1.05;
const RADIUS: f32 = 0.30;
const HEADROOM: f32 = 0.10;

/// Represents main sun used by the `logic::entities::player::init_service` module.
#[derive(Component)]
struct MainSun;

/// Represents double tap space used by the `logic::entities::player::init_service` module.
#[derive(Component)]
struct DoubleTapSpace {
    last_press: f32,
}

/// Represents player kinematics used by the `logic::entities::player::init_service` module.
#[derive(Component)]
struct PlayerKinematics {
    vel_y: f32,
}

/// Represents player initialize used by the `logic::entities::player::init_service` module.
pub struct PlayerInitialize;

impl Plugin for PlayerInitialize {
    /// Builds this component for the `logic::entities::player::init_service` module.
    fn build(&self, app: &mut App) {
        app.insert_resource(GlobalAmbientLight {
            color: Color::WHITE,
            brightness: 125.0,
            affects_lightmapped_meshes: false,
        })
        .add_systems(
            OnEnter(AppState::InGame(InGameStates::Game)),
            (enable_physics_pipeline, spawn_scene, spawn_player),
        )
        .add_systems(
            OnExit(AppState::InGame(InGameStates::Game)),
            (disable_physics_pipeline, despawn_player_entities),
        )
        .add_systems(
            Update,
            (
                grab_cursor_on_click,
                release_cursor_on_escape,
                mouse_look,
                player_move_simple,
                apply_noclip_to_player,
            )
                .run_if(resource_exists::<BlockRegistry>),
        );
    }
}

/// Runs the `enable_physics_pipeline` routine for enable physics pipeline in the `logic::entities::player::init_service` module.
fn enable_physics_pipeline(mut configs: Query<&mut RapierConfiguration>) {
    for mut config in &mut configs {
        config.physics_pipeline_active = true;
    }
}

/// Runs the `disable_physics_pipeline` routine for disable physics pipeline in the `logic::entities::player::init_service` module.
fn disable_physics_pipeline(mut configs: Query<&mut RapierConfiguration>) {
    for mut config in &mut configs {
        config.physics_pipeline_active = false;
    }
}

/// Runs the `despawn_player_entities` routine for despawn player entities in the `logic::entities::player::init_service` module.
fn despawn_player_entities(
    mut commands: Commands,
    existing_players: Query<Entity, With<Player>>,
    existing_player_cams: Query<Entity, With<PlayerCamera>>,
) {
    for entity in &existing_players {
        safe_despawn_entity(&mut commands, entity);
    }
    for entity in &existing_player_cams {
        safe_despawn_entity(&mut commands, entity);
    }
}

/// Spawns scene for the `logic::entities::player::init_service` module.
fn spawn_scene(mut commands: Commands, existing_sun: Query<Entity, With<MainSun>>) {
    if !existing_sun.is_empty() {
        return;
    }
    commands.spawn((
        MainSun,
        DirectionalLight {
            shadows_enabled: false,
            illuminance: 1_000.0,
            color: Color::WHITE,
            ..default()
        },
        CascadeShadowConfigBuilder {
            first_cascade_far_bound: 16.0,
            maximum_distance: 180.0,
            ..default()
        }
        .build(),
        Transform::from_xyz(4.0, 200.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// Spawns player for the `logic::entities::player::init_service` module.
fn spawn_player(
    mut commands: Commands,
    game_config: Res<GlobalConfig>,
    multiplayer_connection: Option<Res<MultiplayerConnectionState>>,
    block_registry: Res<BlockRegistry>,
    chunk_map: Res<ChunkMap>,
    existing_players: Query<Entity, With<Player>>,
    existing_player_cams: Query<Entity, With<PlayerCamera>>,
) {
    for entity in &existing_players {
        safe_despawn_entity(&mut commands, entity);
    }
    for entity in &existing_player_cams {
        safe_despawn_entity(&mut commands, entity);
    }

    let mut spawn_translation = multiplayer_connection
        .as_ref()
        .and_then(|connection| connection.spawn_translation)
        .unwrap_or([0.0, 180.0, 0.0]);
    if let Some(found) = find_loaded_safe_spawn(&chunk_map, &block_registry, spawn_translation) {
        spawn_translation = found;
    }
    let spawn_yaw_pitch = multiplayer_connection
        .as_ref()
        .and_then(|connection| connection.spawn_yaw_pitch)
        .unwrap_or([0.0, 0.0]);

    let fov_deg: f32 = 80.0;
    let half_h = (EYE_HEIGHT + HEADROOM - RADIUS).max(0.60);
    const SKIN: f32 = 0.08;
    const STEP_MAX: f32 = 0.55;
    const STEP_MIN_WIDTH: f32 = 2.0 * RADIUS + 0.04;
    const SNAP: f32 = 0.08;
    let chunk_world_radius =
        (game_config.graphics.chunk_range.max(1) as f32) * (CX as f32) * VOXEL_SIZE;
    let fog_color_arr = game_config.graphics.fog_color;
    let fog_color = Color::srgb(
        fog_color_arr[0].clamp(0.0, 1.0),
        fog_color_arr[1].clamp(0.0, 1.0),
        fog_color_arr[2].clamp(0.0, 1.0),
    );
    let fog_start_factor = game_config.graphics.fog_start_factor.clamp(0.0, 3.0);
    let fog_end_factor = game_config
        .graphics
        .fog_end_factor
        .max(fog_start_factor + 0.01)
        .clamp(0.01, 3.0);
    let fog_start = (chunk_world_radius * fog_start_factor).max(0.1);
    let fog_end = (chunk_world_radius * fog_end_factor).max(fog_start + 1.0);
    let far_plane = (fog_end + game_config.graphics.far_clip_extra.max(0.5)).max(1.0);

    let player = commands
        .spawn((
            Player,
            Name::new("Player"),
            Transform::from_xyz(
                spawn_translation[0],
                spawn_translation[1],
                spawn_translation[2],
            )
            .with_rotation(Quat::from_rotation_y(spawn_yaw_pitch[0])),
            Visibility::default(),
            InheritedVisibility::default(),
            ViewVisibility::default(),
            RigidBody::KinematicPositionBased,
            Collider::capsule_y(half_h, RADIUS),
            CollisionGroups::default(),
            LockedAxes::ROTATION_LOCKED,
            KinematicCharacterController {
                offset: CharacterLength::Absolute(SKIN),
                slide: true,
                autostep: Some(CharacterAutostep {
                    max_height: CharacterLength::Absolute(STEP_MAX),
                    min_width: CharacterLength::Absolute(STEP_MIN_WIDTH),
                    include_dynamic_bodies: false,
                }),
                snap_to_ground: Some(CharacterLength::Absolute(SNAP)),
                max_slope_climb_angle: 50.0_f32.to_radians(),
                min_slope_slide_angle: 60.0_f32.to_radians(),
                up: Vec3::Y,
                ..default()
            },
            FpsController {
                yaw: spawn_yaw_pitch[0],
                pitch: spawn_yaw_pitch[1],
                speed: 8.0,
                sensitivity: 0.001,
            },
            PlayerKinematics { vel_y: 0.0 },
            FlightState { flying: false },
        ))
        .id();

    commands.entity(player).with_children(|c| {
        let mut camera = c.spawn((
            PlayerCamera,
            RenderLayers::from_layers(&[0, 1, 2]),
            Camera3d::default(),
            Projection::Perspective(PerspectiveProjection {
                fov: fov_deg.to_radians(),
                near: 0.05,
                far: far_plane,
                ..default()
            }),
            Camera {
                order: 1,
                clear_color: ClearColorConfig::Custom(fog_color),
                ..default()
            },
            Transform::from_xyz(0.0, EYE_HEIGHT, 0.0)
                .with_rotation(Quat::from_rotation_x(spawn_yaw_pitch[1])),
            Name::new("PlayerCamera"),
        ));

        if game_config.graphics.fog_enabled {
            camera.insert(DistanceFog {
                color: fog_color,
                falloff: FogFalloff::Linear {
                    start: fog_start,
                    end: fog_end,
                },
                ..default()
            });
        }
    });

    commands.entity(player).insert(DoubleTapSpace {
        last_press: -1_000_000.0,
    });
}

fn find_loaded_safe_spawn(
    chunk_map: &ChunkMap,
    block_registry: &BlockRegistry,
    anchor: [f32; 3],
) -> Option<[f32; 3]> {
    let anchor_x = anchor[0].floor() as i32;
    let anchor_z = anchor[2].floor() as i32;
    let mut best: Option<(u8, i32, i32, i32, i32)> = None;
    const SEARCH_RADIUS: i32 = 32;

    for dz in -SEARCH_RADIUS..=SEARCH_RADIUS {
        for dx in -SEARCH_RADIUS..=SEARCH_RADIUS {
            let wx = anchor_x + dx;
            let wz = anchor_z + dz;
            let (chunk_coord, local) = world_to_chunk_xz(wx, wz);
            let Some(chunk) = chunk_map.chunks.get(&chunk_coord) else {
                continue;
            };
            let lx = local.x as usize;
            let lz = local.y as usize;

            for ly in (0..CY).rev() {
                let block_id = chunk.get(lx, ly, lz);
                if block_id == 0
                    || block_registry.is_fluid(block_id)
                    || !block_registry.is_solid_for_collision(block_id)
                {
                    continue;
                }

                let world_y = local_y_to_world(ly);
                let dry_land = world_y >= SEA_LEVEL;
                let clear = loaded_has_clearance(chunk_map, block_registry, wx, world_y, wz);
                let tier = match (clear, dry_land) {
                    (true, true) => 3,
                    (true, false) => 2,
                    (false, true) => 1,
                    (false, false) => 0,
                };
                let dist2 = dx * dx + dz * dz;

                let replace = match best {
                    None => true,
                    Some((best_tier, best_dist2, best_y, _, _)) => {
                        tier > best_tier
                            || (tier == best_tier
                                && (dist2 < best_dist2
                                    || (dist2 == best_dist2 && world_y > best_y)))
                    }
                };

                if replace {
                    best = Some((tier, dist2, world_y, wx, wz));
                }
                break;
            }
        }
    }

    best.map(|(_, _, world_y, wx, wz)| [wx as f32 + 0.5, world_y as f32 + 2.0, wz as f32 + 0.5])
}

fn loaded_has_clearance(
    chunk_map: &ChunkMap,
    block_registry: &BlockRegistry,
    wx: i32,
    ground_y: i32,
    wz: i32,
) -> bool {
    let Some(head1) = loaded_block_id(chunk_map, wx, ground_y + 1, wz) else {
        return false;
    };
    let Some(head2) = loaded_block_id(chunk_map, wx, ground_y + 2, wz) else {
        return false;
    };
    block_registry.is_air(head1) && block_registry.is_air(head2)
}

fn loaded_block_id(chunk_map: &ChunkMap, wx: i32, wy: i32, wz: i32) -> Option<u16> {
    let (chunk_coord, local) = world_to_chunk_xz(wx, wz);
    let chunk = chunk_map.chunks.get(&chunk_coord)?;
    if wy < Y_MIN || wy > Y_MAX {
        return None;
    }
    let ly = world_y_to_local(wy);
    Some(chunk.get(local.x as usize, ly, local.y as usize))
}

/// Runs the `grab_cursor_on_click` routine for grab cursor on click in the `logic::entities::player::init_service` module.
fn grab_cursor_on_click(
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    ui_state: Option<Res<UiInteractionState>>,
) {
    if ui_state
        .as_ref()
        .is_some_and(|state| state.blocks_game_input())
    {
        return;
    }

    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    if let Ok(mut cursor) = cursor_q.single_mut() {
        if cursor.grab_mode == CursorGrabMode::Locked {
            return;
        }

        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

/// Runs the `release_cursor_on_escape` routine for release cursor on escape in the `logic::entities::player::init_service` module.
fn release_cursor_on_escape(
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
    keys: Res<ButtonInput<KeyCode>>,
    game_config: Res<GlobalConfig>,
) {
    let unlock = convert_input(game_config.input.ui_close_back.as_str()).expect("Invalid close/back key");
    if !keys.just_pressed(unlock) {
        return;
    }
    if let Ok(mut cursor) = cursor_q.single_mut() {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
}

/// Runs the `mouse_look` routine for mouse look in the `logic::entities::player::init_service` module.
fn mouse_look(
    mut ev_motion: MessageReader<MouseMotion>,
    mut q_player: Query<
        (Entity, &mut Transform, &mut FpsController),
        (With<Player>, Without<PlayerCamera>),
    >,
    mut q_cam: Query<(&ChildOf, &mut Transform), (With<PlayerCamera>, Without<Player>)>,
    cursor_q: Query<&CursorOptions, With<PrimaryWindow>>,
    game_config: Res<GlobalConfig>,
) {
    let Ok(cursor) = cursor_q.single() else {
        return;
    };
    if cursor.grab_mode != CursorGrabMode::Locked {
        ev_motion.clear();
        return;
    }

    let Ok((player_entity, mut player_tf, mut ctrl)) = q_player.single_mut() else {
        return;
    };

    let mut delta = Vec2::ZERO;
    for ev in ev_motion.read() {
        delta += ev.delta;
    }
    if delta == Vec2::ZERO {
        return;
    }

    ctrl.yaw -= delta.x * ctrl.sensitivity * game_config.gameplay.mouse_sensitivity_horizontal;
    ctrl.pitch -= delta.y * ctrl.sensitivity * game_config.gameplay.mouse_sensitivity_vertical;

    let limit = std::f32::consts::FRAC_PI_2 - 0.01;
    ctrl.pitch = ctrl.pitch.clamp(-limit, limit);
    player_tf.rotation = Quat::from_rotation_y(ctrl.yaw);

    for (parent, mut cam_tf) in &mut q_cam {
        if parent.parent() == player_entity {
            cam_tf.rotation = Quat::from_rotation_x(ctrl.pitch);
        }
    }
}

/// Runs the `player_move_simple` routine for player move simple in the `logic::entities::player::init_service` module.
fn player_move_simple(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    ui_state: Option<Res<UiInteractionState>>,
    mut q_player: Query<
        (
            &Transform,
            &FpsController,
            &mut PlayerKinematics,
            &mut KinematicCharacterController,
            Option<&KinematicCharacterControllerOutput>,
            &mut FlightState,
            &mut DoubleTapSpace,
        ),
        With<Player>,
    >,
    game_mode_state: Res<GameModeState>,
    game_config: Res<GlobalConfig>,
) {
    let Ok((tf, ctrl, mut kin, mut kcc, kcc_out, mut flight, mut tap)) = q_player.single_mut()
    else {
        return;
    };

    let ground_speed = ctrl.speed;
    let fly_multi = 2.4;
    let fly_v_multi = 2.4;
    let gravity = 30.0;
    let fall_multi = 2.2;
    const JUMP_HEIGHT: f32 = 1.65;
    let jump_v0 = (2.0 * gravity * JUMP_HEIGHT).sqrt();
    const DOUBLE_TAP_WIN: f32 = 0.28;

    let forward_key = convert_input(game_config.input.move_up.as_str()).expect("Invalid key");
    let back_key = convert_input(game_config.input.move_down.as_str()).expect("Invalid key");
    let left_key = convert_input(game_config.input.move_left.as_str()).expect("Invalid key");
    let right_key = convert_input(game_config.input.move_right.as_str()).expect("Invalid key");
    let jump_key = convert_input(game_config.input.jump.as_str()).unwrap_or(KeyCode::Space);
    let down_key = convert_input(game_config.input.sprint.as_str()).unwrap_or(KeyCode::ShiftLeft);
    let input_blocked = ui_state
        .as_ref()
        .is_some_and(|state| state.blocks_game_input());

    let f = tf.forward();
    let r = tf.right();
    let forward = Vec3::new(f.x, 0.0, f.z).normalize_or_zero();
    let right = Vec3::new(r.x, 0.0, r.z).normalize_or_zero();

    let mut wish = Vec3::ZERO;
    if !input_blocked && keys.pressed(forward_key) {
        wish += forward;
    }
    if !input_blocked && keys.pressed(back_key) {
        wish -= forward;
    }
    if !input_blocked && keys.pressed(left_key) {
        wish -= right;
    }
    if !input_blocked && keys.pressed(right_key) {
        wish += right;
    }
    if wish.length_squared() > 0.0 {
        wish = wish.normalize();
    }

    let dt = time.delta_secs();
    if !dt.is_finite() || dt <= 0.0 {
        kcc.translation = Some(Vec3::ZERO);
        return;
    }
    let now = time.elapsed_secs();
    let grounded = kcc_out.map(|o| o.grounded).unwrap_or(false);

    if !input_blocked && keys.just_pressed(jump_key) {
        if now - tap.last_press <= DOUBLE_TAP_WIN {
            if game_mode_state.0 == GameMode::Creative {
                flight.flying = !flight.flying;
                tap.last_press = -1_000_000.0;
                kin.vel_y = 0.0;
            }
        } else {
            tap.last_press = now;
            if !flight.flying && grounded {
                kin.vel_y = jump_v0;
            }
        }
    }

    kcc.snap_to_ground = if flight.flying {
        None
    } else {
        Some(CharacterLength::Absolute(0.2))
    };

    if flight.flying {
        let mut delta = Vec3::ZERO;
        let mut up_down = 0.0;
        if !input_blocked && keys.pressed(jump_key) {
            up_down += 1.0;
        }
        if !input_blocked && keys.pressed(down_key) {
            up_down -= 1.0;
        }

        delta += wish * ground_speed * fly_multi * dt;
        delta += Vec3::Y * up_down * ground_speed * fly_v_multi * dt;
        kin.vel_y = 0.0;
        kcc.translation = Some(sanitize_movement_delta(delta));
    } else {
        if grounded && kin.vel_y < 0.0 {
            kin.vel_y = 0.0;
        } else {
            let g_eff = if kin.vel_y < 0.0 {
                gravity * fall_multi
            } else {
                gravity
            };
            kin.vel_y -= g_eff * dt;
            if !kin.vel_y.is_finite() {
                kin.vel_y = 0.0;
            }
        }

        let mut delta = Vec3::ZERO;
        delta += wish * ground_speed * dt;
        delta += Vec3::Y * kin.vel_y * dt;
        kcc.translation = Some(sanitize_movement_delta(delta));
    }
}

#[inline]
fn sanitize_movement_delta(delta: Vec3) -> Vec3 {
    if delta.is_finite() {
        delta
    } else {
        Vec3::ZERO
    }
}

/// Applies noclip to player for the `logic::entities::player::init_service` module.
fn apply_noclip_to_player(
    mut commands: Commands,
    game_mode: Res<GameModeState>,
    mut q: Query<
        (
            Entity,
            Option<&Sensor>,
            Option<&mut CollisionGroups>,
            &mut KinematicCharacterController,
            &mut FlightState,
        ),
        With<Player>,
    >,
) {
    let Ok((e, has_sensor, groups_opt, mut kcc, mut flight)) = q.single_mut() else {
        return;
    };

    let spectator = matches!(game_mode.0, GameMode::Spectator);
    if spectator {
        flight.flying = true;
    }
    let noclip = spectator || flight.flying;

    if noclip {
        if has_sensor.is_none() {
            commands.entity(e).insert(Sensor);
        }
        match groups_opt {
            Some(mut g) => *g = CollisionGroups::new(Group::NONE, Group::NONE),
            None => {
                commands
                    .entity(e)
                    .insert(CollisionGroups::new(Group::NONE, Group::NONE));
            }
        }

        kcc.filter_groups = Some(CollisionGroups::new(Group::NONE, Group::NONE));
        kcc.snap_to_ground = None;
    } else {
        if has_sensor.is_some() {
            commands.entity(e).remove::<Sensor>();
        }
        commands.entity(e).insert(CollisionGroups::default());
        kcc.filter_groups = None;
    }
}
