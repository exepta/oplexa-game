fn send_chunk_interest_updates(
    time: Res<Time>,
    game_config: Res<GlobalConfig>,
    q_player: Query<&Transform, With<Player>>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
    q_connected: Query<Has<Connected>>,
    mut chunk_stream: ResMut<RemoteChunkStreamState>,
    mut chunk_decode_queue: ResMut<RemoteChunkDecodeQueue>,
    mut chunk_decode_tasks: ResMut<RemoteChunkDecodeTasks>,
    mut chunk_decoded_queue: ResMut<RemoteChunkDecodedQueue>,
    mut chunk_remesh_queue: ResMut<RemoteChunkRemeshQueue>,
    runtime: Res<MultiplayerClientRuntime>,
    mut q_sender: Query<&mut MessageSender<ClientChunkInterest>>,
) {
    let Some(entity) = runtime.connection_entity else {
        chunk_stream.reset();
        chunk_decode_queue.reset();
        chunk_decode_tasks.reset();
        chunk_decoded_queue.reset();
        chunk_remesh_queue.reset();
        return;
    };

    if !q_connected.get(entity).unwrap_or(false)
        || multiplayer_connection.active_session_url.is_none()
    {
        chunk_stream.reset();
        chunk_decode_queue.reset();
        chunk_decode_tasks.reset();
        chunk_decoded_queue.reset();
        chunk_remesh_queue.reset();
        return;
    }

    let target_radius = game_config.graphics.chunk_range.max(1);
    let center = if let Ok(transform) = q_player.single() {
        world_to_chunk_xz(
            (transform.translation.x / VOXEL_SIZE).floor() as i32,
            (transform.translation.z / VOXEL_SIZE).floor() as i32,
        )
        .0
    } else if let Some(spawn_translation) = multiplayer_connection.spawn_translation {
        world_to_chunk_xz(
            spawn_translation[0].floor() as i32,
            spawn_translation[2].floor() as i32,
        )
        .0
    } else {
        return;
    };
    let now = time.elapsed_secs();

    let should_reset_progressive = match chunk_stream.last_requested_center {
        None => true,
        Some(last_center) => {
            let dx = (center.x - last_center.x).abs();
            let dz = (center.y - last_center.y).abs();
            dx.max(dz) > 2
        }
    };

    if should_reset_progressive {
        let bootstrap_radius = target_radius
            .min(MULTIPLAYER_CHUNK_INTEREST_BOOTSTRAP_RADIUS)
            .max(1);
        chunk_stream.progressive_radius = Some(bootstrap_radius);
        chunk_stream.next_radius_step_at = now + MULTIPLAYER_CHUNK_INTEREST_STEP_INTERVAL_SECS;
    } else if chunk_stream.progressive_radius.is_none() {
        let carry_radius = chunk_stream.last_requested_radius.unwrap_or(target_radius);
        chunk_stream.progressive_radius = Some(carry_radius.clamp(1, target_radius));
    }

    let mut radius = chunk_stream
        .progressive_radius
        .unwrap_or(target_radius)
        .clamp(1, target_radius);

    if radius < target_radius && now >= chunk_stream.next_radius_step_at {
        radius += 1;
        chunk_stream.progressive_radius = Some(radius);
        chunk_stream.next_radius_step_at = now + MULTIPLAYER_CHUNK_INTEREST_STEP_INTERVAL_SECS;
    } else {
        chunk_stream.progressive_radius = Some(radius);
    }

    if chunk_stream.last_requested_center == Some(center)
        && chunk_stream.last_requested_radius == Some(radius)
    {
        return;
    }

    if let Ok(mut sender) = q_sender.get_mut(entity) {
        sender.send::<UnorderedReliable>(ClientChunkInterest::new([center.x, center.y], radius));
        chunk_stream.last_requested_center = Some(center);
        chunk_stream.last_requested_radius = Some(radius);
    }
}

