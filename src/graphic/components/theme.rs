fn configure_extended_ui(mut config: ResMut<ExtendedUiConfiguration>) {
    config.order = 25;
}

fn full_screen_center_node() -> Node {
    Node {
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        position_type: PositionType::Absolute,
        left: Val::Px(0.0),
        top: Val::Px(0.0),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    }
}

fn menu_panel_node() -> Node {
    Node {
        width: Val::Px(640.0),
        min_height: Val::Px(420.0),
        max_height: Val::Percent(90.0),
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(10.0),
        padding: UiRect::all(Val::Px(14.0)),
        border: UiRect::all(Val::Px(1.0)),
        ..default()
    }
}

fn dialog_overlay_node() -> Node {
    Node {
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        position_type: PositionType::Absolute,
        left: Val::Px(0.0),
        top: Val::Px(0.0),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    }
}

fn dialog_box_node() -> Node {
    Node {
        width: Val::Px(420.0),
        min_height: Val::Px(120.0),
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(8.0),
        padding: UiRect::all(Val::Px(12.0)),
        border: UiRect::all(Val::Px(1.0)),
        ..default()
    }
}

fn color_background() -> Color {
    Color::srgb_u8(0x30, 0x34, 0x40)
}

fn color_background_hover() -> Color {
    Color::srgb_u8(0x39, 0x3d, 0x4a)
}

fn color_single_player_list_background() -> Color {
    Color::srgb_u8(0x26, 0x2a, 0x35)
}

fn color_accent() -> Color {
    Color::srgb_u8(0x40, 0xc2, 0x99)
}

fn color_accent_hover() -> Color {
    Color::srgb_u8(0x42, 0xd4, 0xa5)
}

fn color_text() -> Color {
    Color::WHITE
}

fn color_text_darker() -> Color {
    Color::srgb_u8(0x9f, 0xa1, 0xa0)
}

fn color_server_offline_border() -> Color {
    Color::srgb_u8(0xd4, 0x4c, 0x4c)
}

fn color_server_waiting_border() -> Color {
    Color::srgb_u8(0xe0, 0x98, 0x2d)
}

fn apply_button_layout(node: &mut Node, kind: UiButtonKind) {
    match kind {
        UiButtonKind::Action => {
            node.width = Val::Percent(100.0);
            node.flex_basis = Val::Auto;
            node.min_width = Val::Px(120.0);
            node.max_width = Val::Percent(100.0);
            node.min_height = Val::Px(42.0);
            node.padding = UiRect::axes(Val::Px(10.0), Val::Px(7.0));
            node.border = UiRect::all(Val::Px(1.0));
            node.justify_content = JustifyContent::Center;
            node.align_items = AlignItems::Center;
            node.align_self = AlignSelf::Stretch;
            node.flex_grow = 0.0;
            node.flex_shrink = 0.0;
        }
        UiButtonKind::ActionRow => {
            // Force row actions to share available width instead of overflowing with 100%-style defaults.
            node.width = Val::Px(0.0);
            node.flex_basis = Val::Px(0.0);
            node.min_width = Val::Px(120.0);
            node.max_width = Val::Percent(100.0);
            node.min_height = Val::Px(42.0);
            node.padding = UiRect::axes(Val::Px(10.0), Val::Px(7.0));
            node.border = UiRect::all(Val::Px(1.0));
            node.justify_content = JustifyContent::Center;
            node.align_items = AlignItems::Center;
            node.align_self = AlignSelf::Auto;
            node.flex_grow = 1.0;
            node.flex_shrink = 1.0;
        }
        UiButtonKind::Card => {
            node.width = Val::Percent(100.0);
            node.min_height = Val::Px(76.0);
            node.padding = UiRect::all(Val::Px(8.0));
            node.border = UiRect::all(Val::Px(1.0));
            node.justify_content = JustifyContent::Center;
            node.align_items = AlignItems::Start;
            node.flex_direction = FlexDirection::Column;
            node.row_gap = Val::Px(2.0);
        }
        UiButtonKind::InventorySlot => {
            node.width = Val::Px(56.0);
            node.height = Val::Px(56.0);
            node.border = UiRect::all(Val::Px(1.0));
            node.justify_content = JustifyContent::Center;
            node.align_items = AlignItems::Center;
            node.flex_direction = FlexDirection::Column;
            node.row_gap = Val::Px(2.0);
            node.padding = UiRect::all(Val::Px(3.0));
        }
        UiButtonKind::InventoryResultSlot => {
            node.width = Val::Px(67.2);
            node.height = Val::Px(67.2);
            node.border = UiRect::all(Val::Px(1.0));
            node.justify_content = JustifyContent::Center;
            node.align_items = AlignItems::Center;
            node.flex_direction = FlexDirection::Column;
            node.row_gap = Val::Px(2.0);
            node.padding = UiRect::all(Val::Px(3.0));
        }
    }
}

