fn handle_open_workbench_recipe_menu_request(
    mut requests: MessageReader<OpenWorkbenchMenuRequest>,
    ui_interaction: Res<UiInteractionState>,
    mut workbench_menu: ResMut<WorkbenchRecipeMenuState>,
    mut craft_progress: ResMut<WorkbenchCraftProgressState>,
    mut structure_menu: ResMut<StructureBuildMenuState>,
    mut active_structure_recipe: ResMut<ActiveStructureRecipeState>,
    mut active_structure_placement: ResMut<ActiveStructurePlacementState>,
    mut recipe_preview: ResMut<RecipePreviewDialogState>,
) {
    let mut requested = false;
    for _ in requests.read() {
        requested = true;
    }
    if !requested {
        return;
    }
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
    workbench_menu.open = true;
}

#[allow(clippy::too_many_arguments)]
fn handle_workbench_recipe_menu_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    global_config: Res<GlobalConfig>,
    mut workbench_menu: ResMut<WorkbenchRecipeMenuState>,
    mut craft_progress: ResMut<WorkbenchCraftProgressState>,
    mut recipe_preview: ResMut<RecipePreviewDialogState>,
    mut cursor_item: ResMut<InventoryCursorItemState>,
    mut work_table_crafting: ResMut<WorkTableCraftingState>,
    mut workbench_tools: ResMut<WorkbenchToolSlotsState>,
    mut inventory: ResMut<PlayerInventory>,
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
    }
    reset_workbench_craft_progress(&mut craft_progress);
    recipe_preview.open = false;
    workbench_menu.open = false;
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
            let _ = api::handlers::inventory::apply_creative_panel_click(
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
    root_q: Query<'w, 's, &'static mut Visibility, With<WorkbenchRecipeRoot>>,
    paragraph_q: Query<
        'w,
        's,
        (
            &'static CssID,
            &'static mut Paragraph,
            Option<&'static mut Visibility>,
        ),
        Without<WorkbenchRecipeRoot>,
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
        mut root_q,
        mut paragraph_q,
        mut button_q,
        mut progress_q,
    } = deps;

    if let Ok(mut visibility) = root_q.single_mut() {
        *visibility = if workbench_menu.open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    let resolved_recipe = recipe_registry.as_ref().and_then(|recipes| {
        recipe_type_registry
            .as_ref()
            .and_then(|types| api::handlers::recipe::resolve_work_table_recipe(
                &work_table_crafting,
                recipes,
                types,
                &item_registry,
            ))
    });

    for (css_id, mut paragraph, mut maybe_visibility) in &mut paragraph_q {
        if css_id.0 == WORKBENCH_RECIPE_TITLE_ID {
            let next = language.localize_name_key("KEY_UI_WORKBENCH");
            if paragraph.text != next {
                paragraph.text = next;
            }
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

        if let Some(slot_index) = parse_workbench_craft_badge_index(css_id.0.as_str())
            && let Some(slot) = work_table_crafting.input_slots.get(slot_index)
            && let Some(visibility) = maybe_visibility.as_mut()
        {
            sync_badge(
                &mut paragraph,
                visibility,
                slot.count,
                slot.is_empty() || !workbench_menu.open,
            );
            continue;
        }

        if css_id.0 == WORKBENCH_RESULT_BADGE_ID
            && let Some(visibility) = maybe_visibility.as_mut()
        {
            if let Some(result) = resolved_recipe.as_ref() {
                sync_badge(
                    &mut paragraph,
                    visibility,
                    result.result.count,
                    !workbench_menu.open,
                );
            } else {
                sync_badge(&mut paragraph, visibility, 0, true);
            }
            continue;
        }

        if let Some(slot_index) = parse_workbench_tool_badge_index(css_id.0.as_str())
            && let Some(slot) = workbench_tools.slots.get(slot_index)
            && let Some(visibility) = maybe_visibility.as_mut()
        {
            sync_badge(
                &mut paragraph,
                visibility,
                slot.count,
                slot.is_empty() || !workbench_menu.open,
            );
            continue;
        }

        if let Some(slot_index) = parse_workbench_player_inventory_badge_index(css_id.0.as_str())
            && let Some(slot) = inventory.slots.get(slot_index)
            && let Some(visibility) = maybe_visibility.as_mut()
        {
            sync_badge(
                &mut paragraph,
                visibility,
                slot.count,
                slot.is_empty() || !workbench_menu.open,
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
    mut craft_progress: ResMut<WorkbenchCraftProgressState>,
    mut root_q: Query<&mut Visibility, With<WorkbenchRecipeRoot>>,
    mut recipe_preview: ResMut<RecipePreviewDialogState>,
    mut cursor_item: ResMut<InventoryCursorItemState>,
    mut work_table_crafting: ResMut<WorkTableCraftingState>,
    mut workbench_tools: ResMut<WorkbenchToolSlotsState>,
    mut inventory: ResMut<PlayerInventory>,
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
    }

    workbench_menu.open = false;
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

    let Some(resolved_recipe) = api::handlers::recipe::resolve_work_table_recipe(
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
