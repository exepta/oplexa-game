use crate::core::entities::player::block_selection::SelectionState;
use crate::core::entities::player::inventory::PlayerInventory;
use crate::core::entities::player::{GameMode, GameModeState, Player};
use crate::core::events::block::block_player_events::{
    BlockBreakByPlayerEvent, BlockPlaceByPlayerEvent,
};
use crate::core::events::chunk_events::SubChunkNeedRemeshEvent;
use crate::core::multiplayer::MultiplayerConnectionState;
use crate::core::states::states::{AppState, InGameStates};
use crate::core::world::block::*;
use crate::core::world::chunk::*;
use crate::core::world::chunk_dimension::*;
use crate::core::world::fluid::FluidMap;
use crate::core::world::{mark_dirty_block_and_neighbors, world_access_mut};
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::ecs::relationship::RelatedSpawnerCommands;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;

#[derive(Component)]
struct MiningOverlay;

#[derive(Component)]
struct MiningOverlayFace;

#[derive(Component)]
struct DroppedBlockItem {
    block_id: BlockId,
    resting: bool,
}

#[derive(Component, Clone, Copy, Debug)]
struct DroppedBlockVelocity(Vec3);

#[derive(Component, Clone, Copy, Debug)]
struct DroppedBlockAngularVelocity(Vec3);

const DROP_ITEM_SIZE: f32 = 0.32;
const DROP_PICKUP_RADIUS: f32 = 1.35;
const DROP_ATTRACT_RADIUS: f32 = 3.5;
const DROP_ATTRACT_ACCEL: f32 = 34.0;
const DROP_ATTRACT_MAX_SPEED: f32 = 12.0;
const DROP_GRAVITY: f32 = 12.0;
const DROP_POP_MIN_DIST: f32 = 0.1;
const DROP_POP_MAX_DIST: f32 = 1.0;

#[derive(Clone, Copy)]
enum Axis {
    XY,
    XZ,
    YZ,
}

pub struct BlockEventHandler;

impl Plugin for BlockEventHandler {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                (block_break_handler, sync_mining_overlay).chain(),
                block_place_handler,
                simulate_dropped_block_items,
                pick_up_dropped_block_items,
            )
                .run_if(in_state(AppState::InGame(InGameStates::Game))),
        );
    }
}

