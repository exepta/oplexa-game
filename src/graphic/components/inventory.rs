use api::handlers::recipe::{resolve_hand_crafted_recipe, resolve_work_table_recipe};
use bevy::ecs::{query::QueryFilter, system::SystemParam};

/// Defines the possible inventory ui slot target variants in the `graphic::components::inventory` module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InventoryUiSlotTarget {
    Player(usize),
    HandCrafted(usize),
    WorkTable(usize),
    WorkbenchTool(usize),
    HandCraftedResult,
    WorkbenchResult,
}

/// Represents inventory drop deps used by the `graphic::components::inventory` module.
#[derive(SystemParam)]
struct InventoryDropDeps<'w, 's> {
    commands: Commands<'w, 's>,
    meshes: ResMut<'w, Assets<Mesh>>,
    block_registry: Res<'w, BlockRegistry>,
    item_registry: Res<'w, ItemRegistry>,
}

/// Represents inventory drag drop deps used by the `graphic::components::inventory` module.
#[derive(SystemParam)]
struct InventoryDragDropDeps<'w, 's> {
    global_config: Res<'w, GlobalConfig>,
    inventory_ui: Res<'w, PlayerInventoryUiState>,
    workbench_menu: Res<'w, WorkbenchRecipeMenuState>,
    recipe_preview: ResMut<'w, RecipePreviewDialogState>,
    game_mode: Res<'w, GameModeState>,
    multiplayer_connection: Option<Res<'w, MultiplayerConnectionState>>,
    creative_panel: Res<'w, CreativePanelState>,
    recipe_registry: Option<Res<'w, RecipeRegistry>>,
    recipe_type_registry: Option<Res<'w, RecipeTypeRegistry>>,
    cursor_item: ResMut<'w, InventoryCursorItemState>,
    workbench_craft_progress: ResMut<'w, WorkbenchCraftProgressState>,
    workbench_tools: ResMut<'w, WorkbenchToolSlotsState>,
    inventory: ResMut<'w, PlayerInventory>,
    hand_crafted: ResMut<'w, HandCraftedState>,
    work_table_crafting: ResMut<'w, WorkTableCraftingState>,
    slot_frames: Query<'w, 's, (&'static CssID, &'static UIWidgetState, &'static mut BorderColor)>,
    button_states: Query<'w, 's, (&'static CssID, &'static UIWidgetState), With<Button>>,
    window_q: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    inventory_panel_q: Query<
        'w,
        's,
        (&'static ComputedNode, &'static UiGlobalTransform),
        With<InventoryMainPanel>,
    >,
    inventory_drop_zone_q: Query<
        'w,
        's,
        (&'static ComputedNode, &'static UiGlobalTransform),
        With<InventoryDropZonePanel>,
    >,
    workbench_panel_q: Query<
        'w,
        's,
        (&'static ComputedNode, &'static UiGlobalTransform),
        With<WorkbenchRecipeMainPanel>,
    >,
    recipe_preview_panel_q:
        Query<'w, 's, (&'static ComputedNode, &'static UiGlobalTransform), With<RecipePreviewDialogPanel>>,
    player_q: Query<'w, 's, &'static Transform, With<Player>>,
    drop_requests: MessageWriter<'w, DropItemRequest>,
    craft_requests: MessageWriter<'w, CraftHandCraftedRequest>,
    work_table_craft_requests: MessageWriter<'w, CraftWorkTableRequest>,
}

/// Runs the `toggle_player_inventory_ui` routine for toggle player inventory ui in the `graphic::components::inventory` module.
fn toggle_player_inventory_ui(
    keyboard: Res<ButtonInput<KeyCode>>,
    global_config: Res<GlobalConfig>,
    mut ui_interaction: ResMut<UiInteractionState>,
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut root: Query<&mut Visibility, With<PlayerInventoryRoot>>,
    mut inventory_ui: ResMut<PlayerInventoryUiState>,
    mut cursor_item: ResMut<InventoryCursorItemState>,
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
        recipe_preview.open = false;
        ui_interaction.inventory_open = false;
        set_inventory_cursor(false, &mut cursor_q);
        if let Ok(mut visible) = root.single_mut() {
            *visible = Visibility::Hidden;
        }
        return;
    }

    if ui_interaction.menu_open
        || ui_interaction.chat_open
        || ui_interaction.structure_menu_open
        || ui_interaction.workbench_menu_open
    {
        return;
    }

    if !keyboard.just_pressed(open_key) {
        return;
    }

    inventory_ui.open = !inventory_ui.open;
    if !inventory_ui.open {
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

/// Runs the `close_player_inventory_ui` routine for close player inventory ui in the `graphic::components::inventory` module.
fn close_player_inventory_ui(
    mut ui_interaction: ResMut<UiInteractionState>,
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut inventory_ui: ResMut<PlayerInventoryUiState>,
    mut root: Query<&mut Visibility, With<PlayerInventoryRoot>>,
    mut cursor_item: ResMut<InventoryCursorItemState>,
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
    recipe_preview.open = false;
    ui_interaction.inventory_open = false;
    set_inventory_cursor(false, &mut cursor_q);
    if let Ok(mut visible) = root.single_mut() {
        *visible = Visibility::Hidden;
    }
}

/// Handles inventory drag and drop for the `graphic::components::inventory` module.
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
        workbench_menu,
        mut recipe_preview,
        game_mode,
        multiplayer_connection,
        creative_panel,
        recipe_registry,
        recipe_type_registry,
        mut cursor_item,
        mut workbench_craft_progress,
        mut workbench_tools,
        mut inventory,
        mut hand_crafted,
        mut work_table_crafting,
        mut slot_frames,
        button_states,
        window_q,
        inventory_panel_q,
        inventory_drop_zone_q,
        workbench_panel_q,
        recipe_preview_panel_q,
        player_q,
        mut drop_requests,
        mut craft_requests,
        mut work_table_craft_requests,
    } = deps;

    let ui_open = inventory_ui.open || workbench_menu.open;
    let hovered_slot = sync_inventory_slot_hover_border(&mut slot_frames, ui_open);

    if !ui_open {
        return;
    }

    if mouse.just_pressed(MouseButton::Left)
        && (is_button_hovered(&button_states, INVENTORY_TRASH_BUTTON_ID)
            || is_button_hovered(&button_states, WORKBENCH_TRASH_BUTTON_ID))
    {
        if !cursor_item.slot.is_empty() {
            cursor_item.slot = InventorySlot::default();
        }
        return;
    }

