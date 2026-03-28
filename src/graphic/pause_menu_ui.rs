use crate::core::config::GlobalConfig;
use crate::core::events::ui_events::ConnectToServerRequest;
use crate::core::multiplayer::MultiplayerConnectionState;
use crate::core::states::states::{AppState, InGameStates, is_state_in_game};
use crate::core::ui::UiInteractionState;
use crate::utils::key_utils::convert;
use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_extended_ui::html::HtmlSource;
use bevy_extended_ui::io::HtmlAsset;
use bevy_extended_ui::registry::UiRegistry;
use bevy_extended_ui::styles::CssID;
use bevy_extended_ui::widgets::UIWidgetState;

const PAUSE_MENU_UI_KEY: &str = "pause-menu";
const PAUSE_MENU_UI_PATH: &str = "ui/html/pause_menu.html";
const PAUSE_PLAY_ID: &str = "pause-menu-play";
const PAUSE_CONNECT_ID: &str = "pause-menu-connect";
const PAUSE_SETTINGS_ID: &str = "pause-menu-settings";
const PAUSE_CLOSE_ID: &str = "pause-menu-close";

pub struct PauseMenuUiPlugin;

#[derive(Resource, Debug, Default, Clone, Copy)]
struct PauseMenuState {
    open: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PauseMenuAction {
    Play,
    ConnectToServer,
    Settings,
    GameClose,
}

impl Plugin for PauseMenuUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PauseMenuState>()
            .add_systems(Startup, register_pause_menu_ui)
            .add_systems(
                Update,
                (
                    toggle_pause_menu,
                    enforce_pause_menu_visibility,
                    handle_pause_menu_buttons,
                    sync_pause_time,
                )
                    .chain()
                    .run_if(is_state_in_game),
            )
            .add_systems(
                OnExit(AppState::InGame(InGameStates::Game)),
                close_pause_menu,
            );
    }
}

fn register_pause_menu_ui(mut registry: ResMut<UiRegistry>, asset_server: Res<AssetServer>) {
    if registry.get(PAUSE_MENU_UI_KEY).is_some() {
        return;
    }

    let handle: Handle<HtmlAsset> = asset_server.load(PAUSE_MENU_UI_PATH);
    registry.add(
        PAUSE_MENU_UI_KEY.to_string(),
        HtmlSource::from_handle(handle),
    );
}

fn toggle_pause_menu(
    keyboard: Res<ButtonInput<KeyCode>>,
    global_config: Res<GlobalConfig>,
    asset_server: Res<AssetServer>,
    mut ui_interaction: ResMut<UiInteractionState>,
    mut pause_menu: ResMut<PauseMenuState>,
    mut registry: ResMut<UiRegistry>,
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let menu_key = convert(global_config.input.ui_menu.as_str()).unwrap_or(KeyCode::Enter);
    let close_key =
        convert(global_config.input.ui_close_back.as_str()).expect("Invalid close/back key");
    let toggle_requested = keyboard.just_pressed(menu_key)
        || keyboard.just_pressed(KeyCode::Enter)
        || keyboard.just_pressed(KeyCode::NumpadEnter);
    let close_requested = pause_menu.open && keyboard.just_pressed(close_key);
    if !toggle_requested && !close_requested {
        return;
    }

    if close_requested {
        pause_menu.open = false;
    } else if toggle_requested {
        pause_menu.open = !pause_menu.open;
    }

    ui_interaction.menu_open = pause_menu.open;
    set_pause_menu_cursor(pause_menu.open, &mut cursor_q);
    if pause_menu.open {
        show_pause_menu_ui(&mut registry, &asset_server);
    } else {
        hide_pause_menu_ui(&mut registry);
    }
}

fn enforce_pause_menu_visibility(
    asset_server: Res<AssetServer>,
    pause_menu: Res<PauseMenuState>,
    mut ui_interaction: ResMut<UiInteractionState>,
    mut registry: ResMut<UiRegistry>,
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if !pause_menu.open {
        return;
    }

    ui_interaction.menu_open = true;
    if !pause_menu_ui_in_stack(&registry) {
        show_pause_menu_ui(&mut registry, &asset_server);
    }
    set_pause_menu_cursor(true, &mut cursor_q);
}