/// Runs the `smooth_remote_players` routine for smooth remote players in the `client` module.
fn smooth_remote_players(
    time: Res<Time>,
    mut remote_players: Query<(&RemotePlayerAvatar, &mut Transform)>,
    mut runtime: ResMut<MultiplayerClientRuntime>,
) {
    let now = time.elapsed_secs();
    let render_at = (now - REMOTE_PLAYER_INTERP_BACK_TIME_SECS).max(0.0);
    let alpha = (1.0 - (-REMOTE_PLAYER_SMOOTHING_HZ * time.delta_secs()).exp()).clamp(0.0, 1.0);

    for (avatar, mut transform) in &mut remote_players {
        let Some(smoothing) = runtime.remote_player_smoothing.get_mut(&avatar.player_id) else {
            continue;
        };
        let Some(front) = smoothing.snapshots.front().copied() else {
            continue;
        };

        while smoothing.snapshots.len() >= 2 {
            let next = smoothing.snapshots.get(1).copied();
            if match next {
                Some(snapshot) => snapshot.at_secs > render_at,
                None => true,
            } {
                break;
            }
            smoothing.snapshots.pop_front();
        }

        let (target_translation, target_yaw) = if let Some(next) = smoothing.snapshots.get(1) {
            let from = smoothing.snapshots[0];
            let to = *next;
            let span = (to.at_secs - from.at_secs).max(0.0001);
            let t = ((render_at - from.at_secs) / span).clamp(0.0, 1.0);
            (
                from.translation.lerp(to.translation, t),
                lerp_angle_radians(from.yaw, to.yaw, t),
            )
        } else {
            let latest = smoothing.snapshots.back().copied().unwrap_or(front);
            let extrapolated =
                if let Some(previous) = smoothing.snapshots.iter().rev().nth(1).copied() {
                    let dt = (latest.at_secs - previous.at_secs).max(0.0001);
                    let velocity = (latest.translation - previous.translation) / dt;
                    let ahead = (render_at - latest.at_secs)
                        .clamp(0.0, REMOTE_PLAYER_MAX_EXTRAPOLATION_SECS);
                    latest.translation + velocity * ahead
                } else {
                    latest.translation
                };
            (extrapolated, latest.yaw)
        };

        transform.translation = transform.translation.lerp(target_translation, alpha);
        let current_yaw = transform.rotation.to_euler(EulerRot::YXZ).0;
        let smoothed_yaw = lerp_angle_radians(current_yaw, target_yaw, alpha);
        transform.rotation = Quat::from_rotation_y(smoothed_yaw);
    }
}

/// Runs the `send_local_block_break_events` routine for send local block break events in the `client` module.
fn send_local_block_break_events(
    time: Res<Time>,
    mut break_events: MessageReader<BlockBreakByPlayerEvent>,
    item_registry: Option<Res<ItemRegistry>>,
    block_remap: Res<BlockIdRemap>,
    mut pending_world_acks: ResMut<PendingWorldAckState>,
    mut local_world_edits: ResMut<LocalWorldEditOverlayState>,
    runtime: Res<MultiplayerClientRuntime>,
    q_connected: Query<Has<Connected>>,
    mut q_sender: Query<&mut MessageSender<ClientBlockBreak>>,
) {
    let Some(entity) = runtime.connection_entity else {
        for _ in break_events.read() {}
        return;
    };

    if !q_connected.get(entity).unwrap_or(false) {
        for _ in break_events.read() {}
        return;
    }

    let Ok(mut sender) = q_sender.get_mut(entity) else {
        for _ in break_events.read() {}
        return;
    };

    for event in break_events.read() {
        let drop_block_id = if event.drops_item {
            item_registry
                .as_ref()
                .and_then(|items| items.block_for_item(event.drop_item_id))
                .map(|local_block_id| block_remap.to_server(local_block_id))
                .unwrap_or(0)
        } else {
            0
        };
        pending_world_acks.entries.insert(PendingWorldAck {
            kind: PendingWorldAckKind::Break,
            location: event.location.to_array(),
        });
        upsert_local_world_edit_overlay(
            &mut local_world_edits,
            LocalWorldEditOverlay {
                location: event.location.to_array(),
                kind: LocalWorldEditKind::Break,
                expires_at_secs: time.elapsed_secs() + LOCAL_WORLD_EDIT_OVERLAY_TTL_SECS,
            },
        );
        sender.send::<UnorderedReliable>(ClientBlockBreak::new(
            event.location.to_array(),
            drop_block_id,
            0,
        ));
    }
}

