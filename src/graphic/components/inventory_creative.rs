use api::handlers::inventory::apply_creative_panel_click;

#[derive(Deserialize)]
struct RecipePreviewHandCraftedDataJson {
    #[serde(default)]
    craft: std::collections::HashMap<String, RecipePreviewHandCraftedEntryJson>,
}

#[derive(Deserialize)]
struct RecipePreviewHandCraftedEntryJson {
    item: String,
    #[serde(default = "recipe_preview_default_required_count")]
    count: u16,
}

#[inline]
fn recipe_preview_default_required_count() -> u16 {
    1
}

fn sync_creative_panel_state_from_registry(
    item_registry: Res<ItemRegistry>,
    mut creative_ui: ResMut<CreativePanelUiState>,
    mut creative_panel: ResMut<CreativePanelState>,
) {
    let expected_items = item_registry
        .defs
        .len()
        .saturating_sub(1)
        .min(u16::MAX as usize);
    if !creative_ui.synced_once || creative_panel.item_count() != expected_items {
        creative_panel.rebuild_from_registry(&item_registry);
        creative_ui.synced_once = true;
    }

    creative_panel.clamp_page();
}

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
            let _ = open_recipe_preview_dialog_for_item(
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

#[allow(clippy::too_many_arguments)]
fn sync_creative_panel_ui(
    creative_panel: Res<CreativePanelState>,
    game_mode: Res<GameModeState>,
    item_registry: Res<ItemRegistry>,
    block_registry: Res<BlockRegistry>,
    asset_server: Res<AssetServer>,
    mut image_cache: ResMut<ImageCache>,
    mut images: ResMut<Assets<Image>>,
    mut paragraphs: Query<(&CssID, &mut Paragraph)>,
    mut buttons: Query<(&CssID, &mut Button, &mut UiButtonTone), With<Button>>,
) {
    for (css_id, mut paragraph) in &mut paragraphs {
        if css_id.0 == CREATIVE_PANEL_TOTAL_ID {
            paragraph.text = format!("Registered: {}", creative_panel.item_count());
            continue;
        }
        if css_id.0 == CREATIVE_PANEL_PAGE_ID {
            paragraph.text = creative_panel.page_label();
            continue;
        }
        if css_id.0 == CREATIVE_RECIPE_HINT_ID {
            paragraph.text = match game_mode.0 {
                GameMode::Creative => {
                    "Klick auf Item legt es ins Inventar. Rezepte folgen als Popup.".to_string()
                }
                GameMode::Survival => {
                    "Survival: Klick auf Item zeigt das Recipe als Dialog.".to_string()
                }
                GameMode::Spectator => "Spectator: Item-Klick ist deaktiviert.".to_string(),
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

}

fn open_recipe_preview_dialog_for_item(
    item_id: ItemId,
    recipe_registry: &RecipeRegistry,
    item_registry: &ItemRegistry,
    recipe_preview: &mut RecipePreviewDialogState,
) -> bool {
    for recipe in &recipe_registry.recipes {
        if recipe.result.item_id != item_id {
            continue;
        }

        for crafting in &recipe.crafting {
            if crafting.recipe_type.localized_name()
                != crate::core::inventory::recipe::HAND_CRAFTED_TYPE_LOCALIZED
            {
                continue;
            }

            let Ok(parsed) = serde_json::from_value::<RecipePreviewHandCraftedDataJson>(
                crafting.data.clone(),
            ) else {
                continue;
            };
            if parsed.craft.is_empty() {
                continue;
            }

            let mut input_slots = [InventorySlot::default(); HAND_CRAFTED_INPUT_SLOTS];
            let mut valid = true;

            for (slot_raw, requirement) in parsed.craft {
                let Some(slot_index) = slot_raw.parse::<usize>().ok() else {
                    valid = false;
                    break;
                };
                if slot_index >= HAND_CRAFTED_INPUT_SLOTS {
                    valid = false;
                    break;
                }
                let Some(requirement_item_id) = item_registry.id_opt(requirement.item.as_str())
                else {
                    valid = false;
                    break;
                };
                input_slots[slot_index] = InventorySlot {
                    item_id: requirement_item_id,
                    count: requirement.count.max(1),
                };
            }

            if !valid {
                continue;
            }

            recipe_preview.open = true;
            recipe_preview.input_slots = input_slots;
            recipe_preview.result_slot = InventorySlot {
                item_id: recipe.result.item_id,
                count: recipe.result.count.max(1),
            };
            return true;
        }
    }

    recipe_preview.open = false;
    recipe_preview.input_slots = [InventorySlot::default(); HAND_CRAFTED_INPUT_SLOTS];
    recipe_preview.result_slot = InventorySlot::default();
    false
}


fn parse_creative_slot_index(css_id: &str) -> Option<usize> {
    css_id
        .strip_prefix(CREATIVE_PANEL_SLOT_PREFIX)
        .and_then(|slot_number| slot_number.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
        .filter(|index| *index < CREATIVE_PANEL_PAGE_SIZE)
}

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
