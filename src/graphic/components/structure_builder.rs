const WORKBENCH_STRUCTURE_RECIPE_NAME: &str = "work_table";

fn handle_open_structure_build_menu_request(
    mut requests: MessageReader<OpenStructureBuildMenuRequest>,
    ui_interaction: Res<UiInteractionState>,
    mut structure_menu: ResMut<StructureBuildMenuState>,
    mut active_structure_recipe: ResMut<ActiveStructureRecipeState>,
    mut active_structure_placement: ResMut<ActiveStructurePlacementState>,
) {
    let mut requested = false;
    for _ in requests.read() {
        requested = true;
    }
    if !requested {
        return;
    }
    if ui_interaction.blocks_game_input() {
        return;
    }

    structure_menu.open = true;
    active_structure_recipe.selected_recipe_name = None;
    active_structure_placement.rotation_quarters = 0;
}

fn handle_structure_build_menu_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    global_config: Res<GlobalConfig>,
    inventory: Res<PlayerInventory>,
    hotbar_state: Res<HotbarSelectionState>,
    item_registry: Res<ItemRegistry>,
    structure_recipe_registry: Option<Res<BuildingStructureRecipeRegistry>>,
    mut structure_menu: ResMut<StructureBuildMenuState>,
    mut active_structure_recipe: ResMut<ActiveStructureRecipeState>,
    mut active_structure_placement: ResMut<ActiveStructurePlacementState>,
    mut widgets: Query<(&CssID, &mut UIWidgetState), With<Button>>,
) {
    if !is_hammer_selected(&inventory, &hotbar_state, &item_registry) {
        structure_menu.open = false;
        active_structure_recipe.selected_recipe_name = None;
        active_structure_placement.rotation_quarters = 0;
        return;
    }

    let close_key =
        convert(global_config.input.ui_close_back.as_str()).expect("Invalid close/back key");
    if structure_menu.open && keyboard.just_pressed(close_key) {
        structure_menu.open = false;
        return;
    }

    if !structure_menu.open {
        return;
    }

    let Some(structure_recipe_registry) = structure_recipe_registry.as_ref() else {
        return;
    };
    let Some(recipe) = structure_recipe_registry.recipe_by_name(WORKBENCH_STRUCTURE_RECIPE_NAME)
    else {
        return;
    };

    let can_build = structure_recipe_can_build(&inventory, recipe);

    for (css_id, mut state) in &mut widgets {
        if !state.checked {
            continue;
        }
        state.checked = false;

        if css_id.0 != STRUCTURE_BUILD_WORKBENCH_ID {
            continue;
        }
        if !can_build {
            return;
        }
        active_structure_recipe.selected_recipe_name = Some(recipe.name.clone());
        active_structure_placement.rotation_quarters = 0;
        structure_menu.open = false;
        return;
    }
}

fn rotate_structure_preview_with_scroll(
    mut wheel_reader: MessageReader<MouseWheel>,
    ui_interaction: Res<UiInteractionState>,
    inventory: Res<PlayerInventory>,
    hotbar_state: Res<HotbarSelectionState>,
    item_registry: Res<ItemRegistry>,
    active_structure_recipe: Res<ActiveStructureRecipeState>,
    mut active_structure_placement: ResMut<ActiveStructurePlacementState>,
) {
    if ui_interaction.menu_open
        || ui_interaction.inventory_open
        || ui_interaction.chat_open
        || ui_interaction.structure_menu_open
        || ui_interaction.workbench_menu_open
    {
        for _ in wheel_reader.read() {}
        return;
    }

    if active_structure_recipe.selected_recipe_name.is_none() {
        for _ in wheel_reader.read() {}
        return;
    }
    if !is_hammer_selected(&inventory, &hotbar_state, &item_registry) {
        for _ in wheel_reader.read() {}
        return;
    }

    let mut total_raw = 0.0_f32;
    for wheel in wheel_reader.read() {
        let raw = match wheel.unit {
            MouseScrollUnit::Line => wheel.y,
            MouseScrollUnit::Pixel => wheel.y / 24.0,
        };
        if raw.abs() < f32::EPSILON {
            continue;
        }
        total_raw += raw;
    }
    if total_raw.abs() < f32::EPSILON {
        return;
    }
    // Normalize wheel input so one scroll gesture rotates by one quarter-turn,
    // independent from per-device line/pixel delta magnitudes.
    let total_steps = if total_raw > 0.0 { 1 } else { -1 };

    active_structure_placement.rotation_quarters =
        (active_structure_placement.rotation_quarters + total_steps).rem_euclid(4);
}