    if mouse.just_pressed(MouseButton::Left) && recipe_preview.open {
        if is_button_hovered(&button_states, RECIPE_PREVIEW_TAB_PREV_ID)
            && recipe_preview.tab_page > 0
        {
            recipe_preview.tab_page -= 1;
            return;
        }
        if is_button_hovered(&button_states, RECIPE_PREVIEW_TAB_NEXT_ID) {
            let page_count = recipe_preview_tab_page_count(&recipe_preview);
            if recipe_preview.tab_page + 1 < page_count {
                recipe_preview.tab_page += 1;
                return;
            }
        }
        if let Some(tab_slot_index) = hovered_recipe_preview_tab_slot_index(&button_states) {
            let variant_index =
                recipe_preview.tab_page * RECIPE_PREVIEW_TABS_PER_PAGE + tab_slot_index;
            if select_recipe_preview_variant(&mut recipe_preview, variant_index) {
                return;
            }
        }

        if is_button_hovered(&button_states, RECIPE_PREVIEW_FILL_ID) {
            if workbench_menu.open {
                fill_work_table_from_recipe_preview(
                    &recipe_preview,
                    &mut inventory,
                    &mut work_table_crafting,
                    &drop_deps.item_registry,
                );
                reset_workbench_craft_progress(&mut workbench_craft_progress);
                recipe_preview.open = false;
                return;
            }
            if recipe_preview.crafting_type == Some(RecipePreviewCraftingType::HandCrafted) {
                fill_hand_crafted_from_recipe_preview(
                    &recipe_preview,
                    &mut inventory,
                    &mut hand_crafted,
                    &drop_deps.item_registry,
                );
                recipe_preview.open = false;
                return;
            }
        }

        if !is_cursor_inside_panel(&window_q, &recipe_preview_panel_q) {
            recipe_preview.open = false;
            return;
        }
    }

    let recipe_open_key = convert(global_config.input.inventory_recipe_open.as_str())
        .unwrap_or(KeyCode::KeyR);
    if keyboard.just_pressed(recipe_open_key) {
        let Some(recipe_registry) = recipe_registry.as_ref() else {
            return;
        };
        let resolved_hand_recipe = recipe_type_registry.as_ref().and_then(|types| {
            resolve_hand_crafted_recipe(&hand_crafted, recipe_registry, types, &drop_deps.item_registry)
        });
        let resolved_workbench_recipe = recipe_type_registry.as_ref().and_then(|types| {
            resolve_work_table_recipe(
                &work_table_crafting,
                recipe_registry,
                types,
                &drop_deps.item_registry,
            )
        });
        let Some(item_id) = hovered_item_id(
            &button_states,
            &inventory,
            &hand_crafted,
            &work_table_crafting,
            &workbench_tools,
            &creative_panel,
            &recipe_preview,
            resolved_hand_recipe.as_ref(),
            resolved_workbench_recipe.as_ref(),
        ) else {
            return;
        };
        let _ = open_recipe_preview_dialog_for_hovered_item(
            item_id,
            recipe_registry,
            &drop_deps.item_registry,
            &mut recipe_preview,
        );
        return;
    }

