fn handle_open_workbench_recipe_menu_request(
    mut requests: MessageReader<OpenWorkbenchMenuRequest>,
    ui_interaction: Res<UiInteractionState>,
    chunk_map: Res<ChunkMap>,
    block_registry: Res<BlockRegistry>,
    item_registry: Res<ItemRegistry>,
    structure_recipe_registry: Option<Res<BuildingStructureRecipeRegistry>>,
    structure_runtime: Option<Res<crate::logic::events::block_event_handler::StructureRuntimeState>>,
    mut workbench_menu: ResMut<WorkbenchRecipeMenuState>,
    mut chest_inventory: ResMut<ChestInventoryUiState>,
    mut auto_persist: ResMut<ChestInventoryAutoPersistState>,
    mut craft_progress: ResMut<WorkbenchCraftProgressState>,
    mut structure_menu: ResMut<StructureBuildMenuState>,
    mut active_structure_recipe: ResMut<ActiveStructureRecipeState>,
    mut active_structure_placement: ResMut<ActiveStructurePlacementState>,
    mut recipe_preview: ResMut<RecipePreviewDialogState>,
    mut opened: MessageWriter<ChestInventoryUiOpened>,
) {
    let mut requested_world_pos = None;
    for request in requests.read() {
        requested_world_pos = Some(request.world_pos);
    }
    let Some(workbench_world_pos) = requested_world_pos else {
        return;
    };
    if ui_interaction.menu_open
        || ui_interaction.chat_open
        || ui_interaction.inventory_open
        || ui_interaction.chest_menu_open
    {
        return;
    }

    // Block UI has priority over hammer UI while targeting the workbench.
    structure_menu.open = false;
    active_structure_recipe.selected_recipe_name = None;
    active_structure_placement.rotation_quarters = 0;
    reset_workbench_craft_progress(&mut craft_progress);
    recipe_preview.open = false;
    workbench_menu.world_pos = Some(workbench_world_pos);
    let storage_neighbors = find_adjacent_storage_world_pos(
        workbench_world_pos,
        &chunk_map,
        &block_registry,
        &item_registry,
        structure_runtime.as_deref(),
        structure_recipe_registry.as_deref(),
    );
    workbench_menu.storage_left_world_pos = storage_neighbors.left;
    workbench_menu.storage_right_world_pos = storage_neighbors.right;
    chest_inventory.workbench_left_slots = [InventorySlot::default(); CHEST_INVENTORY_SLOTS];
    chest_inventory.workbench_right_slots = [InventorySlot::default(); CHEST_INVENTORY_SLOTS];
    auto_persist.workbench_left_loaded = false;
    auto_persist.workbench_right_loaded = false;
    auto_persist.last_workbench_left_world_pos = workbench_menu.storage_left_world_pos;
    auto_persist.last_workbench_right_world_pos = workbench_menu.storage_right_world_pos;
    auto_persist.last_workbench_left_slots.clear();
    auto_persist.last_workbench_right_slots.clear();
    if let Some(storage_world_pos) = workbench_menu.storage_left_world_pos {
        opened.write(ChestInventoryUiOpened {
            world_pos: storage_world_pos,
        });
    }
    if let Some(storage_world_pos) = workbench_menu.storage_right_world_pos {
        opened.write(ChestInventoryUiOpened {
            world_pos: storage_world_pos,
        });
    }
    workbench_menu.open = true;
}

