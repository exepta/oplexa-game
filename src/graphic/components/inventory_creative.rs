use api::handlers::inventory::apply_creative_panel_click;

/// Represents recipe preview hand crafted data json used by the `graphic::components::inventory_creative` module.
#[derive(Deserialize)]
struct RecipePreviewHandCraftedDataJson {
    #[serde(default)]
    craft: std::collections::HashMap<String, RecipePreviewHandCraftedEntryJson>,
}

/// Represents recipe preview hand crafted entry json used by the `graphic::components::inventory_creative` module.
#[derive(Deserialize)]
struct RecipePreviewHandCraftedEntryJson {
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

/// Runs the `open_recipe_preview_dialog_for_item` routine for open recipe preview dialog for item in the `graphic::components::inventory_creative` module.
fn open_recipe_preview_dialog_for_item(
    item_id: ItemId,
    recipe_registry: &RecipeRegistry,
    item_registry: &ItemRegistry,
    recipe_preview: &mut RecipePreviewDialogState,
) -> bool {
    for recipe in &recipe_registry.recipes {
        let Some(result_slot) = recipe_preview_result_slot_for_item(recipe, item_id, item_registry)
        else {
            continue;
        };

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
                let requirement_item_id = if !requirement.item.trim().is_empty() {
                    item_registry.id_opt(requirement.item.as_str())
                } else if !requirement.group.trim().is_empty() {
                    resolve_preview_group_requirement_item_id(
                        recipe,
                        item_id,
                        slot_index,
                        requirement.group.as_str(),
                        item_registry,
                    )
                } else {
                    None
                };
                let Some(requirement_item_id) = requirement_item_id else {
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
            recipe_preview.result_slot = result_slot;
            return true;
        }
    }

    recipe_preview.open = false;
    recipe_preview.input_slots = [InventorySlot::default(); HAND_CRAFTED_INPUT_SLOTS];
    recipe_preview.result_slot = InventorySlot::default();
    false
}

fn open_recipe_preview_dialog_for_hovered_item(
    hovered_item_id: ItemId,
    recipe_registry: &RecipeRegistry,
    item_registry: &ItemRegistry,
    recipe_preview: &mut RecipePreviewDialogState,
) -> bool {
    if open_recipe_preview_dialog_for_item(
        hovered_item_id,
        recipe_registry,
        item_registry,
        recipe_preview,
    ) {
        return true;
    }

    for recipe in &recipe_registry.recipes {
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
            let mut hovered_matches_ingredient = false;

            for (slot_raw, requirement) in parsed.craft {
                let Some(slot_index) = slot_raw.parse::<usize>().ok() else {
                    valid = false;
                    break;
                };
                if slot_index >= HAND_CRAFTED_INPUT_SLOTS {
                    valid = false;
                    break;
                }

                let requirement_item_id = if !requirement.item.trim().is_empty() {
                    let item_id = item_registry.id_opt(requirement.item.as_str());
                    if item_id == Some(hovered_item_id) {
                        hovered_matches_ingredient = true;
                    }
                    item_id
                } else if !requirement.group.trim().is_empty() {
                    if item_registry.has_group(hovered_item_id, requirement.group.as_str()) {
                        hovered_matches_ingredient = true;
                        Some(hovered_item_id)
                    } else {
                        resolve_preview_group_requirement_item_id(
                            recipe,
                            hovered_item_id,
                            slot_index,
                            requirement.group.as_str(),
                            item_registry,
                        )
                    }
                } else {
                    None
                };

                let Some(requirement_item_id) = requirement_item_id else {
                    valid = false;
                    break;
                };
                input_slots[slot_index] = InventorySlot {
                    item_id: requirement_item_id,
                    count: requirement.count.max(1),
                };
            }

            if !valid || !hovered_matches_ingredient {
                continue;
            }

            let Some(result_slot) = recipe_preview_result_slot_for_hovered_item(
                recipe,
                hovered_item_id,
                &input_slots,
                item_registry,
            ) else {
                continue;
            };

            recipe_preview.open = true;
            recipe_preview.input_slots = input_slots;
            recipe_preview.result_slot = result_slot;
            return true;
        }
    }

    recipe_preview.open = false;
    recipe_preview.input_slots = [InventorySlot::default(); HAND_CRAFTED_INPUT_SLOTS];
    recipe_preview.result_slot = InventorySlot::default();
    false
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

fn recipe_preview_result_slot_for_hovered_item(
    recipe: &crate::core::inventory::recipe::RecipeDefinition,
    hovered_item_id: ItemId,
    input_slots: &[InventorySlot; HAND_CRAFTED_INPUT_SLOTS],
    item_registry: &ItemRegistry,
) -> Option<InventorySlot> {
    match &recipe.result {
        crate::core::inventory::recipe::RecipeResultTemplateDef::Static {
            item_id,
            count,
            ..
        } => Some(InventorySlot {
            item_id: *item_id,
            count: (*count).max(1),
        }),
        crate::core::inventory::recipe::RecipeResultTemplateDef::ByGroupFromSlot {
            slot_index,
            group,
            count,
        } => {
            let result_item_id = if item_registry.has_group(hovered_item_id, group.as_str()) {
                Some(hovered_item_id)
            } else {
                item_registry
                    .related_item_in_group(hovered_item_id, group.as_str())
                    .or_else(|| {
                        input_slots
                            .get(*slot_index)
                            .copied()
                            .filter(|slot| !slot.is_empty())
                            .and_then(|slot| {
                                item_registry.related_item_in_group(slot.item_id, group.as_str())
                            })
                    })
                    .or_else(|| first_item_in_group(item_registry, group.as_str()))
            }?;

            Some(InventorySlot {
                item_id: result_item_id,
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
