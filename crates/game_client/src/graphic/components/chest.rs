fn handle_open_chest_inventory_menu_request(
    mut requests: MessageReader<OpenChestInventoryMenuRequest>,
    ui_interaction: Res<UiInteractionState>,
    mut chest_menu: ResMut<ChestInventoryMenuState>,
    mut chest_inventory: ResMut<ChestInventoryUiState>,
    mut auto_persist: ResMut<ChestInventoryAutoPersistState>,
    mut recipe_preview: ResMut<RecipePreviewDialogState>,
    mut opened: MessageWriter<ChestInventoryUiOpened>,
) {
    let mut requested_world_pos = None;
    for request in requests.read() {
        requested_world_pos = Some(request.world_pos);
    }
    let Some(world_pos) = requested_world_pos else {
        return;
    };
    if ui_interaction.blocks_game_input() {
        return;
    }

    chest_menu.open = true;
    chest_menu.world_pos = Some(world_pos);
    chest_inventory.slots = [InventorySlot::default(); CHEST_INVENTORY_SLOTS];
    auto_persist.chest_loaded = false;
    auto_persist.last_chest_world_pos = Some(world_pos);
    auto_persist.last_chest_slots.clear();
    recipe_preview.open = false;
    opened.write(ChestInventoryUiOpened { world_pos });
}

fn handle_chest_inventory_contents_sync(
    mut sync_messages: MessageReader<ChestInventoryContentsSync>,
    chest_menu: Res<ChestInventoryMenuState>,
    workbench_menu: Res<WorkbenchRecipeMenuState>,
    item_registry: Res<ItemRegistry>,
    mut chest_inventory: ResMut<ChestInventoryUiState>,
    mut auto_persist: ResMut<ChestInventoryAutoPersistState>,
) {
    let active_chest_world_pos = if chest_menu.open {
        chest_menu.world_pos
    } else {
        None
    };
    let active_workbench_storage_world_pos = if workbench_menu.open {
        Some((
            workbench_menu.storage_left_world_pos,
            workbench_menu.storage_right_world_pos,
        ))
    } else {
        None
    };
    for sync in sync_messages.read() {
        let applies_to_chest = active_chest_world_pos == Some(sync.world_pos);
        if applies_to_chest {
            apply_chest_slots_from_sync(&mut chest_inventory.slots, sync, &item_registry);
            auto_persist.chest_loaded = true;
            auto_persist.last_chest_world_pos = Some(sync.world_pos);
            auto_persist.last_chest_slots = sync.slots.clone();
            continue;
        }
        if let Some((left_world_pos, right_world_pos)) = active_workbench_storage_world_pos {
            if left_world_pos == Some(sync.world_pos) {
                apply_chest_slots_from_sync(
                    chest_inventory.workbench_slots_mut(WorkbenchStorageSide::Left),
                    sync,
                    &item_registry,
                );
                auto_persist.workbench_left_loaded = true;
                auto_persist.last_workbench_left_world_pos = Some(sync.world_pos);
                auto_persist.last_workbench_left_slots = sync.slots.clone();
                continue;
            }
            if right_world_pos == Some(sync.world_pos) {
                apply_chest_slots_from_sync(
                    chest_inventory.workbench_slots_mut(WorkbenchStorageSide::Right),
                    sync,
                    &item_registry,
                );
                auto_persist.workbench_right_loaded = true;
                auto_persist.last_workbench_right_world_pos = Some(sync.world_pos);
                auto_persist.last_workbench_right_slots = sync.slots.clone();
            }
        }
    }
}

