use api::handlers::inventory::apply_creative_panel_click;

/// Represents recipe preview crafting data json used by the `graphic::components::inventory_creative` module.
#[derive(Deserialize)]
struct RecipePreviewCraftDataJson {
    #[serde(default)]
    craft: std::collections::HashMap<String, RecipePreviewCraftEntryJson>,
    #[serde(default)]
    ingredients: Vec<RecipePreviewCraftEntryJson>,
}

/// Represents recipe preview crafting entry json used by the `graphic::components::inventory_creative` module.
#[derive(Deserialize, Clone)]
struct RecipePreviewCraftEntryJson {
    #[serde(default)]
    item: String,
    #[serde(default)]
    group: String,
    #[serde(default = "recipe_preview_default_required_count")]
    count: u16,
}

/// Runs the `recipe_preview_default_required_count` routine for recipe preview default required count in the `graphic::components::inventory_creative` module.
#[inline]
fn recipe_preview_default_required_count() -> u16 {
    1
}

/// Synchronizes creative panel state from registry for the `graphic::components::inventory_creative` module.
fn sync_creative_panel_state_from_registry(
    item_registry: Res<ItemRegistry>,
    mut creative_ui: ResMut<CreativePanelUiState>,
    mut creative_panel: ResMut<CreativePanelState>,
) {
    let mut expected_items = 0usize;
    for raw in 1..item_registry.defs.len() {
        let Ok(item_id) = u16::try_from(raw) else {
            break;
        };
        let Some(def) = item_registry.def_opt(item_id) else {
            continue;
        };
        if def.block_item && is_creative_hidden_flow_variant_item(def.key.as_str()) {
            continue;
        }
        expected_items += 1;
    }
    if !creative_ui.synced_once || creative_panel.item_count() != expected_items {
        creative_panel.rebuild_from_registry(&item_registry);
        creative_ui.synced_once = true;
    }

    creative_panel.clamp_page();
}

#[inline]
fn is_creative_hidden_flow_variant_item(key: &str) -> bool {
    let Some((_, suffix)) = key.rsplit_once("_flow_") else {
        return false;
    };
    !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit())
}

/// Handles creative panel navigation for the `graphic::components::inventory_creative` module.
fn handle_creative_panel_navigation(
    mouse: Res<ButtonInput<MouseButton>>,
    inventory_ui: Res<PlayerInventoryUiState>,
    mut creative_panel: ResMut<CreativePanelState>,
    slot_buttons: Query<(&CssID, &UIWidgetState), With<Button>>,
) {
    if !inventory_ui.open || !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let mut hovered_button: Option<&str> = None;
    for (css_id, state) in &slot_buttons {
        if state.hovered {
            hovered_button = Some(css_id.0.as_str());
            break;
        }
    }

    let Some(css_id) = hovered_button else {
        return;
    };

    if css_id == CREATIVE_PANEL_PREV_ID {
        let _ = creative_panel.prev_page();
        return;
    }
    if css_id == CREATIVE_PANEL_NEXT_ID {
        let _ = creative_panel.next_page();
    }
}