fn handle_pause_menu_buttons(
    mut ui_interaction: ResMut<UiInteractionState>,
    mut pause_menu: ResMut<PauseMenuState>,
    mut widgets: Query<(&CssID, &mut UIWidgetState)>,
    mut registry: ResMut<UiRegistry>,
    asset_server: Res<AssetServer>,
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut connect_writer: MessageWriter<ConnectToServerRequest>,
    mut app_exit_writer: MessageWriter<AppExit>,
) {
    if !pause_menu.open {
        return;
    }

    let Some(action) = consume_pause_menu_action(&mut widgets) else {
        return;
    };

    match action {
        PauseMenuAction::Play => {
            pause_menu.open = false;
            ui_interaction.menu_open = false;
            hide_pause_menu_ui(&mut registry);
            set_pause_menu_cursor(false, &mut cursor_q);
        }
        PauseMenuAction::ConnectToServer => {
            connect_writer.write(ConnectToServerRequest);
            pause_menu.open = false;
            ui_interaction.menu_open = false;
            hide_pause_menu_ui(&mut registry);
            set_pause_menu_cursor(false, &mut cursor_q);
        }
        PauseMenuAction::Settings => {
            info!("Settings button clicked (not implemented yet).");
            show_pause_menu_ui(&mut registry, &asset_server);
        }
        PauseMenuAction::GameClose => {
            app_exit_writer.write(AppExit::Success);
        }
    }
}

fn sync_pause_time(
    pause_menu: Res<PauseMenuState>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
    widget_visibility: Query<(&CssID, &Visibility)>,
    mut virtual_time: ResMut<Time<Virtual>>,
) {
    let should_pause = pause_menu.open
        && !multiplayer_connection.connected
        && pause_menu_widgets_visible(&widget_visibility);

    if should_pause && !virtual_time.is_paused() {
        virtual_time.pause();
        return;
    }

    if !should_pause && virtual_time.is_paused() {
        virtual_time.unpause();
    }
}

fn pause_menu_widgets_visible(widget_visibility: &Query<(&CssID, &Visibility)>) -> bool {
    widget_visibility.iter().any(|(css_id, visibility)| {
        css_id.0.starts_with("pause-menu") && !matches!(*visibility, Visibility::Hidden)
    })
}

fn close_pause_menu(
    mut ui_interaction: ResMut<UiInteractionState>,
    mut pause_menu: ResMut<PauseMenuState>,
    mut registry: ResMut<UiRegistry>,
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if !pause_menu.open {
        return;
    }

    pause_menu.open = false;
    ui_interaction.menu_open = false;
    hide_pause_menu_ui(&mut registry);
    set_pause_menu_cursor(false, &mut cursor_q);
}

fn show_pause_menu_ui(registry: &mut UiRegistry, asset_server: &AssetServer) {
    if registry.get(PAUSE_MENU_UI_KEY).is_none() {
        let handle: Handle<HtmlAsset> = asset_server.load(PAUSE_MENU_UI_PATH);
        registry.add(
            PAUSE_MENU_UI_KEY.to_string(),
            HtmlSource::from_handle(handle),
        );
    }

    set_pause_menu_ui_active(registry, true);
}

fn hide_pause_menu_ui(registry: &mut UiRegistry) {
    set_pause_menu_ui_active(registry, false);
}

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

fn set_pause_menu_ui_active(registry: &mut UiRegistry, active: bool) {
    let mut active_uis = registry.current.clone().unwrap_or_default();
    active_uis.retain(|name| name != PAUSE_MENU_UI_KEY);

    if active {
        active_uis.push(PAUSE_MENU_UI_KEY.to_string());
    }

    registry.use_uis(active_uis);
}

fn pause_menu_ui_in_stack(registry: &UiRegistry) -> bool {
    registry.current.as_ref().is_some_and(|current| {
        current
            .iter()
            .any(|name| name.as_str() == PAUSE_MENU_UI_KEY)
    })
}

fn consume_pause_menu_action(
    widgets: &mut Query<(&CssID, &mut UIWidgetState)>,
) -> Option<PauseMenuAction> {
    widgets.iter_mut().find_map(|(css_id, mut state)| {
        if !state.checked {
            return None;
        }

        state.checked = false;

        match css_id.0.as_str() {
            PAUSE_PLAY_ID => Some(PauseMenuAction::Play),
            PAUSE_CONNECT_ID => Some(PauseMenuAction::ConnectToServer),
            PAUSE_SETTINGS_ID => Some(PauseMenuAction::Settings),
            PAUSE_CLOSE_ID => Some(PauseMenuAction::GameClose),
            _ => None,
        }
    })
}
