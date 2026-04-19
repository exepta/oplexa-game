/// Runs the `toggle_pause_menu` routine for toggle pause menu in the `graphic::components::pause_menu` module.
fn toggle_pause_menu(
    keyboard: Res<ButtonInput<KeyCode>>,
    global_config: Res<GlobalConfig>,
    mut ui_interaction: ResMut<UiInteractionState>,
    mut pause_menu: ResMut<PauseMenuState>,
    mut root: Query<&mut Visibility, With<PauseMenuRoot>>,
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let menu_key = convert(global_config.input.ui_menu.as_str()).unwrap_or(KeyCode::Enter);
    let close_key =
        convert(global_config.input.ui_close_back.as_str()).expect("Invalid close/back key");
    let toggle_requested = keyboard.just_pressed(menu_key);
    let close_requested = pause_menu.open && keyboard.just_pressed(close_key);
    if !toggle_requested && !close_requested {
        return;
    }

    // Do not open pause menu while another in-game UI (e.g. inventory) is open.
    if toggle_requested && !pause_menu.open && ui_interaction.blocks_game_input() {
        return;
    }

    if close_requested {
        pause_menu.open = false;
    } else if toggle_requested {
        pause_menu.open = !pause_menu.open;
    }

    ui_interaction.menu_open = pause_menu.open;
    set_pause_menu_cursor(pause_menu.open, &mut cursor_q);

    if let Ok(mut visible) = root.single_mut() {
        *visible = if pause_menu.open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

/// Runs the `enforce_pause_menu_visibility` routine for enforce pause menu visibility in the `graphic::components::pause_menu` module.
fn enforce_pause_menu_visibility(
    pause_menu: Res<PauseMenuState>,
    mut ui_interaction: ResMut<UiInteractionState>,
    mut root: Query<&mut Visibility, With<PauseMenuRoot>>,
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if !pause_menu.open {
        return;
    }

    ui_interaction.menu_open = true;
    if let Ok(mut visible) = root.single_mut() {
        *visible = Visibility::Inherited;
    }
    set_pause_menu_cursor(true, &mut cursor_q);
}

/// Synchronizes pause menu labels for the `graphic::components::pause_menu` module.
fn sync_pause_menu_labels(
    multiplayer_connection: Res<MultiplayerConnectionState>,
    integrated_server: Res<crate::integrated_server::IntegratedServerSession>,
    language: Res<ClientLanguageState>,
    mut buttons: Query<(&CssID, &mut Button)>,
) {
    for (css_id, mut button) in &mut buttons {
        if css_id.0 == PAUSE_CLOSE_ID {
            let target = if multiplayer_connection.connected && !integrated_server.is_active() {
                language.localize_name_key("KEY_UI_DISCONNECT")
            } else {
                language.localize_name_key("KEY_UI_MAIN_MENU")
            };
            if button.text != target {
                button.text = target;
            }
        }
    }
}

/// Handles pause menu buttons for the `graphic::components::pause_menu` module.
#[allow(clippy::too_many_arguments)]
fn handle_pause_menu_buttons(
    multiplayer_connection: Res<MultiplayerConnectionState>,
    integrated_server: Res<crate::integrated_server::IntegratedServerSession>,
    mut ui_interaction: ResMut<UiInteractionState>,
    mut pause_menu: ResMut<PauseMenuState>,
    mut widgets: Query<(&CssID, &mut UIWidgetState), With<Button>>,
    mut roots: ParamSet<(
        Query<&mut Visibility, With<PauseMenuRoot>>,
        Query<&mut Visibility, With<WorldUnloadRoot>>,
    )>,
    mut world_unload_state: ResMut<WorldUnloadUiState>,
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut next_state: ResMut<NextState<AppState>>,
    mut disconnect_writer: MessageWriter<DisconnectFromServerRequest>,
) {
    if !pause_menu.open {
        return;
    }

    let Some(action) = consume_pause_menu_action(&mut widgets) else {
        return;
    };

    match action {
        PauseMenuAction::BackToGame => {
            pause_menu.open = false;
            ui_interaction.menu_open = false;
            if let Ok(mut visible) = roots.p0().single_mut() {
                *visible = Visibility::Hidden;
            }
            set_pause_menu_cursor(false, &mut cursor_q);
        }
        PauseMenuAction::Settings => {
            info!("Settings button clicked (not implemented yet).");
        }
        PauseMenuAction::ExitToMenu => {
            if multiplayer_connection.connected || integrated_server.is_active() {
                disconnect_writer.write(DisconnectFromServerRequest);
            }
            pause_menu.open = false;
            ui_interaction.menu_open = false;
            if let Ok(mut visible) = roots.p0().single_mut() {
                *visible = Visibility::Hidden;
            }
            set_pause_menu_cursor(true, &mut cursor_q);
            world_unload_state.active = true;
            world_unload_state.timer.reset();
            if let Ok(mut visible) = roots.p1().single_mut() {
                *visible = Visibility::Inherited;
            }
            next_state.set(AppState::Screen(BeforeUiState::Menu));
        }
    }
}

/// Synchronizes pause time for the `graphic::components::pause_menu` module.
fn sync_pause_time(
    pause_menu: Res<PauseMenuState>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
    mut virtual_time: ResMut<Time<Virtual>>,
) {
    let should_pause = pause_menu.open && !multiplayer_connection.connected;

    if should_pause && !virtual_time.is_paused() {
        virtual_time.pause();
        return;
    }

    if !should_pause && virtual_time.is_paused() {
        virtual_time.unpause();
    }
}

/// Runs the `close_pause_menu` routine for close pause menu in the `graphic::components::pause_menu` module.
fn close_pause_menu(
    mut ui_interaction: ResMut<UiInteractionState>,
    mut pause_menu: ResMut<PauseMenuState>,
    mut root: Query<&mut Visibility, With<PauseMenuRoot>>,
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if !pause_menu.open {
        return;
    }

    pause_menu.open = false;
    ui_interaction.menu_open = false;
    if let Ok(mut visible) = root.single_mut() {
        *visible = Visibility::Hidden;
    }
    set_pause_menu_cursor(false, &mut cursor_q);
}

/// Sets pause menu cursor for the `graphic::components::pause_menu` module.
fn set_pause_menu_cursor(
    menu_open: bool,
    cursor_q: &mut Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let Ok(mut cursor) = cursor_q.single_mut() else {
        return;
    };

    if menu_open {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    } else {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

/// Runs the `consume_pause_menu_action` routine for consume pause menu action in the `graphic::components::pause_menu` module.
fn consume_pause_menu_action(
    widgets: &mut Query<(&CssID, &mut UIWidgetState), With<Button>>,
) -> Option<PauseMenuAction> {
    widgets.iter_mut().find_map(|(css_id, mut state)| {
        if !state.checked {
            return None;
        }

        state.checked = false;

        match css_id.0.as_str() {
            PAUSE_PLAY_ID => Some(PauseMenuAction::BackToGame),
            PAUSE_SETTINGS_ID => Some(PauseMenuAction::Settings),
            PAUSE_CLOSE_ID => Some(PauseMenuAction::ExitToMenu),
            _ => None,
        }
    })
}