/// Runs the `send_local_block_place_events` routine for send local block place events in the `client` module.
fn send_local_block_place_events(
    time: Res<Time>,
    mut place_events: MessageReader<BlockPlaceByPlayerEvent>,
    block_remap: Res<BlockIdRemap>,
    mut pending_world_acks: ResMut<PendingWorldAckState>,
    mut local_world_edits: ResMut<LocalWorldEditOverlayState>,
    runtime: Res<MultiplayerClientRuntime>,
    q_connected: Query<Has<Connected>>,
    mut q_sender: Query<&mut MessageSender<ClientBlockPlace>>,
) {
    let Some(entity) = runtime.connection_entity else {
        for _ in place_events.read() {}
        return;
    };

    if !q_connected.get(entity).unwrap_or(false) {
        for _ in place_events.read() {}
        return;
    }

    let Ok(mut sender) = q_sender.get_mut(entity) else {
        for _ in place_events.read() {}
        return;
    };

    for event in place_events.read() {
        pending_world_acks.entries.insert(PendingWorldAck {
            kind: PendingWorldAckKind::Place,
            location: event.location.to_array(),
        });
        upsert_local_world_edit_overlay(
            &mut local_world_edits,
            LocalWorldEditOverlay {
                location: event.location.to_array(),
                kind: LocalWorldEditKind::Place {
                    block_id: event.block_id,
                    stacked_block_id: event.stacked_block_id,
                },
                expires_at_secs: time.elapsed_secs() + LOCAL_WORLD_EDIT_OVERLAY_TTL_SECS,
            },
        );
        sender.send::<UnorderedReliable>(ClientBlockPlace::new(
            event.location.to_array(),
            block_remap.to_server(event.block_id),
            block_remap.to_server(event.stacked_block_id),
        ));
    }
}

fn upsert_local_world_edit_overlay(
    overlays: &mut LocalWorldEditOverlayState,
    next: LocalWorldEditOverlay,
) {
    if let Some(existing) = overlays
        .entries
        .iter_mut()
        .find(|entry| entry.location == next.location)
    {
        *existing = next;
    } else {
        overlays.entries.push(next);
    }
}

/// Sends chest-open requests to the authoritative server instead of reading local save data.
fn send_chest_inventory_snapshot_requests(
    mut requests: MessageReader<ChestInventorySnapshotRequest>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
    runtime: Res<MultiplayerClientRuntime>,
    q_connected: Query<Has<Connected>>,
    mut q_sender: Query<&mut MessageSender<ClientChestInventoryOpen>>,
) {
    if multiplayer_connection.uses_local_save_data() {
        for _ in requests.read() {}
        return;
    }

    let Some(entity) = runtime.connection_entity else {
        for _ in requests.read() {}
        return;
    };
    if !q_connected.get(entity).unwrap_or(false) {
        for _ in requests.read() {}
        return;
    }

    let Ok(mut sender) = q_sender.get_mut(entity) else {
        for _ in requests.read() {}
        return;
    };

    for request in requests.read() {
        sender.send::<UnorderedReliable>(ClientChestInventoryOpen::new(request.world_pos));
    }
}

/// Sends chest-open requests to the authoritative server instead of reading local save data.
fn send_open_chest_inventory_requests(
    mut opened: MessageReader<ChestInventoryUiOpened>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
    runtime: Res<MultiplayerClientRuntime>,
    q_connected: Query<Has<Connected>>,
    mut q_sender: Query<&mut MessageSender<ClientChestInventoryOpen>>,
) {
    if multiplayer_connection.uses_local_save_data() {
        for _ in opened.read() {}
        return;
    }

    let Some(entity) = runtime.connection_entity else {
        for _ in opened.read() {}
        return;
    };
    if !q_connected.get(entity).unwrap_or(false) {
        for _ in opened.read() {}
        return;
    }

    let Ok(mut sender) = q_sender.get_mut(entity) else {
        for _ in opened.read() {}
        return;
    };

    for message in opened.read() {
        sender.send::<OrderedReliable>(ClientChestInventoryOpen::new(message.world_pos));
    }
}

/// Sends authoritative chest inventory persistence requests to the server.
fn send_persist_chest_inventory_requests(
    mut requests: MessageReader<ChestInventoryPersistRequest>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
    runtime: Res<MultiplayerClientRuntime>,
    q_connected: Query<Has<Connected>>,
    mut q_sender: Query<&mut MessageSender<ClientChestInventoryPersist>>,
) {
    if multiplayer_connection.uses_local_save_data() {
        for _ in requests.read() {}
        return;
    }

    let Some(entity) = runtime.connection_entity else {
        for _ in requests.read() {}
        return;
    };
    if !q_connected.get(entity).unwrap_or(false) {
        for _ in requests.read() {}
        return;
    }

    let Ok(mut sender) = q_sender.get_mut(entity) else {
        for _ in requests.read() {}
        return;
    };

    for request in requests.read() {
        sender.send::<OrderedReliable>(ClientChestInventoryPersist::new(
            request.world_pos,
            request.slots.clone(),
        ));
    }
}

/// Receives authoritative chest contents and forwards them into the existing UI sync path.
fn receive_chest_inventory_messages(
    multiplayer_connection: Res<MultiplayerConnectionState>,
    runtime: Res<MultiplayerClientRuntime>,
    mut q: Query<&mut MessageReceiver<ServerChestInventoryContents>>,
    mut sync: MessageWriter<ChestInventoryContentsSync>,
) {
    if multiplayer_connection.uses_local_save_data() {
        return;
    }

    let Some(entity) = runtime.connection_entity else {
        return;
    };
    let Ok(mut receiver) = q.get_mut(entity) else {
        return;
    };

    for message in receiver.receive() {
        sync.write(ChestInventoryContentsSync {
            world_pos: message.world_pos,
            slots: message.slots,
        });
    }
}