fn block_break_handler(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    time: Res<Time>,
    buttons: Res<ButtonInput<MouseButton>>,
    selection: Res<SelectionState>,
    registry: Res<BlockRegistry>,
    game_mode: Res<GameModeState>,

    mut state: ResMut<MiningState>,
    mut chunk_map: ResMut<ChunkMap>,
    mut ev_dirty: MessageWriter<SubChunkNeedRemeshEvent>,
    mut break_ev: MessageWriter<BlockBreakByPlayerEvent>,
    multiplayer_connection: Option<Res<MultiplayerConnectionState>>,
) {
    let multiplayer_connected = multiplayer_connection
        .as_ref()
        .is_some_and(|state| state.connected);

    if game_mode.0.eq(&GameMode::Spectator) {
        return;
    }
    // --- Creative: instant break on click ---
    if matches!(game_mode.0, GameMode::Creative) {
        if !buttons.just_pressed(MouseButton::Left) {
            state.target = None;
            return;
        }

        let Some(hit) = selection.hit else {
            state.target = None;
            return;
        };

        let id_now = get_block_world(&chunk_map, hit.block_pos);
        if id_now == 0 {
            state.target = None;
            return;
        }

        // remove the block immediately
        if let Some(mut access) = world_access_mut(&mut chunk_map, hit.block_pos) {
            access.set(0);
        }
        mark_dirty_block_and_neighbors(&mut chunk_map, hit.block_pos, &mut ev_dirty);

        let (chunk_coord, l) = world_to_chunk_xz(hit.block_pos.x, hit.block_pos.z);
        let lx = l.x as u8;
        let lz = l.y as u8;
        let ly = (hit.block_pos.y - Y_MIN).clamp(0, CY as i32 - 1) as usize;

        break_ev.write(BlockBreakByPlayerEvent {
            chunk_coord,
            location: hit.block_pos,
            chunk_x: lx,
            chunk_y: ly as u16,
            chunk_z: lz,
            block_id: id_now,
            block_name: registry.name_opt(id_now).unwrap_or("").to_string(),
            drops_item: !registry.is_fluid(id_now),
        });

        state.target = None;
        return; // done for creative
    }

    // --- Survival: timed mining as before ---
    if !buttons.pressed(MouseButton::Left) {
        state.target = None;
        return;
    }

    let Some(hit) = selection.hit else {
        state.target = None;
        return;
    };

    let id_now = get_block_world(&chunk_map, hit.block_pos);
    if id_now == 0 {
        state.target = None;
        return;
    }

    let now = time.elapsed_secs();

    let restart = match state.target {
        None => true,
        Some(target) => target.loc != hit.block_pos || target.id != id_now,
    };

    if restart {
        state.target = Some(MiningTarget {
            loc: hit.block_pos,
            id: id_now,
            started_at: now,
            duration: break_time_for(id_now, &registry),
        });
    }

    let target = state.target.as_ref().unwrap();
    let progress = (now - target.started_at) / target.duration;

    if progress < 1.0 {
        return;
    }

    let world_loc = target.loc;
    if let Some(mut access) = world_access_mut(&mut chunk_map, world_loc) {
        access.set(0);
    }
    mark_dirty_block_and_neighbors(&mut chunk_map, world_loc, &mut ev_dirty);

    let (chunk_coord, l) = world_to_chunk_xz(world_loc.x, world_loc.z);
    let lx = l.x as u8;
    let lz = l.y as u8;
    let ly = (world_loc.y - Y_MIN).clamp(0, CY as i32 - 1) as usize;

    break_ev.write(BlockBreakByPlayerEvent {
        chunk_coord,
        location: world_loc,
        chunk_x: lx,
        chunk_y: ly as u16,
        chunk_z: lz,
        block_id: target.id,
        block_name: registry.name_opt(target.id).unwrap_or("").to_string(),
        drops_item: !registry.is_fluid(target.id),
    });

    if !multiplayer_connected && !registry.is_fluid(target.id) {
        spawn_dropped_block_item(
            &mut commands,
            &mut meshes,
            &registry,
            target.id,
            world_loc,
            now,
        );
    }

    state.target = None;
}

fn block_place_handler(
    buttons: Res<ButtonInput<MouseButton>>,
    sel: Res<SelectionState>,
    selected: Res<SelectedBlock>,
    registry: Res<BlockRegistry>,
    game_mode: Res<GameModeState>,

    mut fluids: ResMut<FluidMap>,
    mut chunk_map: ResMut<ChunkMap>,
    mut ev_dirty: MessageWriter<SubChunkNeedRemeshEvent>,
    mut place_ev: MessageWriter<BlockPlaceByPlayerEvent>,
) {
    if game_mode.0.eq(&GameMode::Spectator) {
        return;
    }
    if !buttons.just_pressed(MouseButton::Right) {
        return;
    }
    let id = selected.id;
    if id == 0 {
        return;
    }
    let Some(hit) = sel.hit else {
        return;
    };

    let world_pos = hit.place_pos;
    let (chunk_coord, l) = world_to_chunk_xz(world_pos.x, world_pos.z);
    let lx = l.x.clamp(0, (CX as i32 - 1) as u32) as usize;
    let lz = l.y.clamp(0, (CZ as i32 - 1) as u32) as usize;
    let ly = (world_pos.y - Y_MIN).clamp(0, CY as i32 - 1) as usize;

    let can_place = chunk_map
        .chunks
        .get(&chunk_coord)
        .map(|ch| ch.get(lx, ly, lz) == 0)
        .unwrap_or(false);
    if !can_place {
        return;
    }

    if let Some(fc) = fluids.0.get_mut(&chunk_coord) {
        fc.set(lx, ly, lz, false);
    }

    if let Some(mut access) = world_access_mut(&mut chunk_map, world_pos) {
        access.set(id);
    }

    mark_dirty_block_and_neighbors(&mut chunk_map, world_pos, &mut ev_dirty);

    let name = registry.name_opt(id).unwrap_or("").to_string();
    place_ev.write(BlockPlaceByPlayerEvent {
        location: world_pos,
        block_id: id,
        block_name: name,
    });
}