fn cache_chest_inventory_contents_sync(
    mut sync_messages: MessageReader<ChestInventoryContentsSync>,
    mut cache: ResMut<ChestInventorySnapshotCache>,
) {
    for sync in sync_messages.read() {
        cache
            .slots_by_world_pos
            .insert(sync.world_pos, sync.slots.clone());
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_chest_inventory_menu_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    global_config: Res<GlobalConfig>,
    item_registry: Res<ItemRegistry>,
    mut chest_menu: ResMut<ChestInventoryMenuState>,
    mut chest_inventory: ResMut<ChestInventoryUiState>,
    mut cursor_item: ResMut<InventoryCursorItemState>,
    mut recipe_preview: ResMut<RecipePreviewDialogState>,
    mut inventory: ResMut<PlayerInventory>,
    mut closed: MessageWriter<ChestInventoryUiClosed>,
    mut persist_requests: MessageWriter<ChestInventoryPersistRequest>,
) {
    if !chest_menu.open {
        return;
    }

    let close_key =
        convert(global_config.input.ui_close_back.as_str()).expect("Invalid close/back key");
    if !keyboard.just_pressed(close_key) {
        return;
    }
    if recipe_preview.open {
        recipe_preview.open = false;
        return;
    }

    flush_cursor_item_to_inventory(&mut cursor_item, &mut inventory, &item_registry);
    if let Some(world_pos) = chest_menu.world_pos {
        persist_requests.write(ChestInventoryPersistRequest {
            world_pos,
            slots: serialize_chest_slots_for_persist(&chest_inventory.slots, &item_registry),
        });
        closed.write(ChestInventoryUiClosed { world_pos });
    }
    chest_inventory.slots = [InventorySlot::default(); CHEST_INVENTORY_SLOTS];
    chest_menu.open = false;
    chest_menu.world_pos = None;
}

fn handle_chest_inventory_menu_navigation(
    mouse: Res<ButtonInput<MouseButton>>,
    chest_menu: Res<ChestInventoryMenuState>,
    mut creative_panel: ResMut<CreativePanelState>,
    button_states: Query<(&CssID, &UIWidgetState), With<Button>>,
) {
    if !chest_menu.open || !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let mut hovered_button: Option<&str> = None;
    for (css_id, state) in &button_states {
        if state.hovered {
            hovered_button = Some(css_id.0.as_str());
            break;
        }
    }
    let Some(css_id) = hovered_button else {
        return;
    };

    if css_id == CHEST_ITEMS_PREV_ID {
        let _ = creative_panel.prev_page();
        return;
    }
    if css_id == CHEST_ITEMS_NEXT_ID {
        let _ = creative_panel.next_page();
    }
}

fn cache_chest_inventory_persist_requests(
    mut requests: MessageReader<ChestInventoryPersistRequest>,
    mut cache: ResMut<ChestInventorySnapshotCache>,
) {
    for request in requests.read() {
        cache
            .slots_by_world_pos
            .insert(request.world_pos, request.slots.clone());
    }
}

fn prune_chest_inventory_snapshot_cache(
    mut local_breaks: MessageReader<BlockBreakByPlayerEvent>,
    mut observed_breaks: MessageReader<BlockBreakObservedEvent>,
    mut local_places: MessageReader<BlockPlaceByPlayerEvent>,
    mut observed_places: MessageReader<BlockPlaceObservedEvent>,
    mut cache: ResMut<ChestInventorySnapshotCache>,
) {
    for event in local_breaks.read() {
        cache.slots_by_world_pos.remove(&event.location.to_array());
    }
    for event in observed_breaks.read() {
        cache.slots_by_world_pos.remove(&event.location.to_array());
    }
    for event in local_places.read() {
        cache.slots_by_world_pos.remove(&event.location.to_array());
    }
    for event in observed_places.read() {
        cache.slots_by_world_pos.remove(&event.location.to_array());
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_chest_inventory_menu_item_clicks(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    chest_menu: Res<ChestInventoryMenuState>,
    game_mode: Res<GameModeState>,
    creative_panel: Res<CreativePanelState>,
    recipe_registry: Option<Res<RecipeRegistry>>,
    item_registry: Res<ItemRegistry>,
    cursor_item: Res<InventoryCursorItemState>,
    mut inventory: ResMut<PlayerInventory>,
    mut recipe_preview: ResMut<RecipePreviewDialogState>,
    mut slot_frames: Query<(&CssID, &UIWidgetState, &mut BorderColor), With<Button>>,
) {
    let hovered_slot = sync_chest_item_slot_hover_border(&mut slot_frames, chest_menu.open);

    if !chest_menu.open || !mouse.just_pressed(MouseButton::Left) || !cursor_item.slot.is_empty() {
        return;
    }

    let Some(slot_index) = hovered_slot else {
        return;
    };
    let Some(item_id) = creative_panel.item_at_page_slot(slot_index) else {
        return;
    };

    match game_mode.0 {
        GameMode::Creative => {
            recipe_preview.open = false;
            let grant_full_stack = keyboard.pressed(KeyCode::ShiftLeft);
            let _ = crate::handlers::inventory::apply_creative_panel_click(
                &game_mode,
                item_id,
                grant_full_stack,
                &mut inventory,
                &item_registry,
            );
        }
        GameMode::Survival => {
            let Some(recipe_registry) = recipe_registry.as_ref() else {
                recipe_preview.open = false;
                return;
            };
            let _ = open_recipe_preview_dialog_for_hovered_item(
                item_id,
                recipe_registry,
                &item_registry,
                &mut recipe_preview,
            );
        }
        GameMode::Spectator => {
            recipe_preview.open = false;
        }
    }
}

#[derive(SystemParam)]
struct ChestInventoryUiSyncDeps<'w, 's> {
    game_mode: Res<'w, GameModeState>,
    creative_panel: Res<'w, CreativePanelState>,
    inventory: Res<'w, PlayerInventory>,
    chest_inventory: Res<'w, ChestInventoryUiState>,
    item_registry: Res<'w, ItemRegistry>,
    block_registry: Res<'w, BlockRegistry>,
    language: Res<'w, ClientLanguageState>,
    asset_server: Res<'w, AssetServer>,
    image_cache: ResMut<'w, ImageCache>,
    images: ResMut<'w, Assets<Image>>,
    root_q: Query<'w, 's, &'static mut Visibility, With<ChestInventoryRoot>>,
    paragraph_q: Query<
        'w,
        's,
        (
            &'static CssID,
            &'static mut Paragraph,
            Option<&'static mut Visibility>,
        ),
        Without<ChestInventoryRoot>,
    >,
    button_q: Query<'w, 's, (&'static CssID, &'static mut Button), With<Button>>,
}

fn sync_chest_inventory_menu_ui(
    chest_menu: Res<ChestInventoryMenuState>,
    deps: ChestInventoryUiSyncDeps,
) {
    let ChestInventoryUiSyncDeps {
        game_mode,
        creative_panel,
        inventory,
        chest_inventory,
        item_registry,
        block_registry,
        language,
        asset_server,
        mut image_cache,
        mut images,
        mut root_q,
        mut paragraph_q,
        mut button_q,
    } = deps;

    if let Ok(mut visibility) = root_q.single_mut() {
        *visibility = if chest_menu.open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for (css_id, mut paragraph, mut maybe_visibility) in &mut paragraph_q {
        if css_id.0 == CHEST_INVENTORY_TITLE_ID {
            paragraph.text = language.localize_name_key("KEY_UI_CHEST_INVENTORY");
            continue;
        }
        if css_id.0 == CHEST_INVENTORY_HINT_ID {
            paragraph.text = match game_mode.0 {
                GameMode::Creative => language.localize_name_key("KEY_UI_RECIPE_HINT_CREATIVE"),
                GameMode::Survival => language.localize_name_key("KEY_UI_CHEST_HINT"),
                GameMode::Spectator => {
                    language.localize_name_key("KEY_UI_RECIPE_HINT_SPECTATOR")
                }
            };
            continue;
        }
        if css_id.0 == CHEST_ITEMS_TOTAL_ID {
            paragraph.text = format!(
                "{} {}",
                language.localize_name_key("KEY_UI_REGISTERED"),
                creative_panel.item_count()
            );
            continue;
        }
        if css_id.0 == CHEST_ITEMS_PAGE_ID {
            paragraph.text = creative_panel.page_label();
            continue;
        }

        if let Some(slot_index) = parse_chest_badge_index(css_id.0.as_str())
            && let Some(slot) = chest_inventory.slots.get(slot_index)
            && let Some(visibility) = maybe_visibility.as_mut()
        {
            sync_badge(
                &mut paragraph,
                visibility,
                slot.count,
                slot.is_empty() || !chest_menu.open,
            );
            continue;
        }

        if let Some(slot_index) = parse_chest_player_inventory_badge_index(css_id.0.as_str())
            && let Some(slot) = inventory.slots.get(slot_index)
            && let Some(visibility) = maybe_visibility.as_mut()
        {
            sync_badge(
                &mut paragraph,
                visibility,
                slot.count,
                slot.is_empty() || !chest_menu.open,
            );
        }
    }

    for (css_id, mut button) in &mut button_q {
        if let Some(slot_index) = parse_chest_slot_index(css_id.0.as_str()) {
            if !button.text.is_empty() {
                button.text.clear();
            }
            let slot = chest_inventory
                .slots
                .get(slot_index)
                .copied()
                .unwrap_or_default();
            let next_icon = if !chest_menu.open || slot.is_empty() {
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

        if let Some(slot_index) = parse_chest_player_inventory_slot_index(css_id.0.as_str()) {
            if !button.text.is_empty() {
                button.text.clear();
            }
            let slot = inventory.slots.get(slot_index).copied().unwrap_or_default();
            let next_icon = if !chest_menu.open || slot.is_empty() {
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

        if let Some(slot_index) = parse_chest_item_slot_index(css_id.0.as_str()) {
            if !button.text.is_empty() {
                button.text.clear();
            }
            let next_icon = if !chest_menu.open {
                None
            } else {
                creative_panel.item_at_page_slot(slot_index).and_then(|item_id| {
                    resolve_item_icon_path(
                        &item_registry,
                        &block_registry,
                        &asset_server,
                        &mut image_cache,
                        &mut images,
                        item_id,
                    )
                })
            };
            if button.icon_path != next_icon {
                button.icon_path = next_icon;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn close_chest_inventory_menu_ui(
    mut ui_interaction: ResMut<UiInteractionState>,
    mut chest_menu: ResMut<ChestInventoryMenuState>,
    mut chest_inventory: ResMut<ChestInventoryUiState>,
    mut auto_persist: ResMut<ChestInventoryAutoPersistState>,
    mut cursor_item: ResMut<InventoryCursorItemState>,
    mut inventory: ResMut<PlayerInventory>,
    item_registry: Option<Res<ItemRegistry>>,
    mut root_q: Query<&mut Visibility, With<ChestInventoryRoot>>,
    mut closed: MessageWriter<ChestInventoryUiClosed>,
    mut persist_requests: MessageWriter<ChestInventoryPersistRequest>,
) {
    if chest_menu.open
        && let Some(world_pos) = chest_menu.world_pos
    {
        if let Some(item_registry) = item_registry.as_ref() {
            flush_cursor_item_to_inventory(&mut cursor_item, &mut inventory, item_registry);
            persist_requests.write(ChestInventoryPersistRequest {
                world_pos,
                slots: serialize_chest_slots_for_persist(&chest_inventory.slots, item_registry),
            });
        }
        closed.write(ChestInventoryUiClosed { world_pos });
    }

    chest_menu.open = false;
    chest_menu.world_pos = None;
    chest_inventory.slots = [InventorySlot::default(); CHEST_INVENTORY_SLOTS];
    auto_persist.chest_loaded = false;
    auto_persist.last_chest_world_pos = None;
    auto_persist.last_chest_slots.clear();
    ui_interaction.chest_menu_open = false;
    if let Ok(mut visibility) = root_q.single_mut() {
        *visibility = Visibility::Hidden;
    }
}

fn serialize_chest_slots_for_persist(
    slots: &[InventorySlot; CHEST_INVENTORY_SLOTS],
    item_registry: &ItemRegistry,
) -> Vec<ChestInventorySlotPayload> {
    let mut payload = Vec::new();
    for (slot_index, slot) in slots.iter().copied().enumerate() {
        if slot.is_empty() || slot.count == 0 {
            continue;
        }
        let Some(item) = item_registry.def_opt(slot.item_id) else {
            continue;
        };
        payload.push(ChestInventorySlotPayload {
            slot: slot_index as u16,
            item: item.localized_name.clone(),
            count: slot.count.max(1),
        });
    }
    payload
}

fn apply_chest_slots_from_sync(
    slots: &mut [InventorySlot; CHEST_INVENTORY_SLOTS],
    sync: &ChestInventoryContentsSync,
    item_registry: &ItemRegistry,
) {
    *slots = [InventorySlot::default(); CHEST_INVENTORY_SLOTS];
    for slot_payload in &sync.slots {
        let slot_index = slot_payload.slot as usize;
        if slot_index >= CHEST_INVENTORY_SLOTS || slot_payload.count == 0 {
            continue;
        }
        let item_name = slot_payload.item.trim();
        if item_name.is_empty() {
            continue;
        }
        let Some(item_id) = item_registry.id_opt(item_name) else {
            continue;
        };
        slots[slot_index] = InventorySlot {
            item_id,
            count: slot_payload.count.max(1),
        };
    }
}

fn clear_chest_inventory_snapshot_cache(mut cache: ResMut<ChestInventorySnapshotCache>) {
    cache.slots_by_world_pos.clear();
}

fn auto_persist_open_container_inventories(
    item_registry: Res<ItemRegistry>,
    chest_menu: Res<ChestInventoryMenuState>,
    workbench_menu: Res<WorkbenchRecipeMenuState>,
    chest_inventory: Res<ChestInventoryUiState>,
    mut auto_persist: ResMut<ChestInventoryAutoPersistState>,
    mut persist_requests: MessageWriter<ChestInventoryPersistRequest>,
) {
    if chest_menu.open {
        let world_pos = chest_menu.world_pos;
        let slots = serialize_chest_slots_for_persist(&chest_inventory.slots, &item_registry);
        if auto_persist.chest_loaded
            && (world_pos != auto_persist.last_chest_world_pos
                || slots != auto_persist.last_chest_slots)
        {
            if let Some(world_pos) = world_pos {
                persist_requests.write(ChestInventoryPersistRequest { world_pos, slots: slots.clone() });
            }
        }
        auto_persist.last_chest_world_pos = world_pos;
        auto_persist.last_chest_slots = slots;
    } else {
        auto_persist.chest_loaded = false;
        auto_persist.last_chest_world_pos = None;
        auto_persist.last_chest_slots.clear();
    }

    let left_world_pos = if workbench_menu.open {
        workbench_menu.storage_left_world_pos
    } else {
        None
    };
    let left_slots =
        serialize_chest_slots_for_persist(&chest_inventory.workbench_left_slots, &item_registry);
    if auto_persist.workbench_left_loaded
        && (left_world_pos != auto_persist.last_workbench_left_world_pos
            || left_slots != auto_persist.last_workbench_left_slots)
    {
        if let Some(world_pos) = left_world_pos {
            persist_requests.write(ChestInventoryPersistRequest {
                world_pos,
                slots: left_slots.clone(),
            });
        }
    }
    auto_persist.last_workbench_left_world_pos = left_world_pos;
    auto_persist.last_workbench_left_slots = left_slots;
    if left_world_pos.is_none() {
        auto_persist.workbench_left_loaded = false;
    }

    let right_world_pos = if workbench_menu.open {
        workbench_menu.storage_right_world_pos
    } else {
        None
    };
    let right_slots =
        serialize_chest_slots_for_persist(&chest_inventory.workbench_right_slots, &item_registry);
    if auto_persist.workbench_right_loaded
        && (right_world_pos != auto_persist.last_workbench_right_world_pos
            || right_slots != auto_persist.last_workbench_right_slots)
    {
        if let Some(world_pos) = right_world_pos {
            persist_requests.write(ChestInventoryPersistRequest {
                world_pos,
                slots: right_slots.clone(),
            });
        }
    }
    auto_persist.last_workbench_right_world_pos = right_world_pos;
    auto_persist.last_workbench_right_slots = right_slots;
    if right_world_pos.is_none() {
        auto_persist.workbench_right_loaded = false;
    }
}

fn parse_chest_slot_index(css_id: &str) -> Option<usize> {
    css_id
        .strip_prefix(CHEST_SLOT_FRAME_PREFIX)
        .and_then(|slot_number| slot_number.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
        .filter(|index| *index < CHEST_INVENTORY_SLOTS)
}

fn parse_chest_badge_index(css_id: &str) -> Option<usize> {
    css_id
        .strip_prefix(CHEST_SLOT_BADGE_PREFIX)
        .and_then(|slot_number| slot_number.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
        .filter(|index| *index < CHEST_INVENTORY_SLOTS)
}

fn parse_chest_player_inventory_slot_index(css_id: &str) -> Option<usize> {
    css_id
        .strip_prefix(CHEST_PLAYER_INVENTORY_FRAME_PREFIX)
        .and_then(|slot_number| slot_number.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
        .filter(|index| *index < PLAYER_INVENTORY_SLOTS)
}

fn parse_chest_player_inventory_badge_index(css_id: &str) -> Option<usize> {
    css_id
        .strip_prefix(CHEST_PLAYER_INVENTORY_BADGE_PREFIX)
        .and_then(|slot_number| slot_number.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
        .filter(|index| *index < PLAYER_INVENTORY_SLOTS)
}

fn parse_chest_item_slot_index(css_id: &str) -> Option<usize> {
    css_id
        .strip_prefix(CHEST_ITEMS_SLOT_PREFIX)
        .and_then(|slot_number| slot_number.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
        .filter(|index| *index < CREATIVE_PANEL_PAGE_SIZE)
}

fn sync_chest_item_slot_hover_border(
    slot_frames: &mut Query<(&CssID, &UIWidgetState, &mut BorderColor), With<Button>>,
    chest_open: bool,
) -> Option<usize> {
    let mut hovered_slot = None;
    for (css_id, state, mut border) in slot_frames.iter_mut() {
        let Some(slot_index) = parse_chest_item_slot_index(css_id.0.as_str()) else {
            continue;
        };

        if hovered_slot.is_none() && state.hovered {
            hovered_slot = Some(slot_index);
        }

        let color = if chest_open && state.hovered {
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