/// Runs the `send_local_item_drop_requests` routine for send local item drop requests in the `client` module.
fn send_local_item_drop_requests(
    mut drop_requests: MessageReader<DropItemRequest>,
    item_registry: Option<Res<ItemRegistry>>,
    block_remap: Res<BlockIdRemap>,
    runtime: Res<MultiplayerClientRuntime>,
    q_connected: Query<Has<Connected>>,
    mut q_sender: Query<&mut MessageSender<ClientDropItem>>,
) {
    let Some(entity) = runtime.connection_entity else {
        for _ in drop_requests.read() {}
        return;
    };

    if !q_connected.get(entity).unwrap_or(false) {
        for _ in drop_requests.read() {}
        return;
    }

    let Ok(mut sender) = q_sender.get_mut(entity) else {
        for _ in drop_requests.read() {}
        return;
    };

    for request in drop_requests.read() {
        if request.item_id == 0 || request.amount == 0 {
            continue;
        }
        let local_block_id = item_registry
            .as_ref()
            .and_then(|items| items.block_for_item(request.item_id))
            .unwrap_or(0);

        sender.send::<OrderedReliable>(ClientDropItem::new(
            request.location,
            request.item_id,
            block_remap.to_server(local_block_id),
            request.amount,
            request.spawn_translation,
            request.initial_velocity,
        ));
    }
}

/// Runs the `simulate_multiplayer_drop_items` routine for simulate multiplayer drop items in the `client` module.
fn simulate_multiplayer_drop_items(
    time: Res<Time>,
    chunk_map: Res<ChunkMap>,
    player: Query<&Transform, (With<Player>, Without<MultiplayerDroppedItem>)>,
    mut drops: Query<(&mut MultiplayerDroppedItem, &mut Transform), With<MultiplayerDroppedItem>>,
) {
    let delta = time.delta_secs();
    let now = time.elapsed_secs();
    let player_pos = player.single().ok().map(|t| t.translation);

    for (mut drop, mut transform) in &mut drops {
        drop.velocity.y -= MULTIPLAYER_DROP_GRAVITY * delta;
        let vx = drop.velocity.x;
        let vz = drop.velocity.z;
        drop.angular_velocity += Vec3::new(vz, 0.0, -vx) * (1.25 * delta);
        let max_spin = 36.0;
        let spin_len = drop.angular_velocity.length();
        if spin_len > max_spin {
            drop.angular_velocity = drop.angular_velocity / spin_len * max_spin;
        }
        let mut spin = Quat::IDENTITY;
        if drop.angular_velocity.length_squared() > 0.000_001 {
            spin = Quat::from_scaled_axis(drop.angular_velocity * delta) * spin;
        }
        if !drop.resting
            && drop.spin_axis.length_squared() > 0.000_001
            && drop.spin_speed.abs() > 0.001
        {
            spin = Quat::from_axis_angle(drop.spin_axis, drop.spin_speed * delta) * spin;
        }
        if spin != Quat::IDENTITY {
            transform.rotation = (spin * transform.rotation).normalize();
        }

        let half = MULTIPLAYER_DROP_ITEM_SIZE * 0.5;
        let support_probe = transform.translation - Vec3::Y * (half + 0.06);
        let support_x = support_probe.x.floor() as i32;
        let support_y = support_probe.y.floor() as i32;
        let support_z = support_probe.z.floor() as i32;
        let has_support =
            get_block_world(&chunk_map, IVec3::new(support_x, support_y, support_z)) != 0;

        if now >= drop.pickup_ready_at {
            if let Some(player_pos) = player_pos {
                let to_player = player_pos - transform.translation;
                let dist_sq = to_player.length_squared();
                if dist_sq <= MULTIPLAYER_DROP_ATTRACT_RADIUS * MULTIPLAYER_DROP_ATTRACT_RADIUS
                    && dist_sq > 0.000_001
                {
                    let dist = dist_sq.sqrt();
                    let dir = to_player / dist;
                    let t = 1.0 - (dist / MULTIPLAYER_DROP_ATTRACT_RADIUS).clamp(0.0, 1.0);
                    let accel = MULTIPLAYER_DROP_ATTRACT_ACCEL * (0.35 + t * 1.65);
                    drop.velocity += dir * (accel * delta);
                    let speed = drop.velocity.length();
                    if speed > MULTIPLAYER_DROP_ATTRACT_MAX_SPEED {
                        drop.velocity = drop.velocity / speed * MULTIPLAYER_DROP_ATTRACT_MAX_SPEED;
                    }
                    drop.resting = false;
                }
            }
        }

        if drop.resting {
            if has_support {
                drop.velocity = Vec3::ZERO;
                if drop.block_visual {
                    drop.angular_velocity = Vec3::ZERO;
                    drop.spin_speed = 0.0;
                } else {
                    drop.angular_velocity = Vec3::ZERO;
                    drop.spin_speed = 0.0;
                    transform.rotation = resting_flat_item_rotation(transform.rotation);
                }
                continue;
            }

            drop.resting = false;
            drop.velocity = Vec3::new(0.0, drop.velocity.y.min(-0.1), 0.0);
        }

        transform.translation += drop.velocity * delta;

        let foot = transform.translation - Vec3::Y * (half + 0.03);
        let wx = foot.x.floor() as i32;
        let wy = foot.y.floor() as i32;
        let wz = foot.z.floor() as i32;

        let below_is_solid = get_block_world(&chunk_map, IVec3::new(wx, wy, wz)) != 0;
        if !below_is_solid || drop.velocity.y > 0.0 {
            continue;
        }

        let ground_top = wy as f32 + 1.0;
        if transform.translation.y - half > ground_top {
            continue;
        }

        transform.translation.y = ground_top + half;
        let impact_velocity = drop.velocity;
        drop.velocity = Vec3::ZERO;
        drop.resting = true;
        if drop.block_visual {
            drop.angular_velocity = Vec3::ZERO;
            drop.spin_speed = 0.0;
            transform.rotation = resting_block_drop_rotation(
                transform.rotation,
                transform.translation,
                impact_velocity,
            );
        } else {
            drop.angular_velocity = Vec3::ZERO;
            drop.spin_speed = 0.0;
            transform.rotation = resting_flat_item_rotation(transform.rotation);
        }
    }
}