fn sync_structure_build_menu_ui(
    structure_menu: Res<StructureBuildMenuState>,
    inventory: Res<PlayerInventory>,
    structure_recipe_registry: Option<Res<BuildingStructureRecipeRegistry>>,
    language: Res<ClientLanguageState>,
    mut root_q: Query<&mut Visibility, With<StructureBuildRoot>>,
    mut button_q: Query<
        (
            &CssID,
            &mut Button,
            &mut UIWidgetState,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        With<Button>,
    >,
    mut paragraph_q: Query<(&CssID, &mut Paragraph)>,
) {
    if let Ok(mut visibility) = root_q.single_mut() {
        *visibility = if structure_menu.open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    let recipe = structure_recipe_registry
        .as_ref()
        .and_then(|registry| registry.recipe_by_name(WORKBENCH_STRUCTURE_RECIPE_NAME));
    let can_build = recipe.is_some_and(|recipe| structure_recipe_can_build(&inventory, recipe));

    for (css_id, mut button, mut state, mut background, mut border) in &mut button_q {
        if css_id.0 != STRUCTURE_BUILD_WORKBENCH_ID {
            continue;
        }

        let label = language.localize_name_key("KEY_UI_WORKBENCH");
        if button.text != label {
            button.text = label;
        }

        state.disabled = !can_build;
        if state.disabled {
            state.checked = false;
            background.0 = Color::srgba(0.17, 0.17, 0.18, 0.88);
            let disabled_border = Color::srgba(0.28, 0.28, 0.30, 0.95);
            border.top = disabled_border;
            border.right = disabled_border;
            border.bottom = disabled_border;
            border.left = disabled_border;
        } else {
            background.0 = color_accent();
            let border_col = color_background_hover();
            border.top = border_col;
            border.right = border_col;
            border.bottom = border_col;
            border.left = border_col;
        }
    }

    for (css_id, mut paragraph) in &mut paragraph_q {
        if css_id.0 != STRUCTURE_BUILD_HINT_ID {
            continue;
        }
        let text = if recipe.is_none() {
            language.localize_name_key("KEY_UI_BUILD_HINT_MISSING_RECIPE")
        } else if can_build {
            language.localize_name_key("KEY_UI_BUILD_HINT_READY")
        } else {
            language.localize_name_key("KEY_UI_BUILD_HINT_MISSING_MATERIAL")
        };
        if paragraph.text != text {
            paragraph.text = text;
        }
    }
}

fn close_structure_build_menu_ui(
    mut ui_interaction: ResMut<UiInteractionState>,
    mut structure_menu: ResMut<StructureBuildMenuState>,
    mut active_structure_recipe: ResMut<ActiveStructureRecipeState>,
    mut active_structure_placement: ResMut<ActiveStructurePlacementState>,
    mut root_q: Query<&mut Visibility, With<StructureBuildRoot>>,
) {
    structure_menu.open = false;
    active_structure_recipe.selected_recipe_name = None;
    active_structure_placement.rotation_quarters = 0;
    ui_interaction.structure_menu_open = false;
    if let Ok(mut visibility) = root_q.single_mut() {
        *visibility = Visibility::Hidden;
    }
}

fn structure_recipe_can_build(inventory: &PlayerInventory, recipe: &BuildingStructureRecipe) -> bool {
    recipe.requirements.iter().all(|requirement| {
        inventory_item_total(inventory, requirement.item_id) >= requirement.count as u32
    })
}

fn inventory_item_total(inventory: &PlayerInventory, item_id: ItemId) -> u32 {
    let mut total = 0u32;
    for slot in &inventory.slots {
        if slot.is_empty() || slot.item_id != item_id {
            continue;
        }
        total = total.saturating_add(slot.count as u32);
    }
    total
}

fn is_hammer_selected(
    inventory: &PlayerInventory,
    hotbar_state: &HotbarSelectionState,
    item_registry: &ItemRegistry,
) -> bool {
    let Some(slot) = inventory.slots.get(hotbar_state.selected_index) else {
        return false;
    };
    if slot.is_empty() {
        return false;
    }
    let Some(item) = item_registry.def_opt(slot.item_id) else {
        return false;
    };
    item.localized_name == "oplexa:hammer" || item.key == "hammer"
}
