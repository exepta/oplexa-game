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
    chest_inventory: &ChestInventoryUiState,
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

        if let Some(slot_index) = parse_chest_player_inventory_slot_index(css_id.0.as_str())
            && let Some(slot) = inventory.slots.get(slot_index)
            && !slot.is_empty()
        {
            return Some(slot.item_id);
        }

        if let Some(slot_index) = parse_chest_slot_index(css_id.0.as_str())
            && let Some(slot) = chest_inventory.slots.get(slot_index)
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

        if let Some(index) = parse_chest_item_slot_index(css_id.0.as_str())
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