/// Runs the `send_local_inventory_sync` routine for send local inventory sync in the `client` module.
fn send_local_inventory_sync(
    multiplayer_connection: Res<MultiplayerConnectionState>,
    inventory: Res<PlayerInventory>,
    runtime: Res<MultiplayerClientRuntime>,
    q_connected: Query<Has<Connected>>,
    mut q_sender: Query<&mut MessageSender<ClientInventorySync>>,
) {
    if !multiplayer_connection.connected || !inventory.is_changed() {
        return;
    }

    let Some(entity) = runtime.connection_entity else {
        return;
    };

    if !q_connected.get(entity).unwrap_or(false) {
        return;
    }

    let Ok(mut sender) = q_sender.get_mut(entity) else {
        return;
    };

    sender.send::<UnorderedReliable>(ClientInventorySync::from_slots(&inventory.slots));
}

/// Runs the `send_local_drop_pickup_requests` routine for send local drop pickup requests in the `client` module.
fn send_local_drop_pickup_requests(
    time: Res<Time>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
    inventory: Res<PlayerInventory>,
    item_registry: Option<Res<ItemRegistry>>,
    player: Query<&Transform, With<Player>>,
    mut drops: Query<(&Transform, &mut MultiplayerDroppedItem), With<MultiplayerDroppedItem>>,
    runtime: Res<MultiplayerClientRuntime>,
    q_connected: Query<Has<Connected>>,
    mut q_sender: Query<&mut MessageSender<ClientDropPickup>>,
) {
    if !multiplayer_connection.connected {
        return;
    }

    let Some(entity) = runtime.connection_entity else {
        return;
    };

    if !q_connected.get(entity).unwrap_or(false) {
        return;
    }

    let Ok(mut sender) = q_sender.get_mut(entity) else {
        return;
    };

    let Ok(player_transform) = player.single() else {
        return;
    };

    let radius_sq = MULTIPLAYER_DROP_PICKUP_RADIUS * MULTIPLAYER_DROP_PICKUP_RADIUS;
    let player_pos = player_transform.translation;
    let now = time.elapsed_secs();
    let Some(item_registry) = item_registry.as_ref() else {
        return;
    };

    for (transform, mut drop) in &mut drops {
        if now < drop.pickup_ready_at {
            continue;
        }

        if now < drop.next_pickup_request_at {
            continue;
        }

        if !inventory_can_add_item(&inventory, drop.item_id, item_registry) {
            continue;
        }

        if player_pos.distance_squared(transform.translation) > radius_sq {
            continue;
        }

        sender.send::<OrderedReliable>(ClientDropPickup::new(drop.drop_id));
        drop.next_pickup_request_at = now + 0.25;
    }
}

