use api::handlers::inventory::apply_creative_panel_click;

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
    mouse: Res<ButtonInput<MouseButton>>,
    inventory_ui: Res<PlayerInventoryUiState>,
    game_mode: Res<GameModeState>,
    creative_panel: Res<CreativePanelState>,
    item_registry: Res<ItemRegistry>,
    mut inventory: ResMut<PlayerInventory>,
    mut slot_frames: Query<(&CssID, &UIWidgetState, &mut BorderColor), With<Button>>,
) {
    let hovered_slot = sync_creative_slot_hover_border(&mut slot_frames, inventory_ui.open);

    if !inventory_ui.open || !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let Some(slot_index) = hovered_slot else {
        return;
    };
    let Some(item_id) = creative_panel.item_at_page_slot(slot_index) else {
        return;
    };

    let _ = apply_creative_panel_click(&game_mode, item_id, &mut inventory, &item_registry);
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
                _ => "Survival: Klicks sind ohne Funktion. Rezepte folgen als Popup.".to_string(),
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
