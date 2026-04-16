/// Spawns hardcoded ui for the `graphic::components::spawn` module.
fn spawn_hardcoded_ui(
    mut commands: Commands,
    world_gen_config: Option<Res<WorldGenConfig>>,
    language: Res<ClientLanguageState>,
) {
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
                        text: language.localize_name_key("KEY_UI_SINGLE_PLAYER").to_string(),
                        ..default()
                    },
                    CssID(MAIN_MENU_SINGLE_PLAYER_ID.to_string()),
                    UiButtonKind::Action,
                    UiButtonTone::Accent,
                ));
                panel.spawn((
                    Button {
                        text: language.localize_name_key("KEY_UI_MULTI_PLAYER").to_string(),
                        ..default()
                    },
                    CssID(MAIN_MENU_MULTI_PLAYER_ID.to_string()),
                    UiButtonKind::Action,
                    UiButtonTone::Normal,
                ));
                panel.spawn((
                    Button {
                        text: language.localize_name_key("KEY_UI_SETTINGS").to_string(),
                        ..default()
                    },
                    CssID(MAIN_MENU_SETTINGS_ID.to_string()),
                    UiButtonKind::Action,
                    UiButtonTone::Normal,
                ));
                panel.spawn((
                    Button {
                        text: language.localize_name_key("KEY_UI_QUIT").to_string(),
                        ..default()
                    },
                    CssID(MAIN_MENU_QUIT_ID.to_string()),
                    UiButtonKind::Action,
                    UiButtonTone::Normal,
                    ));
                });

            root.spawn((
                Name::new("Benchmark Menu Dialog"),
                BenchmarkMenuDialogRoot,
                Visibility::Hidden,
                dialog_overlay_node(),
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
                ZIndex(70),
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
                                text: language.localize_name_key("KEY_UI_BENCHMARK_START_QUESTION"),
                                ..default()
                            },
                            CssID(BENCHMARK_DIALOG_TEXT_ID.to_string()),
                            UiTextTone::Heading,
                            BenchmarkMenuDialogText,
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
                                        text: language.localize_name_key("KEY_UI_BENCHMARK_START"),
                                        ..default()
                                    },
                                    CssID(BENCHMARK_DIALOG_START_ID.to_string()),
                                    UiButtonKind::ActionRow,
                                    UiButtonTone::Accent,
                                ));
                                buttons.spawn((
                                    Button {
                                        text: language.localize_name_key("KEY_UI_ABORT"),
                                        ..default()
                                    },
                                    CssID(BENCHMARK_DIALOG_ABORT_ID.to_string()),
                                    UiButtonKind::ActionRow,
                                    UiButtonTone::Normal,
                                ));
                            });
                    });
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
                        text: language.localize_name_key("KEY_UI_SINGLE_PLAYER").to_string(),
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
                            text: language.localize_name_key("KEY_UI_CREATE_WORLD").to_string(),
                            ..default()
                        },
                        CssID(SINGLE_PLAYER_CREATE_WORLD_ID.to_string()),
                        UiButtonKind::ActionRow,
                        UiButtonTone::Accent,
                    ));
                    actions.spawn((
                        Button {
                            text: language.localize_name_key("KEY_UI_PLAY_WORLD").to_string(),
                            ..default()
                        },
                        CssID(SINGLE_PLAYER_PLAY_WORLD_ID.to_string()),
                        UiButtonKind::ActionRow,
                        UiButtonTone::Normal,
                    ));
                    actions.spawn((
                        Button {
                            text: language.localize_name_key("KEY_UI_DELETE_WORLD").to_string(),
                            ..default()
                        },
                        CssID(SINGLE_PLAYER_DELETE_WORLD_ID.to_string()),
                        UiButtonKind::ActionRow,
                        UiButtonTone::Normal,
                    ));
                    actions.spawn((
                        Button {
                            text: language.localize_name_key("KEY_UI_BACK").to_string(),
                            ..default()
                        },
                        CssID(SINGLE_PLAYER_BACK_ID.to_string()),
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
                                    text: language
                                        .localize_name_key("KEY_UI_DELETE_WORLD_QUESTION")
                                        .to_string(),
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
                                            text: language.localize_name_key("KEY_UI_DELETE").to_string(),
                                            ..default()
                                        },
                                        CssID(SINGLE_PLAYER_DELETE_CONFIRM_ID.to_string()),
                                        UiButtonKind::ActionRow,
                                        UiButtonTone::Accent,
                                    ));
                                    buttons.spawn((
                                        Button {
                                            text: language.localize_name_key("KEY_UI_CANCEL").to_string(),
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
                        text: language.localize_name_key("KEY_UI_CREATE_WORLD").to_string(),
                        ..default()
                    },
                    UiTextTone::Heading,
                ));
                panel.spawn((
                    Paragraph {
                        text: language.localize_name_key("KEY_UI_WORLD_NAME").to_string(),
                        ..default()
                    },
                    UiTextTone::Darker,
                ));
                panel.spawn((
                    InputField {
                        label: String::new(),
                        placeholder: language
                            .localize_name_key("KEY_UI_WORLD_NAME_PLACEHOLDER")
                            .to_string(),
                        input_type: InputType::Text,
                        ..default()
                    },
                    CssID(CREATE_WORLD_NAME_INPUT_ID.to_string()),
                ));
                panel.spawn((
                    Paragraph {
                        text: language.localize_name_key("KEY_UI_SEED_OPTIONAL").to_string(),
                        ..default()
                    },
                    UiTextTone::Darker,
                ));
                panel.spawn((
                    InputField {
                        label: String::new(),
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
                            text: language.localize_name_key("KEY_UI_CREATE").to_string(),
                            ..default()
                        },
                        CssID(CREATE_WORLD_CREATE_ID.to_string()),
                        UiButtonKind::ActionRow,
                        UiButtonTone::Accent,
                    ));
                    actions.spawn((
                        Button {
                            text: language.localize_name_key("KEY_UI_ABORT").to_string(),
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
                        text: language.localize_name_key("KEY_UI_MULTI_PLAYER").to_string(),
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
                            text: language.localize_name_key("KEY_UI_JOIN").to_string(),
                            ..default()
                        },
                        CssID(MULTIPLAYER_JOIN_ID.to_string()),
                        UiButtonKind::ActionRow,
                        UiButtonTone::Accent,
                    ));
                    actions.spawn((
                        Button {
                            text: language.localize_name_key("KEY_UI_REFRESH").to_string(),
                            ..default()
                        },
                        CssID(MULTIPLAYER_REFRESH_ID.to_string()),
                        UiButtonKind::ActionRow,
                        UiButtonTone::Normal,
                    ));
                    actions.spawn((
                        Button {
                            text: language.localize_name_key("KEY_UI_ADD").to_string(),
                            ..default()
                        },
                        CssID(MULTIPLAYER_ADD_ID.to_string()),
                        UiButtonKind::ActionRow,
                        UiButtonTone::Normal,
                    ));
                    actions.spawn((
                        Button {
                            text: language.localize_name_key("KEY_UI_EDIT").to_string(),
                            ..default()
                        },
                        CssID(MULTIPLAYER_EDIT_ID.to_string()),
                        UiButtonKind::ActionRow,
                        UiButtonTone::Normal,
                    ));
                    actions.spawn((
                        Button {
                            text: language.localize_name_key("KEY_UI_DELETE").to_string(),
                            ..default()
                        },
                        CssID(MULTIPLAYER_DELETE_ID.to_string()),
                        UiButtonKind::ActionRow,
                        UiButtonTone::Normal,
                    ));
                    actions.spawn((
                        Button {
                            text: language.localize_name_key("KEY_UI_BACK").to_string(),
                            ..default()
                        },
                        CssID(MULTIPLAYER_BACK_ID.to_string()),
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
                                    text: language.localize_name_key("KEY_UI_ADD_SERVER").to_string(),
                                    ..default()
                                },
                                CssID(MULTIPLAYER_FORM_TITLE_ID.to_string()),
                                UiTextTone::Heading,
                            ));
                            box_node.spawn((
                                Paragraph {
                                    text: language.localize_name_key("KEY_UI_SERVER_NAME").to_string(),
                                    ..default()
                                },
                                UiTextTone::Darker,
                            ));
                            box_node.spawn((
                                InputField {
                                    label: String::new(),
                                    placeholder: language
                                        .localize_name_key("KEY_UI_SERVER_NAME_PLACEHOLDER")
                                        .to_string(),
                                    input_type: InputType::Text,
                                    ..default()
                                },
                                CssID(MULTIPLAYER_FORM_NAME_INPUT_ID.to_string()),
                            ));
                            box_node.spawn((
                                Paragraph {
                                    text: language.localize_name_key("KEY_UI_SERVER_ADDRESS").to_string(),
                                    ..default()
                                },
                                UiTextTone::Darker,
                            ));
                            box_node.spawn((
                                InputField {
                                    label: String::new(),
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
                                            text: language.localize_name_key("KEY_UI_ADD").to_string(),
                                            ..default()
                                        },
                                        CssID(MULTIPLAYER_FORM_ADD_ID.to_string()),
                                        MultiplayerFormAddButton,
                                        UiButtonKind::ActionRow,
                                        UiButtonTone::Accent,
                                    ));
                                    buttons.spawn((
                                        Button {
                                            text: language.localize_name_key("KEY_UI_EDIT").to_string(),
                                            ..default()
                                        },
                                        CssID(MULTIPLAYER_FORM_EDIT_ID.to_string()),
                                        MultiplayerFormEditButton,
                                        UiButtonKind::ActionRow,
                                        UiButtonTone::Accent,
                                    ));
                                    buttons.spawn((
                                        Button {
                                            text: language.localize_name_key("KEY_UI_ABORT").to_string(),
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
                                    text: language
                                        .localize_name_key("KEY_UI_DELETE_SERVER_QUESTION")
                                        .to_string(),
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
                                            text: language.localize_name_key("KEY_UI_CONFIRM").to_string(),
                                            ..default()
                                        },
                                        CssID(MULTIPLAYER_DELETE_CONFIRM_ID.to_string()),
                                        UiButtonKind::ActionRow,
                                        UiButtonTone::Accent,
                                    ));
                                    buttons.spawn((
                                        Button {
                                            text: language.localize_name_key("KEY_UI_ABORT").to_string(),
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
                                    text: language
                                        .localize_name_key("KEY_UI_CONNECTING_TO_SERVER")
                                        .to_string(),
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
                                            text: language.localize_name_key("KEY_UI_OK").to_string(),
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

    // Extracted large panel/timer/debug sections for readability.
    spawn_hardcoded_ui_panels::spawn_hardcoded_ui_panels(&mut commands, language.as_ref());
    commands.insert_resource(UiEntities {
        single_player_world_list,
        multiplayer_server_list,
    });
}