/// Runs the `send_local_player_pose` routine for send local player pose in the `client` module.
fn send_local_player_pose(
    time: Res<Time>,
    q_player: Query<(&Transform, &FpsController), With<Player>>,
    mut runtime: ResMut<MultiplayerClientRuntime>,
    q_connected: Query<Has<Connected>>,
    mut q_sender: Query<&mut MessageSender<PlayerMove>>,
) {
    runtime.send_timer.tick(time.delta());
    if !runtime.send_timer.just_finished() {
        return;
    }

    let Some(entity) = runtime.connection_entity else {
        return;
    };

    if !q_connected.get(entity).unwrap_or(false) {
        return;
    }

    let Ok(mut sender) = q_sender.get_mut(entity) else {
        return;
    };

    let Ok((transform, controller)) = q_player.single() else {
        return;
    };

    sender.send::<UnorderedUnreliable>(PlayerMove::new(
        transform.translation.to_array(),
        controller.yaw,
        controller.pitch,
    ));
}

/// Runs the `send_client_keepalive` routine for send client keepalive in the `client` module.
fn send_client_keepalive(
    time: Res<Time>,
    mut runtime: ResMut<MultiplayerClientRuntime>,
    q_connected: Query<Has<Connected>>,
    mut q_sender: Query<&mut MessageSender<ClientKeepAlive>>,
) {
    runtime.keepalive_timer.tick(time.delta());
    if !runtime.keepalive_timer.just_finished() {
        return;
    }

    let Some(entity) = runtime.connection_entity else {
        return;
    };

    if !q_connected.get(entity).unwrap_or(false) {
        return;
    }

    let Ok(mut sender) = q_sender.get_mut(entity) else {
        return;
    };

    let stamp_ms = (time.elapsed_secs_f64() * 1000.0) as u32;
    sender.send::<UnorderedReliable>(ClientKeepAlive::new(stamp_ms));
}

/// Applies remote block break for the `client` module.
fn apply_remote_block_break(
    location: [i32; 3],
    registry: Option<&BlockRegistry>,
    chunk_map: &mut ChunkMap,
    fluids: &mut FluidMap,
    ev_dirty: &mut MessageWriter<SubChunkNeedRemeshEvent>,
) {
    let world_pos = IVec3::from_array(location);
    if world_pos.y < Y_MIN || world_pos.y > Y_MAX {
        return;
    }

    let mut changed = false;
    if let Some(mut access) = world_access_mut(chunk_map, world_pos) {
        let primary = access.get();
        let stacked = access.get_stacked();
        if primary != 0 || stacked != 0 {
            let clear_both = primary != 0
                && stacked != 0
                && registry.is_some_and(|registry| {
                    registry
                        .def_opt(primary)
                        .is_some_and(|definition| !definition.mesh_visible)
                });
            if stacked != 0 && !clear_both {
                access.set(stacked);
                access.set_stacked(0);
            } else {
                access.set(0);
                access.set_stacked(0);
            }
            changed = true;
        }
    }

    let (chunk_coord, local) = world_to_chunk_xz(world_pos.x, world_pos.z);
    let lx = local.x as usize;
    let lz = local.y as usize;
    let ly = world_y_to_local(world_pos.y);
    if let Some(fluid_chunk) = fluids.0.get_mut(&chunk_coord) {
        if fluid_chunk.get(lx, ly, lz) {
            fluid_chunk.set(lx, ly, lz, false);
            changed = true;
        }
    }

    if changed {
        mark_dirty_block_and_neighbors(chunk_map, world_pos, ev_dirty);
    }
}

/// Applies remote block place for the `client` module.
fn apply_remote_block_place(
    location: [i32; 3],
    block_id: u16,
    stacked_block_id: u16,
    registry: &BlockRegistry,
    chunk_map: &mut ChunkMap,
    fluids: &mut FluidMap,
    ev_dirty: &mut MessageWriter<SubChunkNeedRemeshEvent>,
) {
    if block_id == 0 && stacked_block_id == 0 {
        return;
    }

    let world_pos = IVec3::from_array(location);
    if world_pos.y < Y_MIN || world_pos.y > Y_MAX {
        return;
    }

    let (chunk_coord, local) = world_to_chunk_xz(world_pos.x, world_pos.z);
    let lx = local.x as usize;
    let lz = local.y as usize;
    let ly = world_y_to_local(world_pos.y);
    let is_fluid = registry.is_fluid(block_id);

    if let Some(mut access) = world_access_mut(chunk_map, world_pos) {
        if is_fluid {
            access.set(block_id);
            access.set_stacked(0);
        } else {
            access.set(block_id);
            access.set_stacked(stacked_block_id);
        }
        mark_dirty_block_and_neighbors(chunk_map, world_pos, ev_dirty);
    }

    let fluid_chunk = fluids
        .0
        .entry(chunk_coord)
        .or_insert_with(|| FluidChunk::new(SEA_LEVEL));
    if is_fluid {
        fluid_chunk.set(lx, ly, lz, true);
    } else {
        fluid_chunk.set(lx, ly, lz, false);
    }
}