fn simulate_dropped_block_items(
    time: Res<Time>,
    chunk_map: Res<ChunkMap>,
    player: Query<&Transform, (With<Player>, Without<DroppedBlockItem>)>,
    mut items: Query<
        (
            &mut DroppedBlockItem,
            &mut DroppedBlockVelocity,
            &mut DroppedBlockAngularVelocity,
            &mut Transform,
        ),
        With<DroppedBlockItem>,
    >,
) {
    let delta = time.delta_secs();
    let player_pos = player.single().ok().map(|t| t.translation);

    for (mut item, mut velocity, mut angular_velocity, mut transform) in &mut items {
        velocity.0.y -= DROP_GRAVITY * delta;
        angular_velocity.0 += Vec3::new(velocity.0.z, 0.0, -velocity.0.x) * (1.25 * delta);
        let max_spin = 32.0;
        let spin_len = angular_velocity.0.length();
        if spin_len > max_spin {
            angular_velocity.0 = angular_velocity.0 / spin_len * max_spin;
        }

        if angular_velocity.0.length_squared() > 0.000_001 {
            let spin = Quat::from_scaled_axis(angular_velocity.0 * delta);
            transform.rotation = (spin * transform.rotation).normalize();
        }

        let half = DROP_ITEM_SIZE * 0.5;
        let support_probe = transform.translation - Vec3::Y * (half + 0.06);
        let support_x = support_probe.x.floor() as i32;
        let support_y = support_probe.y.floor() as i32;
        let support_z = support_probe.z.floor() as i32;
        let has_support =
            get_block_world(&chunk_map, IVec3::new(support_x, support_y, support_z)) != 0;

        if let Some(player_pos) = player_pos {
            let to_player = player_pos - transform.translation;
            let dist_sq = to_player.length_squared();
            if dist_sq <= DROP_ATTRACT_RADIUS * DROP_ATTRACT_RADIUS && dist_sq > 0.000_001 {
                let dist = dist_sq.sqrt();
                let dir = to_player / dist;
                let t = 1.0 - (dist / DROP_ATTRACT_RADIUS).clamp(0.0, 1.0);
                let accel = DROP_ATTRACT_ACCEL * (0.35 + t * 1.65);
                velocity.0 += dir * (accel * delta);
                let speed = velocity.0.length();
                if speed > DROP_ATTRACT_MAX_SPEED {
                    velocity.0 = velocity.0 / speed * DROP_ATTRACT_MAX_SPEED;
                }
                item.resting = false;
            }
        }

        if item.resting {
            if has_support {
                velocity.0 = Vec3::ZERO;
                let drag = (1.0 - 5.0 * delta).clamp(0.0, 1.0);
                angular_velocity.0 *= drag;
                if angular_velocity.0.length_squared() < 0.0001 {
                    angular_velocity.0 = Vec3::ZERO;
                }
                continue;
            }

            item.resting = false;
            velocity.0 = Vec3::new(0.0, velocity.0.y.min(-0.1), 0.0);
        }

        transform.translation += velocity.0 * delta;

        let foot = transform.translation - Vec3::Y * (half + 0.03);
        let wx = foot.x.floor() as i32;
        let wy = foot.y.floor() as i32;
        let wz = foot.z.floor() as i32;

        let below_is_solid = get_block_world(&chunk_map, IVec3::new(wx, wy, wz)) != 0;
        if !below_is_solid || velocity.0.y > 0.0 {
            continue;
        }

        let ground_top = wy as f32 + 1.0;
        if transform.translation.y - half > ground_top {
            continue;
        }

        transform.translation.y = ground_top + half;
        velocity.0 = Vec3::ZERO;
        angular_velocity.0 *= 0.55;
        item.resting = true;
    }
}

