fn spawn_hardcoded_ui(mut commands: Commands, world_gen_config: Option<Res<WorldGenConfig>>) {
    let default_seed = world_gen_config.as_ref().map(|cfg| cfg.seed).unwrap_or(1337);
    let mut single_player_world_list = Entity::PLACEHOLDER;
    let mut multiplayer_server_list = Entity::PLACEHOLDER;

    let _main_menu_root = commands
        .spawn((
            Name::new("UI Main Menu Root"),
            MainMenuRoot,
            Visibility::Hidden,
            full_screen_center_node(),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
            ZIndex(40),
        ))
        .with_children(|root| {
            root.spawn((
                Name::new("Main Menu Panel"),
                menu_panel_node(),
                BackgroundColor(color_background().into()),
                BorderColor::all(color_background_hover()),
                UiTextTone::Normal,
            ))
            .with_children(|panel| {
                panel.spawn((
                    Paragraph {
                        text: "Oplexa".to_string(),
                        ..default()
                    },
                    UiTextTone::Heading,
                ));
                panel.spawn((
                    Button {
                        text: "Single Player".to_string(),
                        ..default()
                    },
                    CssID(MAIN_MENU_SINGLE_PLAYER_ID.to_string()),
                    UiButtonKind::Action,
                    UiButtonTone::Accent,
                ));
                panel.spawn((
                    Button {
                        text: "Multi Player".to_string(),
                        ..default()
                    },
                    CssID(MAIN_MENU_MULTI_PLAYER_ID.to_string()),
                    UiButtonKind::Action,
                    UiButtonTone::Normal,
                ));
                panel.spawn((
                    Button {
                        text: "Settings".to_string(),
                        ..default()
                    },
                    CssID(MAIN_MENU_SETTINGS_ID.to_string()),
                    UiButtonKind::Action,
                    UiButtonTone::Normal,
                ));
                panel.spawn((
                    Button {
                        text: "Quit".to_string(),
                        ..default()
                    },
                    CssID(MAIN_MENU_QUIT_ID.to_string()),
                    UiButtonKind::Action,
                    UiButtonTone::Normal,
                ));
            });
        })
        .id();

    let _single_player_root = commands
        .spawn((
            Name::new("UI Single Player Root"),
            SinglePlayerRoot,
            Visibility::Hidden,
            Transform::default(),
            GlobalTransform::default(),
            full_screen_center_node(),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.4)),
            ZIndex(42),
        ))
        .with_children(|root| {
            root.spawn((
                Name::new("Single Player Panel"),
                Transform::default(),
                GlobalTransform::default(),
                menu_panel_node(),
                BackgroundColor(color_background().into()),
                BorderColor::all(color_background_hover()),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Paragraph {
                        text: "Single Player".to_string(),
                        ..default()
                    },
                    UiTextTone::Heading,
                ));
                let list_entity = panel
                    .spawn((
                        Name::new("Single Player World List"),
                        Div::default(),
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Px(340.0),
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(8.0),
                            padding: UiRect::all(Val::Px(8.0)),
                            overflow: Overflow::scroll_y(),
                            ..default()
                        },
                        BackgroundColor(color_single_player_list_background().into()),
                        BorderColor::all(color_background_hover()),
                        SinglePlayerWorldList,
                    ))
                    .id();

                panel.spawn((
                    Node {
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(8.0),
                        ..default()
                    },
                    BackgroundColor::DEFAULT,
                ))
                .with_children(|actions| {
                    actions.spawn((
                        Button {
                            text: "Create World".to_string(),
                            ..default()
                        },
                        CssID(SINGLE_PLAYER_CREATE_WORLD_ID.to_string()),
                        UiButtonKind::ActionRow,
                        UiButtonTone::Accent,
                    ));
                    actions.spawn((
                        Button {
                            text: "Play World".to_string(),
                            ..default()
                        },
                        CssID(SINGLE_PLAYER_PLAY_WORLD_ID.to_string()),
                        UiButtonKind::ActionRow,
                        UiButtonTone::Normal,
                    ));
                    actions.spawn((
                        Button {
                            text: "Delete World".to_string(),
                            ..default()
                        },
                        CssID(SINGLE_PLAYER_DELETE_WORLD_ID.to_string()),
                        UiButtonKind::ActionRow,
                        UiButtonTone::Normal,
                    ));
                });

                panel.spawn((
                    Name::new("Single Player Delete Dialog"),
                    SinglePlayerDeleteDialog,
                    Visibility::Hidden,
                    dialog_overlay_node(),
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
                ))
                .with_children(|dialog| {
                    dialog
                        .spawn((
                            dialog_box_node(),
                            BackgroundColor(color_background().into()),
                            BorderColor::all(color_background_hover()),
                        ))
                        .with_children(|box_node| {
                            box_node.spawn((
                                Paragraph {
                                    text: "Delete world?".to_string(),
                                    ..default()
                                },
                                CssID(SINGLE_PLAYER_DELETE_TEXT_ID.to_string()),
                                UiTextTone::Heading,
                                SinglePlayerDeleteText,
                            ));
                            box_node
                                .spawn((
                                    Node {
                                        width: Val::Percent(100.0),
                                        flex_direction: FlexDirection::Row,
                                        column_gap: Val::Px(8.0),
                                        ..default()
                                    },
                                    BackgroundColor::DEFAULT,
                                ))
                                .with_children(|buttons| {
                                    buttons.spawn((
                                        Button {
                                            text: "Delete".to_string(),
                                            ..default()
                                        },
                                        CssID(SINGLE_PLAYER_DELETE_CONFIRM_ID.to_string()),
                                        UiButtonKind::ActionRow,
                                        UiButtonTone::Accent,
                                    ));
                                    buttons.spawn((
                                        Button {
                                            text: "Cancel".to_string(),
                                            ..default()
                                        },
                                        CssID(SINGLE_PLAYER_DELETE_CANCEL_ID.to_string()),
                                        UiButtonKind::ActionRow,
                                        UiButtonTone::Normal,
                                    ));
                                });
                        });
                });

                single_player_world_list = list_entity;
            });
        })
        .id();

    let _create_world_root = commands
        .spawn((
            Name::new("UI Create World Root"),
            CreateWorldRoot,
            Visibility::Hidden,
            full_screen_center_node(),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.4)),
            ZIndex(43),
        ))
        .with_children(|root| {
            root.spawn((
                Name::new("Create World Panel"),
                menu_panel_node(),
                BackgroundColor(color_background().into()),
                BorderColor::all(color_background_hover()),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Paragraph {
                        text: "Create World".to_string(),
                        ..default()
                    },
                    UiTextTone::Heading,
                ));
                panel.spawn((
                    Paragraph {
                        text: "World Name".to_string(),
                        ..default()
                    },
                    UiTextTone::Darker,
                ));
                panel.spawn((
                    InputField {
                        label: "World Name".to_string(),
                        placeholder: "My World".to_string(),
                        input_type: InputType::Text,
                        ..default()
                    },
                    CssID(CREATE_WORLD_NAME_INPUT_ID.to_string()),
                ));
                panel.spawn((
                    Paragraph {
                        text: "Seed (optional)".to_string(),
                        ..default()
                    },
                    UiTextTone::Darker,
                ));
                panel.spawn((
                    InputField {
                        label: "Seed".to_string(),
                        placeholder: default_seed.to_string(),
                        input_type: InputType::Number,
                        ..default()
                    },
                    CssID(CREATE_WORLD_SEED_INPUT_ID.to_string()),
                ));
                panel.spawn((
                    Node {
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(8.0),
                        ..default()
                    },
                    BackgroundColor::DEFAULT,
                ))
                .with_children(|actions| {
                    actions.spawn((
                        Button {
                            text: "Create".to_string(),
                            ..default()
                        },
                        CssID(CREATE_WORLD_CREATE_ID.to_string()),
                        UiButtonKind::ActionRow,
                        UiButtonTone::Accent,
                    ));
                    actions.spawn((
                        Button {
                            text: "Abort".to_string(),
                            ..default()
                        },
                        CssID(CREATE_WORLD_ABORT_ID.to_string()),
                        UiButtonKind::ActionRow,
                        UiButtonTone::Normal,
                    ));
                });
            });
        })
        .id();

    let _multiplayer_root = commands
        .spawn((
            Name::new("UI Multiplayer Root"),
            MultiplayerRoot,
            Visibility::Hidden,
            Transform::default(),
            GlobalTransform::default(),
            full_screen_center_node(),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.42)),
            ZIndex(44),
        ))
        .with_children(|root| {
            root.spawn((
                Name::new("Multiplayer Panel"),
                Transform::default(),
                GlobalTransform::default(),
                menu_panel_node(),
                BackgroundColor(color_background().into()),
                BorderColor::all(color_background_hover()),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Paragraph {
                        text: "Multi Player".to_string(),
                        ..default()
                    },
                    UiTextTone::Heading,
                ));
                let list_entity = panel
                    .spawn((
                        Name::new("Multiplayer Server List"),
                        Div::default(),
                        MultiplayerServerList,
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Px(340.0),
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(8.0),
                            padding: UiRect::all(Val::Px(8.0)),
                            overflow: Overflow::scroll_y(),
                            ..default()
                        },
                        BackgroundColor(color_background_hover().into()),
                        BorderColor::all(color_background_hover()),
                    ))
                    .id();
                panel.spawn((
                    Node {
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(8.0),
                        flex_wrap: FlexWrap::Wrap,
                        row_gap: Val::Px(8.0),
                        ..default()
                    },
                    BackgroundColor::DEFAULT,
                ))
                .with_children(|actions| {
                    actions.spawn((
                        Button {
                            text: "Join".to_string(),
                            ..default()
                        },
                        CssID(MULTIPLAYER_JOIN_ID.to_string()),
                        UiButtonKind::ActionRow,
                        UiButtonTone::Accent,
                    ));
                    actions.spawn((
                        Button {
                            text: "Refresh".to_string(),
                            ..default()
                        },
                        CssID(MULTIPLAYER_REFRESH_ID.to_string()),
                        UiButtonKind::ActionRow,
                        UiButtonTone::Normal,
                    ));
                    actions.spawn((
                        Button {
                            text: "Add".to_string(),
                            ..default()
                        },
                        CssID(MULTIPLAYER_ADD_ID.to_string()),
                        UiButtonKind::ActionRow,
                        UiButtonTone::Normal,
                    ));
                    actions.spawn((
                        Button {
                            text: "Edit".to_string(),
                            ..default()
                        },
                        CssID(MULTIPLAYER_EDIT_ID.to_string()),
                        UiButtonKind::ActionRow,
                        UiButtonTone::Normal,
                    ));
                    actions.spawn((
                        Button {
                            text: "Delete".to_string(),
                            ..default()
                        },
                        CssID(MULTIPLAYER_DELETE_ID.to_string()),
                        UiButtonKind::ActionRow,
                        UiButtonTone::Normal,
                    ));
                });

                panel.spawn((
                    Name::new("Multiplayer Form Dialog"),
                    MultiplayerFormDialog,
                    Visibility::Hidden,
                    dialog_overlay_node(),
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.56)),
                ))
                .with_children(|dialog| {
                    dialog
                        .spawn((
                            dialog_box_node(),
                            BackgroundColor(color_background().into()),
                            BorderColor::all(color_background_hover()),
                        ))
                        .with_children(|box_node| {
                            box_node.spawn((
                                Paragraph {
                                    text: "Add Server".to_string(),
                                    ..default()
                                },
                                CssID(MULTIPLAYER_FORM_TITLE_ID.to_string()),
                                UiTextTone::Heading,
                            ));
                            box_node.spawn((
                                Paragraph {
                                    text: "Server Name".to_string(),
                                    ..default()
                                },
                                UiTextTone::Darker,
                            ));
                            box_node.spawn((
                                InputField {
                                    label: "Server Name".to_string(),
                                    placeholder: "My Server".to_string(),
                                    input_type: InputType::Text,
                                    ..default()
                                },
                                CssID(MULTIPLAYER_FORM_NAME_INPUT_ID.to_string()),
                            ));
                            box_node.spawn((
                                Paragraph {
                                    text: "Server Address".to_string(),
                                    ..default()
                                },
                                UiTextTone::Darker,
                            ));
                            box_node.spawn((
                                InputField {
                                    label: "Server Address".to_string(),
                                    placeholder: "192.168.0.10:14191".to_string(),
                                    input_type: InputType::Text,
                                    ..default()
                                },
                                CssID(MULTIPLAYER_FORM_ADDRESS_INPUT_ID.to_string()),
                            ));
                            box_node
                                .spawn((
                                    Node {
                                        width: Val::Percent(100.0),
                                        flex_direction: FlexDirection::Row,
                                        column_gap: Val::Px(8.0),
                                        ..default()
                                    },
                                    BackgroundColor::DEFAULT,
                                ))
                                .with_children(|buttons| {
                                    buttons.spawn((
                                        Button {
                                            text: "Add".to_string(),
                                            ..default()
                                        },
                                        CssID(MULTIPLAYER_FORM_ADD_ID.to_string()),
                                        MultiplayerFormAddButton,
                                        UiButtonKind::ActionRow,
                                        UiButtonTone::Accent,
                                    ));
                                    buttons.spawn((
                                        Button {
                                            text: "Edit".to_string(),
                                            ..default()
                                        },
                                        CssID(MULTIPLAYER_FORM_EDIT_ID.to_string()),
                                        MultiplayerFormEditButton,
                                        UiButtonKind::ActionRow,
                                        UiButtonTone::Accent,
                                    ));
                                    buttons.spawn((
                                        Button {
                                            text: "Abort".to_string(),
                                            ..default()
                                        },
                                        CssID(MULTIPLAYER_FORM_ABORT_ID.to_string()),
                                        UiButtonKind::ActionRow,
                                        UiButtonTone::Normal,
                                    ));
                                });
                        });
                });

                panel.spawn((
                    Name::new("Multiplayer Delete Dialog"),
                    MultiplayerDeleteDialog,
                    Visibility::Hidden,
                    dialog_overlay_node(),
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.56)),
                ))
                .with_children(|dialog| {
                    dialog
                        .spawn((
                            dialog_box_node(),
                            BackgroundColor(color_background().into()),
                            BorderColor::all(color_background_hover()),
                        ))
                        .with_children(|box_node| {
                            box_node.spawn((
                                Paragraph {
                                    text: "Delete server?".to_string(),
                                    ..default()
                                },
                                CssID(MULTIPLAYER_DELETE_TEXT_ID.to_string()),
                                UiTextTone::Heading,
                                MultiplayerDeleteText,
                            ));
                            box_node
                                .spawn((
                                    Node {
                                        width: Val::Percent(100.0),
                                        flex_direction: FlexDirection::Row,
                                        column_gap: Val::Px(8.0),
                                        ..default()
                                    },
                                    BackgroundColor::DEFAULT,
                                ))
                                .with_children(|buttons| {
                                    buttons.spawn((
                                        Button {
                                            text: "Confirm".to_string(),
                                            ..default()
                                        },
                                        CssID(MULTIPLAYER_DELETE_CONFIRM_ID.to_string()),
                                        UiButtonKind::ActionRow,
                                        UiButtonTone::Accent,
                                    ));
                                    buttons.spawn((
                                        Button {
                                            text: "Abort".to_string(),
                                            ..default()
                                        },
                                        CssID(MULTIPLAYER_DELETE_ABORT_ID.to_string()),
                                        UiButtonKind::ActionRow,
                                        UiButtonTone::Normal,
                                    ));
                                });
                        });
                });

                panel.spawn((
                    Name::new("Multiplayer Connect Dialog"),
                    MultiplayerConnectDialog,
                    Visibility::Hidden,
                    dialog_overlay_node(),
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.56)),
                ))
                .with_children(|dialog| {
                    dialog
                        .spawn((
                            dialog_box_node(),
                            BackgroundColor(color_background().into()),
                            BorderColor::all(color_background_hover()),
                        ))
                        .with_children(|box_node| {
                            box_node.spawn((
                                Paragraph {
                                    text: "Connecting to server ...".to_string(),
                                    ..default()
                                },
                                CssID(MULTIPLAYER_CONNECT_TEXT_ID.to_string()),
                                UiTextTone::Heading,
                            ));
                            box_node
                                .spawn((
                                    Node {
                                        width: Val::Percent(100.0),
                                        flex_direction: FlexDirection::Row,
                                        justify_content: JustifyContent::Center,
                                        margin: UiRect::top(Val::Px(8.0)),
                                        ..default()
                                    },
                                    BackgroundColor::DEFAULT,
                                ))
                                .with_children(|actions| {
                                    actions.spawn((
                                        Button {
                                            text: "OK".to_string(),
                                            ..default()
                                        },
                                        CssID(MULTIPLAYER_CONNECT_OK_ID.to_string()),
                                        MultiplayerConnectOkButton,
                                        Visibility::Hidden,
                                        UiButtonKind::ActionRow,
                                        UiButtonTone::Accent,
                                    ));
                                });
                        });
                });

                multiplayer_server_list = list_entity;
            });
        })
        .id();

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
                        text: "Generating World ...".to_string(),
                        ..default()
                    },
                    UiTextTone::Heading,
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
                    text: "Leaving world ...".to_string(),
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
                        text: "Pause Menu".to_string(),
                        ..default()
                    },
                    UiTextTone::Heading,
                ));
                panel.spawn((
                    Button {
                        text: "Back to Game".to_string(),
                        ..default()
                    },
                    CssID(PAUSE_PLAY_ID.to_string()),
                    UiButtonKind::Action,
                    UiButtonTone::Accent,
                ));
                panel.spawn((
                    Button {
                        text: "Open to LAN".to_string(),
                        ..default()
                    },
                    CssID(PAUSE_CONNECT_ID.to_string()),
                    UiButtonKind::Action,
                    UiButtonTone::Normal,
                ));
                panel.spawn((
                    Button {
                        text: "Settings".to_string(),
                        ..default()
                    },
                    CssID(PAUSE_SETTINGS_ID.to_string()),
                    UiButtonKind::Action,
                    UiButtonTone::Normal,
                ));
                panel.spawn((
                    Button {
                        text: "Main Menu".to_string(),
                        ..default()
                    },
                    CssID(PAUSE_CLOSE_ID.to_string()),
                    UiButtonKind::Action,
                    UiButtonTone::Normal,
                ));
            });
        })
        .id();

    let _inventory_root = commands
        .spawn((
            Name::new("UI Inventory Root"),
            PlayerInventoryRoot,
            Visibility::Hidden,
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
                            text: "Inventory".to_string(),
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
                                text: "Hand Crafted".to_string(),
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
                            text: "Items: 0".to_string(),
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
                            text: "Items".to_string(),
                            ..default()
                        },
                        UiTextTone::Heading,
                    ));
                    right.spawn((
                        Paragraph {
                            text: "Registered: 0".to_string(),
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
                                text: "Rezept-Info".to_string(),
                                ..default()
                            },
                            UiTextTone::CardName,
                        ));
                        recipes.spawn((
                            Paragraph {
                                text: "Nutze oben 2 Slots -> Ergebnis rechts. Klick auf das Ergebnis craftet."
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
                    CssID(INVENTORY_TOOLTIP_NAME_ID.to_string()),
                    UiTextTone::TooltipName,
                    Pickable::IGNORE,
                ));
                tooltip.spawn((
                    Paragraph {
                        text: String::new(),
                        ..default()
                    },
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
                            width: Val::Px(300.0),
                            min_width: Val::Px(300.0),
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
                                text: "Recipe".to_string(),
                                ..default()
                            },
                            CssID(RECIPE_PREVIEW_TITLE_ID.to_string()),
                            UiTextTone::Heading,
                        ));
                        panel.spawn((
                            Paragraph {
                                text: "Hand Crafted".to_string(),
                                ..default()
                            },
                            UiTextTone::Darker,
                        ));
                        panel.spawn((
                            Visibility::Inherited,
                            Node {
                                width: Val::Percent(100.0),
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                column_gap: Val::Px(8.0),
                                padding: UiRect::all(Val::Px(8.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BackgroundColor(color_single_player_list_background().into()),
                            BorderColor::all(color_background_hover()),
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
                        panel.spawn((
                            Paragraph {
                                text: "Nur Vorschau: Items sind hier nicht entnehmbar.".to_string(),
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
                    width: Val::Px(420.0),
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
                for id in [
                    ID_BUILD,
                    ID_CPU_NAME,
                    ID_GPU_NAME,
                    ID_VRAM,
                    ID_BIOME,
                    ID_GLOBAL_CPU,
                    ID_APP_CPU,
                    ID_APP_MEM,
                    ID_PLAYER_POS,
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

    commands.insert_resource(UiEntities {
        single_player_world_list,
        multiplayer_server_list,
    });
}