/// Handles creative panel clicks for the `graphic::components::inventory_creative` module.
fn handle_creative_panel_clicks(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    inventory_ui: Res<PlayerInventoryUiState>,
    game_mode: Res<GameModeState>,
    creative_panel: Res<CreativePanelState>,
    recipe_registry: Option<Res<RecipeRegistry>>,
    item_registry: Res<ItemRegistry>,
    cursor_item: Res<InventoryCursorItemState>,
    mut recipe_preview: ResMut<RecipePreviewDialogState>,
    mut inventory: ResMut<PlayerInventory>,
    mut slot_frames: Query<(&CssID, &UIWidgetState, &mut BorderColor), With<Button>>,
) {
    let hovered_slot = sync_creative_slot_hover_border(&mut slot_frames, inventory_ui.open);

    if !inventory_ui.open || !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    if !cursor_item.slot.is_empty() {
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
            let _ = apply_creative_panel_click(
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

/// Synchronizes creative panel ui for the `graphic::components::inventory_creative` module.
#[allow(clippy::too_many_arguments)]
fn sync_creative_panel_ui(
    creative_panel: Res<CreativePanelState>,
    game_mode: Res<GameModeState>,
    language: Res<ClientLanguageState>,
    item_registry: Res<ItemRegistry>,
    block_registry: Res<BlockRegistry>,
    asset_server: Res<AssetServer>,
    mut image_cache: ResMut<ImageCache>,
    mut images: ResMut<Assets<Image>>,
    mut paragraphs: Query<(&CssID, &mut Paragraph)>,
    mut buttons: Query<(&CssID, &mut Button, &mut UiButtonTone), With<Button>>,
    mut button_visibility: Query<(&CssID, &mut Visibility), With<Button>>,
) {
    for (css_id, mut paragraph) in &mut paragraphs {
        if css_id.0 == CREATIVE_PANEL_TOTAL_ID {
            paragraph.text = format!(
                "{} {}",
                language.localize_name_key("KEY_UI_REGISTERED"),
                creative_panel.item_count()
            );
            continue;
        }
        if css_id.0 == CREATIVE_PANEL_PAGE_ID {
            paragraph.text = creative_panel.page_label();
            continue;
        }
        if css_id.0 == CREATIVE_RECIPE_HINT_ID {
            paragraph.text = match game_mode.0 {
                GameMode::Creative => language.localize_name_key("KEY_UI_RECIPE_HINT_CREATIVE"),
                GameMode::Survival => language.localize_name_key("KEY_UI_RECIPE_HINT_SURVIVAL"),
                GameMode::Spectator => {
                    language.localize_name_key("KEY_UI_RECIPE_HINT_SPECTATOR")
                }
            };
            continue;
        }
    }

    for (css_id, mut button, _tone) in &mut buttons {
        let Some(slot_index) = parse_creative_slot_index(css_id.0.as_str()) else {
            continue;
        };

        let item_id = creative_panel.item_at_page_slot(slot_index);

        if !button.text.is_empty() {
            button.text.clear();
        }

        let next_icon = item_id.and_then(|id| {
            resolve_item_icon_path(
                &item_registry,
                &block_registry,
                &asset_server,
                &mut image_cache,
                &mut images,
                id,
            )
        });
        if button.icon_path != next_icon {
            button.icon_path = next_icon;
        }
    }

    let show_trash = matches!(game_mode.0, GameMode::Creative);
    for (css_id, mut visibility) in &mut button_visibility {
        if css_id.0 == INVENTORY_TRASH_BUTTON_ID || css_id.0 == WORKBENCH_TRASH_BUTTON_ID {
            *visibility = if show_trash {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }

}

/// Runs the `open_recipe_preview_dialog_for_item` routine for open recipe preview dialog for item in the `graphic::components::inventory_creative` module.
fn open_recipe_preview_dialog_for_item(
    item_id: ItemId,
    recipe_registry: &RecipeRegistry,
    item_registry: &ItemRegistry,
    recipe_preview: &mut RecipePreviewDialogState,
) -> bool {
    let mut variants = Vec::new();

    for recipe in &recipe_registry.recipes {
        let Some(result_slot) = recipe_preview_result_slot_for_item(recipe, item_id, item_registry)
        else {
            continue;
        };

        for crafting in &recipe.crafting {
            let Some(parsed_preview) =
                parse_recipe_preview_inputs(recipe, crafting, item_id, item_registry)
            else {
                continue;
            };

            push_or_merge_recipe_preview_variant(
                &mut variants,
                RecipePreviewVariant {
                    crafting_type: parsed_preview.crafting_type,
                    input_slot_count: parsed_preview.slot_count,
                    input_slots: parsed_preview.input_slots,
                    input_slot_alternatives: parsed_preview.input_slot_alternatives,
                    result_slot,
                },
                item_registry,
            );
        }
    }

    if !variants.is_empty() {
        apply_recipe_preview_state(recipe_preview, variants);
        return true;
    }

    clear_recipe_preview_state(recipe_preview);
    false
}

fn open_recipe_preview_dialog_for_hovered_item(
    hovered_item_id: ItemId,
    recipe_registry: &RecipeRegistry,
    item_registry: &ItemRegistry,
    recipe_preview: &mut RecipePreviewDialogState,
) -> bool {
    open_recipe_preview_dialog_for_item(
        hovered_item_id,
        recipe_registry,
        item_registry,
        recipe_preview,
    )
}

struct ParsedRecipePreviewInputs {
    crafting_type: RecipePreviewCraftingType,
    slot_count: usize,
    input_slots: [InventorySlot; RECIPE_PREVIEW_INPUT_SLOTS],
    input_slot_alternatives: [Vec<ItemId>; RECIPE_PREVIEW_INPUT_SLOTS],
}

fn parse_recipe_preview_inputs(
    recipe: &crate::core::inventory::recipe::RecipeDefinition,
    crafting: &crate::core::inventory::recipe::RecipeCraftingEntry,
    selected_result_item_id: ItemId,
    item_registry: &ItemRegistry,
) -> Option<ParsedRecipePreviewInputs> {
    let (crafting_type, slot_count) =
        recipe_preview_input_capacity_and_type(crafting.recipe_type.localized_name().as_str())?;

    let Ok(parsed) = serde_json::from_value::<RecipePreviewCraftDataJson>(crafting.data.clone())
    else {
        return None;
    };

    let is_shapeless = crafting
        .format
        .trim()
        .eq_ignore_ascii_case(crate::core::inventory::recipe::CRAFTING_SHAPELESS_RECIPE_KIND);
    let requirements = collect_preview_requirements(parsed, is_shapeless, slot_count)?;
    if requirements.is_empty() {
        return None;
    }

    let mut input_slots = [InventorySlot::default(); RECIPE_PREVIEW_INPUT_SLOTS];
    let mut input_slot_alternatives = empty_preview_slot_alternatives();

    for (slot_index, requirement) in requirements {
        let requirement_item_id = if !requirement.item.trim().is_empty() {
            item_registry.id_opt(requirement.item.as_str())
        } else if !requirement.group.trim().is_empty() {
            input_slot_alternatives[slot_index] =
                preview_items_in_group(item_registry, requirement.group.as_str());
            resolve_preview_group_requirement_item_id(
                recipe,
                selected_result_item_id,
                slot_index,
                requirement.group.as_str(),
                item_registry,
            )
        } else {
            None
        }?;

        input_slots[slot_index] = InventorySlot {
            item_id: requirement_item_id,
            count: requirement.count.max(1),
        };
    }

    Some(ParsedRecipePreviewInputs {
        crafting_type,
        slot_count,
        input_slots,
        input_slot_alternatives,
    })
}

fn empty_preview_slot_alternatives() -> [Vec<ItemId>; RECIPE_PREVIEW_INPUT_SLOTS] {
    std::array::from_fn(|_| Vec::new())
}

fn preview_items_in_group(item_registry: &ItemRegistry, group: &str) -> Vec<ItemId> {
    let mut items = Vec::new();
    for (index, _) in item_registry.defs.iter().enumerate().skip(1) {
        let item_id = index as ItemId;
        if item_registry.has_group(item_id, group) {
            items.push(item_id);
        }
    }
    items
}

fn recipe_preview_input_capacity_and_type(
    recipe_type_localized: &str,
) -> Option<(RecipePreviewCraftingType, usize)> {
    if recipe_type_localized == crate::core::inventory::recipe::HAND_CRAFTED_TYPE_LOCALIZED {
        return Some((RecipePreviewCraftingType::HandCrafted, HAND_CRAFTED_INPUT_SLOTS));
    }
    if recipe_type_localized == crate::core::inventory::recipe::WORK_TABLE_CRAFTING_TYPE_LOCALIZED {
        return Some((
            RecipePreviewCraftingType::WorkTable,
            WORK_TABLE_CRAFTING_INPUT_SLOTS,
        ));
    }
    None
}

fn collect_preview_requirements(
    parsed: RecipePreviewCraftDataJson,
    is_shapeless: bool,
    slot_count: usize,
) -> Option<Vec<(usize, RecipePreviewCraftEntryJson)>> {
    if is_shapeless {
        let ingredients = if parsed.ingredients.is_empty() {
            let mut craft_entries = parsed
                .craft
                .into_iter()
                .filter_map(|(slot_raw, entry)| Some((slot_raw.parse::<usize>().ok()?, entry)))
                .collect::<Vec<_>>();
            craft_entries.sort_by_key(|(slot_index, _)| *slot_index);
            craft_entries
                .into_iter()
                .map(|(_, entry)| entry)
                .collect::<Vec<_>>()
        } else {
            parsed.ingredients
        };

        let expanded = expand_shapeless_preview_ingredients(ingredients);
        if expanded.is_empty() || expanded.len() > slot_count {
            return None;
        }

        let mut requirements = Vec::with_capacity(expanded.len());
        for (slot_index, entry) in expanded.into_iter().enumerate() {
            requirements.push((slot_index, entry));
        }
        return Some(requirements);
    }

    if parsed.craft.is_empty() {
        return None;
    }

    let mut requirements = Vec::with_capacity(parsed.craft.len());
    for (slot_raw, entry) in parsed.craft {
        let slot_index = slot_raw.parse::<usize>().ok()?;
        if slot_index >= slot_count {
            return None;
        }
        requirements.push((slot_index, entry));
    }
    requirements.sort_by_key(|(slot_index, _)| *slot_index);
    Some(requirements)
}

fn expand_shapeless_preview_ingredients(
    ingredients: Vec<RecipePreviewCraftEntryJson>,
) -> Vec<RecipePreviewCraftEntryJson> {
    let mut expanded = Vec::new();
    for mut ingredient in ingredients {
        let repeats = ingredient.count.max(1);
        ingredient.count = 1;
        for _ in 0..repeats {
            expanded.push(ingredient.clone());
        }
    }
    expanded
}

fn apply_recipe_preview_state(
    recipe_preview: &mut RecipePreviewDialogState,
    variants: Vec<RecipePreviewVariant>,
) {
    recipe_preview.open = true;
    recipe_preview.variants = variants;
    recipe_preview.selected_variant_index = 0;
    recipe_preview.tab_page = 0;
    sync_selected_recipe_preview_variant(recipe_preview);
}

fn sync_selected_recipe_preview_variant(recipe_preview: &mut RecipePreviewDialogState) {
    let Some(variant) = recipe_preview
        .variants
        .get(recipe_preview.selected_variant_index)
    else {
        recipe_preview.crafting_type = None;
        recipe_preview.input_slot_count = 0;
        recipe_preview.input_slots = [InventorySlot::default(); RECIPE_PREVIEW_INPUT_SLOTS];
        recipe_preview.result_slot = InventorySlot::default();
        return;
    };

    recipe_preview.crafting_type = Some(variant.crafting_type);
    recipe_preview.input_slot_count = variant.input_slot_count.min(RECIPE_PREVIEW_INPUT_SLOTS);
    recipe_preview.input_slots = variant.input_slots;
    recipe_preview.result_slot = variant.result_slot;
}

fn recipe_preview_variant_at(
    recipe_preview: &RecipePreviewDialogState,
    variant_index: usize,
) -> Option<&RecipePreviewVariant> {
    recipe_preview.variants.get(variant_index)
}

fn select_recipe_preview_variant(
    recipe_preview: &mut RecipePreviewDialogState,
    variant_index: usize,
) -> bool {
    if variant_index >= recipe_preview.variants.len() {
        return false;
    }
    recipe_preview.selected_variant_index = variant_index;
    recipe_preview.tab_page = variant_index / RECIPE_PREVIEW_TABS_PER_PAGE;
    sync_selected_recipe_preview_variant(recipe_preview);
    true
}

fn clear_recipe_preview_state(recipe_preview: &mut RecipePreviewDialogState) {
    recipe_preview.open = false;
    recipe_preview.variants.clear();
    recipe_preview.selected_variant_index = 0;
    recipe_preview.tab_page = 0;
    recipe_preview.crafting_type = None;
    recipe_preview.input_slot_count = 0;
    recipe_preview.input_slots = [InventorySlot::default(); RECIPE_PREVIEW_INPUT_SLOTS];
    recipe_preview.result_slot = InventorySlot::default();
}

fn push_or_merge_recipe_preview_variant(
    variants: &mut Vec<RecipePreviewVariant>,
    mut candidate: RecipePreviewVariant,
    item_registry: &ItemRegistry,
) {
    for existing in variants.iter_mut() {
        if !can_merge_recipe_preview_variants(existing, &candidate, item_registry) {
            continue;
        }
        merge_recipe_preview_variant(existing, &mut candidate);
        return;
    }
    variants.push(candidate);
}

fn can_merge_recipe_preview_variants(
    left: &RecipePreviewVariant,
    right: &RecipePreviewVariant,
    item_registry: &ItemRegistry,
) -> bool {
    if left.crafting_type != right.crafting_type
        || left.input_slot_count != right.input_slot_count
        || left.result_slot != right.result_slot
    {
        return false;
    }

    for slot_index in 0..RECIPE_PREVIEW_INPUT_SLOTS {
        let left_slot = left.input_slots[slot_index];
        let right_slot = right.input_slots[slot_index];
        if left_slot.is_empty() != right_slot.is_empty() || left_slot.count != right_slot.count {
            return false;
        }
        if left_slot.is_empty() || left_slot.item_id == right_slot.item_id {
            continue;
        }
        if !items_share_any_group(item_registry, left_slot.item_id, right_slot.item_id) {
            return false;
        }
    }

    true
}

fn merge_recipe_preview_variant(
    target: &mut RecipePreviewVariant,
    source: &mut RecipePreviewVariant,
) {
    for slot_index in 0..RECIPE_PREVIEW_INPUT_SLOTS {
        let target_slot = target.input_slots[slot_index];
        let source_slot = source.input_slots[slot_index];
        if target_slot.is_empty() || source_slot.is_empty() || target_slot.item_id == source_slot.item_id {
            continue;
        }

        if target.input_slot_alternatives[slot_index].is_empty() {
            target.input_slot_alternatives[slot_index].push(target_slot.item_id);
        }
        target.input_slot_alternatives[slot_index].push(source_slot.item_id);
        target.input_slot_alternatives[slot_index]
            .extend(source.input_slot_alternatives[slot_index].iter().copied());
        target.input_slot_alternatives[slot_index].sort_unstable();
        target.input_slot_alternatives[slot_index].dedup();

        if let Some(first) = target.input_slot_alternatives[slot_index].first().copied() {
            target.input_slots[slot_index].item_id = first;
        }
    }
}

fn items_share_any_group(item_registry: &ItemRegistry, left: ItemId, right: ItemId) -> bool {
    let Some(left_def) = item_registry.def_opt(left) else {
        return false;
    };
    let Some(right_def) = item_registry.def_opt(right) else {
        return false;
    };

    left_def
        .groups
        .iter()
        .any(|group| right_def.groups.iter().any(|other| other == group))
}

fn recipe_preview_result_slot_for_item(
    recipe: &crate::core::inventory::recipe::RecipeDefinition,
    selected_item_id: ItemId,
    item_registry: &ItemRegistry,
) -> Option<InventorySlot> {
    match &recipe.result {
        crate::core::inventory::recipe::RecipeResultTemplateDef::Static {
            item_id,
            count,
            ..
        } => {
            if *item_id != selected_item_id {
                return None;
            }
            Some(InventorySlot {
                item_id: selected_item_id,
                count: (*count).max(1),
            })
        }
        crate::core::inventory::recipe::RecipeResultTemplateDef::ByGroupFromSlot {
            group, count, ..
        } => {
            if !item_registry.has_group(selected_item_id, group.as_str()) {
                return None;
            }
            Some(InventorySlot {
                item_id: selected_item_id,
                count: (*count).max(1),
            })
        }
    }
}

fn resolve_preview_group_requirement_item_id(
    recipe: &crate::core::inventory::recipe::RecipeDefinition,
    selected_result_item_id: ItemId,
    requirement_slot_index: usize,
    requirement_group: &str,
    item_registry: &ItemRegistry,
) -> Option<ItemId> {
    if let crate::core::inventory::recipe::RecipeResultTemplateDef::ByGroupFromSlot {
        slot_index,
        group: result_group,
        ..
    } = &recipe.result
        && *slot_index == requirement_slot_index
        && item_registry.has_group(selected_result_item_id, result_group.as_str())
    {
        return item_registry.related_item_in_group(selected_result_item_id, requirement_group);
    }

    if item_registry.has_group(selected_result_item_id, requirement_group) {
        return Some(selected_result_item_id);
    }

    first_item_in_group(item_registry, requirement_group)
}

fn first_item_in_group(item_registry: &ItemRegistry, group: &str) -> Option<ItemId> {
    item_registry
        .defs
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(index, _)| {
            let item_id = index as ItemId;
            if item_registry.has_group(item_id, group) {
                Some(item_id)
            } else {
                None
            }
        })
}

/// Parses creative slot index for the `graphic::components::inventory_creative` module.
fn parse_creative_slot_index(css_id: &str) -> Option<usize> {
    css_id
        .strip_prefix(CREATIVE_PANEL_SLOT_PREFIX)
        .and_then(|slot_number| slot_number.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
        .filter(|index| *index < CREATIVE_PANEL_PAGE_SIZE)
}

/// Synchronizes creative slot hover border for the `graphic::components::inventory_creative` module.
fn sync_creative_slot_hover_border(
    slot_frames: &mut Query<(&CssID, &UIWidgetState, &mut BorderColor), With<Button>>,
    inventory_open: bool,
) -> Option<usize> {
    let mut hovered_slot = None;

    for (css_id, state, mut border) in slot_frames.iter_mut() {
        let Some(slot_index) = parse_creative_slot_index(css_id.0.as_str()) else {
            continue;
        };

        if hovered_slot.is_none() && state.hovered {
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