/// Spawns multiplayer drop for the `client` module.
#[allow(clippy::too_many_arguments)]
fn spawn_multiplayer_drop(
    commands: &mut Commands,
    registry: &BlockRegistry,
    item_registry: &ItemRegistry,
    meshes: &mut Assets<Mesh>,
    drops: &mut MultiplayerDropIndex,
    drop_id: u64,
    location: [i32; 3],
    item_id: u16,
    has_motion: bool,
    spawn_translation: [f32; 3],
    initial_velocity: [f32; 3],
    spawn_now: f32,
) {
    if item_id == 0 {
        return;
    }

    if drops.entities.contains_key(&drop_id) {
        return;
    }

    let Some((mesh, material, visual_scale, block_visual)) =
        build_world_item_drop_visual(registry, item_registry, item_id, MULTIPLAYER_DROP_ITEM_SIZE)
    else {
        return;
    };

    let world_loc = IVec3::from_array(location);
    let pop_velocity = if has_motion {
        Vec3::from_array(initial_velocity)
    } else {
        compute_multiplayer_drop_pop_velocity(world_loc, drop_id)
    };
    let angular_velocity = compute_multiplayer_drop_angular_velocity(world_loc, drop_id);
    let spin_axis = compute_multiplayer_drop_spin_axis(world_loc, drop_id);
    let spin_speed = compute_multiplayer_drop_spin_speed(world_loc, drop_id);
    let initial_rotation = Quat::from_euler(
        EulerRot::XYZ,
        hash01_u64(seed_from_world_loc(world_loc) ^ drop_id ^ 0xA11CE) * std::f32::consts::TAU,
        hash01_u64(seed_from_world_loc(world_loc) ^ drop_id ^ 0xB00B5) * std::f32::consts::TAU,
        hash01_u64(seed_from_world_loc(world_loc) ^ drop_id ^ 0xC0FFEE) * std::f32::consts::TAU,
    );
    let center = if has_motion {
        Vec3::from_array(spawn_translation)
    } else {
        Vec3::new(
            (world_loc.x as f32 + 0.5) * VOXEL_SIZE,
            (world_loc.y as f32 + 0.5) * VOXEL_SIZE + 0.28,
            (world_loc.z as f32 + 0.5) * VOXEL_SIZE,
        )
    };

    let entity = commands
        .spawn((
            MultiplayerDroppedItem {
                drop_id,
                item_id,
                block_visual,
                pickup_ready_at: spawn_now + MULTIPLAYER_DROP_PICKUP_DELAY_SECS,
                next_pickup_request_at: 0.0,
                resting: false,
                velocity: pop_velocity,
                angular_velocity,
                spin_axis,
                spin_speed,
            },
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(material),
            Transform {
                translation: center,
                rotation: initial_rotation,
                scale: visual_scale,
            },
            Visibility::default(),
            Name::new(format!("MultiplayerDrop#{drop_id}")),
        ))
        .id();

    drops.entities.insert(drop_id, entity);
}

/// Clears multiplayer drops for the `client` module.
fn clear_multiplayer_drops(commands: &mut Commands, drops: &mut MultiplayerDropIndex) {
    for entity in drops.entities.drain().map(|(_, entity)| entity) {
        safe_despawn_entity(commands, entity);
    }
}

/// Runs the `inventory_can_add_item` routine for inventory can add item in the `client` module.
fn inventory_can_add_item(
    inventory: &PlayerInventory,
    item_id: u16,
    item_registry: &ItemRegistry,
) -> bool {
    if item_id == 0 {
        return false;
    }

    let stack_limit = item_registry.stack_limit(item_id);
    inventory
        .slots
        .iter()
        .any(|slot| slot.is_empty() || (slot.item_id == item_id && slot.count < stack_limit))
}

