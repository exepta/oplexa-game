{
    let _world_gen_root = commands
        .spawn((
            Name::new("UI WorldGen Root"),
            WorldGenRoot,
            Visibility::Hidden,
            full_screen_center_node(),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.58)),
            ZIndex(60),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Px(560.0),
                    min_height: Val::Px(150.0),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Stretch,
                    row_gap: Val::Px(12.0),
                    padding: UiRect::all(Val::Px(16.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(color_background().into()),
                BorderColor::all(color_background_hover()),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Paragraph {
                        text: language.localize_name_key("KEY_UI_GENERATING_WORLD").to_string(),
                        ..default()
                    },
                    UiTextTone::Heading,
                ));
                panel.spawn((
                    Paragraph {
                        text: "|".to_string(),
                        ..default()
                    },
                    CssID(WORLD_GEN_SPINNER_ID.to_string()),
                    UiTextTone::Darker,
                ));
                panel.spawn((
                    ProgressBar {
                        value: 0.0,
                        min: 0.0,
                        max: 100.0,
                        ..default()
                    },
                    CssID(WORLD_GEN_PROGRESS_ID.to_string()),
                ));
                panel.spawn((
                    Paragraph {
                        text: format!(
                            "{} 0 / 0",
                            language.localize_name_key("KEY_UI_CHUNKS_LOADED")
                        ),
                        ..default()
                    },
                    CssID(WORLD_GEN_CHUNKS_ID.to_string()),
                    UiTextTone::Darker,
                ));
            });
        })
        .id();

    let _world_unload_root = commands
        .spawn((
            Name::new("UI WorldUnload Root"),
            WorldUnloadRoot,
            Visibility::Hidden,
            full_screen_center_node(),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.65)),
            ZIndex(62),
        ))
        .with_children(|root| {
            root.spawn((
                Paragraph {
                    text: language.localize_name_key("KEY_UI_LEAVING_WORLD").to_string(),
                    ..default()
                },
                UiTextTone::Heading,
            ));
        })
        .id();

    let _hud_root = commands
        .spawn((
            Name::new("UI HUD Root"),
            HudRoot,
            Visibility::Hidden,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::End,
                padding: UiRect::bottom(Val::Px(24.0)),
                ..default()
            },
            BackgroundColor::DEFAULT,
            ZIndex(20),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Percent(100.0),
                    position_type: PositionType::Absolute,
                    top: Val::Px(16.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Start,
                    ..default()
                },
                BackgroundColor::DEFAULT,
                Pickable::IGNORE,
            ))
            .with_children(|overlay| {
                overlay
                    .spawn((
                        Name::new("UI HUD Looked Block Card"),
                        HudLookedBlockCard,
                        Visibility::Hidden,
                        Node {
                            width: Val::Px(360.0),
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(8.0),
                            padding: UiRect::all(Val::Px(10.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.06, 0.08, 0.11, 0.78)),
                        BorderColor::all(Color::srgba(0.22, 0.28, 0.33, 0.88)),
                        Pickable::IGNORE,
                    ))
                    .with_children(|card| {
                        card.spawn((
                            Node {
                                width: Val::Percent(100.0),
                                column_gap: Val::Px(10.0),
                                align_items: AlignItems::Start,
                                ..default()
                            },
                            BackgroundColor::DEFAULT,
                            Pickable::IGNORE,
                        ))
                        .with_children(|content| {
                            content.spawn((
                                ImageNode::default(),
                                Node {
                                    width: Val::Px(48.0),
                                    height: Val::Px(48.0),
                                    min_width: Val::Px(48.0),
                                    min_height: Val::Px(48.0),
                                    max_width: Val::Px(48.0),
                                    max_height: Val::Px(48.0),
                                    ..default()
                                },
                                HudLookedBlockIcon,
                                Pickable::IGNORE,
                            ));
                            content
                                .spawn((
                                    Node {
                                        flex_direction: FlexDirection::Column,
                                        row_gap: Val::Px(2.0),
                                        flex_grow: 1.0,
                                        ..default()
                                    },
                                    BackgroundColor::DEFAULT,
                                    Pickable::IGNORE,
                                ))
                                .with_children(|text| {
                                    text.spawn((
                                        Paragraph {
                                            text: String::new(),
                                            ..default()
                                        },
                                        HudLookedBlockDisplayName,
                                        UiTextTone::Heading,
                                        Pickable::IGNORE,
                                    ));
                                    text.spawn((
                                        Paragraph {
                                            text: String::new(),
                                            ..default()
                                        },
                                        HudLookedBlockLocalizedName,
                                        UiTextTone::Darker,
                                        Pickable::IGNORE,
                                    ));
                                    text.spawn((
                                        Paragraph {
                                            text: String::new(),
                                            ..default()
                                        },
                                        HudLookedBlockLevel,
                                        UiTextTone::Darker,
                                        Pickable::IGNORE,
                                    ));
                                    text.spawn((
                                        Paragraph {
                                            text: String::new(),
                                            ..default()
                                        },
                                        HudLookedBlockChestCount,
                                        UiTextTone::Darker,
                                        Visibility::Hidden,
                                        Pickable::IGNORE,
                                    ));
                                    text.spawn((
                                        Node {
                                            width: Val::Percent(100.0),
                                            flex_direction: FlexDirection::Row,
                                            column_gap: Val::Px(6.0),
                                            align_items: AlignItems::Center,
                                            ..default()
                                        },
                                        HudLookedBlockChestPreviewRow,
                                        Visibility::Hidden,
                                        Pickable::IGNORE,
                                    ))
                                    .with_children(|row| {
                                        for index in 0..5usize {
                                            row.spawn((
                                                Node {
                                                    width: Val::Px(32.0),
                                                    height: Val::Px(32.0),
                                                    min_width: Val::Px(32.0),
                                                    min_height: Val::Px(32.0),
                                                    max_width: Val::Px(32.0),
                                                    max_height: Val::Px(32.0),
                                                    justify_content: JustifyContent::Center,
                                                    align_items: AlignItems::Center,
                                                    border: UiRect::all(Val::Px(1.0)),
                                                    ..default()
                                                },
                                                HudLookedBlockChestPreviewSlot { index },
                                                BackgroundColor(color_background().into()),
                                                BorderColor::all(color_background_hover()),
                                                Visibility::Hidden,
                                                Pickable::IGNORE,
                                            ))
                                            .with_children(|slot| {
                                                slot.spawn((
                                                    ImageNode::default(),
                                                    Node {
                                                        width: Val::Px(22.0),
                                                        height: Val::Px(22.0),
                                                        min_width: Val::Px(22.0),
                                                        min_height: Val::Px(22.0),
                                                        max_width: Val::Px(22.0),
                                                        max_height: Val::Px(22.0),
                                                        ..default()
                                                    },
                                                    HudLookedBlockChestPreviewIcon { index },
                                                    Pickable::IGNORE,
                                                ));
                                                slot.spawn((
                                                    Paragraph {
                                                        text: String::new(),
                                                        ..default()
                                                    },
                                                    Node {
                                                        position_type: PositionType::Absolute,
                                                        right: Val::Px(1.0),
                                                        top: Val::Px(1.0),
                                                        min_width: Val::Px(12.0),
                                                        height: Val::Px(12.0),
                                                        padding: UiRect::axes(Val::Px(2.0), Val::Px(0.0)),
                                                        justify_content: JustifyContent::Center,
                                                        align_items: AlignItems::Center,
                                                        ..default()
                                                    },
                                                    BackgroundColor(Color::srgba(0.06, 0.06, 0.08, 0.9)),
                                                    HudLookedBlockChestPreviewBadge { index },
                                                    Visibility::Hidden,
                                                    Pickable::IGNORE,
                                                ));
                                            });
                                        }
                                        row.spawn((
                                            Node {
                                                width: Val::Px(32.0),
                                                height: Val::Px(32.0),
                                                min_width: Val::Px(32.0),
                                                min_height: Val::Px(32.0),
                                                max_width: Val::Px(32.0),
                                                max_height: Val::Px(32.0),
                                                justify_content: JustifyContent::Center,
                                                align_items: AlignItems::Center,
                                                border: UiRect::all(Val::Px(1.0)),
                                                ..default()
                                            },
                                            HudLookedBlockChestPreviewMore,
                                            BackgroundColor(color_background().into()),
                                            BorderColor::all(color_background_hover()),
                                            Visibility::Hidden,
                                            Pickable::IGNORE,
                                        ))
                                        .with_children(|slot| {
                                            slot.spawn((
                                                Paragraph {
                                                    text: "+".to_string(),
                                                    ..default()
                                                },
                                                UiTextTone::Heading,
                                                Pickable::IGNORE,
                                            ));
                                        });
                                    });
                                });
                        });
                        card.spawn((
                            ProgressBar {
                                value: 0.0,
                                min: 0.0,
                                max: 100.0,
                                ..default()
                            },
                            HudLookedBlockProgress,
                            Visibility::Hidden,
                            Pickable::IGNORE,
                        ));
                    });
            });

            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(8.0),
                    ..default()
                },
                BackgroundColor::DEFAULT,
            ))
            .with_children(|hud| {
                hud.spawn((
                    Paragraph {
                        text: String::new(),
                        ..default()
                    },
                    Node {
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.06, 0.08, 0.11, 0.58)),
                    BorderColor::all(Color::srgba(0.22, 0.28, 0.33, 0.58)),
                    CssID(HUD_SELECTED_TOOLTIP_ID.to_string()),
                    HotbarSelectionTooltipText,
                    UiTextTone::HotbarTooltip,
                    Visibility::Hidden,
                    Pickable::IGNORE,
                ));
                hud.spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(10.0),
                        padding: UiRect::all(Val::Px(10.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(color_background().into()),
                    BorderColor::all(color_background_hover()),
                ))
                .with_children(|bar| {
                    for i in 1..=HOTBAR_SLOTS {
                        let idx = format!("{i:02}");
                        bar.spawn((
                            Button {
                                text: String::new(),
                                ..default()
                            },
                            Visibility::Inherited,
                            CssID(format!("{HUD_SLOT_PREFIX}{idx}")),
                            UiButtonKind::InventorySlot,
                            UiButtonTone::Normal,
                        ))
                        .with_children(|slot| {
                            slot.spawn((
                                Paragraph {
                                    text: String::new(),
                                    ..default()
                                },
                                CssID(format!("{HUD_SLOT_BADGE_PREFIX}{idx}")),
                                BackgroundColor(Color::srgba(0.06, 0.06, 0.08, 0.9)),
                                Visibility::Hidden,
                                Pickable::IGNORE,
                            ));
                        });
                    }
                });
            });
        })
        .id();

    let _pause_menu_root = commands
        .spawn((
            Name::new("UI Pause Root"),
            PauseMenuRoot,
            Visibility::Hidden,
            full_screen_center_node(),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            ZIndex(50),
        ))
        .with_children(|root| {
            root.spawn((
                menu_panel_node(),
                BackgroundColor(color_background().into()),
                BorderColor::all(color_background_hover()),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Paragraph {
                        text: language.localize_name_key("KEY_UI_PAUSE_MENU").to_string(),
                        ..default()
                    },
                    UiTextTone::Heading,
                ));
                panel.spawn((
                    Button {
                        text: language.localize_name_key("KEY_UI_BACK_TO_GAME").to_string(),
                        ..default()
                    },
                    CssID(PAUSE_PLAY_ID.to_string()),
                    UiButtonKind::Action,
                    UiButtonTone::Accent,
                ));
                panel.spawn((
                    Button {
                        text: language.localize_name_key("KEY_UI_SETTINGS").to_string(),
                        ..default()
                    },
                    CssID(PAUSE_SETTINGS_ID.to_string()),
                    UiButtonKind::Action,
                    UiButtonTone::Normal,
                ));
                panel.spawn((
                    Button {
                        text: language.localize_name_key("KEY_UI_MAIN_MENU").to_string(),
                        ..default()
                    },
                    CssID(PAUSE_CLOSE_ID.to_string()),
                    UiButtonKind::Action,
                    UiButtonTone::Normal,
                ));
            });
        })
        .id();

    let _structure_build_root = commands
        .spawn((
            Name::new("UI Structure Build Root"),
            StructureBuildRoot,
            Visibility::Hidden,
            full_screen_center_node(),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.40)),
            ZIndex(52),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Px(420.0),
                    min_height: Val::Px(190.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(10.0),
                    padding: UiRect::all(Val::Px(14.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(color_background().into()),
                BorderColor::all(color_accent()),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Paragraph {
                        text: language.localize_name_key("KEY_UI_BUILD_STRUCTURES").to_string(),
                        ..default()
                    },
                    UiTextTone::Heading,
                ));
                panel.spawn((
                    Paragraph {
                        text: language
                            .localize_name_key("KEY_UI_BUILD_SELECT_STRUCTURE")
                            .to_string(),
                        ..default()
                    },
                    UiTextTone::Darker,
                ));
                panel.spawn((
                    Button {
                        text: language.localize_name_key("KEY_UI_WORKBENCH").to_string(),
                        ..default()
                    },
                    CssID(STRUCTURE_BUILD_WORKBENCH_ID.to_string()),
                    UiButtonKind::ActionRow,
                    UiButtonTone::Accent,
                ));
                panel.spawn((
                    Paragraph {
                        text: language
                            .localize_name_key("KEY_UI_BUILD_HINT_MISSING_MATERIAL")
                            .to_string(),
                        ..default()
                    },
                    CssID(STRUCTURE_BUILD_HINT_ID.to_string()),
                    UiTextTone::Darker,
                ));
            });
        })
        .id();

    let _workbench_recipe_root = commands
        .spawn((
            Name::new("UI Workbench Recipe Root"),
            WorkbenchRecipeRoot,
            Visibility::Hidden,
            full_screen_center_node(),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.40)),
            ZIndex(53),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Percent(89.0),
                    max_width: Val::Px(1032.0),
                    min_height: Val::Px(620.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(12.0),
                    padding: UiRect::all(Val::Px(18.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                WorkbenchRecipeMainPanel,
                BackgroundColor(color_background().into()),
                BorderColor::all(color_accent()),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Paragraph {
                        text: language.localize_name_key("KEY_UI_WORKBENCH").to_string(),
                        ..default()
                    },
                    CssID(WORKBENCH_RECIPE_TITLE_ID.to_string()),
                    UiTextTone::Heading,
                ));
                panel.spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        min_height: Val::Px(540.0),
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(12.0),
                        ..default()
                    },
                    BackgroundColor::DEFAULT,
                ))
                .with_children(|content| {
                    content
                        .spawn((
                            Node {
                                width: Val::Auto,
                                min_width: Val::Px(0.0),
                                flex_grow: 1.0,
                                flex_shrink: 1.0,
                                flex_direction: FlexDirection::Column,
                                row_gap: Val::Px(14.0),
                                padding: UiRect::all(Val::Px(10.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            WorkbenchRecipeInventoryPanel,
                            BackgroundColor(color_single_player_list_background().into()),
                            BorderColor::all(color_background_hover()),
                        ))
                        .with_children(|left| {
                            left.spawn((
                                Node {
                                    width: Val::Percent(100.0),
                                    flex_direction: FlexDirection::Row,
                                    justify_content: JustifyContent::Start,
                                    align_items: AlignItems::Start,
                                    column_gap: Val::Px(24.0),
                                    min_height: Val::Px(190.0),
                                    ..default()
                                },
                                BackgroundColor::DEFAULT,
                            ))
                            .with_children(|craft_row| {
                                craft_row
                                    .spawn((
                                        Node {
                                            width: Val::Px(280.0),
                                            flex_direction: FlexDirection::Row,
                                            justify_content: JustifyContent::SpaceBetween,
                                            align_items: AlignItems::Start,
                                            ..default()
                                        },
                                        BackgroundColor::DEFAULT,
                                    ))
                                    .with_children(|slots| {
                                        let mut slot_counter = 1usize;
                                        for (column, count) in
                                            [3usize, 2usize, 3usize].into_iter().enumerate()
                                        {
                                            slots
                                                .spawn((
                                                    Node {
                                                        width: Val::Px(62.0),
                                                        flex_direction: FlexDirection::Column,
                                                        row_gap: Val::Px(10.0),
                                                        margin: if column == 1 {
                                                            UiRect::top(Val::Px(22.0))
                                                        } else {
                                                            UiRect::default()
                                                        },
                                                        ..default()
                                                    },
                                                    BackgroundColor::DEFAULT,
                                                ))
                                                .with_children(|col| {
                                                    for _ in 0..count {
                                                        let idx = format!("{slot_counter:02}");
                                                        slot_counter += 1;
                                                        col.spawn((
                                                            Button {
                                                                text: String::new(),
                                                                ..default()
                                                            },
                                                            Visibility::Inherited,
                                                            CssID(format!(
                                                                "{WORKBENCH_CRAFT_FRAME_PREFIX}{idx}"
                                                            )),
                                                            UiButtonKind::InventorySlot,
                                                            UiButtonTone::Normal,
                                                        ))
                                                        .with_children(|slot| {
                                                            slot.spawn((
                                                                Paragraph {
                                                                    text: String::new(),
                                                                    ..default()
                                                                },
                                                                CssID(format!(
                                                                    "{WORKBENCH_CRAFT_BADGE_PREFIX}{idx}"
                                                                )),
                                                                BackgroundColor(Color::srgba(
                                                                    0.06, 0.06, 0.08, 0.9,
                                                                )),
                                                                Visibility::Hidden,
                                                                Pickable::IGNORE,
                                                            ));
                                                        });
                                                    }
                                                });
                                        }
                                    });
                                craft_row
                                    .spawn((
                                        Node {
                                            min_height: Val::Px(188.0),
                                            flex_direction: FlexDirection::Column,
                                            justify_content: JustifyContent::Center,
                                            align_items: AlignItems::Center,
                                            row_gap: Val::Px(8.0),
                                            ..default()
                                        },
                                        BackgroundColor::DEFAULT,
                                    ))
                                    .with_children(|right_side| {
                                        right_side.spawn((
                                            Paragraph {
                                                text: String::new(),
                                                ..default()
                                            },
                                            CssID(WORKBENCH_RESULT_TIME_ID.to_string()),
                                            UiTextTone::Darker,
                                        ));
                                        right_side
                                            .spawn((
                                                Node {
                                                    flex_direction: FlexDirection::Row,
                                                    justify_content: JustifyContent::Center,
                                                    align_items: AlignItems::Center,
                                                    column_gap: Val::Px(10.0),
                                                    ..default()
                                                },
                                                BackgroundColor::DEFAULT,
                                            ))
                                            .with_children(|result_row| {
                                                result_row.spawn((
                                                    Paragraph {
                                                        text: "->".to_string(),
                                                        ..default()
                                                    },
                                                    UiTextTone::Darker,
                                                ));
                                                result_row
                                                    .spawn((
                                                        Button {
                                                            text: String::new(),
                                                            ..default()
                                                        },
                                                        Visibility::Inherited,
                                                        CssID(WORKBENCH_RESULT_FRAME_ID.to_string()),
                                                        UiButtonKind::InventoryResultSlot,
                                                        UiButtonTone::Normal,
                                                    ))
                                                    .with_children(|slot| {
                                                        slot.spawn((
                                                            Paragraph {
                                                                text: String::new(),
                                                                ..default()
                                                            },
                                                            CssID(WORKBENCH_RESULT_BADGE_ID.to_string()),
                                                            BackgroundColor(Color::srgba(
                                                                0.06, 0.06, 0.08, 0.9,
                                                            )),
                                                            Visibility::Hidden,
                                                            Pickable::IGNORE,
                                                        ));
                                                    });
                                            });
                                    });
                            });
                            left.spawn((
                                Node {
                                    width: Val::Px(312.0),
                                    ..default()
                                },
                                BackgroundColor::DEFAULT,
                            ))
                            .with_children(|progress| {
                                progress.spawn((
                                    ProgressBar {
                                        value: 0.0,
                                        min: 0.0,
                                        max: 100.0,
                                        ..default()
                                    },
                                    Node {
                                        width: Val::Px(312.0),
                                        ..default()
                                    },
                                    CssID(WORKBENCH_RESULT_PROGRESS_ID.to_string()),
                                ));
                            });
                            left.spawn((
                                Paragraph {
                                    text: "Tools".to_string(),
                                    ..default()
                                },
                                UiTextTone::CardName,
                            ));
                            left.spawn((
                                Node {
                                    width: Val::Percent(100.0),
                                    align_items: AlignItems::Center,
                                    column_gap: Val::Px(8.0),
                                    margin: UiRect::top(Val::Px(16.0)),
                                    ..default()
                                },
                                BackgroundColor::DEFAULT,
                            ))
                            .with_children(|tools| {
                                for i in 1..=5usize {
                                    let idx = format!("{i:02}");
                                    tools.spawn((
                                        Button {
                                            text: String::new(),
                                            ..default()
                                        },
                                        Visibility::Inherited,
                                        CssID(format!("{WORKBENCH_TOOL_FRAME_PREFIX}{idx}")),
                                        UiButtonKind::InventorySlot,
                                        UiButtonTone::Normal,
                                    ))
                                    .with_children(|slot| {
                                        slot.spawn((
                                            Paragraph {
                                                text: String::new(),
                                                ..default()
                                            },
                                            CssID(format!("{WORKBENCH_TOOL_BADGE_PREFIX}{idx}")),
                                            BackgroundColor(Color::srgba(0.06, 0.06, 0.08, 0.9)),
                                            Visibility::Hidden,
                                            Pickable::IGNORE,
                                        ));
                                    });
                                }
                            });
                            left.spawn((
                                Paragraph {
                                    text: language
                                        .localize_name_key("KEY_UI_WORKBENCH_PLAYER_INVENTORY")
                                        .to_string(),
                                    ..default()
                                },
                                UiTextTone::CardName,
                            ));
                            left.spawn((
                                Node {
                                    width: Val::Percent(100.0),
                                    display: Display::Grid,
                                    grid_template_columns: RepeatedGridTrack::fr(6, 1.0),
                                    grid_auto_rows: vec![GridTrack::px(56.0)],
                                    row_gap: Val::Px(8.0),
                                    column_gap: Val::Px(8.0),
                                    ..default()
                                },
                                BackgroundColor::DEFAULT,
                            ))
                            .with_children(|grid| {
                                for i in 1..=PLAYER_INVENTORY_SLOTS {
                                    let idx = format!("{i:02}");
                                    grid.spawn((
                                        Button {
                                            text: String::new(),
                                            ..default()
                                        },
                                        Visibility::Inherited,
                                        CssID(format!(
                                            "{WORKBENCH_PLAYER_INVENTORY_FRAME_PREFIX}{idx}"
                                        )),
                                        UiButtonKind::InventorySlot,
                                        UiButtonTone::Normal,
                                    ))
                                    .with_children(|slot| {
                                        slot.spawn((
                                            Paragraph {
                                                text: String::new(),
                                                ..default()
                                            },
                                            CssID(format!(
                                                "{WORKBENCH_PLAYER_INVENTORY_BADGE_PREFIX}{idx}"
                                            )),
                                            BackgroundColor(Color::srgba(0.06, 0.06, 0.08, 0.9)),
                                            Visibility::Hidden,
                                            Pickable::IGNORE,
                                        ));
                                    });
                                }
                            });
                        });
                    content
                        .spawn((
                            Node {
                                width: Val::Px(448.0),
                                min_width: Val::Px(448.0),
                                max_width: Val::Px(448.0),
                                flex_shrink: 0.0,
                                flex_direction: FlexDirection::Column,
                                row_gap: Val::Px(8.0),
                                padding: UiRect::all(Val::Px(10.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BackgroundColor(color_single_player_list_background().into()),
                            BorderColor::all(color_background_hover()),
                        ))
                        .with_children(|right| {
                            right.spawn((
                                Paragraph {
                                    text: language.localize_name_key("KEY_UI_ITEMS_TITLE").to_string(),
                                    ..default()
                                },
                                UiTextTone::Heading,
                            ));
                            right.spawn((
                                Paragraph {
                                    text: format!(
                                        "{} 0",
                                        language.localize_name_key("KEY_UI_REGISTERED")
                                    ),
                                    ..default()
                                },
                                CssID(WORKBENCH_ITEMS_TOTAL_ID.to_string()),
                                UiTextTone::Darker,
                            ));
                            right.spawn((
                                Node {
                                    width: Val::Percent(100.0),
                                    align_items: AlignItems::Center,
                                    column_gap: Val::Px(8.0),
                                    ..default()
                                },
                                BackgroundColor::DEFAULT,
                            ))
                            .with_children(|pager| {
                                pager.spawn((
                                    Button {
                                        text: "<".to_string(),
                                        ..default()
                                    },
                                    CssID(WORKBENCH_ITEMS_PREV_ID.to_string()),
                                    UiButtonKind::ActionRow,
                                    UiButtonTone::Normal,
                                ));
                                pager.spawn((
                                    Paragraph {
                                        text: "1/1".to_string(),
                                        ..default()
                                    },
                                    CssID(WORKBENCH_ITEMS_PAGE_ID.to_string()),
                                    UiTextTone::Darker,
                                ));
                                pager.spawn((
                                    Button {
                                        text: ">".to_string(),
                                        ..default()
                                    },
                                    CssID(WORKBENCH_ITEMS_NEXT_ID.to_string()),
                                    UiButtonKind::ActionRow,
                                    UiButtonTone::Normal,
                                ));
                            });
                            right.spawn((
                                WorkbenchRecipeItemGridRoot,
                                Node {
                                    width: Val::Percent(100.0),
                                    display: Display::Grid,
                                    grid_template_columns: RepeatedGridTrack::fr(
                                        CREATIVE_PANEL_COLUMNS as u16,
                                        1.0,
                                    ),
                                    grid_auto_rows: vec![GridTrack::px(56.0)],
                                    row_gap: Val::Px(6.0),
                                    column_gap: Val::Px(6.0),
                                    ..default()
                                },
                                BackgroundColor::DEFAULT,
                            ))
                            .with_children(|grid| {
                                for i in 1..=CREATIVE_PANEL_PAGE_SIZE {
                                    let idx = format!("{i:02}");
                                    grid.spawn((
                                        Button {
                                            text: String::new(),
                                            ..default()
                                        },
                                        Visibility::Inherited,
                                        CssID(format!("{WORKBENCH_ITEMS_SLOT_PREFIX}{idx}")),
                                        UiButtonKind::InventorySlot,
                                        UiButtonTone::Normal,
                                    ));
                                }
                            });
                            right.spawn((
                                Button {
                                    text: language.localize_name_key("KEY_UI_TRASH").to_string(),
                                    ..default()
                                },
                                CssID(WORKBENCH_TRASH_BUTTON_ID.to_string()),
                                UiButtonKind::ActionRow,
                                UiButtonTone::Normal,
                            ));
                            right.spawn((
                                Node {
                                    width: Val::Percent(100.0),
                                    min_height: Val::Px(84.0),
                                    flex_direction: FlexDirection::Column,
                                    row_gap: Val::Px(4.0),
                                    padding: UiRect::all(Val::Px(8.0)),
                                    border: UiRect::all(Val::Px(1.0)),
                                    ..default()
                                },
                                BackgroundColor(color_background().into()),
                                BorderColor::all(color_background_hover()),
                            ))
                            .with_children(|recipes| {
                                recipes.spawn((
                                    Paragraph {
                                        text: language.localize_name_key("KEY_UI_RECIPE_INFO").to_string(),
                                        ..default()
                                    },
                                    UiTextTone::CardName,
                                ));
                                recipes.spawn((
                                    Paragraph {
                                        text: language
                                            .localize_name_key("KEY_UI_RECIPE_HINT_SURVIVAL")
                                            .to_string(),
                                        ..default()
                                    },
                                    CssID(WORKBENCH_RECIPE_HINT_ID.to_string()),
                                    UiTextTone::Darker,
                                ));
                            });
                        });
                });
            });
        })
        .id();

    let _chest_inventory_root = commands
        .spawn((
            Name::new("UI Chest Inventory Root"),
            ChestInventoryRoot,
            Visibility::Hidden,
            full_screen_center_node(),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.40)),
            ZIndex(54),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Percent(92.0),
                    max_width: Val::Px(1080.0),
                    min_height: Val::Px(430.0),
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(12.0),
                    padding: UiRect::all(Val::Px(14.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                ChestInventoryMainPanel,
                BackgroundColor(color_background().into()),
                BorderColor::all(color_accent()),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Node {
                        width: Val::Percent(58.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(10.0),
                        ..default()
                    },
                    BackgroundColor::DEFAULT,
                ))
                .with_children(|left| {
                    left.spawn((
                        Paragraph {
                            text: language.localize_name_key("KEY_UI_CHEST_INVENTORY").to_string(),
                            ..default()
                        },
                        CssID(CHEST_INVENTORY_TITLE_ID.to_string()),
                        UiTextTone::Heading,
                    ));
                    left.spawn((
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(8.0),
                            padding: UiRect::all(Val::Px(8.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(color_single_player_list_background().into()),
                        BorderColor::all(color_background_hover()),
                    ))
                    .with_children(|chest_panel| {
                        chest_panel.spawn((
                            Paragraph {
                                text: language.localize_name_key("KEY_UI_CHEST_INVENTORY").to_string(),
                                ..default()
                            },
                            UiTextTone::CardName,
                        ));
                        chest_panel.spawn((
                            Node {
                                width: Val::Percent(100.0),
                                display: Display::Grid,
                                grid_template_columns: RepeatedGridTrack::fr(6, 1.0),
                                grid_auto_rows: vec![GridTrack::px(56.0)],
                                row_gap: Val::Px(8.0),
                                column_gap: Val::Px(8.0),
                                ..default()
                            },
                            BackgroundColor::DEFAULT,
                        ))
                        .with_children(|grid| {
                            for i in 1..=CHEST_INVENTORY_SLOTS {
                                let idx = format!("{i:02}");
                                grid.spawn((
                                    Button {
                                        text: String::new(),
                                        ..default()
                                    },
                                    Visibility::Inherited,
                                    CssID(format!("{CHEST_SLOT_FRAME_PREFIX}{idx}")),
                                    UiButtonKind::InventorySlot,
                                    UiButtonTone::Normal,
                                ))
                                .with_children(|slot| {
                                    slot.spawn((
                                        Paragraph {
                                            text: String::new(),
                                            ..default()
                                        },
                                        CssID(format!("{CHEST_SLOT_BADGE_PREFIX}{idx}")),
                                        BackgroundColor(Color::srgba(0.06, 0.06, 0.08, 0.9)),
                                        Visibility::Hidden,
                                        Pickable::IGNORE,
                                    ));
                                });
                            }
                        });
                    });
                    left.spawn((
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(8.0),
                            padding: UiRect::all(Val::Px(8.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(color_single_player_list_background().into()),
                        BorderColor::all(color_background_hover()),
                    ))
                    .with_children(|inventory_panel| {
                        inventory_panel.spawn((
                            Paragraph {
                                text: language
                                    .localize_name_key("KEY_UI_WORKBENCH_PLAYER_INVENTORY")
                                    .to_string(),
                                ..default()
                            },
                            UiTextTone::CardName,
                        ));
                        inventory_panel.spawn((
                            Node {
                                width: Val::Percent(100.0),
                                display: Display::Grid,
                                grid_template_columns: RepeatedGridTrack::fr(6, 1.0),
                                grid_auto_rows: vec![GridTrack::px(56.0)],
                                row_gap: Val::Px(8.0),
                                column_gap: Val::Px(8.0),
                                ..default()
                            },
                            BackgroundColor::DEFAULT,
                        ))
                        .with_children(|grid| {
                            for i in 1..=PLAYER_INVENTORY_SLOTS {
                                let idx = format!("{i:02}");
                                grid.spawn((
                                    Button {
                                        text: String::new(),
                                        ..default()
                                    },
                                    Visibility::Inherited,
                                    CssID(format!("{CHEST_PLAYER_INVENTORY_FRAME_PREFIX}{idx}")),
                                    UiButtonKind::InventorySlot,
                                    UiButtonTone::Normal,
                                ))
                                .with_children(|slot| {
                                    slot.spawn((
                                        Paragraph {
                                            text: String::new(),
                                            ..default()
                                        },
                                        CssID(format!("{CHEST_PLAYER_INVENTORY_BADGE_PREFIX}{idx}")),
                                        BackgroundColor(Color::srgba(0.06, 0.06, 0.08, 0.9)),
                                        Visibility::Hidden,
                                        Pickable::IGNORE,
                                    ));
                                });
                            }
                        });
                    });
                });
                panel
                    .spawn((
                        Node {
                            width: Val::Px(448.0),
                            min_width: Val::Px(448.0),
                            max_width: Val::Px(448.0),
                            flex_shrink: 0.0,
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(8.0),
                            padding: UiRect::all(Val::Px(10.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(color_single_player_list_background().into()),
                        BorderColor::all(color_background_hover()),
                    ))
                    .with_children(|right| {
                        right.spawn((
                            Paragraph {
                                text: language.localize_name_key("KEY_UI_ITEMS_TITLE").to_string(),
                                ..default()
                            },
                            UiTextTone::Heading,
                        ));
                        right.spawn((
                            Paragraph {
                                text: format!(
                                    "{} 0",
                                    language.localize_name_key("KEY_UI_REGISTERED")
                                ),
                                ..default()
                            },
                            CssID(CHEST_ITEMS_TOTAL_ID.to_string()),
                            UiTextTone::Darker,
                        ));
                        right.spawn((
                            Node {
                                width: Val::Percent(100.0),
                                align_items: AlignItems::Center,
                                column_gap: Val::Px(8.0),
                                ..default()
                            },
                            BackgroundColor::DEFAULT,
                        ))
                        .with_children(|pager| {
                            pager.spawn((
                                Button {
                                    text: "<".to_string(),
                                    ..default()
                                },
                                CssID(CHEST_ITEMS_PREV_ID.to_string()),
                                UiButtonKind::ActionRow,
                                UiButtonTone::Normal,
                            ));
                            pager.spawn((
                                Paragraph {
                                    text: "1/1".to_string(),
                                    ..default()
                                },
                                CssID(CHEST_ITEMS_PAGE_ID.to_string()),
                                UiTextTone::Darker,
                            ));
                            pager.spawn((
                                Button {
                                    text: ">".to_string(),
                                    ..default()
                                },
                                CssID(CHEST_ITEMS_NEXT_ID.to_string()),
                                UiButtonKind::ActionRow,
                                UiButtonTone::Normal,
                            ));
                        });
                        right.spawn((
                            ChestInventoryItemGridRoot,
                            Node {
                                width: Val::Percent(100.0),
                                display: Display::Grid,
                                grid_template_columns: RepeatedGridTrack::fr(
                                    CREATIVE_PANEL_COLUMNS as u16,
                                    1.0,
                                ),
                                grid_auto_rows: vec![GridTrack::px(56.0)],
                                row_gap: Val::Px(6.0),
                                column_gap: Val::Px(6.0),
                                ..default()
                            },
                            BackgroundColor::DEFAULT,
                        ))
                        .with_children(|grid| {
                            for i in 1..=CREATIVE_PANEL_PAGE_SIZE {
                                let idx = format!("{i:02}");
                                grid.spawn((
                                    Button {
                                        text: String::new(),
                                        ..default()
                                    },
                                    Visibility::Inherited,
                                    CssID(format!("{CHEST_ITEMS_SLOT_PREFIX}{idx}")),
                                    UiButtonKind::InventorySlot,
                                    UiButtonTone::Normal,
                                ));
                            }
                        });
                        right.spawn((
                            Button {
                                text: language.localize_name_key("KEY_UI_TRASH").to_string(),
                                ..default()
                            },
                            CssID(CHEST_TRASH_BUTTON_ID.to_string()),
                            UiButtonKind::ActionRow,
                            UiButtonTone::Normal,
                        ));
                        right.spawn((
                            Paragraph {
                                text: language.localize_name_key("KEY_UI_CHEST_HINT").to_string(),
                                ..default()
                            },
                            CssID(CHEST_INVENTORY_HINT_ID.to_string()),
                            UiTextTone::Darker,
                        ));
                    });
            });
        })
        .id();

    let _inventory_root = commands
        .spawn((
            Name::new("UI Inventory Root"),
            PlayerInventoryRoot,
            Visibility::Hidden,
            Pickable::IGNORE,
            full_screen_center_node(),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
            ZIndex(51),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Percent(92.0),
                    max_width: Val::Px(1020.0),
                    min_height: Val::Px(420.0),
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(12.0),
                    padding: UiRect::all(Val::Px(14.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                InventoryMainPanel,
                BackgroundColor(color_background().into()),
                BorderColor::all(color_accent()),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Node {
                        width: Val::Percent(56.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(10.0),
                        ..default()
                    },
                    BackgroundColor::DEFAULT,
                ))
                .with_children(|left| {
                    left.spawn((
                        Paragraph {
                            text: language.localize_name_key("KEY_UI_INVENTORY").to_string(),
                            ..default()
                        },
                        UiTextTone::Heading,
                    ));
                    left.spawn((
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(8.0),
                            padding: UiRect::all(Val::Px(8.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(color_single_player_list_background().into()),
                        BorderColor::all(color_background_hover()),
                    ))
                    .with_children(|craft_panel| {
                        craft_panel.spawn((
                            Paragraph {
                                text: language.localize_name_key("KEY_UI_HAND_CRAFTED").to_string(),
                                ..default()
                            },
                            UiTextTone::CardName,
                        ));
                        craft_panel.spawn((
                            Node {
                                width: Val::Percent(100.0),
                                align_items: AlignItems::Center,
                                column_gap: Val::Px(8.0),
                                ..default()
                            },
                            BackgroundColor::DEFAULT,
                        ))
                        .with_children(|row| {
                            for i in 1..=HAND_CRAFTED_INPUT_SLOTS {
                                let idx = format!("{i:02}");
                                row.spawn((
                                    Button {
                                        text: String::new(),
                                        ..default()
                                    },
                                    Visibility::Inherited,
                                    CssID(format!("{HAND_CRAFTED_FRAME_PREFIX}{idx}")),
                                    UiButtonKind::InventorySlot,
                                    UiButtonTone::Normal,
                                ))
                                .with_children(|slot| {
                                    slot.spawn((
                                        Paragraph {
                                            text: String::new(),
                                            ..default()
                                        },
                                        CssID(format!("{HAND_CRAFTED_BADGE_PREFIX}{idx}")),
                                        BackgroundColor(Color::srgba(0.06, 0.06, 0.08, 0.9)),
                                        Visibility::Hidden,
                                        Pickable::IGNORE,
                                    ));
                                });
                            }

                            row.spawn((
                                Paragraph {
                                    text: "->".to_string(),
                                    ..default()
                                },
                                UiTextTone::Darker,
                            ));
                            row.spawn((
                                Button {
                                    text: String::new(),
                                    ..default()
                                },
                                Visibility::Inherited,
                                CssID(HAND_CRAFTED_RESULT_FRAME_ID.to_string()),
                                UiButtonKind::InventoryResultSlot,
                                UiButtonTone::Normal,
                            ))
                            .with_children(|slot| {
                                slot.spawn((
                                    Paragraph {
                                        text: String::new(),
                                        ..default()
                                    },
                                    CssID(HAND_CRAFTED_RESULT_BADGE_ID.to_string()),
                                    BackgroundColor(Color::srgba(0.06, 0.06, 0.08, 0.9)),
                                    Visibility::Hidden,
                                    Pickable::IGNORE,
                                ));
                            });
                        });
                    });
                    left.spawn((
                        Paragraph {
                            text: format!("{} 0", language.localize_name_key("KEY_UI_ITEMS")),
                            ..default()
                        },
                        CssID(PLAYER_INVENTORY_TOTAL_ID.to_string()),
                    ));
                    left.spawn((
                        Node {
                            width: Val::Percent(100.0),
                            display: Display::Grid,
                            grid_template_columns: RepeatedGridTrack::fr(6, 1.0),
                            grid_auto_rows: vec![GridTrack::px(56.0)],
                            row_gap: Val::Px(8.0),
                            column_gap: Val::Px(8.0),
                            ..default()
                        },
                        BackgroundColor::DEFAULT,
                    ))
                    .with_children(|grid| {
                        for i in 1..=PLAYER_INVENTORY_SLOTS {
                            let idx = format!("{i:02}");
                            grid.spawn((
                                Button {
                                    text: String::new(),
                                    ..default()
                                },
                                Visibility::Inherited,
                                CssID(format!("{PLAYER_INVENTORY_FRAME_PREFIX}{idx}")),
                                UiButtonKind::InventorySlot,
                                UiButtonTone::Normal,
                            ))
                            .with_children(|slot| {
                                slot.spawn((
                                    Paragraph {
                                        text: String::new(),
                                        ..default()
                                    },
                                    CssID(format!("{PLAYER_INVENTORY_BADGE_PREFIX}{idx}")),
                                    BackgroundColor(Color::srgba(0.06, 0.06, 0.08, 0.9)),
                                    Visibility::Hidden,
                                    Pickable::IGNORE,
                                ));
                            });
                        }
                    });
                });

                panel.spawn((
                    Node {
                        width: Val::Percent(44.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(8.0),
                        padding: UiRect::all(Val::Px(10.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    InventoryDropZonePanel,
                    BackgroundColor(color_single_player_list_background().into()),
                    BorderColor::all(color_background_hover()),
                ))
                .with_children(|right| {
                    right.spawn((
                        Paragraph {
                            text: language.localize_name_key("KEY_UI_ITEMS_TITLE").to_string(),
                            ..default()
                        },
                        UiTextTone::Heading,
                    ));
                    right.spawn((
                        Paragraph {
                            text: format!("{} 0", language.localize_name_key("KEY_UI_REGISTERED")),
                            ..default()
                        },
                        CssID(CREATIVE_PANEL_TOTAL_ID.to_string()),
                        UiTextTone::Darker,
                    ));
                    right.spawn((
                        Node {
                            width: Val::Percent(100.0),
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(8.0),
                            ..default()
                        },
                        BackgroundColor::DEFAULT,
                    ))
                    .with_children(|pager| {
                        pager.spawn((
                            Button {
                                text: "<".to_string(),
                                ..default()
                            },
                            CssID(CREATIVE_PANEL_PREV_ID.to_string()),
                            UiButtonKind::ActionRow,
                            UiButtonTone::Normal,
                        ));
                        pager.spawn((
                            Paragraph {
                                text: "1/1".to_string(),
                                ..default()
                            },
                            CssID(CREATIVE_PANEL_PAGE_ID.to_string()),
                            UiTextTone::Darker,
                        ));
                        pager.spawn((
                            Button {
                                text: ">".to_string(),
                                ..default()
                            },
                            CssID(CREATIVE_PANEL_NEXT_ID.to_string()),
                            UiButtonKind::ActionRow,
                            UiButtonTone::Normal,
                        ));
                    });
                    right.spawn((
                        CreativePanelGridRoot,
                        Node {
                            width: Val::Percent(100.0),
                            display: Display::Grid,
                            grid_template_columns: RepeatedGridTrack::fr(
                                CREATIVE_PANEL_COLUMNS as u16,
                                1.0,
                            ),
                            grid_auto_rows: vec![GridTrack::px(56.0)],
                            row_gap: Val::Px(6.0),
                            column_gap: Val::Px(6.0),
                            ..default()
                        },
                        BackgroundColor::DEFAULT,
                    ))
                    .with_children(|grid| {
                        for i in 1..=CREATIVE_PANEL_PAGE_SIZE {
                            let idx = format!("{i:02}");
                            grid.spawn((
                                Button {
                                    text: String::new(),
                                    ..default()
                                },
                                Visibility::Inherited,
                                CssID(format!("{CREATIVE_PANEL_SLOT_PREFIX}{idx}")),
                                UiButtonKind::InventorySlot,
                                UiButtonTone::Normal,
                            ));
                        }
                    });
                    right.spawn((
                        Button {
                            text: language.localize_name_key("KEY_UI_TRASH").to_string(),
                            ..default()
                        },
                        CssID(INVENTORY_TRASH_BUTTON_ID.to_string()),
                        UiButtonKind::ActionRow,
                        UiButtonTone::Normal,
                    ));
                    right.spawn((
                        Node {
                            width: Val::Percent(100.0),
                            min_height: Val::Px(84.0),
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(4.0),
                            padding: UiRect::all(Val::Px(8.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(color_background().into()),
                        BorderColor::all(color_background_hover()),
                    ))
                    .with_children(|recipes| {
                        recipes.spawn((
                            Paragraph {
                                text: language.localize_name_key("KEY_UI_RECIPE_INFO").to_string(),
                                ..default()
                            },
                            UiTextTone::CardName,
                        ));
                        recipes.spawn((
                            Paragraph {
                                text: language
                                    .localize_name_key("KEY_UI_RECIPE_HINT_DEFAULT")
                                    .to_string(),
                                ..default()
                            },
                            CssID(CREATIVE_RECIPE_HINT_ID.to_string()),
                            UiTextTone::Darker,
                        ));
                    });
                });
            });
            root.spawn((
                Name::new("Inventory Tooltip"),
                InventoryTooltipRoot,
                Visibility::Hidden,
                Pickable::IGNORE,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    min_width: Val::Px(170.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(2.0),
                    padding: UiRect::axes(Val::Px(13.0), Val::Px(11.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(color_background_hover().with_alpha(0.96)),
                BorderColor::all(color_accent()),
                ZIndex(110),
            ))
            .with_children(|tooltip| {
                tooltip.spawn((
                    Paragraph {
                        text: String::new(),
                        ..default()
                    },
                    TextLayout::new_with_justify(Justify::Center),
                    CssID(INVENTORY_TOOLTIP_NAME_ID.to_string()),
                    UiTextTone::TooltipName,
                    Pickable::IGNORE,
                ));
                tooltip.spawn((
                    Paragraph {
                        text: String::new(),
                        ..default()
                    },
                    TextLayout::new_with_justify(Justify::Center),
                    CssID(INVENTORY_TOOLTIP_KEY_ID.to_string()),
                    UiTextTone::TooltipKey,
                    Pickable::IGNORE,
                ));
            });
            root.spawn((
                Name::new("Recipe Preview Dialog Root"),
                RecipePreviewDialogRoot,
                Visibility::Hidden,
                Pickable::IGNORE,
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor::DEFAULT,
                ZIndex(85),
            ))
            .with_children(|overlay| {
                overlay
                    .spawn((
                        Visibility::Inherited,
                        RecipePreviewDialogPanel,
                        Node {
                            width: Val::Px(860.0),
                            min_width: Val::Px(860.0),
                            min_height: Val::Px(400.0),
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(10.0),
                            padding: UiRect::all(Val::Px(12.0)),
                            border: UiRect::all(Val::Px(2.0)),
                            ..default()
                        },
                        BackgroundColor(color_background().into()),
                        BorderColor::all(color_accent()),
                    ))
                    .with_children(|panel| {
                        panel.spawn((
                            Paragraph {
                                text: language.localize_name_key("KEY_UI_RECIPE").to_string(),
                                ..default()
                            },
                            CssID(RECIPE_PREVIEW_TITLE_ID.to_string()),
                            UiTextTone::Heading,
                        ));
                        panel.spawn((
                            Paragraph {
                                text: language.localize_name_key("KEY_UI_HAND_CRAFTED").to_string(),
                                ..default()
                            },
                            CssID(RECIPE_PREVIEW_MODE_ID.to_string()),
                            UiTextTone::Darker,
                        ));
                        panel.spawn((
                            Node {
                                width: Val::Percent(100.0),
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                column_gap: Val::Px(8.0),
                                ..default()
                            },
                            BackgroundColor::DEFAULT,
                        ))
                        .with_children(|tabs| {
                            tabs.spawn((
                                Button {
                                    text: "<".to_string(),
                                    ..default()
                                },
                                CssID(RECIPE_PREVIEW_TAB_PREV_ID.to_string()),
                                UiButtonKind::RecipeTab,
                                UiButtonTone::Normal,
                            ));
                            for i in 1..=RECIPE_PREVIEW_TABS_PER_PAGE {
                                tabs.spawn((
                                    Button {
                                        text: String::new(),
                                        ..default()
                                    },
                                    CssID(format!("{RECIPE_PREVIEW_TAB_PREFIX}{i:02}")),
                                    UiButtonKind::RecipeTab,
                                    UiButtonTone::Normal,
                                ))
                                .with_children(|tab| {
                                    tab.spawn((
                                        Img {
                                            src: None,
                                            ..default()
                                        },
                                        CssID(format!("{RECIPE_PREVIEW_TAB_ICON_PREFIX}{i:02}")),
                                        Pickable::IGNORE,
                                    ));
                                });
                            }
                            tabs.spawn((
                                Button {
                                    text: ">".to_string(),
                                    ..default()
                                },
                                CssID(RECIPE_PREVIEW_TAB_NEXT_ID.to_string()),
                                UiButtonKind::RecipeTab,
                                UiButtonTone::Normal,
                            ));
                        });
                        panel.spawn((
                            Paragraph {
                                text: String::new(),
                                ..default()
                            },
                            CssID(RECIPE_PREVIEW_TAB_TOOLTIP_ID.to_string()),
                            UiTextTone::Darker,
                            Visibility::Hidden,
                        ));
                        panel.spawn((
                            Visibility::Inherited,
                            Node {
                                width: Val::Auto,
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                align_self: AlignSelf::Center,
                                column_gap: Val::Px(6.0),
                                padding: UiRect::all(Val::Px(10.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BackgroundColor(color_single_player_list_background().into()),
                            BorderColor::all(color_background_hover()),
                        ))
                        .with_children(|row| {
                            row.spawn((
                                Node {
                                    display: Display::Grid,
                                    grid_template_columns: RepeatedGridTrack::fr(4, 1.0),
                                    grid_auto_rows: vec![GridTrack::px(56.0)],
                                    row_gap: Val::Px(6.0),
                                    column_gap: Val::Px(8.0),
                                    ..default()
                                },
                                RecipePreviewInputGrid,
                                BackgroundColor::DEFAULT,
                            ))
                            .with_children(|grid| {
                                for i in 1..=RECIPE_PREVIEW_INPUT_SLOTS {
                                    let idx = format!("{i:02}");
                                    grid.spawn((
                                        Button {
                                            text: String::new(),
                                            ..default()
                                        },
                                        Visibility::Inherited,
                                        CssID(format!("{RECIPE_PREVIEW_INPUT_FRAME_PREFIX}{idx}")),
                                        UiButtonKind::InventorySlot,
                                        UiButtonTone::Normal,
                                    ))
                                    .with_children(|slot| {
                                        slot.spawn((
                                            Paragraph {
                                                text: String::new(),
                                                ..default()
                                            },
                                            CssID(format!("{RECIPE_PREVIEW_INPUT_BADGE_PREFIX}{idx}")),
                                            BackgroundColor(Color::srgba(0.06, 0.06, 0.08, 0.9)),
                                            Visibility::Hidden,
                                            Pickable::IGNORE,
                                        ));
                                    });
                                }
                            });

                            row.spawn((
                                Node {
                                    width: Val::Px(74.0),
                                    align_items: AlignItems::Center,
                                    justify_content: JustifyContent::Center,
                                    column_gap: Val::Px(8.0),
                                    padding: UiRect {
                                        right: Val::Px(10.0),
                                        ..default()
                                    },
                                    ..default()
                                },
                                BackgroundColor::DEFAULT,
                            ))
                            .with_children(|result| {
                                result.spawn((
                                    Paragraph {
                                        text: "->".to_string(),
                                        ..default()
                                    },
                                    UiTextTone::Darker,
                                ));
                                result
                                    .spawn((
                                        Button {
                                            text: String::new(),
                                            ..default()
                                        },
                                        Visibility::Inherited,
                                        CssID(RECIPE_PREVIEW_RESULT_FRAME_ID.to_string()),
                                        UiButtonKind::InventoryResultSlot,
                                        UiButtonTone::Normal,
                                    ))
                                    .with_children(|slot| {
                                        slot.spawn((
                                            Paragraph {
                                                text: String::new(),
                                                ..default()
                                            },
                                            CssID(RECIPE_PREVIEW_RESULT_BADGE_ID.to_string()),
                                            BackgroundColor(Color::srgba(0.06, 0.06, 0.08, 0.9)),
                                            Visibility::Hidden,
                                            Pickable::IGNORE,
                                        ));
                                    });
                            });
                        });
                        panel.spawn((
                            Paragraph {
                                text: language
                                    .localize_name_key("KEY_UI_RECIPE_PREVIEW_ONLY")
                                    .to_string(),
                                ..default()
                            },
                            UiTextTone::Darker,
                        ));
                        panel.spawn((
                            Button {
                                text: "+".to_string(),
                                ..default()
                            },
                            CssID(RECIPE_PREVIEW_FILL_ID.to_string()),
                            UiButtonKind::Action,
                            UiButtonTone::Accent,
                        ));
                    });
            });
            root.spawn((
                Name::new("Inventory Cursor Item"),
                InventoryCursorItemRoot,
                Visibility::Hidden,
                Pickable::IGNORE,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Px(56.0),
                    height: Val::Px(56.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(2.0),
                    padding: UiRect::all(Val::Px(3.0)),
                    ..default()
                },
                ZIndex(95),
            ))
            .with_children(|slot| {
                slot.spawn((
                    InventoryCursorItemIcon,
                    ImageNode::default(),
                    Node {
                        width: Val::Px(36.8),
                        height: Val::Px(36.8),
                        justify_self: JustifySelf::Center,
                        align_self: AlignSelf::Center,
                        ..default()
                    },
                    Pickable::IGNORE,
                ));
                slot.spawn((
                    Paragraph {
                        text: String::new(),
                        ..default()
                    },
                    CssID(INVENTORY_CURSOR_BADGE_ID.to_string()),
                    BackgroundColor(Color::srgba(0.06, 0.06, 0.08, 0.9)),
                    Visibility::Hidden,
                    Pickable::IGNORE,
                    InventoryCursorItemBadge,
                ));
            });
        })
        .id();

    let _debug_overlay_root = commands
        .spawn((
            Name::new("UI Debug Overlay Root"),
            DebugOverlayRoot,
            Visibility::Hidden,
            Pickable::IGNORE,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                align_items: AlignItems::Start,
                justify_content: JustifyContent::Start,
                padding: UiRect::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor::DEFAULT,
            ZIndex(65),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Px(460.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    padding: UiRect::all(Val::Px(8.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.08, 0.10, 0.13, 0.86)),
                BorderColor::all(color_background_hover()),
                Pickable::IGNORE,
            ))
            .with_children(|panel| {
                panel.spawn((
                    Paragraph {
                        text: language.localize_name_key("KEY_UI_DEBUG_SYSTEM").to_string(),
                        ..default()
                    },
                    UiTextTone::Heading,
                    Pickable::IGNORE,
                ));
                panel.spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(2.0),
                        ..default()
                    },
                    BackgroundColor(color_accent().into()),
                    Pickable::IGNORE,
                ));
                for id in [
                    ID_BUILD,
                    ID_FPS,
                    ID_FPS_LOW,
                    ID_STREAM_DECODE_QUEUE,
                    ID_STREAM_REMESH_QUEUE,
                    ID_TICK_SPEED,
                    ID_CPU_NAME,
                    ID_APP_CPU,
                    ID_APP_MEM,
                    ID_GPU_NAME,
                    ID_GPU_LOAD,
                    ID_GPU_CLOCK,
                    ID_VRAM,
                ] {
                    panel.spawn((
                        Paragraph {
                            text: String::new(),
                            ..default()
                        },
                        CssID(id.to_string()),
                        Pickable::IGNORE,
                    ));
                }

                panel.spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(6.0),
                        ..default()
                    },
                    BackgroundColor::DEFAULT,
                    Pickable::IGNORE,
                ));
                panel.spawn((
                    Paragraph {
                        text: language.localize_name_key("KEY_UI_DEBUG_WORLD").to_string(),
                        ..default()
                    },
                    UiTextTone::Heading,
                    Pickable::IGNORE,
                ));
                panel.spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(2.0),
                        ..default()
                    },
                    BackgroundColor(color_accent().into()),
                    Pickable::IGNORE,
                ));
                for id in [
                    ID_BIOME,
                    ID_BIOME_CLIMATE,
                    ID_LOOK_BLOCK,
                    ID_CHUNK_COORD,
                    ID_PLAYER_POS,
                ] {
                    panel.spawn((
                        Paragraph {
                            text: String::new(),
                            ..default()
                        },
                        CssID(id.to_string()),
                        Pickable::IGNORE,
                    ));
                }

                panel.spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(6.0),
                        ..default()
                    },
                    BackgroundColor::DEFAULT,
                    Pickable::IGNORE,
                ));
                panel.spawn((
                    Paragraph {
                        text: language.localize_name_key("KEY_UI_DEBUG_SECTION").to_string(),
                        ..default()
                    },
                    UiTextTone::Heading,
                    Pickable::IGNORE,
                ));
                panel.spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(2.0),
                        ..default()
                    },
                    BackgroundColor(color_accent().into()),
                    Pickable::IGNORE,
                ));
                for id in [
                    ID_GRID,
                    ID_INSPECTOR,
                    ID_OVERLAY,
                ] {
                    panel.spawn((
                        Paragraph {
                            text: String::new(),
                            ..default()
                        },
                        CssID(id.to_string()),
                        Pickable::IGNORE,
                    ));
                }
            });
        })
        .id();

    let _benchmark_border_root = commands
        .spawn((
            Name::new("UI Benchmark Border"),
            BenchmarkBorderRoot,
            Visibility::Hidden,
            Pickable::IGNORE,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BorderColor::all(Color::srgb(1.0, 0.1, 0.1)),
            BackgroundColor::DEFAULT,
            ZIndex(180),
        ))
        .id();

    commands
        .spawn((
            Name::new("UI Benchmark Automation Timer"),
            BenchmarkAutomationTimerRoot,
            Visibility::Hidden,
            Pickable::IGNORE,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(16.0),
                bottom: Val::Px(16.0),
                padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            ZIndex(181),
            BackgroundColor(Color::srgba(0.05, 0.05, 0.05, 0.72)),
            BorderColor::all(color_background_hover()),
        ))
        .with_children(|root| {
            root.spawn((
                Paragraph {
                    text: String::new(),
                    ..default()
                },
                BenchmarkAutomationTimerText,
                UiTextTone::Normal,
                Pickable::IGNORE,
            ));
        });
}
