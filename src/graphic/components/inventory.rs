use api::handlers::recipe::resolve_hand_crafted_recipe;
use bevy::ecs::{query::QueryFilter, system::SystemParam};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InventoryUiSlotTarget {
    Player(usize),
    HandCrafted(usize),
    HandCraftedResult,
}

#[derive(SystemParam)]
struct InventoryDropDeps<'w, 's> {
    commands: Commands<'w, 's>,
    meshes: ResMut<'w, Assets<Mesh>>,
    block_registry: Res<'w, BlockRegistry>,
    item_registry: Res<'w, ItemRegistry>,
}

#[derive(SystemParam)]
struct InventoryDragDropDeps<'w, 's> {
    global_config: Res<'w, GlobalConfig>,
    inventory_ui: Res<'w, PlayerInventoryUiState>,
    recipe_preview: ResMut<'w, RecipePreviewDialogState>,
    game_mode: Res<'w, GameModeState>,
    multiplayer_connection: Option<Res<'w, MultiplayerConnectionState>>,
    cursor_item: ResMut<'w, InventoryCursorItemState>,
    left_hold: ResMut<'w, InventoryLeftHoldState>,
    inventory: ResMut<'w, PlayerInventory>,
    hand_crafted: ResMut<'w, HandCraftedState>,
    slot_frames: Query<'w, 's, (&'static CssID, &'static UIWidgetState, &'static mut BorderColor)>,
    button_states: Query<'w, 's, (&'static CssID, &'static UIWidgetState), With<Button>>,
    window_q: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    inventory_panel_q:
        Query<'w, 's, (&'static ComputedNode, &'static GlobalTransform), With<InventoryMainPanel>>,
    inventory_drop_zone_q: Query<
        'w,
        's,
        (&'static ComputedNode, &'static GlobalTransform),
        With<InventoryDropZonePanel>,
    >,
    recipe_preview_panel_q: Query<
        'w,
        's,
        (&'static ComputedNode, &'static GlobalTransform),
        With<RecipePreviewDialogPanel>,
    >,
    player_q: Query<'w, 's, &'static Transform, With<Player>>,
    drop_requests: MessageWriter<'w, DropItemRequest>,
    craft_requests: MessageWriter<'w, CraftHandCraftedRequest>,
}

const LEFT_HOLD_INITIAL_DELAY_SECS: f64 = 0.18;
const LEFT_HOLD_REPEAT_INTERVAL_SECS: f64 = 0.03;

fn toggle_player_inventory_ui(
    keyboard: Res<ButtonInput<KeyCode>>,
    global_config: Res<GlobalConfig>,
    mut ui_interaction: ResMut<UiInteractionState>,
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut root: Query<&mut Visibility, With<PlayerInventoryRoot>>,
    mut inventory_ui: ResMut<PlayerInventoryUiState>,
    mut cursor_item: ResMut<InventoryCursorItemState>,
    mut left_hold: ResMut<InventoryLeftHoldState>,
    mut recipe_preview: ResMut<RecipePreviewDialogState>,
    mut inventory: ResMut<PlayerInventory>,
    mut hand_crafted: ResMut<HandCraftedState>,
    item_registry: Option<Res<ItemRegistry>>,
) {
    let open_key =
        convert(global_config.input.ui_inventory.as_str()).expect("Invalid inventory key");
    let close_key =
        convert(global_config.input.ui_close_back.as_str()).expect("Invalid close/back key");

    if inventory_ui.open && keyboard.just_pressed(close_key) {
        if recipe_preview.open {
            recipe_preview.open = false;
            left_hold.source_slot = None;
            return;
        }

        if let Some(item_registry) = item_registry.as_ref() {
            flush_hand_crafted_inputs_to_inventory(
                &mut hand_crafted,
                &mut inventory,
                item_registry,
            );
            flush_cursor_item_to_inventory(&mut cursor_item, &mut inventory, item_registry);
        }
        inventory_ui.open = false;
        left_hold.source_slot = None;
        recipe_preview.open = false;
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
        left_hold.source_slot = None;
        recipe_preview.open = false;
        if let Some(item_registry) = item_registry.as_ref() {
            flush_hand_crafted_inputs_to_inventory(
                &mut hand_crafted,
                &mut inventory,
                item_registry,
            );
            flush_cursor_item_to_inventory(&mut cursor_item, &mut inventory, item_registry);
        }
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
    mut cursor_item: ResMut<InventoryCursorItemState>,
    mut left_hold: ResMut<InventoryLeftHoldState>,
    mut recipe_preview: ResMut<RecipePreviewDialogState>,
    mut inventory: ResMut<PlayerInventory>,
    mut hand_crafted: ResMut<HandCraftedState>,
    item_registry: Option<Res<ItemRegistry>>,
) {
    if !inventory_ui.open {
        return;
    }

    if let Some(item_registry) = item_registry.as_ref() {
        flush_hand_crafted_inputs_to_inventory(&mut hand_crafted, &mut inventory, item_registry);
        flush_cursor_item_to_inventory(&mut cursor_item, &mut inventory, item_registry);
    }
    inventory_ui.open = false;
    left_hold.source_slot = None;
    recipe_preview.open = false;
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
    deps: InventoryDragDropDeps,
    mut drop_deps: InventoryDropDeps,
) {
    let InventoryDragDropDeps {
        global_config,
        inventory_ui,
        mut recipe_preview,
        game_mode,
        multiplayer_connection,
        mut cursor_item,
        mut left_hold,
        mut inventory,
        mut hand_crafted,
        mut slot_frames,
        button_states,
        window_q,
        inventory_panel_q,
        inventory_drop_zone_q,
        recipe_preview_panel_q,
        player_q,
        mut drop_requests,
        mut craft_requests,
    } = deps;

    let hovered_slot = sync_inventory_slot_hover_border(&mut slot_frames, inventory_ui.open);

    if !inventory_ui.open {
        left_hold.source_slot = None;
        return;
    }

    if mouse.just_released(MouseButton::Left) {
        left_hold.source_slot = None;
    }

    let close_key = convert(global_config.input.ui_close_back.as_str()).unwrap_or(KeyCode::Escape);
    if recipe_preview.open && keyboard.just_pressed(close_key) {
        recipe_preview.open = false;
        left_hold.source_slot = None;
        return;
    }

    if mouse.just_pressed(MouseButton::Left) && recipe_preview.open {
        if is_button_hovered(&button_states, RECIPE_PREVIEW_FILL_ID) {
            left_hold.source_slot = None;
            fill_hand_crafted_from_recipe_preview(
                &recipe_preview,
                &mut inventory,
                &mut hand_crafted,
                &drop_deps.item_registry,
            );
            recipe_preview.open = false;
            return;
        }

        if !is_cursor_inside_panel(&window_q, &recipe_preview_panel_q) {
            recipe_preview.open = false;
            left_hold.source_slot = None;
            return;
        }
    }

    let shift_pressed = keyboard.pressed(KeyCode::ShiftLeft);
    if shift_pressed && mouse.just_pressed(MouseButton::Left) && cursor_item.slot.is_empty() {
        left_hold.source_slot = None;
        match hovered_slot {
            Some(InventoryUiSlotTarget::Player(slot_index)) => {
                let _ = transfer_player_slot_to_hand_crafted(
                    slot_index,
                    &mut inventory,
                    &mut hand_crafted,
                );
                return;
            }
            Some(InventoryUiSlotTarget::HandCrafted(slot_index)) => {
                let _ = transfer_hand_crafted_slot_to_inventory(
                    slot_index,
                    &mut hand_crafted,
                    &mut inventory,
                    &drop_deps.item_registry,
                );
                return;
            }
            _ => {}
        }
    }

    if mouse.just_pressed(MouseButton::Left)
        && hovered_slot == Some(InventoryUiSlotTarget::HandCraftedResult)
        && !matches!(game_mode.0, GameMode::Spectator)
    {
        left_hold.source_slot = None;
        craft_requests.write(CraftHandCraftedRequest);
        return;
    }

    if mouse.just_pressed(MouseButton::Middle)
        && let Some(slot_target) = hovered_slot
    {
        left_hold.source_slot = None;
        let _ = take_half_from_target_to_cursor(
            slot_target,
            &mut cursor_item,
            &mut inventory,
            &mut hand_crafted,
            &drop_deps.item_registry,
        );
        return;
    }

    if mouse.just_pressed(MouseButton::Right)
        && let Some(slot_target) = hovered_slot
    {
        left_hold.source_slot = None;
        let _ = place_one_from_cursor_on_target(
            slot_target,
            &mut cursor_item,
            &mut inventory,
            &mut hand_crafted,
            &drop_deps.item_registry,
        );
        return;
    }

    if mouse.just_pressed(MouseButton::Left)
        && let Some(slot_target) = hovered_slot
    {
        left_hold.source_slot = None;
        if cursor_item.slot.is_empty() {
            if take_one_from_target_to_cursor(
                slot_target,
                &mut cursor_item,
                &mut inventory,
                &mut hand_crafted,
            ) {
                left_hold.source_slot = hold_source_from_target(slot_target);
                left_hold.next_pull_at_secs = time.elapsed_secs_f64() + LEFT_HOLD_INITIAL_DELAY_SECS;
            }
        } else {
            let _ = place_all_from_cursor_on_target(
                slot_target,
                &mut cursor_item,
                &mut inventory,
                &mut hand_crafted,
                &drop_deps.item_registry,
            );
        }
        return;
    }

    if mouse.just_pressed(MouseButton::Left)
        && hovered_slot.is_none()
        && !cursor_item.slot.is_empty()
    {
        let clicked_inside_inventory = is_cursor_inside_panel(&window_q, &inventory_panel_q);
        let clicked_inside_drop_zone = is_cursor_inside_panel(&window_q, &inventory_drop_zone_q);
        if clicked_inside_inventory && !clicked_inside_drop_zone {
            left_hold.source_slot = None;
            return;
        }

        if matches!(game_mode.0, GameMode::Spectator) {
            return;
        }

        let Ok(player_tf) = player_q.single() else {
            return;
        };
        let dropped_slot = cursor_item.slot;
        cursor_item.slot = InventorySlot::default();
        left_hold.source_slot = None;

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
                &mut drop_deps.commands,
                &mut drop_deps.meshes,
                &drop_deps.block_registry,
                &drop_deps.item_registry,
                dropped_slot.item_id,
                dropped_slot.count,
                player_tf.translation,
                player_tf.forward().as_vec3(),
                time.elapsed_secs(),
            );
        }
        return;
    }