fn layout_buttons_once(
    mut commands: Commands,
    mut buttons: Query<
        (Entity, &UiButtonKind, &mut Node),
        (With<Button>, Without<UiButtonLayoutApplied>),
    >,
) {
    for (entity, kind, mut node) in &mut buttons {
        apply_button_layout(&mut node, *kind);
        commands.entity(entity).insert(UiButtonLayoutApplied);
    }
}

fn style_buttons(
    mut buttons: Query<
        (
            &UiButtonTone,
            &UIWidgetState,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        With<Button>,
    >,
) {
    for (tone, state, mut background, mut border) in &mut buttons {
        let hovered = state.hovered;
        let (bg, border_col) = match tone {
            UiButtonTone::Normal => (
                if hovered {
                    color_background_hover()
                } else {
                    color_background()
                },
                color_background_hover(),
            ),
            UiButtonTone::Accent => (
                if hovered {
                    color_accent_hover()
                } else {
                    color_accent()
                },
                color_background_hover(),
            ),
        };

        background.0 = bg;
        border.top = border_col;
        border.right = border_col;
        border.bottom = border_col;
        border.left = border_col;
    }
}

fn layout_inputs_once(
    mut commands: Commands,
    mut fields: Query<(Entity, &mut Node), (With<InputField>, Without<UiInputLayoutApplied>)>,
) {
    for (entity, mut node) in &mut fields {
        node.width = Val::Percent(100.0);
        node.min_height = Val::Px(38.0);
        node.padding = UiRect::axes(Val::Px(8.0), Val::Px(6.0));
        node.border = UiRect::all(Val::Px(1.0));
        node.align_items = AlignItems::Center;
        commands.entity(entity).insert(UiInputLayoutApplied);
    }
}

fn style_inputs(
    mut fields: Query<(&UIWidgetState, &mut BackgroundColor, &mut BorderColor), With<InputField>>,
) {
    for (state, mut bg, mut border) in &mut fields {
        bg.0 = color_background();
        let border_col = if state.focused || state.hovered {
            color_accent()
        } else {
            color_background_hover()
        };
        border.top = border_col;
        border.right = border_col;
        border.bottom = border_col;
        border.left = border_col;
    }
}

fn style_paragraphs(
    mut texts: Query<(&mut TextColor, &mut TextFont, Option<&UiTextTone>), With<Node>>,
) {
    for (mut color, mut font, tone) in &mut texts {
        let tone = tone.copied();
        color.0 = match tone {
            Some(UiTextTone::Darker) => color_text_darker(),
            Some(UiTextTone::CardPing) => color_text_darker(),
            Some(UiTextTone::TooltipName) => color_accent(),
            Some(UiTextTone::TooltipKey) => Color::srgb_u8(0xc5, 0xc7, 0xcd),
            _ => color_text(),
        };
        font.font_size = match tone {
            Some(UiTextTone::Heading) => 14.0,
            Some(UiTextTone::CardName) => 13.0,
            Some(UiTextTone::CardPing) => 11.0,
            Some(UiTextTone::TooltipName) => 12.0,
            Some(UiTextTone::TooltipKey) => 11.0,
            _ => 12.0,
        };
    }
}

fn style_pause_menu_button_texts(
    buttons: Query<(&UIGenID, &CssID), With<Button>>,
    mut texts: Query<(&BindToID, &mut TextFont, &mut TextColor), With<Text>>,
) {
    let mut pause_ids = HashSet::new();
    for (id, css_id) in &buttons {
        if css_id.0 == PAUSE_PLAY_ID
            || css_id.0 == PAUSE_CONNECT_ID
            || css_id.0 == PAUSE_SETTINGS_ID
            || css_id.0 == PAUSE_CLOSE_ID
        {
            pause_ids.insert(id.get());
        }
    }

    for (bind, mut font, mut color) in &mut texts {
        if pause_ids.contains(&bind.get()) {
            font.font_size = 12.0;
            color.0 = Color::WHITE;
        }
    }
}

fn style_images(mut images: Query<(&CssID, &mut Node), With<Img>>) {
    for (_css_id, mut node) in &mut images {
        node.width = Val::Px(32.0);
        node.height = Val::Px(32.0);
        node.justify_self = JustifySelf::Center;
        node.align_self = AlignSelf::Center;
    }
}

fn style_button_icons(mut images: Query<(&Name, &mut Node), With<ImageNode>>) {
    /// Inventory and hotbar item icon edge length in pixels.
    const ITEM_ICON_SIZE_PX: f32 = 36.8;

    for (name, mut node) in &mut images {
        if !name.as_str().starts_with("Button-Icon-") {
            continue;
        }
        node.width = Val::Px(ITEM_ICON_SIZE_PX);
        node.height = Val::Px(ITEM_ICON_SIZE_PX);
        node.justify_self = JustifySelf::Center;
        node.align_self = AlignSelf::Center;
    }
}

fn style_slot_count_badges(
    mut badges: Query<(&CssID, &mut Node, &mut TextFont, &mut TextColor), With<Paragraph>>,
) {
    for (css_id, mut node, mut font, mut text_color) in &mut badges {
        if !css_id.0.starts_with(HUD_SLOT_BADGE_PREFIX)
            && !css_id.0.starts_with(PLAYER_INVENTORY_BADGE_PREFIX)
            && !css_id.0.starts_with(HAND_CRAFTED_BADGE_PREFIX)
            && !css_id.0.starts_with(RECIPE_PREVIEW_INPUT_BADGE_PREFIX)
            && css_id.0 != HAND_CRAFTED_RESULT_BADGE_ID
            && css_id.0 != RECIPE_PREVIEW_RESULT_BADGE_ID
            && css_id.0 != INVENTORY_CURSOR_BADGE_ID
        {
            continue;
        }

        node.position_type = PositionType::Absolute;
        node.right = Val::Px(2.0);
        node.top = Val::Px(2.0);
        node.min_width = Val::Px(14.0);
        node.height = Val::Px(14.0);
        node.padding = UiRect::axes(Val::Px(3.0), Val::Px(1.0));
        node.justify_content = JustifyContent::Center;
        node.align_items = AlignItems::Center;

        font.font_size = 9.0;
        text_color.0 = Color::WHITE;
    }
}

fn style_scroll_div_lists(
    mut commands: Commands,
    mut lists: ParamSet<(
        Query<
            (
                Entity,
                &mut Node,
                &mut BackgroundColor,
                &mut BorderColor,
                Option<&Children>,
                Option<&ListDivScrollReady>,
            ),
            (With<SinglePlayerWorldList>, Without<MultiplayerServerList>),
        >,
        Query<
            (
                Entity,
                &mut Node,
                &mut BackgroundColor,
                &mut BorderColor,
                Option<&Children>,
                Option<&ListDivScrollReady>,
            ),
            (With<MultiplayerServerList>, Without<SinglePlayerWorldList>),
        >,
    )>,
    child_names: Query<&Name>,
) {
    for (entity, mut node, mut bg, mut border, children, ready) in &mut lists.p0() {
        let has_scroll_content = children.is_some_and(|children| {
            children.iter().any(|child| {
                child_names
                    .get(child)
                    .is_ok_and(|name| name.as_str().starts_with("Div-ScrollContent-"))
            })
        });

        if ready.is_none() {
            if node.width != Val::Percent(100.0) {
                node.width = Val::Percent(100.0);
            }
            if node.height != Val::Px(340.0) {
                node.height = Val::Px(340.0);
            }
            if node.flex_direction != FlexDirection::Column {
                node.flex_direction = FlexDirection::Column;
            }
            if node.row_gap != Val::Px(8.0) {
                node.row_gap = Val::Px(8.0);
            }
            let wanted_padding = UiRect::all(Val::Px(8.0));
            if node.padding != wanted_padding {
                node.padding = wanted_padding;
            }

            if has_scroll_content {
                commands.entity(entity).insert(ListDivScrollReady);
                node.overflow.y = OverflowAxis::Clip;
                node.overflow.x = OverflowAxis::Clip;
            } else if node.overflow.y != OverflowAxis::Scroll || node.overflow.x != OverflowAxis::Clip {
                node.overflow.y = OverflowAxis::Scroll;
                node.overflow.x = OverflowAxis::Clip;
            }
        } else if has_scroll_content {
            // Keep the wrapper clipped once scroll content exists.
            // Do not continuously mutate other layout fields to avoid scroll reset jitter.
            if node.overflow.y != OverflowAxis::Clip || node.overflow.x != OverflowAxis::Clip {
                node.overflow.y = OverflowAxis::Clip;
                node.overflow.x = OverflowAxis::Clip;
            }
        }
        if bg.0 != color_single_player_list_background() {
            bg.0 = color_single_player_list_background();
        }
        let border_col = color_background_hover();
        if border.top != border_col
            || border.right != border_col
            || border.bottom != border_col
            || border.left != border_col
        {
            border.top = border_col;
            border.right = border_col;
            border.bottom = border_col;
            border.left = border_col;
        }
    }

    for (entity, mut node, mut bg, mut border, children, ready) in &mut lists.p1() {
        let has_scroll_content = children.is_some_and(|children| {
            children.iter().any(|child| {
                child_names
                    .get(child)
                    .is_ok_and(|name| name.as_str().starts_with("Div-ScrollContent-"))
            })
        });

        if ready.is_none() {
            if node.width != Val::Percent(100.0) {
                node.width = Val::Percent(100.0);
            }
            if node.height != Val::Px(340.0) {
                node.height = Val::Px(340.0);
            }
            if node.flex_direction != FlexDirection::Column {
                node.flex_direction = FlexDirection::Column;
            }
            if node.row_gap != Val::Px(8.0) {
                node.row_gap = Val::Px(8.0);
            }
            let wanted_padding = UiRect::all(Val::Px(8.0));
            if node.padding != wanted_padding {
                node.padding = wanted_padding;
            }

            if has_scroll_content {
                commands.entity(entity).insert(ListDivScrollReady);
                node.overflow.y = OverflowAxis::Clip;
                node.overflow.x = OverflowAxis::Clip;
            } else if node.overflow.y != OverflowAxis::Scroll || node.overflow.x != OverflowAxis::Clip {
                node.overflow.y = OverflowAxis::Scroll;
                node.overflow.x = OverflowAxis::Clip;
            }
        } else if has_scroll_content {
            // Keep the wrapper clipped once scroll content exists.
            // Do not continuously mutate other layout fields to avoid scroll reset jitter.
            if node.overflow.y != OverflowAxis::Clip || node.overflow.x != OverflowAxis::Clip {
                node.overflow.y = OverflowAxis::Clip;
                node.overflow.x = OverflowAxis::Clip;
            }
        }
        if bg.0 != color_background_hover() {
            bg.0 = color_background_hover();
        }
        let border_col = color_background_hover();
        if border.top != border_col
            || border.right != border_col
            || border.bottom != border_col
            || border.left != border_col
        {
            border.top = border_col;
            border.right = border_col;
            border.bottom = border_col;
            border.left = border_col;
        }
    }
}

fn style_div_scrollbars(
    mut nodes: Query<(
        &Name,
        &mut Node,
        &mut BackgroundColor,
        Option<&mut BorderColor>,
        Option<&Scrollbar>,
    )>,
) {
    for (name, mut node, mut bg, border, scrollbar) in &mut nodes {
        if scrollbar.is_some() {
            if name.as_str().starts_with("Div-Scrollbar-H-") {
                if node.position_type != PositionType::Absolute {
                    node.position_type = PositionType::Absolute;
                }
                if node.left != Val::Px(0.0) {
                    node.left = Val::Px(0.0);
                }
                if node.right != Val::Px(0.0) {
                    node.right = Val::Px(0.0);
                }
                if node.bottom != Val::Px(0.0) {
                    node.bottom = Val::Px(0.0);
                }
                if node.top != Val::Auto {
                    node.top = Val::Auto;
                }
                if node.height != Val::Px(10.0) {
                    node.height = Val::Px(10.0);
                }
                let bar_color = Color::srgba(0.22, 0.24, 0.29, 0.85);
                if bg.0 != bar_color {
                    bg.0 = bar_color;
                }
            } else if name.as_str().starts_with("Div-Scrollbar-") {
                if node.position_type != PositionType::Absolute {
                    node.position_type = PositionType::Absolute;
                }
                if node.left != Val::Auto {
                    node.left = Val::Auto;
                }
                if node.right != Val::Px(0.0) {
                    node.right = Val::Px(0.0);
                }
                if node.top != Val::Px(0.0) {
                    node.top = Val::Px(0.0);
                }
                if node.bottom != Val::Px(0.0) {
                    node.bottom = Val::Px(0.0);
                }
                if node.width != Val::Px(10.0) {
                    node.width = Val::Px(10.0);
                }
                let bar_color = Color::srgba(0.22, 0.24, 0.29, 0.85);
                if bg.0 != bar_color {
                    bg.0 = bar_color;
                }
            }

            if let Some(mut border) = border {
                let border_col = color_background_hover();
                if border.top != border_col
                    || border.right != border_col
                    || border.bottom != border_col
                    || border.left != border_col
                {
                    border.top = border_col;
                    border.right = border_col;
                    border.bottom = border_col;
                    border.left = border_col;
                }
            }
            continue;
        }

        if name.as_str().starts_with("Scroll-Track-") {
            if node.width != Val::Percent(100.0) {
                node.width = Val::Percent(100.0);
            }
            if node.height != Val::Percent(100.0) {
                node.height = Val::Percent(100.0);
            }
            let track_color = Color::srgba(0.19, 0.20, 0.25, 0.90);
            if bg.0 != track_color {
                bg.0 = track_color;
            }
        } else if name.as_str().starts_with("Scroll-Thumb-") {
            if bg.0 != color_accent() {
                bg.0 = color_accent();
            }
        }
    }
}

fn style_scroll_div_contents(
    mut contents: Query<(&Name, &mut Node), With<ScrollPosition>>,
) {
    for (name, mut node) in &mut contents {
        if !name.as_str().starts_with("Div-ScrollContent-") {
            continue;
        }

        if node.flex_direction != FlexDirection::Column {
            node.flex_direction = FlexDirection::Column;
        }
        if node.align_items != AlignItems::Stretch {
            node.align_items = AlignItems::Stretch;
        }
        if node.row_gap != Val::Px(8.0) {
            node.row_gap = Val::Px(8.0);
        }
        let wanted_padding = UiRect::all(Val::Px(8.0));
        if node.padding != wanted_padding {
            node.padding = wanted_padding;
        }
        if node.overflow.y != OverflowAxis::Scroll || node.overflow.x != OverflowAxis::Hidden {
            node.overflow = Overflow::scroll_y();
        }
    }
}

fn style_progress_bars(
    mut bars: Query<(&mut Node, &mut BackgroundColor, &Children), With<ProgressBar>>,
    mut tracks: Query<&mut BackgroundColor, (With<BindToID>, Without<ProgressBar>)>,
) {
    for (mut node, mut bg, children) in &mut bars {
        node.width = Val::Percent(100.0);
        node.height = Val::Px(20.0);
        node.border = UiRect::all(Val::Px(1.0));
        bg.0 = color_background_hover();

        for child in children.iter() {
            if let Ok(mut child_bg) = tracks.get_mut(child) {
                child_bg.0 = color_accent();
            }
        }
    }
}
