fn show_hud_hotbar_ui(mut root: Query<&mut Visibility, With<HudRoot>>) {
    if let Ok(mut visible) = root.single_mut() {
        *visible = Visibility::Inherited;
    }
}

fn hide_hud_hotbar_ui(mut root: Query<&mut Visibility, With<HudRoot>>) {
    if let Ok(mut visible) = root.single_mut() {
        *visible = Visibility::Hidden;
    }
}

fn cycle_hotbar_with_scroll(
    mut wheel_reader: MessageReader<MouseWheel>,
    mut hotbar_state: ResMut<HotbarSelectionState>,
) {
    let mut total_steps = 0_i32;

    for wheel in wheel_reader.read() {
        let raw = match wheel.unit {
            MouseScrollUnit::Line => wheel.y,
            MouseScrollUnit::Pixel => wheel.y / 24.0,
        };
        if raw.abs() < f32::EPSILON {
            continue;
        }

        let discrete = raw.round() as i32;
        if discrete != 0 {
            total_steps += discrete;
        } else {
            total_steps += raw.signum() as i32;
        }
    }

    if total_steps == 0 {
        return;
    }

    let steps = total_steps.unsigned_abs() as usize;
    for _ in 0..steps {
        if total_steps > 0 {
            hotbar_state.selected_index =
                (hotbar_state.selected_index + HOTBAR_SLOTS - 1) % HOTBAR_SLOTS;
        } else {
            hotbar_state.selected_index = (hotbar_state.selected_index + 1) % HOTBAR_SLOTS;
        }
    }
}

fn drop_selected_hotbar_item(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    global_config: Res<GlobalConfig>,
    game_mode: Res<GameModeState>,
    ui_interaction: Res<UiInteractionState>,
    hotbar_state: Res<HotbarSelectionState>,
    multiplayer_connection: Option<Res<MultiplayerConnectionState>>,
    mut inventory: ResMut<PlayerInventory>,
    player_q: Query<&Transform, With<Player>>,
    mut drop_requests: MessageWriter<DropItemRequest>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    block_registry: Res<BlockRegistry>,
    item_registry: Res<ItemRegistry>,
) {
    if ui_interaction.menu_open || ui_interaction.inventory_open {
        return;
    }

    if !matches!(game_mode.0, GameMode::Survival) {
        return;
    }

    let drop_key = convert(global_config.input.drop_item.as_str()).unwrap_or(KeyCode::KeyQ);
    if !keyboard.just_pressed(drop_key) {
        return;
    }

    let slot_index = hotbar_state.selected_index;
    if slot_index >= HOTBAR_SLOTS || slot_index >= inventory.slots.len() {
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
        *slot = Default::default();
    } else {
        slot.count -= 1;
    }

    if multiplayer_connection
        .as_ref()
        .is_some_and(|state| state.connected)
    {
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
}

fn sync_hotbar_selected_block(
    hotbar_state: Res<HotbarSelectionState>,
    inventory: Res<PlayerInventory>,
    item_registry: Res<ItemRegistry>,
    registry: Res<BlockRegistry>,
    mut selected: ResMut<SelectedBlock>,
) {
    let Some(slot) = inventory.slots.get(hotbar_state.selected_index).copied() else {
        selected.id = 0;
        selected.name = "air".to_string();
        return;
    };

    if slot.is_empty() {
        selected.id = 0;
        selected.name = "air".to_string();
        return;
    }

    let Some(block_id) = item_registry.block_for_item(slot.item_id) else {
        selected.id = 0;
        selected.name = "air".to_string();
        return;
    };

    selected.id = block_id;
    selected.name = registry.name_opt(block_id).unwrap_or("air").to_string();
}

fn sync_hud_hotbar_ui(
    hotbar_state: Res<HotbarSelectionState>,
    inventory: Res<PlayerInventory>,
    item_registry: Res<ItemRegistry>,
    block_registry: Res<BlockRegistry>,
    asset_server: Res<AssetServer>,
    mut image_cache: ResMut<ImageCache>,
    mut images: ResMut<Assets<Image>>,
    mut buttons: Query<(&CssID, &mut Button, &mut BorderColor, &mut BackgroundColor)>,
    mut badges: Query<(&CssID, &mut Paragraph, &mut Visibility)>,
) {
    for (css_id, mut button, mut border, mut background) in &mut buttons {
        if let Some(slot_number) = css_id.0.strip_prefix(HUD_SLOT_PREFIX) {
            let Ok(slot_index) = slot_number.parse::<usize>() else {
                continue;
            };
            let Some(slot) = inventory.slots.get(slot_index.saturating_sub(1)) else {
                continue;
            };

            if !button.text.is_empty() {
                button.text.clear();
            }
            let next_icon = if slot.is_empty() {
                None
            } else {
                resolve_item_icon_path(
                    &item_registry,
                    &block_registry,
                    &asset_server,
                    &mut image_cache,
                    &mut images,
                    slot.item_id,
                )
            };
            if button.icon_path != next_icon {
                button.icon_path = next_icon;
            }

            let selected = slot_index.saturating_sub(1) == hotbar_state.selected_index;
            let border_color = if selected {
                color_accent()
            } else {
                color_background_hover()
            };
            border.top = border_color;
            border.right = border_color;
            border.bottom = border_color;
            border.left = border_color;
            background.0 = if selected {
                color_background_hover()
            } else {
                color_background()
            };
        }
    }

    for (css_id, mut paragraph, mut visibility) in &mut badges {
        let Some(slot_number) = css_id.0.strip_prefix(HUD_SLOT_BADGE_PREFIX) else {
            continue;
        };
        let Ok(slot_index) = slot_number.parse::<usize>() else {
            continue;
        };
        let Some(slot) = inventory.slots.get(slot_index.saturating_sub(1)) else {
            continue;
        };

        if slot.is_empty() {
            if !paragraph.text.is_empty() {
                paragraph.text.clear();
            }
            *visibility = Visibility::Hidden;
            continue;
        }

        let count_text = slot.count.to_string();
        if paragraph.text != count_text {
            paragraph.text = count_text;
        }
        *visibility = Visibility::Inherited;
    }
}

fn resolve_item_icon_path(
    registry: &ItemRegistry,
    block_registry: &BlockRegistry,
    asset_server: &AssetServer,
    image_cache: &mut ImageCache,
    images: &mut Assets<Image>,
    item_id: u16,
) -> Option<String> {
    let path = registry.icon_path(asset_server, item_id)?;
    ensure_block_icon_cached(
        block_registry,
        asset_server,
        image_cache,
        images,
        path.as_str(),
    );
    Some(path)
}

/// Ensures a virtual block-icon path has a populated in-memory image cache entry.
fn ensure_block_icon_cached(
    block_registry: &BlockRegistry,
    asset_server: &AssetServer,
    image_cache: &mut ImageCache,
    images: &mut Assets<Image>,
    path: &str,
) {
    let Some(block_id) = parse_block_icon_cache_key(path) else {
        return;
    };
    if image_cache.map.contains_key(path) {
        return;
    }
    let Some(image) = build_block_item_icon_image(block_registry, asset_server, block_id) else {
        return;
    };
    let handle = images.add(image);
    image_cache.map.insert(path.to_string(), handle);
}
