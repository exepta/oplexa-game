/// Runs the `show_main_menu` routine for show main menu in the `graphic::components::main_menu` module.
fn show_main_menu(mut visibility: Query<&mut Visibility, With<MainMenuRoot>>) {
    if let Ok(mut visible) = visibility.single_mut() {
        *visible = Visibility::Inherited;
    }
}

/// Runs the `hide_main_menu` routine for hide main menu in the `graphic::components::main_menu` module.
fn hide_main_menu(
    mut visibility: Query<&mut Visibility, With<MainMenuRoot>>,
    mut ui_interaction: ResMut<UiInteractionState>,
) {
    if let Ok(mut visible) = visibility.single_mut() {
        *visible = Visibility::Hidden;
    }
    ui_interaction.menu_open = false;
}

/// Sets menu cursor for the `graphic::components::main_menu` module.
fn set_menu_cursor(
    mut ui_interaction: ResMut<UiInteractionState>,
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    ui_interaction.menu_open = true;
    if let Ok(mut cursor) = cursor_q.single_mut() {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
}

/// Handles main menu buttons for the `graphic::components::main_menu` module.
fn handle_main_menu_buttons(
    mut widgets: Query<(&CssID, &mut UIWidgetState), With<Button>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let Some(action) = consume_main_menu_action(&mut widgets) else {
        return;
    };

    match action {
        MainMenuAction::SinglePlayer => {
            next_state.set(AppState::Screen(BeforeUiState::SinglePlayer));
        }
        MainMenuAction::MultiPlayer => {
            next_state.set(AppState::Screen(BeforeUiState::MultiPlayer));
        }
        MainMenuAction::Settings => info!("Settings clicked (not implemented yet)."),
        MainMenuAction::QuitGame => info!("Quit clicked (not implemented yet)."),
    }
}

/// Runs the `consume_main_menu_action` routine for consume main menu action in the `graphic::components::main_menu` module.
fn consume_main_menu_action(
    widgets: &mut Query<(&CssID, &mut UIWidgetState), With<Button>>,
) -> Option<MainMenuAction> {
    widgets.iter_mut().find_map(|(css_id, mut state)| {
        if !state.checked {
            return None;
        }

        state.checked = false;
        match css_id.0.as_str() {
            MAIN_MENU_SINGLE_PLAYER_ID => Some(MainMenuAction::SinglePlayer),
            MAIN_MENU_MULTI_PLAYER_ID => Some(MainMenuAction::MultiPlayer),
            MAIN_MENU_SETTINGS_ID => Some(MainMenuAction::Settings),
            MAIN_MENU_QUIT_ID => Some(MainMenuAction::QuitGame),
            _ => None,
        }
    })
}