    let drop_key = convert(global_config.input.drop_item.as_str()).unwrap_or(KeyCode::KeyQ);

    if keyboard.just_pressed(drop_key) {
        let Some(InventoryUiSlotTarget::Player(slot_index)) = hovered_slot else {
            return;
        };
        if slot_index >= PLAYER_INVENTORY_SLOTS || matches!(game_mode.0, GameMode::Spectator) {
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
                &mut drop_deps.commands,
                &mut drop_deps.meshes,
                &drop_deps.block_registry,
                &drop_deps.item_registry,
                dropped_item_id,
                1,
                player_tf.translation,
                player_tf.forward().as_vec3(),
                time.elapsed_secs(),
            );
        }
        return;
    }

    if mouse.pressed(MouseButton::Left)
        && let Some(source_slot) = left_hold.source_slot
    {
        if cursor_item.slot.is_empty() {
            left_hold.source_slot = None;
            return;
        }

        let now_secs = time.elapsed_secs_f64();
        if now_secs < left_hold.next_pull_at_secs {
            return;
        }

        let mut steps = 0u8;
        while left_hold.next_pull_at_secs <= now_secs && steps < 8 {
            if !pull_one_from_hold_source(
                source_slot,
                &mut cursor_item,
                &mut inventory,
                &mut hand_crafted,
                &drop_deps.item_registry,
            ) {
                left_hold.source_slot = None;
                break;
            }
            left_hold.next_pull_at_secs += LEFT_HOLD_REPEAT_INTERVAL_SECS;
            steps += 1;
        }
    }
}