#[allow(clippy::too_many_arguments)]
fn handle_workbench_recipe_menu_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    global_config: Res<GlobalConfig>,
    mut workbench_menu: ResMut<WorkbenchRecipeMenuState>,
    mut chest_inventory: ResMut<ChestInventoryUiState>,
    mut craft_progress: ResMut<WorkbenchCraftProgressState>,
    mut recipe_preview: ResMut<RecipePreviewDialogState>,
    mut cursor_item: ResMut<InventoryCursorItemState>,
    mut work_table_crafting: ResMut<WorkTableCraftingState>,
    mut workbench_tools: ResMut<WorkbenchToolSlotsState>,
    mut inventory: ResMut<PlayerInventory>,
    mut closed: MessageWriter<ChestInventoryUiClosed>,
    mut persist_requests: MessageWriter<ChestInventoryPersistRequest>,
    item_registry: Option<Res<ItemRegistry>>,
) {
    if !workbench_menu.open {
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

    if let Some(item_registry) = item_registry.as_ref() {
        flush_work_table_inputs_to_inventory(
            &mut work_table_crafting,
            &mut inventory,
            item_registry,
        );
        flush_workbench_tools_to_inventory(&mut workbench_tools, &mut inventory, item_registry);
        flush_cursor_item_to_inventory(&mut cursor_item, &mut inventory, item_registry);
        if let Some(storage_world_pos) = workbench_menu.storage_left_world_pos {
            persist_requests.write(ChestInventoryPersistRequest {
                world_pos: storage_world_pos,
                slots: serialize_chest_slots_for_persist(
                    &chest_inventory.workbench_left_slots,
                    item_registry,
                ),
            });
        }
        if let Some(storage_world_pos) = workbench_menu.storage_right_world_pos {
            persist_requests.write(ChestInventoryPersistRequest {
                world_pos: storage_world_pos,
                slots: serialize_chest_slots_for_persist(
                    &chest_inventory.workbench_right_slots,
                    item_registry,
                ),
            });
        }
    }
    if let Some(storage_world_pos) = workbench_menu.storage_left_world_pos {
        closed.write(ChestInventoryUiClosed {
            world_pos: storage_world_pos,
        });
    }
    if let Some(storage_world_pos) = workbench_menu.storage_right_world_pos {
        closed.write(ChestInventoryUiClosed {
            world_pos: storage_world_pos,
        });
    }
    reset_workbench_craft_progress(&mut craft_progress);
    recipe_preview.open = false;
    workbench_menu.world_pos = None;
    workbench_menu.storage_left_world_pos = None;
    workbench_menu.storage_right_world_pos = None;
    workbench_menu.open = false;
    chest_inventory.workbench_left_slots = [InventorySlot::default(); CHEST_INVENTORY_SLOTS];
    chest_inventory.workbench_right_slots = [InventorySlot::default(); CHEST_INVENTORY_SLOTS];
}

fn handle_workbench_recipe_menu_navigation(
    mouse: Res<ButtonInput<MouseButton>>,
    workbench_menu: Res<WorkbenchRecipeMenuState>,
    mut creative_panel: ResMut<CreativePanelState>,
    button_states: Query<(&CssID, &UIWidgetState), With<Button>>,
) {
    if !workbench_menu.open || !mouse.just_pressed(MouseButton::Left) {
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

    if css_id == WORKBENCH_ITEMS_PREV_ID {
        let _ = creative_panel.prev_page();
        return;
    }
    if css_id == WORKBENCH_ITEMS_NEXT_ID {
        let _ = creative_panel.next_page();
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_workbench_recipe_menu_item_clicks(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    workbench_menu: Res<WorkbenchRecipeMenuState>,
    game_mode: Res<GameModeState>,
    creative_panel: Res<CreativePanelState>,
    recipe_registry: Option<Res<RecipeRegistry>>,
    item_registry: Res<ItemRegistry>,
    cursor_item: Res<InventoryCursorItemState>,
    mut inventory: ResMut<PlayerInventory>,
    mut recipe_preview: ResMut<RecipePreviewDialogState>,
    mut slot_frames: Query<(&CssID, &UIWidgetState, &mut BorderColor), With<Button>>,
) {
    let hovered_slot = sync_workbench_item_slot_hover_border(&mut slot_frames, workbench_menu.open);

    if !workbench_menu.open || !mouse.just_pressed(MouseButton::Left) || !cursor_item.slot.is_empty() {
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
struct WorkbenchRecipeUiSyncDeps<'w, 's> {
    game_mode: Res<'w, GameModeState>,
    creative_panel: Res<'w, CreativePanelState>,
    inventory: Res<'w, PlayerInventory>,
    chest_inventory: Res<'w, ChestInventoryUiState>,
    work_table_crafting: Res<'w, WorkTableCraftingState>,
    workbench_tools: Res<'w, WorkbenchToolSlotsState>,
    craft_progress: Res<'w, WorkbenchCraftProgressState>,
    recipe_registry: Option<Res<'w, RecipeRegistry>>,
    recipe_type_registry: Option<Res<'w, RecipeTypeRegistry>>,
    item_registry: Res<'w, ItemRegistry>,
    block_registry: Res<'w, BlockRegistry>,
    language: Res<'w, ClientLanguageState>,
    asset_server: Res<'w, AssetServer>,
    image_cache: ResMut<'w, ImageCache>,
    images: ResMut<'w, Assets<Image>>,
    ui_q: ParamSet<
        'w,
        's,
        (
            Query<
                'w,
                's,
                &'static mut Visibility,
                (
                    With<WorkbenchRecipeRoot>,
                    Without<WorkbenchRecipeStorageRoot>,
                    Without<WorkbenchRecipeStoragePanel>,
                ),
            >,
            Query<
                'w,
                's,
                &'static mut Visibility,
                (
                    With<WorkbenchRecipeStorageRoot>,
                    Without<WorkbenchRecipeRoot>,
                    Without<WorkbenchRecipeStoragePanel>,
                ),
            >,
            Query<
                'w,
                's,
                (
                    &'static WorkbenchRecipeStoragePanel,
                    &'static mut Visibility,
                    &'static mut Node,
                ),
                (
                    Without<WorkbenchRecipeRoot>,
                    Without<WorkbenchRecipeStorageRoot>,
                ),
            >,
            Query<
                'w,
                's,
                (&'static CssID, &'static mut Paragraph),
                (
                    Without<WorkbenchRecipeRoot>,
                    Without<WorkbenchRecipeStorageRoot>,
                    Without<WorkbenchRecipeStoragePanel>,
                ),
            >,
            Query<
                'w,
                's,
                (&'static CssID, &'static mut Paragraph, &'static mut Visibility),
                (
                    Without<WorkbenchRecipeRoot>,
                    Without<WorkbenchRecipeStorageRoot>,
                    Without<WorkbenchRecipeStoragePanel>,
                ),
            >,
        ),
    >,
    button_q: Query<'w, 's, (&'static CssID, &'static mut Button), With<Button>>,
    progress_q: Query<'w, 's, (&'static CssID, &'static mut ProgressBar), With<ProgressBar>>,
}

fn sync_workbench_recipe_menu_ui(
    workbench_menu: Res<WorkbenchRecipeMenuState>,
    deps: WorkbenchRecipeUiSyncDeps,
) {
    let WorkbenchRecipeUiSyncDeps {
        game_mode,
        creative_panel,
        inventory,
        chest_inventory,
        work_table_crafting,
        workbench_tools,
        craft_progress,
        recipe_registry,
        recipe_type_registry,
        item_registry,
        block_registry,
        language,
        asset_server,
        mut image_cache,
        mut images,
        mut ui_q,
        mut button_q,
        mut progress_q,
    } = deps;

    if let Ok(mut visibility) = ui_q.p0().single_mut() {
        *visibility = if workbench_menu.open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    let has_any_storage = workbench_menu.storage_left_world_pos.is_some()
        || workbench_menu.storage_right_world_pos.is_some();
    if let Ok(mut visibility) = ui_q.p1().single_mut() {
        *visibility = if workbench_menu.open && has_any_storage {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for (panel, mut visibility, mut node) in &mut ui_q.p2() {
        let side_world_pos = workbench_menu.storage_world_pos(panel.side);
        let show_panel = workbench_menu.open && side_world_pos.is_some();
        *visibility = if show_panel {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        node.display = if show_panel {
            Display::Flex
        } else {
            Display::None
        };
    }

    let resolved_recipe = recipe_registry.as_ref().and_then(|recipes| {
        recipe_type_registry
            .as_ref()
            .and_then(|types| crate::handlers::recipe::resolve_work_table_recipe(
                &work_table_crafting,
                recipes,
                types,
                &item_registry,
            ))
    });

    for (css_id, mut paragraph) in &mut ui_q.p3() {
        if css_id.0 == WORKBENCH_RECIPE_TITLE_ID {
            let next = language.localize_name_key("KEY_UI_WORKBENCH");
            if paragraph.text != next {
                paragraph.text = next;
            }
            continue;
        }
        if css_id.0 == WORKBENCH_STORAGE_LEFT_TITLE_ID {
            paragraph.text = language.localize_name_key("KEY_UI_CHEST_INVENTORY");
            continue;
        }
        if css_id.0 == WORKBENCH_STORAGE_RIGHT_TITLE_ID {
            paragraph.text = language.localize_name_key("KEY_UI_CHEST_INVENTORY");
            continue;
        }
        if css_id.0 == WORKBENCH_ITEMS_TOTAL_ID {
            paragraph.text = format!(
                "{} {}",
                language.localize_name_key("KEY_UI_REGISTERED"),
                creative_panel.item_count()
            );
            continue;
        }
        if css_id.0 == WORKBENCH_ITEMS_PAGE_ID {
            paragraph.text = creative_panel.page_label();
            continue;
        }
        if css_id.0 == WORKBENCH_RECIPE_HINT_ID {
            paragraph.text = match game_mode.0 {
                GameMode::Creative => language.localize_name_key("KEY_UI_RECIPE_HINT_CREATIVE"),
                GameMode::Survival => language.localize_name_key("KEY_UI_RECIPE_HINT_SURVIVAL"),
                GameMode::Spectator => {
                    language.localize_name_key("KEY_UI_RECIPE_HINT_SPECTATOR")
                }
            };
            continue;
        }
        if css_id.0 == WORKBENCH_RESULT_TIME_ID {
            let next = if !workbench_menu.open {
                String::new()
            } else if let Some(recipe) = resolved_recipe.as_ref() {
                format!("time: {}", format_build_time_label(recipe.build_time_secs))
            } else {
                String::new()
            };
            if paragraph.text != next {
                paragraph.text = next;
            }
            continue;
        }
    }

    for (css_id, mut paragraph, mut visibility) in &mut ui_q.p4() {
        if let Some(slot_index) = parse_workbench_craft_badge_index(css_id.0.as_str())
            && let Some(slot) = work_table_crafting.input_slots.get(slot_index)
        {
            sync_badge(
                &mut paragraph,
                &mut visibility,
                slot.count,
                slot.is_empty() || !workbench_menu.open,
            );
            continue;
        }

        if css_id.0 == WORKBENCH_RESULT_BADGE_ID {
            if let Some(result) = resolved_recipe.as_ref() {
                sync_badge(
                    &mut paragraph,
                    &mut visibility,
                    result.result.count,
                    !workbench_menu.open,
                );
            } else {
                sync_badge(&mut paragraph, &mut visibility, 0, true);
            }
            continue;
        }

        if let Some(slot_index) = parse_workbench_tool_badge_index(css_id.0.as_str())
            && let Some(slot) = workbench_tools.slots.get(slot_index)
        {
            sync_badge(
                &mut paragraph,
                &mut visibility,
                slot.count,
                slot.is_empty() || !workbench_menu.open,
            );
            continue;
        }

        if let Some(slot_index) = parse_workbench_player_inventory_badge_index(css_id.0.as_str())
            && let Some(slot) = inventory.slots.get(slot_index)
        {
            sync_badge(
                &mut paragraph,
                &mut visibility,
                slot.count,
                slot.is_empty() || !workbench_menu.open,
            );
            continue;
        }

        if let Some((side, slot_index)) = parse_workbench_storage_badge_index(css_id.0.as_str())
            && let Some(slot) = chest_inventory.workbench_slots_ref(side).get(slot_index)
        {
            sync_badge(
                &mut paragraph,
                &mut visibility,
                slot.count,
                slot.is_empty()
                    || !workbench_menu.open
                    || workbench_menu.storage_world_pos(side).is_none(),
            );
        }
    }

    for (css_id, mut button) in &mut button_q {
        if let Some(slot_index) = parse_workbench_craft_slot_index(css_id.0.as_str()) {
            if !button.text.is_empty() {
                button.text.clear();
            }
            let slot = work_table_crafting
                .input_slots
                .get(slot_index)
                .copied()
                .unwrap_or_default();
            let next_icon = if !workbench_menu.open || slot.is_empty() {
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

        if css_id.0 == WORKBENCH_RESULT_FRAME_ID {
            if !button.text.is_empty() {
                button.text.clear();
            }
            let next_icon = if !workbench_menu.open {
                None
            } else {
                resolved_recipe.as_ref().and_then(|recipe| {
                    resolve_item_icon_path(
                        &item_registry,
                        &block_registry,
                        &asset_server,
                        &mut image_cache,
                        &mut images,
                        recipe.result.item_id,
                    )
                })
            };
            if button.icon_path != next_icon {
                button.icon_path = next_icon;
            }
            continue;
        }

        if let Some(slot_index) = parse_workbench_tool_slot_index(css_id.0.as_str()) {
            if !button.text.is_empty() {
                button.text.clear();
            }
            let slot = workbench_tools
                .slots
                .get(slot_index)
                .copied()
                .unwrap_or_default();
            let next_icon = if !workbench_menu.open || slot.is_empty() {
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

        if let Some(slot_index) = parse_workbench_player_inventory_slot_index(css_id.0.as_str()) {
            if !button.text.is_empty() {
                button.text.clear();
            }
            let slot = inventory
                .slots
                .get(slot_index)
                .copied()
                .unwrap_or_default();
            let next_icon = if !workbench_menu.open || slot.is_empty() {
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

        if let Some((side, slot_index)) = parse_workbench_storage_slot_index(css_id.0.as_str()) {
            if !button.text.is_empty() {
                button.text.clear();
            }
            let slot = chest_inventory
                .workbench_slots_ref(side)
                .get(slot_index)
                .copied()
                .unwrap_or_default();
            let next_icon = if !workbench_menu.open
                || workbench_menu.storage_world_pos(side).is_none()
                || slot.is_empty()
            {
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

        let Some(slot_index) = parse_workbench_item_slot_index(css_id.0.as_str()) else {
            continue;
        };
        if !button.text.is_empty() {
            button.text.clear();
        }
        let next_icon = if !workbench_menu.open {
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

    let progress_percent = if !workbench_menu.open || !craft_progress.active {
        0.0
    } else if craft_progress.duration_secs <= f32::EPSILON {
        100.0
    } else {
        ((craft_progress.elapsed_secs / craft_progress.duration_secs) * 100.0).clamp(0.0, 100.0)
    };
    for (css_id, mut progress) in &mut progress_q {
        if css_id.0 != WORKBENCH_RESULT_PROGRESS_ID {
            continue;
        }
        if (progress.value - progress_percent).abs() > f32::EPSILON {
            progress.value = progress_percent;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn close_workbench_recipe_menu_ui(
    mut ui_interaction: ResMut<UiInteractionState>,
    mut workbench_menu: ResMut<WorkbenchRecipeMenuState>,
    mut chest_inventory: ResMut<ChestInventoryUiState>,
    mut auto_persist: ResMut<ChestInventoryAutoPersistState>,
    mut craft_progress: ResMut<WorkbenchCraftProgressState>,
    mut root_q: Query<&mut Visibility, With<WorkbenchRecipeRoot>>,
    mut recipe_preview: ResMut<RecipePreviewDialogState>,
    mut cursor_item: ResMut<InventoryCursorItemState>,
    mut work_table_crafting: ResMut<WorkTableCraftingState>,
    mut workbench_tools: ResMut<WorkbenchToolSlotsState>,
    mut inventory: ResMut<PlayerInventory>,
    mut closed: MessageWriter<ChestInventoryUiClosed>,
    mut persist_requests: MessageWriter<ChestInventoryPersistRequest>,
    item_registry: Option<Res<ItemRegistry>>,
) {
    if let Some(item_registry) = item_registry.as_ref() {
        flush_work_table_inputs_to_inventory(
            &mut work_table_crafting,
            &mut inventory,
            item_registry,
        );
        flush_workbench_tools_to_inventory(&mut workbench_tools, &mut inventory, item_registry);
        flush_cursor_item_to_inventory(&mut cursor_item, &mut inventory, item_registry);
        if workbench_menu.open
            && let Some(storage_world_pos) = workbench_menu.storage_left_world_pos
        {
            persist_requests.write(ChestInventoryPersistRequest {
                world_pos: storage_world_pos,
                slots: serialize_chest_slots_for_persist(
                    &chest_inventory.workbench_left_slots,
                    item_registry,
                ),
            });
        }
        if workbench_menu.open
            && let Some(storage_world_pos) = workbench_menu.storage_right_world_pos
        {
            persist_requests.write(ChestInventoryPersistRequest {
                world_pos: storage_world_pos,
                slots: serialize_chest_slots_for_persist(
                    &chest_inventory.workbench_right_slots,
                    item_registry,
                ),
            });
        }
    }

    if workbench_menu.open
        && let Some(storage_world_pos) = workbench_menu.storage_left_world_pos
    {
        closed.write(ChestInventoryUiClosed {
            world_pos: storage_world_pos,
        });
    }
    if workbench_menu.open
        && let Some(storage_world_pos) = workbench_menu.storage_right_world_pos
    {
        closed.write(ChestInventoryUiClosed {
            world_pos: storage_world_pos,
        });
    }

    workbench_menu.open = false;
    workbench_menu.world_pos = None;
    workbench_menu.storage_left_world_pos = None;
    workbench_menu.storage_right_world_pos = None;
    chest_inventory.workbench_left_slots = [InventorySlot::default(); CHEST_INVENTORY_SLOTS];
    chest_inventory.workbench_right_slots = [InventorySlot::default(); CHEST_INVENTORY_SLOTS];
    auto_persist.workbench_left_loaded = false;
    auto_persist.workbench_right_loaded = false;
    auto_persist.last_workbench_left_world_pos = None;
    auto_persist.last_workbench_right_world_pos = None;
    auto_persist.last_workbench_left_slots.clear();
    auto_persist.last_workbench_right_slots.clear();
    reset_workbench_craft_progress(&mut craft_progress);
    ui_interaction.workbench_menu_open = false;
    recipe_preview.open = false;
    if let Ok(mut visibility) = root_q.single_mut() {
        *visibility = Visibility::Hidden;
    }
}

fn tick_workbench_craft_progress(
    time: Res<Time>,
    workbench_menu: Res<WorkbenchRecipeMenuState>,
    mut craft_progress: ResMut<WorkbenchCraftProgressState>,
    work_table_crafting: Res<WorkTableCraftingState>,
    recipe_registry: Option<Res<RecipeRegistry>>,
    recipe_type_registry: Option<Res<RecipeTypeRegistry>>,
    item_registry: Option<Res<ItemRegistry>>,
    mut craft_requests: MessageWriter<CraftWorkTableRequest>,
) {
    if !craft_progress.active {
        return;
    }

    if !workbench_menu.open {
        reset_workbench_craft_progress(&mut craft_progress);
        return;
    }

    let (Some(recipe_registry), Some(recipe_type_registry), Some(item_registry)) =
        (recipe_registry, recipe_type_registry, item_registry)
    else {
        reset_workbench_craft_progress(&mut craft_progress);
        return;
    };

    let Some(resolved_recipe) = crate::handlers::recipe::resolve_work_table_recipe(
        &work_table_crafting,
        &recipe_registry,
        &recipe_type_registry,
        &item_registry,
    ) else {
        reset_workbench_craft_progress(&mut craft_progress);
        return;
    };

    if resolved_recipe.source_path != craft_progress.recipe_source_path {
        reset_workbench_craft_progress(&mut craft_progress);
        return;
    }

    craft_progress.elapsed_secs += time.delta_secs();
    if craft_progress.elapsed_secs + f32::EPSILON >= craft_progress.duration_secs {
        craft_requests.write(CraftWorkTableRequest);
        reset_workbench_craft_progress(&mut craft_progress);
    }
}

fn reset_workbench_craft_progress(craft_progress: &mut WorkbenchCraftProgressState) {
    craft_progress.active = false;
    craft_progress.elapsed_secs = 0.0;
    craft_progress.duration_secs = 0.0;
    craft_progress.recipe_source_path.clear();
}

fn format_build_time_label(seconds: f32) -> String {
    let clamped = seconds.max(0.0);
    let rounded = clamped.round();
    if (clamped - rounded).abs() < 0.01 {
        format!("{:.0}s", rounded)
    } else {
        format!("{:.1}s", clamped)
    }
}

fn flush_work_table_inputs_to_inventory(
    work_table_crafting: &mut WorkTableCraftingState,
    inventory: &mut PlayerInventory,
    item_registry: &ItemRegistry,
) {
    for slot in &mut work_table_crafting.input_slots {
        if slot.is_empty() {
            continue;
        }
        let leftover = inventory.add_item(slot.item_id, slot.count, item_registry);
        if leftover == 0 {
            *slot = InventorySlot::default();
        } else {
            slot.count = leftover;
        }
    }
}

fn flush_workbench_tools_to_inventory(
    workbench_tools: &mut WorkbenchToolSlotsState,
    inventory: &mut PlayerInventory,
    item_registry: &ItemRegistry,
) {
    for slot in &mut workbench_tools.slots {
        if slot.is_empty() {
            continue;
        }
        let leftover = inventory.add_item(slot.item_id, slot.count, item_registry);
        if leftover == 0 {
            *slot = InventorySlot::default();
        } else {
            slot.count = leftover;
        }
    }
}

fn parse_workbench_craft_slot_index(css_id: &str) -> Option<usize> {
    css_id
        .strip_prefix(WORKBENCH_CRAFT_FRAME_PREFIX)
        .and_then(|slot_number| slot_number.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
        .filter(|index| *index < WORK_TABLE_CRAFTING_INPUT_SLOTS)
}

fn parse_workbench_craft_badge_index(css_id: &str) -> Option<usize> {
    css_id
        .strip_prefix(WORKBENCH_CRAFT_BADGE_PREFIX)
        .and_then(|slot_number| slot_number.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
        .filter(|index| *index < WORK_TABLE_CRAFTING_INPUT_SLOTS)
}

fn parse_workbench_tool_slot_index(css_id: &str) -> Option<usize> {
    css_id
        .strip_prefix(WORKBENCH_TOOL_FRAME_PREFIX)
        .and_then(|slot_number| slot_number.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
        .filter(|index| *index < 5)
}

fn parse_workbench_tool_badge_index(css_id: &str) -> Option<usize> {
    css_id
        .strip_prefix(WORKBENCH_TOOL_BADGE_PREFIX)
        .and_then(|slot_number| slot_number.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
        .filter(|index| *index < 5)
}

fn parse_workbench_player_inventory_slot_index(css_id: &str) -> Option<usize> {
    css_id
        .strip_prefix(WORKBENCH_PLAYER_INVENTORY_FRAME_PREFIX)
        .and_then(|slot_number| slot_number.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
        .filter(|index| *index < PLAYER_INVENTORY_SLOTS)
}

fn parse_workbench_player_inventory_badge_index(css_id: &str) -> Option<usize> {
    css_id
        .strip_prefix(WORKBENCH_PLAYER_INVENTORY_BADGE_PREFIX)
        .and_then(|slot_number| slot_number.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
        .filter(|index| *index < PLAYER_INVENTORY_SLOTS)
}

fn parse_workbench_storage_slot_index(css_id: &str) -> Option<(WorkbenchStorageSide, usize)> {
    if let Some(index) = css_id
        .strip_prefix(WORKBENCH_STORAGE_LEFT_FRAME_PREFIX)
        .and_then(|slot_number| slot_number.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
        .filter(|index| *index < CHEST_INVENTORY_SLOTS)
    {
        return Some((WorkbenchStorageSide::Left, index));
    }
    css_id
        .strip_prefix(WORKBENCH_STORAGE_RIGHT_FRAME_PREFIX)
        .and_then(|slot_number| slot_number.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
        .filter(|index| *index < CHEST_INVENTORY_SLOTS)
        .map(|index| (WorkbenchStorageSide::Right, index))
}

fn parse_workbench_storage_badge_index(css_id: &str) -> Option<(WorkbenchStorageSide, usize)> {
    if let Some(index) = css_id
        .strip_prefix(WORKBENCH_STORAGE_LEFT_BADGE_PREFIX)
        .and_then(|slot_number| slot_number.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
        .filter(|index| *index < CHEST_INVENTORY_SLOTS)
    {
        return Some((WorkbenchStorageSide::Left, index));
    }
    css_id
        .strip_prefix(WORKBENCH_STORAGE_RIGHT_BADGE_PREFIX)
        .and_then(|slot_number| slot_number.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
        .filter(|index| *index < CHEST_INVENTORY_SLOTS)
        .map(|index| (WorkbenchStorageSide::Right, index))
}

fn parse_workbench_item_slot_index(css_id: &str) -> Option<usize> {
    css_id
        .strip_prefix(WORKBENCH_ITEMS_SLOT_PREFIX)
        .and_then(|slot_number| slot_number.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
        .filter(|index| *index < CREATIVE_PANEL_PAGE_SIZE)
}

fn sync_workbench_item_slot_hover_border(
    slot_frames: &mut Query<(&CssID, &UIWidgetState, &mut BorderColor), With<Button>>,
    workbench_open: bool,
) -> Option<usize> {
    let mut hovered_slot = None;

    for (css_id, state, mut border) in slot_frames.iter_mut() {
        let Some(slot_index) = parse_workbench_item_slot_index(css_id.0.as_str()) else {
            continue;
        };

        if hovered_slot.is_none() && state.hovered {
            hovered_slot = Some(slot_index);
        }

        let color = if workbench_open && state.hovered {
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

#[derive(Clone, Copy, Debug, Default)]
struct WorkbenchStorageNeighbors {
    left: Option<[i32; 3]>,
    right: Option<[i32; 3]>,
}

fn find_adjacent_storage_world_pos(
    workbench_world_pos: [i32; 3],
    chunk_map: &ChunkMap,
    block_registry: &BlockRegistry,
    item_registry: &ItemRegistry,
    structure_runtime: Option<&crate::logic::events::block_event_handler::StructureRuntimeState>,
    structure_recipe_registry: Option<&BuildingStructureRecipeRegistry>,
) -> WorkbenchStorageNeighbors {
    let base = IVec3::new(
        workbench_world_pos[0],
        workbench_world_pos[1],
        workbench_world_pos[2],
    );

    let (footprint, side_axis) = resolve_workbench_footprint_and_side_axis(
        base,
        chunk_map,
        block_registry,
        structure_runtime,
        structure_recipe_registry,
    );
    let mut left_candidates = Vec::<IVec3>::new();
    let mut right_candidates = Vec::<IVec3>::new();
    for cell in &footprint {
        let left_pos = *cell - side_axis;
        if !footprint.contains(&left_pos) && !left_candidates.contains(&left_pos) {
            left_candidates.push(left_pos);
        }
        let right_pos = *cell + side_axis;
        if !footprint.contains(&right_pos) && !right_candidates.contains(&right_pos) {
            right_candidates.push(right_pos);
        }
    }

    let left = left_candidates.into_iter().find_map(|storage_pos| {
        let block_id = get_block_world(chunk_map, storage_pos);
        if !is_storage_block_id_for_workbench(block_id, block_registry, item_registry) {
            return None;
        }
        Some([storage_pos.x, storage_pos.y, storage_pos.z])
    });

    let right = right_candidates.into_iter().find_map(|storage_pos| {
        let block_id = get_block_world(chunk_map, storage_pos);
        if !is_storage_block_id_for_workbench(block_id, block_registry, item_registry) {
            return None;
        }
        Some([storage_pos.x, storage_pos.y, storage_pos.z])
    });

    WorkbenchStorageNeighbors { left, right }
}

fn resolve_workbench_footprint_and_side_axis(
    base: IVec3,
    chunk_map: &ChunkMap,
    block_registry: &BlockRegistry,
    structure_runtime: Option<&crate::logic::events::block_event_handler::StructureRuntimeState>,
    structure_recipe_registry: Option<&BuildingStructureRecipeRegistry>,
) -> (Vec<IVec3>, IVec3) {
    if let Some((space_x, space_z, rotation_quarters)) = resolve_workbench_structure_shape(
        base,
        structure_runtime,
        structure_recipe_registry,
    ) {
        let mut footprint = Vec::new();
        for local_z in 0..space_z {
            for local_x in 0..space_x {
                let (x_offset, z_offset) = rotated_structure_offset(
                    local_x,
                    local_z,
                    space_x,
                    space_z,
                    rotation_quarters,
                );
                footprint.push(base + IVec3::new(x_offset, 0, z_offset));
            }
        }
        if !footprint.is_empty() {
            return (footprint, workbench_local_x_axis_world(rotation_quarters));
        }
    }

    let block_id = get_block_world(chunk_map, base);
    let rotation_quarters =
        parse_block_rotation_quarters(block_id, block_registry).unwrap_or(0);
    (vec![base], workbench_local_x_axis_world(rotation_quarters))
}

fn resolve_workbench_structure_shape(
    base: IVec3,
    structure_runtime: Option<&crate::logic::events::block_event_handler::StructureRuntimeState>,
    structure_recipe_registry: Option<&BuildingStructureRecipeRegistry>,
) -> Option<(i32, i32, u8)> {
    let structure_runtime = structure_runtime?;
    let structure_recipe_registry = structure_recipe_registry?;
    let (coord, _) = world_to_chunk_xz(base.x, base.z);
    let entry = structure_runtime
        .records_by_chunk
        .get(&coord)
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.place_origin == [base.x, base.y, base.z])
        })?;
    let recipe = structure_recipe_registry.recipe_by_name(entry.recipe_name.as_str())?;
    let space_x = recipe.space.x as i32;
    let space_z = recipe.space.z as i32;
    if space_x <= 0 || space_z <= 0 {
        return None;
    }

    let rotation_steps = entry
        .rotation_steps
        .map(i32::from)
        .unwrap_or((entry.rotation_quarters as i32) * 2);
    let normalized_steps = rotation_steps.rem_euclid(8) as u8;
    let rotation_quarters = (normalized_steps / 2) % 4;
    Some((space_x, space_z, rotation_quarters))
}

#[inline]
fn rotated_structure_offset(
    local_x: i32,
    local_z: i32,
    size_x: i32,
    size_z: i32,
    rotation_quarters: u8,
) -> (i32, i32) {
    match rotation_quarters % 4 {
        0 => (local_x, local_z),
        1 => (local_z, size_x - 1 - local_x),
        2 => (size_x - 1 - local_x, size_z - 1 - local_z),
        _ => (size_z - 1 - local_z, local_x),
    }
}

#[inline]
fn workbench_local_x_axis_world(rotation_quarters: u8) -> IVec3 {
    match rotation_quarters % 4 {
        0 => IVec3::new(1, 0, 0),
        1 => IVec3::new(0, 0, -1),
        2 => IVec3::new(-1, 0, 0),
        _ => IVec3::new(0, 0, 1),
    }
}

fn parse_block_rotation_quarters(block_id: u16, block_registry: &BlockRegistry) -> Option<u8> {
    if block_id == 0 {
        return None;
    }
    let def = block_registry.def_opt(block_id)?;
    parse_rotation_suffix(def.localized_name.as_str()).or_else(|| parse_rotation_suffix(def.name.as_str()))
}

fn parse_rotation_suffix(value: &str) -> Option<u8> {
    if let Some(rotation) = value.rsplit_once("_r").and_then(|(_, suffix)| suffix.parse::<u8>().ok()) {
        return Some(rotation % 4);
    }
    value
        .rsplit_once("_R")
        .and_then(|(_, suffix)| suffix.parse::<u8>().ok())
        .map(|rotation| rotation % 4)
}

fn is_storage_block_id_for_workbench(
    block_id: u16,
    block_registry: &BlockRegistry,
    item_registry: &ItemRegistry,
) -> bool {
    if block_id == 0 {
        return false;
    }

    let is_storage_category = item_registry
        .item_for_block(block_id)
        .and_then(|item_id| item_registry.def_opt(item_id))
        .is_some_and(|item| item.category.eq_ignore_ascii_case("storage"));
    if is_storage_category {
        return true;
    }

    block_registry.def_opt(block_id).is_some_and(|def| {
        let localized = def.localized_name.to_ascii_lowercase();
        let key = def.name.to_ascii_uppercase();
        localized == "chest_block"
            || localized.starts_with("chest_block_r")
            || key == "KEY_CHEST_BLOCK"
            || key.starts_with("KEY_CHEST_BLOCK_R")
    })
}