/// Computes multiplayer drop pop velocity for the `client` module.
fn compute_multiplayer_drop_pop_velocity(world_loc: IVec3, drop_id: u64) -> Vec3 {
    let seed_base = seed_from_world_loc(world_loc) ^ drop_id;
    let angle = hash01_u64(seed_base ^ 0x10) * std::f32::consts::TAU;
    let distance = MULTIPLAYER_DROP_POP_MIN_DIST
        + (MULTIPLAYER_DROP_POP_MAX_DIST - MULTIPLAYER_DROP_POP_MIN_DIST)
            * hash01_u64(seed_base ^ 0x20);
    let flight_time = 0.35 + hash01_u64(seed_base ^ 0x30) * 0.25;
    let horizontal_speed = (distance / flight_time).max(0.2);

    Vec3::new(
        angle.cos() * horizontal_speed,
        2.8 + hash01_u64(seed_base ^ 0x40) * 1.2,
        angle.sin() * horizontal_speed,
    )
}

/// Computes multiplayer drop angular velocity for the `client` module.
fn compute_multiplayer_drop_angular_velocity(world_loc: IVec3, drop_id: u64) -> Vec3 {
    let seed_base = seed_from_world_loc(world_loc) ^ drop_id ^ 0x5EED;

    Vec3::new(
        -8.0 + hash01_u64(seed_base ^ 0x51) * 16.0,
        -10.0 + hash01_u64(seed_base ^ 0x52) * 20.0,
        -8.0 + hash01_u64(seed_base ^ 0x53) * 16.0,
    )
}

/// Computes multiplayer drop spin axis for the `client` module.
fn compute_multiplayer_drop_spin_axis(world_loc: IVec3, drop_id: u64) -> Vec3 {
    let seed_base = seed_from_world_loc(world_loc) ^ drop_id ^ 0x7A51_5EED;
    let axis = Vec3::new(
        -1.0 + hash01_u64(seed_base ^ 0x71) * 2.0,
        0.35 + hash01_u64(seed_base ^ 0x72) * 1.3,
        -1.0 + hash01_u64(seed_base ^ 0x73) * 2.0,
    );
    let axis = axis.normalize_or_zero();
    if axis.length_squared() > 0.000_001 {
        axis
    } else {
        Vec3::new(0.78, 0.44, 0.44).normalize()
    }
}

/// Computes multiplayer drop spin speed for the `client` module.
fn compute_multiplayer_drop_spin_speed(world_loc: IVec3, drop_id: u64) -> f32 {
    let seed_base = seed_from_world_loc(world_loc) ^ drop_id ^ 0x8BAD_F00D;
    let magnitude = 18.0 + hash01_u64(seed_base ^ 0x81) * 14.0;
    let sign = if hash01_u64(seed_base ^ 0x82) < 0.5 {
        -1.0
    } else {
        1.0
    };
    sign * magnitude
}

/// Runs the `angle_abs_diff` routine for angle abs diff in the `client` module.
#[inline]
fn angle_abs_diff(from: f32, to: f32) -> f32 {
    let wrapped =
        (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    wrapped.abs()
}

/// Runs the `lerp_angle_radians` routine for lerp angle radians in the `client` module.
#[inline]
fn lerp_angle_radians(from: f32, to: f32, t: f32) -> f32 {
    let wrapped =
        (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    from + wrapped * t.clamp(0.0, 1.0)
}

/// Runs the `seed_from_world_loc` routine for seed from world loc in the `client` module.
fn seed_from_world_loc(world_loc: IVec3) -> u64 {
    (world_loc.x as i64 as u64).wrapping_mul(0x9E37_79B1_85EB_CA87)
        ^ (world_loc.y as i64 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ (world_loc.z as i64 as u64).wrapping_mul(0x1656_67B1_9E37_79F9)
}

/// Runs the `hash01_u64` routine for hash01 u64 in the `client` module.
fn hash01_u64(mut x: u64) -> f32 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;

    (x as f64 / u64::MAX as f64) as f32
}

/// Runs the `ensure_remote_player` routine for ensure remote player in the `client` module.
fn ensure_remote_player(
    commands: &mut Commands,
    visuals: &RemotePlayerVisuals,
    remote_players: &mut HashMap<u64, Entity>,
    player_id: u64,
    translation: Vec3,
    yaw: f32,
) -> Entity {
    if let Some(entity) = remote_players.get(&player_id) {
        return *entity;
    }

    let entity = commands
        .spawn((
            RemotePlayerAvatar { player_id },
            Name::new(format!("RemotePlayer#{player_id}")),
            Mesh3d(visuals.mesh.clone()),
            MeshMaterial3d(visuals.material.clone()),
            Transform {
                translation,
                rotation: Quat::from_rotation_y(yaw),
                scale: Vec3::ONE,
            },
            Visibility::default(),
            InheritedVisibility::default(),
            ViewVisibility::default(),
        ))
        .id();

    remote_players.insert(player_id, entity);
    entity
}