fn sync_player_inventory_ui(
    inventory_ui: Res<PlayerInventoryUiState>,
    inventory: Res<PlayerInventory>,
    hand_crafted: Res<HandCraftedState>,
    recipe_preview: Res<RecipePreviewDialogState>,
    recipe_registry: Option<Res<RecipeRegistry>>,
    recipe_type_registry: Option<Res<RecipeTypeRegistry>>,
    item_registry: Res<ItemRegistry>,
    block_registry: Res<BlockRegistry>,
    asset_server: Res<AssetServer>,
    mut image_cache: ResMut<ImageCache>,
    mut images: ResMut<Assets<Image>>,
    mut recipe_preview_root: Query<&mut Visibility, With<RecipePreviewDialogRoot>>,
    mut paragraphs: Query<
        (&CssID, &mut Paragraph, Option<&mut Visibility>),
        Without<RecipePreviewDialogRoot>,
    >,
    mut slot_buttons: Query<(&CssID, &mut Button)>,
) {
    let resolved_recipe = recipe_registry.as_ref().and_then(|recipes| {
        recipe_type_registry
            .as_ref()
            .and_then(|types| resolve_hand_crafted_recipe(&hand_crafted, recipes, types, &item_registry))
    });

    if let Ok(mut visibility) = recipe_preview_root.single_mut() {
        *visibility = if inventory_ui.open && recipe_preview.open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for (css_id, mut paragraph, mut maybe_visibility) in &mut paragraphs {
        if css_id.0 == PLAYER_INVENTORY_TOTAL_ID {
            paragraph.text = format!("Items: {}", inventory.total_items());
            continue;
        }

        if css_id.0 == RECIPE_PREVIEW_TITLE_ID {
            let next_title = if recipe_preview.open {
                item_registry
                    .def_opt(recipe_preview.result_slot.item_id)
                    .map(|item| format!("Recipe: {}", item.name))
                    .unwrap_or_else(|| "Recipe".to_string())
            } else {
                "Recipe".to_string()
            };
            if paragraph.text != next_title {
                paragraph.text = next_title;
            }
            continue;
        }

        if let Some(slot_number) = css_id.0.strip_prefix(PLAYER_INVENTORY_BADGE_PREFIX)
            && let Ok(slot_index) = slot_number.parse::<usize>()
            && let Some(slot) = inventory.slots.get(slot_index.saturating_sub(1))
            && let Some(visibility) = maybe_visibility.as_mut()
        {
            sync_badge(&mut paragraph, visibility, slot.count, slot.is_empty());
            continue;
        }

        if let Some(slot_number) = css_id.0.strip_prefix(HAND_CRAFTED_BADGE_PREFIX)
            && let Ok(slot_index) = slot_number.parse::<usize>()
            && let Some(slot) = hand_crafted.input_slots.get(slot_index.saturating_sub(1))
            && let Some(visibility) = maybe_visibility.as_mut()
        {
            sync_badge(&mut paragraph, visibility, slot.count, slot.is_empty());
            continue;
        }

        if css_id.0 == HAND_CRAFTED_RESULT_BADGE_ID
            && let Some(visibility) = maybe_visibility.as_mut()
        {
            if let Some(result) = resolved_recipe.as_ref() {
                sync_badge(&mut paragraph, visibility, result.result.count, false);
            } else {
                sync_badge(&mut paragraph, visibility, 0, true);
            }
            continue;
        }

        if let Some(slot_number) = css_id.0.strip_prefix(RECIPE_PREVIEW_INPUT_BADGE_PREFIX)
            && let Ok(slot_index) = slot_number.parse::<usize>()
            && let Some(slot) = recipe_preview.input_slots.get(slot_index.saturating_sub(1))
            && let Some(visibility) = maybe_visibility.as_mut()
        {
            sync_badge(
                &mut paragraph,
                visibility,
                slot.count,
                slot.is_empty() || !recipe_preview.open,
            );
            continue;
        }

        if css_id.0 == RECIPE_PREVIEW_RESULT_BADGE_ID
            && let Some(visibility) = maybe_visibility.as_mut()
        {
            sync_badge(
                &mut paragraph,
                visibility,
                recipe_preview.result_slot.count,
                recipe_preview.result_slot.is_empty() || !recipe_preview.open,
            );
        }
    }

    for (css_id, mut button) in &mut slot_buttons {
        if let Some(slot_number) = css_id.0.strip_prefix(PLAYER_INVENTORY_FRAME_PREFIX)
            && let Ok(slot_index) = slot_number.parse::<usize>()
            && let Some(slot) = inventory.slots.get(slot_index.saturating_sub(1))
        {
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
            continue;
        }

        if let Some(slot_number) = css_id.0.strip_prefix(HAND_CRAFTED_FRAME_PREFIX)
            && let Ok(slot_index) = slot_number.parse::<usize>()
            && let Some(slot) = hand_crafted.input_slots.get(slot_index.saturating_sub(1))
        {
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
            continue;
        }

        if css_id.0 == HAND_CRAFTED_RESULT_FRAME_ID {
            if !button.text.is_empty() {
                button.text.clear();
            }
            let next_icon = resolved_recipe.as_ref().and_then(|recipe| {
                resolve_item_icon_path(
                    &item_registry,
                    &block_registry,
                    &asset_server,
                    &mut image_cache,
                    &mut images,
                    recipe.result.item_id,
                )
            });
            if button.icon_path != next_icon {
                button.icon_path = next_icon;
            }
            continue;
        }

        if let Some(slot_index) = parse_recipe_preview_input_index(css_id.0.as_str()) {
            if !button.text.is_empty() {
                button.text.clear();
            }
            let slot = recipe_preview.input_slots.get(slot_index).copied().unwrap_or_default();
            let next_icon = if !recipe_preview.open || slot.is_empty() {
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
            continue;
        }

        if css_id.0 == RECIPE_PREVIEW_RESULT_FRAME_ID {
            if !button.text.is_empty() {
                button.text.clear();
            }
            let next_icon = if !recipe_preview.open || recipe_preview.result_slot.is_empty() {
                None
            } else {
                resolve_item_icon_path(
                    &item_registry,
                    &block_registry,
                    &asset_server,
                    &mut image_cache,
                    &mut images,
                    recipe_preview.result_slot.item_id,
                )
            };
            if button.icon_path != next_icon {
                button.icon_path = next_icon;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_inventory_tooltip_ui(
    inventory_ui: Res<PlayerInventoryUiState>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    inventory: Res<PlayerInventory>,
    hand_crafted: Res<HandCraftedState>,
    creative_panel: Res<CreativePanelState>,
    recipe_preview: Res<RecipePreviewDialogState>,
    recipe_registry: Option<Res<RecipeRegistry>>,
    recipe_type_registry: Option<Res<RecipeTypeRegistry>>,
    item_registry: Res<ItemRegistry>,
    slot_states: Query<(&CssID, &UIWidgetState), With<Button>>,
    mut tooltip_root: Query<(&mut Visibility, &mut Node), With<InventoryTooltipRoot>>,
    mut tooltip_text: Query<(&CssID, &mut Paragraph)>,
) {
    let Ok((mut tooltip_visibility, mut tooltip_node)) = tooltip_root.single_mut() else {
        return;
    };

    if !inventory_ui.open {
        *tooltip_visibility = Visibility::Hidden;
        return;
    }

    let Ok(window) = window_q.single() else {
        *tooltip_visibility = Visibility::Hidden;
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        *tooltip_visibility = Visibility::Hidden;
        return;
    };

    let resolved_recipe = recipe_registry.as_ref().and_then(|recipes| {
        recipe_type_registry
            .as_ref()
            .and_then(|types| resolve_hand_crafted_recipe(&hand_crafted, recipes, types, &item_registry))
    });

    let hovered_item_id = hovered_item_id(
        &slot_states,
        &inventory,
        &hand_crafted,
        &creative_panel,
        &recipe_preview,
        resolved_recipe.as_ref(),
    );
    let Some(item_id) = hovered_item_id else {
        *tooltip_visibility = Visibility::Hidden;
        return;
    };
    let Some(item) = item_registry.def_opt(item_id) else {
        *tooltip_visibility = Visibility::Hidden;
        return;
    };

    for (css_id, mut paragraph) in &mut tooltip_text {
        if css_id.0 == INVENTORY_TOOLTIP_NAME_ID {
            if paragraph.text != item.name {
                paragraph.text = item.name.clone();
            }
        } else if css_id.0 == INVENTORY_TOOLTIP_KEY_ID
            && paragraph.text != item.localized_name
        {
            paragraph.text = item.localized_name.clone();
        }
    }

    let offset = Vec2::new(14.0, 16.0);
    let mut tooltip_pos = cursor_pos + offset;
    tooltip_pos.x = tooltip_pos.x.clamp(0.0, (window.width() - 220.0).max(0.0));
    tooltip_pos.y = tooltip_pos.y.clamp(0.0, (window.height() - 72.0).max(0.0));
    tooltip_node.left = Val::Px(tooltip_pos.x);
    tooltip_node.top = Val::Px(tooltip_pos.y);

    *tooltip_visibility = Visibility::Inherited;
}

#[allow(clippy::too_many_arguments)]
fn sync_inventory_cursor_item_ui(
    inventory_ui: Res<PlayerInventoryUiState>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    cursor_item: Res<InventoryCursorItemState>,
    item_registry: Res<ItemRegistry>,
    block_registry: Res<BlockRegistry>,
    asset_server: Res<AssetServer>,
    mut image_cache: ResMut<ImageCache>,
    mut images: ResMut<Assets<Image>>,
    mut cursor_root: Query<(&mut Visibility, &mut Node), With<InventoryCursorItemRoot>>,
    mut cursor_icon: Query<&mut ImageNode, With<InventoryCursorItemIcon>>,
    mut cursor_badges: Query<
        (&mut Paragraph, &mut Visibility),
        (With<InventoryCursorItemBadge>, Without<InventoryCursorItemRoot>),
    >,
) {
    let Ok((mut root_visibility, mut root_node)) = cursor_root.single_mut() else {
        return;
    };
    let Ok(mut icon_node) = cursor_icon.single_mut() else {
        return;
    };

    if !inventory_ui.open || cursor_item.slot.is_empty() {
        *root_visibility = Visibility::Hidden;
        for (mut paragraph, mut badge_visibility) in &mut cursor_badges {
            if !paragraph.text.is_empty() {
                paragraph.text.clear();
            }
            *badge_visibility = Visibility::Hidden;
        }
        return;
    }

    let Ok(window) = window_q.single() else {
        *root_visibility = Visibility::Hidden;
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        *root_visibility = Visibility::Hidden;
        return;
    };

    let icon_path = resolve_item_icon_path(
        &item_registry,
        &block_registry,
        &asset_server,
        &mut image_cache,
        &mut images,
        cursor_item.slot.item_id,
    );
    let Some(icon_path) = icon_path else {
        *root_visibility = Visibility::Hidden;
        return;
    };

    let next_icon_handle = image_cache
        .map
        .get(icon_path.as_str())
        .cloned()
        .unwrap_or_else(|| asset_server.load(icon_path));
    if icon_node.image != next_icon_handle {
        icon_node.image = next_icon_handle;
    }

    let slot_half = 28.0;
    let max_x = (window.width() - 56.0).max(0.0);
    let max_y = (window.height() - 56.0).max(0.0);
    root_node.left = Val::Px((cursor_pos.x - slot_half).clamp(0.0, max_x));
    root_node.top = Val::Px((cursor_pos.y - slot_half).clamp(0.0, max_y));

    let count_text = cursor_item.slot.count.to_string();
    for (mut paragraph, mut badge_visibility) in &mut cursor_badges {
        if paragraph.text != count_text {
            paragraph.text = count_text.clone();
        }
        *badge_visibility = Visibility::Inherited;
    }

    *root_visibility = Visibility::Inherited;
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
) -> Option<InventoryUiSlotTarget> {
    let mut hovered_slot = None;

    for (css_id, state, mut border) in slot_frames.iter_mut() {
        let is_recipe_preview_slot = parse_recipe_preview_input_index(css_id.0.as_str()).is_some()
            || css_id.0 == RECIPE_PREVIEW_RESULT_FRAME_ID;

        let slot_target = if css_id.0 == HAND_CRAFTED_RESULT_FRAME_ID {
            Some(InventoryUiSlotTarget::HandCraftedResult)
        } else if let Some(slot_number) = css_id.0.strip_prefix(PLAYER_INVENTORY_FRAME_PREFIX) {
            slot_number
                .parse::<usize>()
                .ok()
                .and_then(|value| value.checked_sub(1))
                .filter(|slot_index| *slot_index < PLAYER_INVENTORY_SLOTS)
                .map(InventoryUiSlotTarget::Player)
        } else if let Some(slot_number) = css_id.0.strip_prefix(HAND_CRAFTED_FRAME_PREFIX) {
            slot_number
                .parse::<usize>()
                .ok()
                .and_then(|value| value.checked_sub(1))
                .filter(|slot_index| *slot_index < HAND_CRAFTED_INPUT_SLOTS)
                .map(InventoryUiSlotTarget::HandCrafted)
        } else {
            None
        };

        if slot_target.is_none() && !is_recipe_preview_slot {
            continue;
        }

        if hovered_slot.is_none()
            && state.hovered
            && let Some(slot_target) = slot_target
        {
            hovered_slot = Some(slot_target);
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

fn transfer_player_slot_to_hand_crafted(
    slot_index: usize,
    inventory: &mut PlayerInventory,
    hand_crafted: &mut HandCraftedState,
) -> bool {
    if slot_index >= PLAYER_INVENTORY_SLOTS {
        return false;
    }
    let source_slot = inventory.slots[slot_index];
    if source_slot.is_empty() {
        return false;
    }

    let Some(free_index) = hand_crafted
        .input_slots
        .iter()
        .position(InventorySlot::is_empty)
    else {
        return false;
    };

    hand_crafted.input_slots[free_index] = source_slot;
    inventory.slots[slot_index] = InventorySlot::default();
    true
}

fn transfer_hand_crafted_slot_to_inventory(
    slot_index: usize,
    hand_crafted: &mut HandCraftedState,
    inventory: &mut PlayerInventory,
    item_registry: &ItemRegistry,
) -> bool {
    if slot_index >= HAND_CRAFTED_INPUT_SLOTS {
        return false;
    }
    let source_slot = hand_crafted.input_slots[slot_index];
    if source_slot.is_empty() {
        return false;
    }

    let leftover = inventory.add_item(source_slot.item_id, source_slot.count, item_registry);
    if leftover == source_slot.count {
        return false;
    }
    if leftover == 0 {
        hand_crafted.input_slots[slot_index] = InventorySlot::default();
    } else {
        hand_crafted.input_slots[slot_index].count = leftover;
    }
    true
}

fn hold_source_from_target(target: InventoryUiSlotTarget) -> Option<InventoryHoldSource> {
    match target {
        InventoryUiSlotTarget::Player(index) if index < PLAYER_INVENTORY_SLOTS => {
            Some(InventoryHoldSource::Player(index))
        }
        InventoryUiSlotTarget::HandCrafted(index) if index < HAND_CRAFTED_INPUT_SLOTS => {
            Some(InventoryHoldSource::HandCrafted(index))
        }
        _ => None,
    }
}

fn take_one_from_target_to_cursor(
    slot_target: InventoryUiSlotTarget,
    cursor_item: &mut InventoryCursorItemState,
    inventory: &mut PlayerInventory,
    hand_crafted: &mut HandCraftedState,
) -> bool {
    if !cursor_item.slot.is_empty() {
        return false;
    }

    match slot_target {
        InventoryUiSlotTarget::Player(index) if index < PLAYER_INVENTORY_SLOTS => {
            take_one_from_slot_to_cursor(&mut inventory.slots[index], &mut cursor_item.slot)
        }
        InventoryUiSlotTarget::HandCrafted(index) if index < HAND_CRAFTED_INPUT_SLOTS => {
            take_one_from_slot_to_cursor(&mut hand_crafted.input_slots[index], &mut cursor_item.slot)
        }
        _ => false,
    }
}

fn take_half_from_target_to_cursor(
    slot_target: InventoryUiSlotTarget,
    cursor_item: &mut InventoryCursorItemState,
    inventory: &mut PlayerInventory,
    hand_crafted: &mut HandCraftedState,
    item_registry: &ItemRegistry,
) -> bool {
    match slot_target {
        InventoryUiSlotTarget::Player(index) if index < PLAYER_INVENTORY_SLOTS => {
            take_half_from_slot_to_cursor(
                &mut inventory.slots[index],
                &mut cursor_item.slot,
                item_registry,
            )
        }
        InventoryUiSlotTarget::HandCrafted(index) if index < HAND_CRAFTED_INPUT_SLOTS => {
            take_half_from_slot_to_cursor(
                &mut hand_crafted.input_slots[index],
                &mut cursor_item.slot,
                item_registry,
            )
        }
        _ => false,
    }
}

fn place_one_from_cursor_on_target(
    slot_target: InventoryUiSlotTarget,
    cursor_item: &mut InventoryCursorItemState,
    inventory: &mut PlayerInventory,
    hand_crafted: &mut HandCraftedState,
    item_registry: &ItemRegistry,
) -> bool {
    match slot_target {
        InventoryUiSlotTarget::Player(index) if index < PLAYER_INVENTORY_SLOTS => place_one_from_cursor(
            &mut inventory.slots[index],
            &mut cursor_item.slot,
            item_registry,
        ),
        InventoryUiSlotTarget::HandCrafted(index) if index < HAND_CRAFTED_INPUT_SLOTS => {
            place_one_from_cursor(
                &mut hand_crafted.input_slots[index],
                &mut cursor_item.slot,
                item_registry,
            )
        }
        _ => false,
    }
}

fn place_all_from_cursor_on_target(
    slot_target: InventoryUiSlotTarget,
    cursor_item: &mut InventoryCursorItemState,
    inventory: &mut PlayerInventory,
    hand_crafted: &mut HandCraftedState,
    item_registry: &ItemRegistry,
) -> bool {
    match slot_target {
        InventoryUiSlotTarget::Player(index) if index < PLAYER_INVENTORY_SLOTS => place_all_from_cursor(
            &mut inventory.slots[index],
            &mut cursor_item.slot,
            item_registry,
        ),
        InventoryUiSlotTarget::HandCrafted(index) if index < HAND_CRAFTED_INPUT_SLOTS => {
            place_all_from_cursor(
                &mut hand_crafted.input_slots[index],
                &mut cursor_item.slot,
                item_registry,
            )
        }
        _ => false,
    }
}

fn pull_one_from_hold_source(
    source_slot: InventoryHoldSource,
    cursor_item: &mut InventoryCursorItemState,
    inventory: &mut PlayerInventory,
    hand_crafted: &mut HandCraftedState,
    item_registry: &ItemRegistry,
) -> bool {
    if cursor_item.slot.is_empty() {
        return false;
    }

    match source_slot {
        InventoryHoldSource::Player(index) if index < PLAYER_INVENTORY_SLOTS => pull_one_to_cursor_from_slot(
            &mut inventory.slots[index],
            &mut cursor_item.slot,
            item_registry,
        ),
        InventoryHoldSource::HandCrafted(index) if index < HAND_CRAFTED_INPUT_SLOTS => {
            pull_one_to_cursor_from_slot(
                &mut hand_crafted.input_slots[index],
                &mut cursor_item.slot,
                item_registry,
            )
        }
        _ => false,
    }
}

fn pull_one_to_cursor_from_slot(
    slot: &mut InventorySlot,
    cursor_slot: &mut InventorySlot,
    item_registry: &ItemRegistry,
) -> bool {
    if slot.is_empty() || cursor_slot.is_empty() || slot.item_id != cursor_slot.item_id {
        return false;
    }

    let stack_max = item_registry
        .stack_limit(cursor_slot.item_id)
        .min(PLAYER_INVENTORY_STACK_MAX)
        .max(1);
    if cursor_slot.count >= stack_max {
        return false;
    }

    cursor_slot.count += 1;
    decrement_slot_count(slot);
    true
}

fn take_one_from_slot_to_cursor(slot: &mut InventorySlot, cursor_slot: &mut InventorySlot) -> bool {
    if slot.is_empty() {
        return false;
    }

    cursor_slot.item_id = slot.item_id;
    cursor_slot.count = 1;
    decrement_slot_count(slot);
    true
}

fn take_half_from_slot_to_cursor(
    slot: &mut InventorySlot,
    cursor_slot: &mut InventorySlot,
    item_registry: &ItemRegistry,
) -> bool {
    if slot.is_empty() {
        return false;
    }
    if !cursor_slot.is_empty() && cursor_slot.item_id != slot.item_id {
        return false;
    }

    let stack_max = item_registry
        .stack_limit(slot.item_id)
        .min(PLAYER_INVENTORY_STACK_MAX)
        .max(1);
    let free_capacity = if cursor_slot.is_empty() {
        stack_max
    } else {
        stack_max.saturating_sub(cursor_slot.count)
    };
    if free_capacity == 0 {
        return false;
    }

    let mut take_count = slot.count.div_ceil(2);
    take_count = take_count.min(free_capacity);
    if take_count == 0 {
        return false;
    }

    if cursor_slot.is_empty() {
        cursor_slot.item_id = slot.item_id;
        cursor_slot.count = 0;
    }
    cursor_slot.count += take_count;
    slot.count = slot.count.saturating_sub(take_count);
    if slot.count == 0 {
        *slot = InventorySlot::default();
    }
    true
}

fn place_one_from_cursor(
    slot: &mut InventorySlot,
    cursor_slot: &mut InventorySlot,
    item_registry: &ItemRegistry,
) -> bool {
    if cursor_slot.is_empty() {
        return false;
    }

    if slot.is_empty() {
        slot.item_id = cursor_slot.item_id;
        slot.count = 1;
        decrement_slot_count(cursor_slot);
        return true;
    }

    if slot.item_id != cursor_slot.item_id {
        return false;
    }

    let stack_max = item_registry
        .stack_limit(cursor_slot.item_id)
        .min(PLAYER_INVENTORY_STACK_MAX)
        .max(1);
    if slot.count >= stack_max {
        return false;
    }

    slot.count += 1;
    decrement_slot_count(cursor_slot);
    true
}

fn place_all_from_cursor(
    slot: &mut InventorySlot,
    cursor_slot: &mut InventorySlot,
    item_registry: &ItemRegistry,
) -> bool {
    if cursor_slot.is_empty() {
        return false;
    }

    let stack_max = item_registry
        .stack_limit(cursor_slot.item_id)
        .min(PLAYER_INVENTORY_STACK_MAX)
        .max(1);

    if slot.is_empty() {
        let move_count = cursor_slot.count.min(stack_max);
        if move_count == 0 {
            return false;
        }
        slot.item_id = cursor_slot.item_id;
        slot.count = move_count;
        cursor_slot.count -= move_count;
        if cursor_slot.count == 0 {
            *cursor_slot = InventorySlot::default();
        }
        return true;
    }

    if slot.item_id != cursor_slot.item_id {
        std::mem::swap(slot, cursor_slot);
        return true;
    }

    let free_capacity = stack_max.saturating_sub(slot.count);
    if free_capacity == 0 {
        return false;
    }

    let move_count = cursor_slot.count.min(free_capacity);
    if move_count == 0 {
        return false;
    }
    slot.count += move_count;
    cursor_slot.count -= move_count;
    if cursor_slot.count == 0 {
        *cursor_slot = InventorySlot::default();
    }
    true
}

fn decrement_slot_count(slot: &mut InventorySlot) {
    if slot.count <= 1 {
        *slot = InventorySlot::default();
    } else {
        slot.count -= 1;
    }
}

fn parse_recipe_preview_input_index(css_id: &str) -> Option<usize> {
    css_id
        .strip_prefix(RECIPE_PREVIEW_INPUT_FRAME_PREFIX)
        .and_then(|slot_number| slot_number.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
        .filter(|index| *index < HAND_CRAFTED_INPUT_SLOTS)
}

fn hovered_item_id(
    slot_states: &Query<(&CssID, &UIWidgetState), With<Button>>,
    inventory: &PlayerInventory,
    hand_crafted: &HandCraftedState,
    creative_panel: &CreativePanelState,
    recipe_preview: &RecipePreviewDialogState,
    resolved_recipe: Option<&ResolvedRecipe>,
) -> Option<ItemId> {
    for (css_id, state) in slot_states.iter() {
        if !state.hovered {
            continue;
        }

        if let Some(slot_number) = css_id.0.strip_prefix(PLAYER_INVENTORY_FRAME_PREFIX)
            && let Ok(slot_index) = slot_number.parse::<usize>()
            && let Some(slot) = inventory.slots.get(slot_index.saturating_sub(1))
            && !slot.is_empty()
        {
            return Some(slot.item_id);
        }

        if let Some(slot_number) = css_id.0.strip_prefix(HAND_CRAFTED_FRAME_PREFIX)
            && let Ok(slot_index) = slot_number.parse::<usize>()
            && let Some(slot) = hand_crafted.input_slots.get(slot_index.saturating_sub(1))
            && !slot.is_empty()
        {
            return Some(slot.item_id);
        }

        if css_id.0 == HAND_CRAFTED_RESULT_FRAME_ID
            && let Some(recipe) = resolved_recipe
        {
            return Some(recipe.result.item_id);
        }

        if let Some(slot_number) = css_id.0.strip_prefix(CREATIVE_PANEL_SLOT_PREFIX)
            && let Ok(slot_index) = slot_number.parse::<usize>()
            && let Some(index) = slot_index.checked_sub(1)
            && index < CREATIVE_PANEL_PAGE_SIZE
            && let Some(item_id) = creative_panel.item_at_page_slot(index)
        {
            return Some(item_id);
        }

        if recipe_preview.open
            && let Some(index) = parse_recipe_preview_input_index(css_id.0.as_str())
            && let Some(slot) = recipe_preview.input_slots.get(index)
            && !slot.is_empty()
        {
            return Some(slot.item_id);
        }

        if recipe_preview.open
            && css_id.0 == RECIPE_PREVIEW_RESULT_FRAME_ID
            && !recipe_preview.result_slot.is_empty()
        {
            return Some(recipe_preview.result_slot.item_id);
        }
    }

    None
}

fn flush_hand_crafted_inputs_to_inventory(
    hand_crafted: &mut HandCraftedState,
    inventory: &mut PlayerInventory,
    item_registry: &ItemRegistry,
) {
    for slot in &mut hand_crafted.input_slots {
        if slot.is_empty() {
            continue;
        }
        let original = slot.count;
        let leftover = inventory.add_item(slot.item_id, slot.count, item_registry);
        if leftover == 0 {
            *slot = InventorySlot::default();
        } else {
            slot.count = leftover;
            debug!(
                "Could not fully return hand_crafted input to inventory (item_id={}, returned={}, leftover={})",
                slot.item_id,
                original.saturating_sub(leftover),
                leftover
            );
        }
    }
}

fn flush_cursor_item_to_inventory(
    cursor_item: &mut InventoryCursorItemState,
    inventory: &mut PlayerInventory,
    item_registry: &ItemRegistry,
) {
    if cursor_item.slot.is_empty() {
        return;
    }

    let original_count = cursor_item.slot.count;
    let leftover = inventory.add_item(
        cursor_item.slot.item_id,
        cursor_item.slot.count,
        item_registry,
    );
    if leftover == 0 {
        cursor_item.slot = InventorySlot::default();
    } else {
        cursor_item.slot.count = leftover;
        debug!(
            "Could not fully return cursor-held item to inventory (item_id={}, returned={}, leftover={})",
            cursor_item.slot.item_id,
            original_count.saturating_sub(leftover),
            leftover
        );
    }
}

fn fill_hand_crafted_from_recipe_preview(
    recipe_preview: &RecipePreviewDialogState,
    inventory: &mut PlayerInventory,
    hand_crafted: &mut HandCraftedState,
    item_registry: &ItemRegistry,
) {
    for slot_index in 0..HAND_CRAFTED_INPUT_SLOTS {
        let required = recipe_preview.input_slots[slot_index];
        if required.is_empty() {
            continue;
        }

        let target = &mut hand_crafted.input_slots[slot_index];
        if target.is_empty() {
            let moved = take_items_from_inventory(inventory, required.item_id, required.count);
            if moved > 0 {
                target.item_id = required.item_id;
                target.count = moved;
            }
            continue;
        }

        if target.item_id != required.item_id {
            continue;
        }

        let stack_max = item_registry
            .stack_limit(required.item_id)
            .min(PLAYER_INVENTORY_STACK_MAX)
            .max(1);
        let desired_count = required.count.min(stack_max);
        if target.count >= desired_count {
            continue;
        }

        let missing = desired_count.saturating_sub(target.count);
        let moved = take_items_from_inventory(inventory, required.item_id, missing);
        target.count = target.count.saturating_add(moved).min(stack_max);
    }
}

fn take_items_from_inventory(
    inventory: &mut PlayerInventory,
    item_id: ItemId,
    mut amount: u16,
) -> u16 {
    if item_id == 0 || amount == 0 {
        return 0;
    }

    let mut moved = 0u16;
    for slot in &mut inventory.slots {
        if amount == 0 {
            break;
        }
        if slot.is_empty() || slot.item_id != item_id {
            continue;
        }

        let take = slot.count.min(amount);
        slot.count -= take;
        moved += take;
        amount -= take;
        if slot.count == 0 {
            *slot = InventorySlot::default();
        }
    }

    moved
}

fn sync_badge(
    paragraph: &mut Paragraph,
    visibility: &mut Mut<'_, Visibility>,
    count: u16,
    empty: bool,
) {
    if empty || count == 0 {
        if !paragraph.text.is_empty() {
            paragraph.text.clear();
        }
        **visibility = Visibility::Hidden;
        return;
    }

    let next_text = count.to_string();
    if paragraph.text != next_text {
        paragraph.text = next_text;
    }
    **visibility = Visibility::Inherited;
}

fn is_button_hovered(
    button_states: &Query<(&CssID, &UIWidgetState), With<Button>>,
    css_id_target: &str,
) -> bool {
    button_states
        .iter()
        .any(|(css_id, state)| css_id.0 == css_id_target && state.hovered)
}

fn is_cursor_inside_panel<F: QueryFilter>(
    window_q: &Query<&Window, With<PrimaryWindow>>,
    panel_q: &Query<(&ComputedNode, &GlobalTransform), F>,
) -> bool {
    let Ok(window) = window_q.single() else {
        return false;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return false;
    };
    let Ok((computed_node, transform)) = panel_q.single() else {
        return false;
    };

    let size = computed_node.size();
    if size.x <= 0.0 || size.y <= 0.0 {
        return false;
    }

    let cursor_ui = Vec2::new(
        cursor_pos.x - (window.width() * 0.5),
        (window.height() * 0.5) - cursor_pos.y,
    );
    let center = transform.translation().truncate();
    let half = size * 0.5;

    cursor_ui.x >= center.x - half.x
        && cursor_ui.x <= center.x + half.x
        && cursor_ui.y >= center.y - half.y
        && cursor_ui.y <= center.y + half.y
}
