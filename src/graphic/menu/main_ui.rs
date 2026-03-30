use crate::core::states::states::{AppState, BeforeUiState};
use crate::core::ui::UiInteractionState;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_extended_ui::html::HtmlSource;
use bevy_extended_ui::io::HtmlAsset;
use bevy_extended_ui::registry::UiRegistry;
use bevy_extended_ui::styles::CssID;
use bevy_extended_ui::widgets::UIWidgetState;

const MAIN_MENU_UI_KEY: &str = "main-menu";
const MAIN_MENU_UI_PATH: &str = "ui/html/main_menu.html";
const MAIN_MENU_SINGLE_PLAYER_ID: &str = "main-menu-single-player";
const MAIN_MENU_MULTI_PLAYER_ID: &str = "main-menu-multi-player";
const MAIN_MENU_SETTINGS_ID: &str = "main-menu-settings";
const MAIN_MENU_QUIT_ID: &str = "main-menu-quit";

pub struct MainMenuPlugin;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MainMenuAction {
    SinglePlayer,
    MultiPlayer,
    Settings,
    QuitGame,
}

impl Plugin for MainMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_main_menu_ui)
            .add_systems(
                OnEnter(AppState::Screen(BeforeUiState::Menu)),
                (show_main_menu_ui, set_main_menu_interaction),
            )
            .add_systems(
                Update,
                (set_main_menu_interaction, handle_main_menu_buttons)
                    .chain()
                    .run_if(in_state(AppState::Screen(BeforeUiState::Menu))),
            )
            .add_systems(
                OnExit(AppState::Screen(BeforeUiState::Menu)),
                (hide_main_menu_ui, clear_main_menu_interaction),
            );
    }
}

fn register_main_menu_ui(mut registry: ResMut<UiRegistry>, asset_server: Res<AssetServer>) {
    if registry.get(MAIN_MENU_UI_KEY).is_some() {
        return;
    }

    let handle: Handle<HtmlAsset> = asset_server.load(MAIN_MENU_UI_PATH);
    registry.add(
        MAIN_MENU_UI_KEY.to_string(),
        HtmlSource::from_handle(handle),
    );
}

fn show_main_menu_ui(mut registry: ResMut<UiRegistry>, asset_server: Res<AssetServer>) {
    if registry.get(MAIN_MENU_UI_KEY).is_none() {
        let handle: Handle<HtmlAsset> = asset_server.load(MAIN_MENU_UI_PATH);
        registry.add(
            MAIN_MENU_UI_KEY.to_string(),
            HtmlSource::from_handle(handle),
        );
    }

    activate_main_menu_ui(&mut registry);
}

fn set_main_menu_interaction(
    mut ui_interaction: ResMut<UiInteractionState>,
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    ui_interaction.menu_open = true;
    if let Ok(mut cursor) = cursor_q.single_mut() {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
}

fn handle_main_menu_buttons(
    mut widgets: Query<(&CssID, &mut UIWidgetState)>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let Some(action) = consume_main_menu_action(&mut widgets) else {
        return;
    };

    match action {
        MainMenuAction::SinglePlayer => {
            next_state.set(AppState::Screen(BeforeUiState::SinglePlayer));
        }
        MainMenuAction::MultiPlayer => info!("Multi Player clicked (not implemented yet)."),
        MainMenuAction::Settings => info!("Settings clicked (not implemented yet)."),
        MainMenuAction::QuitGame => info!("Quit Game clicked (not implemented yet)."),
    }
}

fn hide_main_menu_ui(mut registry: ResMut<UiRegistry>) {
    let mut clear_current = false;

    if let Some(current) = registry.current.as_mut() {
        current.retain(|name| name != MAIN_MENU_UI_KEY);
        clear_current = current.is_empty();
        registry.ui_update = true;
    }

    if clear_current {
        registry.current = None;
    }
}

fn clear_main_menu_interaction(mut ui_interaction: ResMut<UiInteractionState>) {
    ui_interaction.menu_open = false;
}

fn activate_main_menu_ui(registry: &mut UiRegistry) {
    if registry.get(MAIN_MENU_UI_KEY).is_none() {
        return;
    }

    if let Some(current) = registry.current.as_mut() {
        if current.iter().any(|name| name == MAIN_MENU_UI_KEY) {
            return;
        }
        current.push(MAIN_MENU_UI_KEY.to_string());
        registry.ui_update = true;
        return;
    }

    registry.current = Some(vec![MAIN_MENU_UI_KEY.to_string()]);
    registry.ui_update = true;
}

fn consume_main_menu_action(
    widgets: &mut Query<(&CssID, &mut UIWidgetState)>,
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
