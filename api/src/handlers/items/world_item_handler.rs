use crate::core::entities::player::inventory::PlayerInventory;
use crate::core::entities::player::{GameMode, GameModeState, Player};
use crate::core::inventory::items::{
    ItemRegistry, WORLD_ITEM_ATTRACT_ACCEL, WORLD_ITEM_ATTRACT_MAX_SPEED,
    WORLD_ITEM_ATTRACT_RADIUS, WORLD_ITEM_DROP_GRAVITY, WORLD_ITEM_PICKUP_RADIUS, WORLD_ITEM_SIZE,
    WorldItemAngularVelocity, WorldItemEntity, WorldItemVelocity,
};
use crate::core::multiplayer::MultiplayerConnectionState;
use crate::core::states::states::{AppState, InGameStates};
use crate::core::world::block::get_block_world;
use crate::core::world::chunk::ChunkMap;
use crate::generator::chunk::chunk_utils::safe_despawn_entity;
use bevy::prelude::*;

/// Runtime systems for dropped world-item simulation and pickup behavior.
pub struct WorldItemHandlerPlugin;

impl Plugin for WorldItemHandlerPlugin {
    /// Builds this component for the `handlers::items::world_item_handler` module.
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (simulate_world_items, pick_up_world_items)
                .run_if(in_state(AppState::InGame(InGameStates::Game))),
        );
    }
}

/// Runs the `simulate_world_items` routine for simulate world items in the `handlers::items::world_item_handler` module.
fn simulate_world_items(
    time: Res<Time>,
    chunk_map: Res<ChunkMap>,
    player: Query<&Transform, (With<Player>, Without<WorldItemEntity>)>,
    mut items: Query<
        (
            &mut WorldItemEntity,
            &mut WorldItemVelocity,
            &mut WorldItemAngularVelocity,
            &mut Transform,
        ),
        With<WorldItemEntity>,
    >,
) {
    let delta = time.delta_secs();
    let now = time.elapsed_secs();
    let player_pos = player.single().ok().map(|t| t.translation);

    for (mut item, mut velocity, mut angular_velocity, mut transform) in &mut items {
        velocity.0.y -= WORLD_ITEM_DROP_GRAVITY * delta;
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

        let half = WORLD_ITEM_SIZE * 0.5;
        let support_probe = transform.translation - Vec3::Y * (half + 0.06);
        let support_x = support_probe.x.floor() as i32;
        let support_y = support_probe.y.floor() as i32;
        let support_z = support_probe.z.floor() as i32;
        let has_support =
            get_block_world(&chunk_map, IVec3::new(support_x, support_y, support_z)) != 0;

        if now >= item.pickup_ready_at
            && let Some(player_pos) = player_pos
        {
            let to_player = player_pos - transform.translation;
            let dist_sq = to_player.length_squared();
            if dist_sq <= WORLD_ITEM_ATTRACT_RADIUS * WORLD_ITEM_ATTRACT_RADIUS
                && dist_sq > 0.000_001
            {
                let dist = dist_sq.sqrt();
                let dir = to_player / dist;
                let t = 1.0 - (dist / WORLD_ITEM_ATTRACT_RADIUS).clamp(0.0, 1.0);
                let accel = WORLD_ITEM_ATTRACT_ACCEL * (0.35 + t * 1.65);
                velocity.0 += dir * (accel * delta);
                let speed = velocity.0.length();
                if speed > WORLD_ITEM_ATTRACT_MAX_SPEED {
                    velocity.0 = velocity.0 / speed * WORLD_ITEM_ATTRACT_MAX_SPEED;
                }
                item.resting = false;
            }
        }

        if item.resting {
            if has_support {
                velocity.0 = Vec3::ZERO;
                if item.block_visual {
                    let drag = (1.0 - 5.0 * delta).clamp(0.0, 1.0);
                    angular_velocity.0 *= drag;
                    if angular_velocity.0.length_squared() < 0.0001 {
                        angular_velocity.0 = Vec3::ZERO;
                    }
                } else {
                    angular_velocity.0 = Vec3::ZERO;
                    transform.rotation = flat_item_rotation(transform.rotation);
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
        if item.block_visual {
            angular_velocity.0 *= 0.55;
        } else {
            angular_velocity.0 = Vec3::ZERO;
            transform.rotation = flat_item_rotation(transform.rotation);
        }
        item.resting = true;
    }
}

#[inline]
fn flat_item_rotation(current_rotation: Quat) -> Quat {
    let forward = current_rotation * Vec3::Z;
    let yaw = forward.x.atan2(forward.z);
    Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)
}

/// Picks up world items for the `handlers::items::world_item_handler` module.
fn pick_up_world_items(
    mut commands: Commands,
    time: Res<Time>,
    game_mode: Res<GameModeState>,
    multiplayer_connection: Option<Res<MultiplayerConnectionState>>,
    item_registry: Res<ItemRegistry>,
    mut inventory: ResMut<PlayerInventory>,
    player: Query<&Transform, With<Player>>,
    mut drops: Query<(Entity, &mut WorldItemEntity, &Transform), With<WorldItemEntity>>,
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

    let radius_sq = WORLD_ITEM_PICKUP_RADIUS * WORLD_ITEM_PICKUP_RADIUS;
    let player_pos = player_transform.translation;
    let now = time.elapsed_secs();

    for (entity, mut item, transform) in &mut drops {
        if now < item.pickup_ready_at {
            continue;
        }
        if !item_registry.is_pickupable(item.item_id) {
            continue;
        }
        if player_pos.distance_squared(transform.translation) > radius_sq {
            continue;
        }

        let leftover = inventory.add_item(item.item_id, item.amount, &item_registry);
        if leftover == 0 {
            safe_despawn_entity(&mut commands, entity);
        } else {
            item.amount = leftover;
        }
    }
}