fn pick_up_dropped_block_items(
    mut commands: Commands,
    game_mode: Res<GameModeState>,
    multiplayer_connection: Option<Res<MultiplayerConnectionState>>,
    mut inventory: ResMut<PlayerInventory>,
    player: Query<&Transform, With<Player>>,
    drops: Query<(Entity, &DroppedBlockItem, &Transform), With<DroppedBlockItem>>,
) {
    if multiplayer_connection
        .as_ref()
        .is_some_and(|state| state.connected)
    {
        return;
    }

    if game_mode.0 == GameMode::Spectator {
        return;
    }

    let Ok(player_transform) = player.single() else {
        return;
    };

    let radius_sq = DROP_PICKUP_RADIUS * DROP_PICKUP_RADIUS;
    let player_pos = player_transform.translation;

    for (entity, item, transform) in &drops {
        if player_pos.distance_squared(transform.translation) > radius_sq {
            continue;
        }

        if inventory.add_block(item.block_id, 1) == 0 {
            commands.entity(entity).despawn();
        }
    }
}

fn spawn_dropped_block_item(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    registry: &BlockRegistry,
    block_id: BlockId,
    world_loc: IVec3,
    now: f32,
) {
    let mut mesh = build_block_cube_mesh(registry, block_id, DROP_ITEM_SIZE);
    center_mesh_vertices(&mut mesh, DROP_ITEM_SIZE * 0.5);

    let center = Vec3::new(
        (world_loc.x as f32 + 0.5) * VOXEL_SIZE,
        (world_loc.y as f32 + 0.5) * VOXEL_SIZE + 0.28,
        (world_loc.z as f32 + 0.5) * VOXEL_SIZE,
    );

    commands.spawn((
        DroppedBlockItem {
            block_id,
            resting: false,
        },
        DroppedBlockVelocity(compute_drop_pop_velocity(world_loc, now)),
        DroppedBlockAngularVelocity(compute_drop_angular_velocity(world_loc, now)),
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(registry.material(block_id)),
        Transform {
            translation: center,
            rotation: compute_drop_initial_rotation(world_loc, now),
            scale: Vec3::ONE,
        },
        Visibility::default(),
        Name::new(format!("Drop::{}", registry.name(block_id))),
        NotShadowCaster,
        NotShadowReceiver,
    ));
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

fn sync_mining_overlay(
    mut commands: Commands,
    mut root: ResMut<MiningOverlayRoot>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    time: Res<Time>,
    state: Res<MiningState>,
    mut q_faces: Query<
        (&mut Transform, &MiningOverlayFace),
        (With<MiningOverlayFace>, Without<MiningOverlay>),
    >,
    mut q_parent_tf: Query<&mut Transform, (With<MiningOverlay>, Without<MiningOverlayFace>)>,
) {
    let Some(target) = state.target else {
        if let Some(e) = root.0.take() {
            commands.entity(e).despawn();
        }
        return;
    };

    let now = time.elapsed_secs();
    let progress = ((now - target.started_at) / target.duration).clamp(0.0, 1.0);

    let s = VOXEL_SIZE;
    let center = Vec3::new(
        (target.loc.x as f32 + 0.5) * s,
        (target.loc.y as f32 + 0.5) * s,
        (target.loc.z as f32 + 0.5) * s,
    );

    let parent_e = if let Some(e) = root.0 {
        e
    } else {
        let e = spawn_overlay_at(
            &mut commands,
            &mut meshes,
            &mut mats,
            center,
            Some(RenderLayers::layer(2)),
            progress,
        );
        root.0 = Some(e);
        e
    };

    if let Ok(mut tf) = q_parent_tf.get_mut(parent_e) {
        tf.translation = center;
    }

    let max_scale = 0.98 * s;
    let size = max_scale * progress;
    let face_scale = Vec3::new(size, size, 1.0);

    for (mut tf, _) in q_faces.iter_mut() {
        tf.scale = face_scale;
    }

    if progress >= 1.0 {
        if let Some(e) = root.0.take() {
            commands.entity(e).despawn();
        }
    }
}

#[inline]
fn spawn_overlay_at(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    world_center: Vec3,
    layer: Option<RenderLayers>,
    initial_progress: f32,
) -> Entity {
    let quad = meshes.add(unit_centered_quad());
    let mat = mats.add(StandardMaterial {
        base_color: Color::srgba(0.9, 0.9, 0.9, 0.02),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        perceptual_roughness: 1.0,
        ..default()
    });

    let s = VOXEL_SIZE;
    let half = 0.5 * s;
    let eps = 0.003 * s;

    let max_scale = 0.98 * s;
    let init_scale = (initial_progress.clamp(0.0, 1.0).max(0.001)) * max_scale;
    let init_vec = Vec3::new(init_scale, init_scale, 1.0);

    let mut parent = commands.spawn((
        MiningOverlay,
        Visibility::default(),
        NoFrustumCulling,
        Transform::from_translation(world_center),
        GlobalTransform::default(),
        NotShadowCaster,
        NotShadowReceiver,
        Name::new("MiningOverlay"),
    ));
    if let Some(l) = layer.as_ref() {
        parent.insert(l.clone());
    }
    let parent_id = parent.id();

    let spawn_face = |c: &mut RelatedSpawnerCommands<ChildOf>, _: Axis, tf: Transform| {
        let mut e = c.spawn((
            MiningOverlayFace,
            Visibility::default(),
            Mesh3d(quad.clone()),
            MeshMaterial3d(mat.clone()),
            tf.with_scale(init_vec),
            GlobalTransform::default(),
            NotShadowCaster,
            NotShadowReceiver,
            Name::new("MiningOverlayFace"),
        ));
        if let Some(l) = layer.as_ref() {
            e.insert(l.clone());
        }
    };

    commands.entity(parent_id).with_children(|c| {
        // +Z / -Z (XY)
        spawn_face(
            c,
            Axis::XY,
            Transform::from_translation(Vec3::new(0.0, 0.0, half + eps)),
        );
        spawn_face(
            c,
            Axis::XY,
            Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::PI))
                .with_translation(Vec3::new(0.0, 0.0, -half - eps)),
        );

        // +Y / -Y (XZ)
        spawn_face(
            c,
            Axis::XZ,
            Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
                .with_translation(Vec3::new(0.0, half + eps, 0.0)),
        );
        spawn_face(
            c,
            Axis::XZ,
            Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
                .with_translation(Vec3::new(0.0, -half - eps, 0.0)),
        );

        // +X / -X (YZ)
        spawn_face(
            c,
            Axis::YZ,
            Transform::from_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2))
                .with_translation(Vec3::new(half + eps, 0.0, 0.0)),
        );
        spawn_face(
            c,
            Axis::YZ,
            Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2))
                .with_translation(Vec3::new(-half - eps, 0.0, 0.0)),
        );
    });

    parent_id
}

#[inline]
fn unit_centered_quad() -> Mesh {
    use bevy::mesh::{Indices, PrimitiveTopology};
    use bevy::prelude::Mesh;
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, Default::default());
    m.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [-0.5, -0.5, 0.0],
            [0.5, -0.5, 0.0],
            [0.5, 0.5, 0.0],
            [-0.5, 0.5, 0.0],
        ],
    );
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 4]);
    m.insert_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
    );
    m.insert_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]));
    m
}
