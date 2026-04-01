fn toggle_player_inventory_ui(
    keyboard: Res<ButtonInput<KeyCode>>,
    global_config: Res<GlobalConfig>,
    mut ui_interaction: ResMut<UiInteractionState>,
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut root: Query<&mut Visibility, With<PlayerInventoryRoot>>,
    mut inventory_ui: ResMut<PlayerInventoryUiState>,
    mut drag_state: ResMut<InventoryDragState>,
) {
    let open_key =
        convert(global_config.input.ui_inventory.as_str()).expect("Invalid inventory key");
    let close_key =
        convert(global_config.input.ui_close_back.as_str()).expect("Invalid close/back key");

    if inventory_ui.open && keyboard.just_pressed(close_key) {
        inventory_ui.open = false;
        drag_state.source_slot = None;
        ui_interaction.inventory_open = false;
        set_inventory_cursor(false, &mut cursor_q);
        if let Ok(mut visible) = root.single_mut() {
            *visible = Visibility::Hidden;
        }
        return;
    }

    if ui_interaction.menu_open {
        return;
    }

    if !keyboard.just_pressed(open_key) {
        return;
    }

    inventory_ui.open = !inventory_ui.open;
    if !inventory_ui.open {
        drag_state.source_slot = None;
    }
    ui_interaction.inventory_open = inventory_ui.open;
    set_inventory_cursor(inventory_ui.open, &mut cursor_q);
    if let Ok(mut visible) = root.single_mut() {
        *visible = if inventory_ui.open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

fn close_player_inventory_ui(
    mut ui_interaction: ResMut<UiInteractionState>,
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut inventory_ui: ResMut<PlayerInventoryUiState>,
    mut root: Query<&mut Visibility, With<PlayerInventoryRoot>>,
    mut drag_state: ResMut<InventoryDragState>,
) {
    if !inventory_ui.open {
        return;
    }

    inventory_ui.open = false;
    drag_state.source_slot = None;
    ui_interaction.inventory_open = false;
    set_inventory_cursor(false, &mut cursor_q);
    if let Ok(mut visible) = root.single_mut() {
        *visible = Visibility::Hidden;
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_inventory_drag_and_drop(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    time: Res<Time>,
    global_config: Res<GlobalConfig>,
    inventory_ui: Res<PlayerInventoryUiState>,
    game_mode: Res<GameModeState>,
    multiplayer_connection: Option<Res<MultiplayerConnectionState>>,
    mut drag_state: ResMut<InventoryDragState>,
    mut inventory: ResMut<PlayerInventory>,
    mut slot_frames: Query<(&CssID, &UIWidgetState, &mut BorderColor)>,
    player_q: Query<&Transform, With<Player>>,
    mut drop_requests: MessageWriter<DropItemRequest>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    block_registry: Res<BlockRegistry>,
    item_registry: Res<ItemRegistry>,
) {
    let hovered_slot = sync_inventory_slot_hover_border(&mut slot_frames, inventory_ui.open);

    if !inventory_ui.open {
        drag_state.source_slot = None;
        return;
    }

    let drop_key = convert(global_config.input.drop_item.as_str()).unwrap_or(KeyCode::KeyQ);

    if keyboard.just_pressed(drop_key) {
        let Some(slot_index) = hovered_slot else {
            return;
        };
        if slot_index >= PLAYER_INVENTORY_SLOTS || !matches!(game_mode.0, GameMode::Survival) {
            return;
        }
        let slot = &mut inventory.slots[slot_index];
        if slot.is_empty() {
            return;
        }
        let Ok(player_tf) = player_q.single() else {
            return;
        };

        let dropped_item_id = slot.item_id;
        if slot.count <= 1 {
            *slot = InventorySlot::default();
        } else {
            slot.count -= 1;
        }

        if multiplayer_connection.as_ref().is_some_and(|state| state.connected) {
            let (spawn_center, initial_velocity) =
                player_drop_spawn_motion(player_tf.translation, player_tf.forward().as_vec3());
            let world_loc =
                player_drop_world_location(player_tf.translation, player_tf.forward().as_vec3());
            drop_requests.write(DropItemRequest::new(
                dropped_item_id,
                1,
                world_loc.to_array(),
                spawn_center.to_array(),
                initial_velocity.to_array(),
            ));
        } else {
            spawn_player_dropped_item_stack(
                &mut commands,
                &mut meshes,
                &block_registry,
                &item_registry,
                dropped_item_id,
                1,
                player_tf.translation,
                player_tf.forward().as_vec3(),
                time.elapsed_secs(),
            );
        }
        return;
    }

    if mouse.just_pressed(MouseButton::Left) && drag_state.source_slot.is_none()
        && let Some(source_index) = hovered_slot
        && inventory
            .slots
            .get(source_index)
            .is_some_and(|slot| !slot.is_empty())
    {
        drag_state.source_slot = Some(source_index);
    }

    if !mouse.just_released(MouseButton::Left) {
        return;
    }

    let Some(source_index) = drag_state.source_slot.take() else {
        return;
    };

    if source_index >= PLAYER_INVENTORY_SLOTS {
        return;
    }

    if let Some(target_index) = hovered_slot {
        if target_index < PLAYER_INVENTORY_SLOTS && target_index != source_index {
            inventory.slots.swap(source_index, target_index);
        }
        return;
    }

    let dropped_slot = inventory.slots[source_index];
    if dropped_slot.is_empty() || !matches!(game_mode.0, GameMode::Survival) {
        return;
    }
    let Ok(player_tf) = player_q.single() else {
        return;
    };

    inventory.slots[source_index] = InventorySlot::default();

    if multiplayer_connection.as_ref().is_some_and(|state| state.connected) {
        let (spawn_center, initial_velocity) =
            player_drop_spawn_motion(player_tf.translation, player_tf.forward().as_vec3());
        let world_loc =
            player_drop_world_location(player_tf.translation, player_tf.forward().as_vec3());
        drop_requests.write(DropItemRequest::new(
            dropped_slot.item_id,
            dropped_slot.count,
            world_loc.to_array(),
            spawn_center.to_array(),
            initial_velocity.to_array(),
        ));
    } else {
        spawn_player_dropped_item_stack(
            &mut commands,
            &mut meshes,
            &block_registry,
            &item_registry,
            dropped_slot.item_id,
            dropped_slot.count,
            player_tf.translation,
            player_tf.forward().as_vec3(),
            time.elapsed_secs(),
        );
    }
}

fn sync_player_inventory_ui(
    inventory: Res<PlayerInventory>,
    item_registry: Res<ItemRegistry>,
    asset_server: Res<AssetServer>,
    mut paragraphs: Query<(&CssID, &mut Paragraph)>,
    mut slot_buttons: Query<(&CssID, &mut Button)>,
) {
    for (css_id, mut paragraph) in &mut paragraphs {
        if css_id.0 == PLAYER_INVENTORY_TOTAL_ID {
            paragraph.text = format!("Items: {}", inventory.total_items());
        }
    }

    for (css_id, mut button) in &mut slot_buttons {
        let Some(slot_number) = css_id.0.strip_prefix(PLAYER_INVENTORY_FRAME_PREFIX) else {
            continue;
        };
        let Ok(slot_index) = slot_number.parse::<usize>() else {
            continue;
        };
        let Some(slot) = inventory.slots.get(slot_index.saturating_sub(1)) else {
            continue;
        };
        let next_text = if slot.is_empty() {
            String::new()
        } else {
            slot.count.to_string()
        };
        if button.text != next_text {
            button.text = next_text;
        }
        let next_icon = if slot.is_empty() {
            None
        } else {
            resolve_item_icon_path(&item_registry, &asset_server, slot.item_id)
        };
        if button.icon_path != next_icon {
            button.icon_path = next_icon;
        }
    }
}

fn set_inventory_cursor(
    inventory_open: bool,
    cursor_q: &mut Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let Ok(mut cursor) = cursor_q.single_mut() else {
        return;
    };

    if inventory_open {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    } else {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

fn sync_inventory_slot_hover_border(
    slot_frames: &mut Query<(&CssID, &UIWidgetState, &mut BorderColor)>,
    inventory_open: bool,
) -> Option<usize> {
    let mut hovered_slot = None;

    for (css_id, state, mut border) in slot_frames.iter_mut() {
        if !css_id.0.starts_with(PLAYER_INVENTORY_FRAME_PREFIX) {
            continue;
        }

        if hovered_slot.is_none() && state.hovered
            && let Some(slot_number) = css_id.0.strip_prefix(PLAYER_INVENTORY_FRAME_PREFIX)
            && let Some(slot_index) = slot_number
                .parse::<usize>()
                .ok()
                .and_then(|index| index.checked_sub(1))
            && slot_index < PLAYER_INVENTORY_SLOTS
        {
            hovered_slot = Some(slot_index);
        }

        let color = if inventory_open && state.hovered {
            color_accent()
        } else {
            color_background_hover()
        };

        border.top = color;
        border.right = color;
        border.bottom = color;
        border.left = color;
    }

    hovered_slot
}