    let shift_pressed = keyboard.pressed(KeyCode::ShiftLeft);
    if shift_pressed && mouse.just_pressed(MouseButton::Left) && cursor_item.slot.is_empty() {
        match hovered_slot {
            Some(InventoryUiSlotTarget::Player(slot_index)) => {
                if workbench_menu.open {
                    if transfer_player_slot_to_work_table(
                        slot_index,
                        &mut inventory,
                        &mut work_table_crafting,
                    ) {
                        reset_workbench_craft_progress(&mut workbench_craft_progress);
                    }
                } else {
                    let _ = transfer_player_slot_to_hand_crafted(
                        slot_index,
                        &mut inventory,
                        &mut hand_crafted,
                    );
                }
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
            Some(InventoryUiSlotTarget::WorkTable(slot_index)) => {
                if transfer_work_table_slot_to_inventory(
                    slot_index,
                    &mut work_table_crafting,
                    &mut inventory,
                    &drop_deps.item_registry,
                ) {
                    reset_workbench_craft_progress(&mut workbench_craft_progress);
                }
                return;
            }
            _ => {}
        }
    }

    if mouse.just_pressed(MouseButton::Left)
        && !matches!(game_mode.0, GameMode::Spectator)
    {
        if hovered_slot == Some(InventoryUiSlotTarget::HandCraftedResult) {
            craft_requests.write(CraftHandCraftedRequest);
            return;
        }
        if hovered_slot == Some(InventoryUiSlotTarget::WorkbenchResult) {
            let Some(recipe_registry) = recipe_registry.as_ref() else {
                return;
            };
            let Some(recipe_type_registry) = recipe_type_registry.as_ref() else {
                return;
            };
            let Some(resolved_recipe) = resolve_work_table_recipe(
                &work_table_crafting,
                recipe_registry,
                recipe_type_registry,
                &drop_deps.item_registry,
            ) else {
                return;
            };

            if resolved_recipe.build_time_secs <= f32::EPSILON {
                work_table_craft_requests.write(CraftWorkTableRequest);
                reset_workbench_craft_progress(&mut workbench_craft_progress);
            } else {
                workbench_craft_progress.active = true;
                workbench_craft_progress.elapsed_secs = 0.0;
                workbench_craft_progress.duration_secs = resolved_recipe.build_time_secs;
                workbench_craft_progress.recipe_source_path = resolved_recipe.source_path;
            }
            return;
        }
    }

    if mouse.just_pressed(MouseButton::Middle)
        && let Some(slot_target) = hovered_slot
    {
        let moved = take_half_from_target_to_cursor(
            slot_target,
            &mut cursor_item,
            &mut inventory,
            &mut hand_crafted,
            &mut work_table_crafting,
            &mut workbench_tools,
            &drop_deps.item_registry,
        );
        if moved && matches!(slot_target, InventoryUiSlotTarget::WorkTable(_)) {
            reset_workbench_craft_progress(&mut workbench_craft_progress);
        }
        return;
    }

    if mouse.just_pressed(MouseButton::Right)
        && let Some(slot_target) = hovered_slot
    {
        if !cursor_item.slot.is_empty() {
            let placed = place_one_from_cursor_on_target(
                slot_target,
                &mut cursor_item,
                &mut inventory,
                &mut hand_crafted,
                &mut work_table_crafting,
                &mut workbench_tools,
                &drop_deps.item_registry,
            );
            if placed && matches!(slot_target, InventoryUiSlotTarget::WorkTable(_)) {
                reset_workbench_craft_progress(&mut workbench_craft_progress);
            }
        }
        return;
    }

    if mouse.just_pressed(MouseButton::Left)
        && let Some(slot_target) = hovered_slot
    {
        if cursor_item.slot.is_empty() {
            let moved = take_all_from_target_to_cursor(
                slot_target,
                &mut cursor_item,
                &mut inventory,
                &mut hand_crafted,
                &mut work_table_crafting,
                &mut workbench_tools,
            );
            if moved && matches!(slot_target, InventoryUiSlotTarget::WorkTable(_)) {
                reset_workbench_craft_progress(&mut workbench_craft_progress);
            }
        } else {
            let placed = place_all_from_cursor_on_target(
                slot_target,
                &mut cursor_item,
                &mut inventory,
                &mut hand_crafted,
                &mut work_table_crafting,
                &mut workbench_tools,
                &drop_deps.item_registry,
            );
            if placed && matches!(slot_target, InventoryUiSlotTarget::WorkTable(_)) {
                reset_workbench_craft_progress(&mut workbench_craft_progress);
            }
        }
        return;
    }

    if mouse.just_pressed(MouseButton::Left)
        && hovered_slot.is_none()
        && !cursor_item.slot.is_empty()
    {
        let clicked_inside_inventory = is_cursor_inside_panel(&window_q, &inventory_panel_q);
        let clicked_inside_drop_zone = is_cursor_inside_panel(&window_q, &inventory_drop_zone_q);
        let clicked_inside_workbench = is_cursor_inside_panel(&window_q, &workbench_panel_q);

        if clicked_inside_workbench {
            return;
        }
        if clicked_inside_inventory && !clicked_inside_drop_zone {
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
}

#[derive(SystemParam)]
struct InventoryUiSyncDeps<'w, 's> {
    inventory: Res<'w, PlayerInventory>,
    hand_crafted: Res<'w, HandCraftedState>,
    recipe_preview: Res<'w, RecipePreviewDialogState>,
    recipe_registry: Option<Res<'w, RecipeRegistry>>,
    recipe_type_registry: Option<Res<'w, RecipeTypeRegistry>>,
    item_registry: Res<'w, ItemRegistry>,
    block_registry: Res<'w, BlockRegistry>,
    language: Res<'w, ClientLanguageState>,
    time: Res<'w, Time>,
    asset_server: Res<'w, AssetServer>,
    image_cache: ResMut<'w, ImageCache>,
    images: ResMut<'w, Assets<Image>>,
    inventory_main_panel_q: Query<
        'w,
        's,
        &'static mut Visibility,
        (With<InventoryMainPanel>, Without<RecipePreviewDialogRoot>),
    >,
    inventory_root_bg_q: Query<'w, 's, &'static mut BackgroundColor, With<PlayerInventoryRoot>>,
    inventory_root_zindex_q: Query<'w, 's, &'static mut ZIndex, With<PlayerInventoryRoot>>,
    recipe_preview_root: Query<'w, 's, &'static mut Visibility, With<RecipePreviewDialogRoot>>,
    recipe_preview_panel_node_q: Query<
        'w,
        's,
        &'static mut Node,
        (
            With<RecipePreviewDialogPanel>,
            Without<RecipePreviewInputGrid>,
            Without<Button>,
        ),
    >,
    recipe_preview_input_grid_q: Query<
        'w,
        's,
        &'static mut Node,
        (
            With<RecipePreviewInputGrid>,
            Without<RecipePreviewDialogPanel>,
            Without<Button>,
        ),
    >,
    paragraphs: Query<
        'w,
        's,
        (&'static CssID, &'static mut Paragraph, Option<&'static mut Visibility>),
        (Without<RecipePreviewDialogRoot>, Without<InventoryMainPanel>),
    >,
    slot_buttons: Query<
        'w,
        's,
        (
            &'static CssID,
            &'static mut Button,
            Option<&'static mut UiButtonTone>,
            Option<&'static mut Node>,
        ),
        With<Button>,
    >,
    tab_images: Query<'w, 's, (&'static CssID, &'static mut Img), Without<Button>>,
}

/// Synchronizes player inventory ui for the `graphic::components::inventory` module.
fn sync_player_inventory_ui(
    inventory_ui: Res<PlayerInventoryUiState>,
    workbench_menu: Res<WorkbenchRecipeMenuState>,
    deps: InventoryUiSyncDeps,
) {
    let InventoryUiSyncDeps {
        inventory,
        hand_crafted,
        recipe_preview,
        recipe_registry,
        recipe_type_registry,
        item_registry,
        block_registry,
        language,
        time,
        asset_server,
        mut image_cache,
        mut images,
        mut inventory_main_panel_q,
        mut inventory_root_bg_q,
        mut inventory_root_zindex_q,
        mut recipe_preview_root,
        mut recipe_preview_panel_node_q,
        mut recipe_preview_input_grid_q,
        mut paragraphs,
        mut slot_buttons,
        mut tab_images,
    } = deps;

    if let Ok(mut panel_visibility) = inventory_main_panel_q.single_mut() {
        *panel_visibility = if inventory_ui.open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    if let Ok(mut bg) = inventory_root_bg_q.single_mut() {
        bg.0 = if inventory_ui.open {
            Color::srgba(0.0, 0.0, 0.0, 0.45)
        } else {
            Color::NONE
        };
    }

    if let Ok(mut zindex) = inventory_root_zindex_q.single_mut() {
        *zindex = if inventory_ui.open {
            ZIndex(51)
        } else if workbench_menu.open {
            // Keep cursor/tooltip/recipe-preview above workbench overlay while the inventory panel is hidden.
            ZIndex(60)
        } else {
            ZIndex(51)
        };
    }

    let resolved_recipe = recipe_registry.as_ref().and_then(|recipes| {
        recipe_type_registry
            .as_ref()
            .and_then(|types| resolve_hand_crafted_recipe(&hand_crafted, recipes, types, &item_registry))
    });

    if let Ok(mut visibility) = recipe_preview_root.single_mut() {
        *visibility = if (inventory_ui.open || workbench_menu.open) && recipe_preview.open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    if let Ok(mut panel_node) = recipe_preview_panel_node_q.single_mut() {
        if recipe_preview.crafting_type == Some(RecipePreviewCraftingType::HandCrafted) {
            panel_node.width = Val::Px(780.0);
            panel_node.min_width = Val::Px(780.0);
        } else {
            panel_node.width = Val::Px(860.0);
            panel_node.min_width = Val::Px(860.0);
        }
    }
    if let Ok(mut grid_node) = recipe_preview_input_grid_q.single_mut() {
        if recipe_preview.crafting_type == Some(RecipePreviewCraftingType::HandCrafted) {
            grid_node.width = Val::Px(120.0);
            grid_node.grid_template_columns = RepeatedGridTrack::fr(2, 1.0);
            grid_node.grid_auto_rows = vec![GridTrack::px(56.0)];
        } else {
            grid_node.width = Val::Px(184.0);
            grid_node.grid_template_columns = RepeatedGridTrack::fr(3, 1.0);
            grid_node.grid_auto_rows = vec![GridTrack::px(56.0)];
        }
    }

    for (css_id, mut paragraph, mut maybe_visibility) in &mut paragraphs {
        if css_id.0 == PLAYER_INVENTORY_TOTAL_ID {
            paragraph.text = format!(
                "{} {}",
                language.localize_name_key("KEY_UI_ITEMS"),
                inventory.total_items()
            );
            continue;
        }

        if css_id.0 == RECIPE_PREVIEW_TITLE_ID {
            let next_title = if recipe_preview.open {
                item_registry
                    .def_opt(recipe_preview.result_slot.item_id)
                    .map(|item| {
                        format!(
                            "{}: {}",
                            language.localize_name_key("KEY_UI_RECIPE"),
                            localize_item_name(language.as_ref(), item)
                        )
                    })
                    .unwrap_or_else(|| language.localize_name_key("KEY_UI_RECIPE"))
            } else {
                language.localize_name_key("KEY_UI_RECIPE")
            };
            if paragraph.text != next_title {
                paragraph.text = next_title;
            }
            continue;
        }
        if css_id.0 == RECIPE_PREVIEW_MODE_ID {
            let next_mode = match recipe_preview.crafting_type {
                Some(RecipePreviewCraftingType::HandCrafted) => {
                    language.localize_name_key("KEY_UI_HAND_CRAFTED")
                }
                Some(RecipePreviewCraftingType::WorkTable) => {
                    language.localize_name_key("KEY_UI_WORKBENCH")
                }
                None => language.localize_name_key("KEY_UI_RECIPE"),
            };
            if paragraph.text != next_mode {
                paragraph.text = next_mode;
            }
            continue;
        }
        if css_id.0 == RECIPE_PREVIEW_TAB_TOOLTIP_ID
            && let Some(visibility) = maybe_visibility.as_mut()
        {
            paragraph.text.clear();
            **visibility = Visibility::Hidden;
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
            let hidden_by_layout = slot_index == 0 || slot_index > recipe_preview.input_slot_count;
            sync_badge(
                &mut paragraph,
                visibility,
                slot.count,
                slot.is_empty() || !recipe_preview.open || hidden_by_layout,
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

    for (css_id, mut button, mut maybe_tone, mut maybe_node) in &mut slot_buttons {
        if css_id.0 == RECIPE_PREVIEW_TAB_PREV_ID {
            let has_prev = recipe_preview.open
                && recipe_preview_tab_page_count(&recipe_preview) > 1
                && recipe_preview.tab_page > 0;
            button.text = "<".to_string();
            button.icon_path = None;
            if let Some(tone) = maybe_tone.as_mut() {
                **tone = UiButtonTone::Normal;
            }
            if let Some(node) = maybe_node.as_mut() {
                node.display = if has_prev {
                    Display::Flex
                } else {
                    Display::None
                };
            }
            continue;
        }
        if css_id.0 == RECIPE_PREVIEW_TAB_NEXT_ID {
            let page_count = recipe_preview_tab_page_count(&recipe_preview);
            let has_next =
                recipe_preview.open && page_count > 1 && recipe_preview.tab_page + 1 < page_count;
            button.text = ">".to_string();
            button.icon_path = None;
            if let Some(tone) = maybe_tone.as_mut() {
                **tone = UiButtonTone::Normal;
            }
            if let Some(node) = maybe_node.as_mut() {
                node.display = if has_next {
                    Display::Flex
                } else {
                    Display::None
                };
            }
            continue;
        }
        if let Some(tab_slot_index) = parse_recipe_preview_tab_slot_index(css_id.0.as_str()) {
            let variant_index =
                recipe_preview.tab_page * RECIPE_PREVIEW_TABS_PER_PAGE + tab_slot_index;
            let variant = recipe_preview_variant_at(&recipe_preview, variant_index);
            button.text.clear();
            if variant.is_some() {
                button.icon_path = None;
                if let Some(tone) = maybe_tone.as_mut() {
                    **tone = if variant_index == recipe_preview.selected_variant_index {
                        UiButtonTone::Accent
                    } else {
                        UiButtonTone::Normal
                    };
                }
                if let Some(node) = maybe_node.as_mut() {
                    node.display = if recipe_preview.open {
                        Display::Flex
                    } else {
                        Display::None
                    };
                }
            } else {
                button.icon_path = None;
                if let Some(tone) = maybe_tone.as_mut() {
                    **tone = UiButtonTone::Normal;
                }
                if let Some(node) = maybe_node.as_mut() {
                    node.display = Display::None;
                }
            }
            continue;
        }
        if css_id.0 == RECIPE_PREVIEW_FILL_ID {
            let show_fill = recipe_preview.open
                && (workbench_menu.open
                    || recipe_preview.crafting_type == Some(RecipePreviewCraftingType::HandCrafted));
            button.text = if show_fill {
                "+".to_string()
            } else {
                String::new()
            };
            if let Some(node) = maybe_node.as_mut() {
                node.display = if show_fill {
                    Display::Flex
                } else {
                    Display::None
                };
            }
            continue;
        }

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
            let mut slot = recipe_preview.input_slots.get(slot_index).copied().unwrap_or_default();
            let slot_visible = recipe_preview.open && slot_index < recipe_preview.input_slot_count;
            if slot_visible
                && let Some(variant) = recipe_preview_variant_at(
                    &recipe_preview,
                    recipe_preview.selected_variant_index,
                )
                && let Some(alternatives) = variant.input_slot_alternatives.get(slot_index)
                && !alternatives.is_empty()
            {
                let cycle_index = ((time.elapsed_secs_f64() / 2.0).floor() as usize) % alternatives.len();
                slot.item_id = alternatives[cycle_index];
            }
            if let Some(node) = maybe_node.as_mut() {
                node.display = if slot_visible {
                    Display::Flex
                } else {
                    Display::None
                };
                if recipe_preview.crafting_type == Some(RecipePreviewCraftingType::WorkTable) {
                    let (column, row, centered_column) = match slot_index {
                        0 => (1, 1, false),
                        1 => (1, 2, false),
                        2 => (1, 3, false),
                        3 => (2, 1, true),
                        4 => (2, 2, true),
                        5 => (3, 1, false),
                        6 => (3, 2, false),
                        7 => (3, 3, false),
                        _ => (1, 1, false),
                    };
                    node.grid_column = GridPlacement::start(column);
                    node.grid_row = GridPlacement::start(row);
                    node.margin = if centered_column {
                        UiRect::top(Val::Px(11.0))
                    } else {
                        UiRect::default()
                    };
                } else {
                    let (column, row) = match slot_index {
                        0 => (1, 1),
                        1 => (2, 1),
                        _ => (1, 1),
                    };
                    node.grid_column = GridPlacement::start(column);
                    node.grid_row = GridPlacement::start(row);
                    node.margin = UiRect::default();
                }
            }
            let next_icon = if !slot_visible || slot.is_empty() {
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

    for (css_id, mut img) in &mut tab_images {
        let Some(tab_slot_index) = parse_recipe_preview_tab_icon_slot_index(css_id.0.as_str()) else {
            continue;
        };
        let variant_index = recipe_preview.tab_page * RECIPE_PREVIEW_TABS_PER_PAGE + tab_slot_index;
        let next_src = recipe_preview_variant_at(&recipe_preview, variant_index).and_then(|variant| {
            if recipe_preview.open {
                Some(recipe_preview_tab_icon_path(variant.crafting_type).to_string())
            } else {
                None
            }
        });
        if img.src != next_src {
            img.src = next_src;
        }
    }
}

/// Synchronizes inventory tooltip ui for the `graphic::components::inventory` module.
#[allow(clippy::too_many_arguments)]
fn sync_inventory_tooltip_ui(
    inventory_ui: Res<PlayerInventoryUiState>,
    workbench_menu: Res<WorkbenchRecipeMenuState>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    inventory: Res<PlayerInventory>,
    hand_crafted: Res<HandCraftedState>,
    work_table_crafting: Res<WorkTableCraftingState>,
    workbench_tools: Res<WorkbenchToolSlotsState>,
    creative_panel: Res<CreativePanelState>,
    recipe_preview: Res<RecipePreviewDialogState>,
    recipe_registry: Option<Res<RecipeRegistry>>,
    recipe_type_registry: Option<Res<RecipeTypeRegistry>>,
    item_registry: Res<ItemRegistry>,
    language: Res<ClientLanguageState>,
    slot_states: Query<(&CssID, &UIWidgetState), With<Button>>,
    mut tooltip_root: Query<(&mut Visibility, &mut Node), With<InventoryTooltipRoot>>,
    mut tooltip_text: Query<(&CssID, &mut Paragraph)>,
) {
    let Ok((mut tooltip_visibility, mut tooltip_node)) = tooltip_root.single_mut() else {
        return;
    };

    if !inventory_ui.open && !workbench_menu.open {
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

    let resolved_hand_recipe = recipe_registry.as_ref().and_then(|recipes| {
        recipe_type_registry
            .as_ref()
            .and_then(|types| resolve_hand_crafted_recipe(&hand_crafted, recipes, types, &item_registry))
    });
    let resolved_workbench_recipe = recipe_registry.as_ref().and_then(|recipes| {
        recipe_type_registry
            .as_ref()
            .and_then(|types| {
                resolve_work_table_recipe(&work_table_crafting, recipes, types, &item_registry)
            })
    });

    let hovered_item_id = hovered_item_id(
        &slot_states,
        &inventory,
        &hand_crafted,
        &work_table_crafting,
        &workbench_tools,
        &creative_panel,
        &recipe_preview,
        resolved_hand_recipe.as_ref(),
        resolved_workbench_recipe.as_ref(),
    );
    let hovered_tab_label = if recipe_preview.open {
        hovered_recipe_preview_tab_slot_index(&slot_states).and_then(|tab_slot_index| {
            let variant_index = recipe_preview.tab_page * RECIPE_PREVIEW_TABS_PER_PAGE + tab_slot_index;
            recipe_preview_variant_at(&recipe_preview, variant_index).map(|variant| {
                recipe_preview_variant_tab_label(&recipe_preview, variant, variant_index, &language)
            })
        })
    } else {
        None
    };
    let showing_tab_tooltip = hovered_tab_label.is_some() && hovered_item_id.is_none();

    if let Some(item_id) = hovered_item_id {
        let Some(item) = item_registry.def_opt(item_id) else {
            *tooltip_visibility = Visibility::Hidden;
            return;
        };

        for (css_id, mut paragraph) in &mut tooltip_text {
            if css_id.0 == INVENTORY_TOOLTIP_NAME_ID {
                let localized = localize_item_name(language.as_ref(), item);
                if paragraph.text != localized {
                    paragraph.text = localized;
                }
            } else if css_id.0 == INVENTORY_TOOLTIP_KEY_ID
                && paragraph.text != item.localized_name
            {
                paragraph.text = item.localized_name.clone();
            }
        }
    } else if let Some(tab_label) = hovered_tab_label {
        for (css_id, mut paragraph) in &mut tooltip_text {
            if css_id.0 == INVENTORY_TOOLTIP_NAME_ID {
                if paragraph.text != tab_label {
                    paragraph.text = tab_label.clone();
                }
            } else if css_id.0 == INVENTORY_TOOLTIP_KEY_ID && !paragraph.text.is_empty() {
                paragraph.text.clear();
            }
        }
    } else {
        *tooltip_visibility = Visibility::Hidden;
        return;
    }

    let offset = Vec2::new(14.0, 16.0);
    let mut tooltip_pos = cursor_pos + offset;
    tooltip_pos.x = tooltip_pos.x.clamp(0.0, (window.width() - 220.0).max(0.0));
    tooltip_pos.y = tooltip_pos.y.clamp(0.0, (window.height() - 72.0).max(0.0));
    tooltip_node.left = Val::Px(tooltip_pos.x);
    tooltip_node.top = Val::Px(tooltip_pos.y);
    tooltip_node.align_items = if showing_tab_tooltip {
        AlignItems::Center
    } else {
        AlignItems::Start
    };

    *tooltip_visibility = Visibility::Inherited;
}

/// Synchronizes inventory cursor item ui for the `graphic::components::inventory` module.
#[allow(clippy::too_many_arguments)]
fn sync_inventory_cursor_item_ui(
    inventory_ui: Res<PlayerInventoryUiState>,
    workbench_menu: Res<WorkbenchRecipeMenuState>,
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

    if (!inventory_ui.open && !workbench_menu.open) || cursor_item.slot.is_empty() {
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

    let cursor_offset = Vec2::new(8.0, 2.0);
    let max_x = (window.width() - 56.0).max(0.0);
    let max_y = (window.height() - 56.0).max(0.0);
    root_node.left = Val::Px((cursor_pos.x + cursor_offset.x).clamp(0.0, max_x));
    root_node.top = Val::Px((cursor_pos.y + cursor_offset.y).clamp(0.0, max_y));

    let count_text = cursor_item.slot.count.to_string();
    for (mut paragraph, mut badge_visibility) in &mut cursor_badges {
        if paragraph.text != count_text {
            paragraph.text = count_text.clone();
        }
        *badge_visibility = Visibility::Inherited;
    }

    *root_visibility = Visibility::Inherited;
}

/// Sets inventory cursor for the `graphic::components::inventory` module.
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

/// Synchronizes inventory slot hover border for the `graphic::components::inventory` module.
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
        } else if css_id.0 == WORKBENCH_RESULT_FRAME_ID {
            Some(InventoryUiSlotTarget::WorkbenchResult)
        } else if let Some(slot_index) = parse_workbench_player_inventory_slot_index(css_id.0.as_str()) {
            Some(InventoryUiSlotTarget::Player(slot_index))
        } else if let Some(slot_index) = parse_workbench_craft_slot_index(css_id.0.as_str()) {
            Some(InventoryUiSlotTarget::WorkTable(slot_index))
        } else if let Some(slot_index) = parse_workbench_tool_slot_index(css_id.0.as_str()) {
            Some(InventoryUiSlotTarget::WorkbenchTool(slot_index))
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

/// Runs the `transfer_player_slot_to_hand_crafted` routine for transfer player slot to hand crafted in the `graphic::components::inventory` module.
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

/// Runs the `transfer_player_slot_to_work_table` routine for transfer player slot to work table in the `graphic::components::inventory` module.
fn transfer_player_slot_to_work_table(
    slot_index: usize,
    inventory: &mut PlayerInventory,
    work_table: &mut WorkTableCraftingState,
) -> bool {
    if slot_index >= PLAYER_INVENTORY_SLOTS {
        return false;
    }
    let source_slot = inventory.slots[slot_index];
    if source_slot.is_empty() {
        return false;
    }

    let Some(free_index) = work_table
        .input_slots
        .iter()
        .position(InventorySlot::is_empty)
    else {
        return false;
    };

    work_table.input_slots[free_index] = source_slot;
    inventory.slots[slot_index] = InventorySlot::default();
    true
}

/// Runs the `transfer_hand_crafted_slot_to_inventory` routine for transfer hand crafted slot to inventory in the `graphic::components::inventory` module.
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

/// Runs the `transfer_work_table_slot_to_inventory` routine for transfer work table slot to inventory in the `graphic::components::inventory` module.
fn transfer_work_table_slot_to_inventory(
    slot_index: usize,
    work_table: &mut WorkTableCraftingState,
    inventory: &mut PlayerInventory,
    item_registry: &ItemRegistry,
) -> bool {
    if slot_index >= WORK_TABLE_CRAFTING_INPUT_SLOTS {
        return false;
    }
    let source_slot = work_table.input_slots[slot_index];
    if source_slot.is_empty() {
        return false;
    }

    let leftover = inventory.add_item(source_slot.item_id, source_slot.count, item_registry);
    if leftover == source_slot.count {
        return false;
    }
    if leftover == 0 {
        work_table.input_slots[slot_index] = InventorySlot::default();
    } else {
        work_table.input_slots[slot_index].count = leftover;
    }
    true
}

/// Runs the `take_all_from_target_to_cursor` routine for take all from target to cursor in the `graphic::components::inventory` module.
fn take_all_from_target_to_cursor(
    slot_target: InventoryUiSlotTarget,
    cursor_item: &mut InventoryCursorItemState,
    inventory: &mut PlayerInventory,
    hand_crafted: &mut HandCraftedState,
    work_table: &mut WorkTableCraftingState,
    workbench_tools: &mut WorkbenchToolSlotsState,
) -> bool {
    if !cursor_item.slot.is_empty() {
        return false;
    }

    match slot_target {
        InventoryUiSlotTarget::Player(index) if index < PLAYER_INVENTORY_SLOTS => {
            take_all_from_slot_to_cursor(&mut inventory.slots[index], &mut cursor_item.slot)
        }
        InventoryUiSlotTarget::HandCrafted(index) if index < HAND_CRAFTED_INPUT_SLOTS => {
            take_all_from_slot_to_cursor(&mut hand_crafted.input_slots[index], &mut cursor_item.slot)
        }
        InventoryUiSlotTarget::WorkTable(index) if index < WORK_TABLE_CRAFTING_INPUT_SLOTS => {
            take_all_from_slot_to_cursor(&mut work_table.input_slots[index], &mut cursor_item.slot)
        }
        InventoryUiSlotTarget::WorkbenchTool(index) if index < workbench_tools.slots.len() => {
            take_all_from_slot_to_cursor(&mut workbench_tools.slots[index], &mut cursor_item.slot)
        }
        _ => false,
    }
}

/// Runs the `take_half_from_target_to_cursor` routine for take half from target to cursor in the `graphic::components::inventory` module.
fn take_half_from_target_to_cursor(
    slot_target: InventoryUiSlotTarget,
    cursor_item: &mut InventoryCursorItemState,
    inventory: &mut PlayerInventory,
    hand_crafted: &mut HandCraftedState,
    work_table: &mut WorkTableCraftingState,
    workbench_tools: &mut WorkbenchToolSlotsState,
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
        InventoryUiSlotTarget::WorkTable(index) if index < WORK_TABLE_CRAFTING_INPUT_SLOTS => {
            take_half_from_slot_to_cursor(
                &mut work_table.input_slots[index],
                &mut cursor_item.slot,
                item_registry,
            )
        }
        InventoryUiSlotTarget::WorkbenchTool(index) if index < workbench_tools.slots.len() => {
            take_half_from_slot_to_cursor(
                &mut workbench_tools.slots[index],
                &mut cursor_item.slot,
                item_registry,
            )
        }
        _ => false,
    }
}

/// Runs the `place_one_from_cursor_on_target` routine for place one from cursor on target in the `graphic::components::inventory` module.
fn place_one_from_cursor_on_target(
    slot_target: InventoryUiSlotTarget,
    cursor_item: &mut InventoryCursorItemState,
    inventory: &mut PlayerInventory,
    hand_crafted: &mut HandCraftedState,
    work_table: &mut WorkTableCraftingState,
    workbench_tools: &mut WorkbenchToolSlotsState,
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
        InventoryUiSlotTarget::WorkTable(index) if index < WORK_TABLE_CRAFTING_INPUT_SLOTS => {
            place_one_from_cursor(
                &mut work_table.input_slots[index],
                &mut cursor_item.slot,
                item_registry,
            )
        }
        InventoryUiSlotTarget::WorkbenchTool(index) if index < workbench_tools.slots.len() => {
            place_one_from_cursor(
                &mut workbench_tools.slots[index],
                &mut cursor_item.slot,
                item_registry,
            )
        }
        _ => false,
    }
}

/// Runs the `place_all_from_cursor_on_target` routine for place all from cursor on target in the `graphic::components::inventory` module.
fn place_all_from_cursor_on_target(
    slot_target: InventoryUiSlotTarget,
    cursor_item: &mut InventoryCursorItemState,
    inventory: &mut PlayerInventory,
    hand_crafted: &mut HandCraftedState,
    work_table: &mut WorkTableCraftingState,
    workbench_tools: &mut WorkbenchToolSlotsState,
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
        InventoryUiSlotTarget::WorkTable(index) if index < WORK_TABLE_CRAFTING_INPUT_SLOTS => {
            place_all_from_cursor(
                &mut work_table.input_slots[index],
                &mut cursor_item.slot,
                item_registry,
            )
        }
        InventoryUiSlotTarget::WorkbenchTool(index) if index < workbench_tools.slots.len() => {
            place_all_from_cursor(
                &mut workbench_tools.slots[index],
                &mut cursor_item.slot,
                item_registry,
            )
        }
        _ => false,
    }
}

/// Runs the `take_all_from_slot_to_cursor` routine for take all from slot to cursor in the `graphic::components::inventory` module.
fn take_all_from_slot_to_cursor(slot: &mut InventorySlot, cursor_slot: &mut InventorySlot) -> bool {
    if slot.is_empty() || !cursor_slot.is_empty() {
        return false;
    }

    *cursor_slot = *slot;
    *slot = InventorySlot::default();
    true
}

/// Runs the `take_half_from_slot_to_cursor` routine for take half from slot to cursor in the `graphic::components::inventory` module.
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

/// Runs the `place_one_from_cursor` routine for place one from cursor in the `graphic::components::inventory` module.
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

/// Runs the `place_all_from_cursor` routine for place all from cursor in the `graphic::components::inventory` module.
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

/// Runs the `decrement_slot_count` routine for decrement slot count in the `graphic::components::inventory` module.
fn decrement_slot_count(slot: &mut InventorySlot) {
    if slot.count <= 1 {
        *slot = InventorySlot::default();
    } else {
        slot.count -= 1;
    }
}

/// Parses recipe preview input index for the `graphic::components::inventory` module.
fn parse_recipe_preview_input_index(css_id: &str) -> Option<usize> {
    css_id
        .strip_prefix(RECIPE_PREVIEW_INPUT_FRAME_PREFIX)
        .and_then(|slot_number| slot_number.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
        .filter(|index| *index < RECIPE_PREVIEW_INPUT_SLOTS)
}

fn parse_recipe_preview_tab_slot_index(css_id: &str) -> Option<usize> {
    css_id
        .strip_prefix(RECIPE_PREVIEW_TAB_PREFIX)
        .and_then(|slot_number| slot_number.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
        .filter(|index| *index < RECIPE_PREVIEW_TABS_PER_PAGE)
}

fn parse_recipe_preview_tab_icon_slot_index(css_id: &str) -> Option<usize> {
    css_id
        .strip_prefix(RECIPE_PREVIEW_TAB_ICON_PREFIX)
        .and_then(|slot_number| slot_number.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
        .filter(|index| *index < RECIPE_PREVIEW_TABS_PER_PAGE)
}

fn hovered_recipe_preview_tab_slot_index(
    button_states: &Query<(&CssID, &UIWidgetState), With<Button>>,
) -> Option<usize> {
    for (css_id, state) in button_states.iter() {
        if !state.hovered {
            continue;
        }
        if let Some(tab_index) = parse_recipe_preview_tab_slot_index(css_id.0.as_str()) {
            return Some(tab_index);
        }
    }
    None
}

fn recipe_preview_tab_page_count(recipe_preview: &RecipePreviewDialogState) -> usize {
    if recipe_preview.variants.is_empty() {
        return 0;
    }
    recipe_preview
        .variants
        .len()
        .div_ceil(RECIPE_PREVIEW_TABS_PER_PAGE)
}

fn recipe_preview_variant_tab_label(
    recipe_preview: &RecipePreviewDialogState,
    variant: &RecipePreviewVariant,
    variant_index: usize,
    language: &ClientLanguageState,
) -> String {
    let base = match variant.crafting_type {
        RecipePreviewCraftingType::HandCrafted => language.localize_name_key("KEY_UI_HAND_CRAFTED"),
        RecipePreviewCraftingType::WorkTable => language.localize_name_key("KEY_UI_WORKBENCH"),
    };

    let type_total = recipe_preview
        .variants
        .iter()
        .filter(|entry| entry.crafting_type == variant.crafting_type)
        .count();
    if type_total <= 1 {
        return base;
    }

    let ordinal = recipe_preview
        .variants
        .iter()
        .take(variant_index + 1)
        .filter(|entry| entry.crafting_type == variant.crafting_type)
        .count();
    format!("{base} {ordinal}")
}

fn recipe_preview_tab_icon_path(crafting_type: RecipePreviewCraftingType) -> &'static str {
    match crafting_type {
        RecipePreviewCraftingType::HandCrafted => "assets/textures/icons/hand_crafted_icon.png",
        RecipePreviewCraftingType::WorkTable => "assets/textures/icons/workbench_icon.png",
    }
}

/// Runs the `hovered_item_id` routine for hovered item id in the `graphic::components::inventory` module.
fn hovered_item_id(
    slot_states: &Query<(&CssID, &UIWidgetState), With<Button>>,
    inventory: &PlayerInventory,
    hand_crafted: &HandCraftedState,
    work_table_crafting: &WorkTableCraftingState,
    workbench_tools: &WorkbenchToolSlotsState,
    creative_panel: &CreativePanelState,
    recipe_preview: &RecipePreviewDialogState,
    resolved_hand_recipe: Option<&ResolvedRecipe>,
    resolved_workbench_recipe: Option<&ResolvedRecipe>,
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

        if let Some(slot_index) = parse_workbench_player_inventory_slot_index(css_id.0.as_str())
            && let Some(slot) = inventory.slots.get(slot_index)
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

        if let Some(slot_index) = parse_workbench_craft_slot_index(css_id.0.as_str())
            && let Some(slot) = work_table_crafting.input_slots.get(slot_index)
            && !slot.is_empty()
        {
            return Some(slot.item_id);
        }

        if let Some(slot_index) = parse_workbench_tool_slot_index(css_id.0.as_str())
            && let Some(slot) = workbench_tools.slots.get(slot_index)
            && !slot.is_empty()
        {
            return Some(slot.item_id);
        }

        if css_id.0 == HAND_CRAFTED_RESULT_FRAME_ID
            && let Some(recipe) = resolved_hand_recipe
        {
            return Some(recipe.result.item_id);
        }

        if css_id.0 == WORKBENCH_RESULT_FRAME_ID
            && let Some(recipe) = resolved_workbench_recipe
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

        if let Some(index) = parse_workbench_item_slot_index(css_id.0.as_str())
            && let Some(item_id) = creative_panel.item_at_page_slot(index)
        {
            return Some(item_id);
        }

        if recipe_preview.open
            && let Some(index) = parse_recipe_preview_input_index(css_id.0.as_str())
            && index < recipe_preview.input_slot_count
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

/// Runs the `flush_hand_crafted_inputs_to_inventory` routine for flush hand crafted inputs to inventory in the `graphic::components::inventory` module.
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

/// Runs the `flush_cursor_item_to_inventory` routine for flush cursor item to inventory in the `graphic::components::inventory` module.
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

fn fill_work_table_from_recipe_preview(
    recipe_preview: &RecipePreviewDialogState,
    inventory: &mut PlayerInventory,
    work_table_crafting: &mut WorkTableCraftingState,
    item_registry: &ItemRegistry,
) {
    let selected_variant =
        recipe_preview_variant_at(recipe_preview, recipe_preview.selected_variant_index);

    for slot_index in 0..WORK_TABLE_CRAFTING_INPUT_SLOTS {
        let mut required = recipe_preview
            .input_slots
            .get(slot_index)
            .copied()
            .unwrap_or_default();
        if required.is_empty() {
            continue;
        }
        let alternatives = selected_variant
            .and_then(|variant| variant.input_slot_alternatives.get(slot_index))
            .cloned()
            .unwrap_or_default();

        let Some(target) = work_table_crafting.input_slots.get_mut(slot_index) else {
            continue;
        };
        if !target.is_empty() && alternatives.contains(&target.item_id) {
            required.item_id = target.item_id;
        } else if !alternatives.is_empty() {
            let mut chosen = required.item_id;
            for alt in &alternatives {
                if count_item_in_inventory(inventory, *alt) > 0 {
                    chosen = *alt;
                    break;
                }
            }
            required.item_id = chosen;
        }

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

fn fill_hand_crafted_from_recipe_preview(
    recipe_preview: &RecipePreviewDialogState,
    inventory: &mut PlayerInventory,
    hand_crafted: &mut HandCraftedState,
    item_registry: &ItemRegistry,
) {
    let selected_variant =
        recipe_preview_variant_at(recipe_preview, recipe_preview.selected_variant_index);

    for slot_index in 0..HAND_CRAFTED_INPUT_SLOTS {
        let mut required = recipe_preview
            .input_slots
            .get(slot_index)
            .copied()
            .unwrap_or_default();
        if required.is_empty() {
            continue;
        }
        let alternatives = selected_variant
            .and_then(|variant| variant.input_slot_alternatives.get(slot_index))
            .cloned()
            .unwrap_or_default();

        let Some(target) = hand_crafted.input_slots.get_mut(slot_index) else {
            continue;
        };
        if !target.is_empty() && alternatives.contains(&target.item_id) {
            required.item_id = target.item_id;
        } else if !alternatives.is_empty() {
            let mut chosen = required.item_id;
            for alt in &alternatives {
                if count_item_in_inventory(inventory, *alt) > 0 {
                    chosen = *alt;
                    break;
                }
            }
            required.item_id = chosen;
        }

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

fn count_item_in_inventory(inventory: &PlayerInventory, item_id: ItemId) -> u16 {
    if item_id == 0 {
        return 0;
    }
    let mut count = 0u16;
    for slot in &inventory.slots {
        if slot.item_id == item_id {
            count = count.saturating_add(slot.count);
        }
    }
    count
}

/// Runs the `take_items_from_inventory` routine for take items from inventory in the `graphic::components::inventory` module.
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

/// Synchronizes badge for the `graphic::components::inventory` module.
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

/// Checks whether button hovered in the `graphic::components::inventory` module.
fn is_button_hovered(
    button_states: &Query<(&CssID, &UIWidgetState), With<Button>>,
    css_id_target: &str,
) -> bool {
    button_states
        .iter()
        .any(|(css_id, state)| css_id.0 == css_id_target && state.hovered)
}

/// Checks whether cursor inside panel in the `graphic::components::inventory` module.
fn is_cursor_inside_panel<F: QueryFilter>(
    window_q: &Query<&Window, With<PrimaryWindow>>,
    panel_q: &Query<(&ComputedNode, &UiGlobalTransform), F>,
) -> bool {
    let Ok(window) = window_q.single() else {
        return false;
    };
    let Some(cursor_pos) = window.physical_cursor_position() else {
        return false;
    };
    let Ok((computed_node, transform)) = panel_q.single() else {
        return false;
    };

    if !computed_node.size().cmpgt(Vec2::ZERO).all() {
        return false;
    }
    computed_node.contains_point(*transform, cursor_pos)
}
