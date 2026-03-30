use crate::core::config::GlobalConfig;
use crate::core::events::ui_events::{
    DisconnectFromServerRequest, OpenToLanRequest, StopLanHostRequest,
};
use crate::core::multiplayer::MultiplayerConnectionState;
use crate::core::states::states::{AppState, BeforeUiState, InGameStates, is_state_in_game};
use crate::core::ui::UiInteractionState;
use crate::graphic::world_unload_ui::{WorldUnloadUiState, trigger_world_unload_ui};
use crate::utils::key_utils::convert;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_extended_ui::html::HtmlSource;
use bevy_extended_ui::io::HtmlAsset;
use bevy_extended_ui::registry::UiRegistry;
use bevy_extended_ui::styles::CssID;
use bevy_extended_ui::widgets::UIWidgetState;

const PAUSE_MENU_UI_KEY: &str = "pause-menu";
const PAUSE_MENU_MULTIPLAYER_UI_KEY: &str = "pause-menu-multiplayer";
const PAUSE_MENU_UI_PATH: &str = "ui/html/pause_menu.html";
const PAUSE_MENU_MULTIPLAYER_UI_PATH: &str = "ui/html/pause_menu_disconnect.html";
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
    BackToGame,
    OpenToLan,
    Settings,
    ExitToMenu,
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

fn register_pause_menu_ui(
    mut registry: ResMut<UiRegistry>,
    asset_server: Res<AssetServer>,
) {
    if registry.get(PAUSE_MENU_UI_KEY).is_none() {
        let handle: Handle<HtmlAsset> = asset_server.load(PAUSE_MENU_UI_PATH);
        registry.add(
            PAUSE_MENU_UI_KEY.to_string(),
            HtmlSource::from_handle(handle),
        );
    }

    if registry.get(PAUSE_MENU_MULTIPLAYER_UI_KEY).is_none() {
        let handle: Handle<HtmlAsset> = asset_server.load(PAUSE_MENU_MULTIPLAYER_UI_PATH);
        registry.add(
            PAUSE_MENU_MULTIPLAYER_UI_KEY.to_string(),
            HtmlSource::from_handle(handle),
        );
    }
}

fn toggle_pause_menu(
    keyboard: Res<ButtonInput<KeyCode>>,
    global_config: Res<GlobalConfig>,
    asset_server: Res<AssetServer>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
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
        show_pause_menu_ui(&mut registry, &asset_server, multiplayer_connection.connected);
    } else {
        hide_pause_menu_ui(&mut registry);
    }
}

fn enforce_pause_menu_visibility(
    asset_server: Res<AssetServer>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
    pause_menu: Res<PauseMenuState>,
    mut ui_interaction: ResMut<UiInteractionState>,
    mut registry: ResMut<UiRegistry>,
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if !pause_menu.open {
        return;
    }

    ui_interaction.menu_open = true;
    if !pause_menu_ui_in_stack(&registry, multiplayer_connection.connected) {
        show_pause_menu_ui(&mut registry, &asset_server, multiplayer_connection.connected);
    }
    set_pause_menu_cursor(true, &mut cursor_q);
}

fn handle_pause_menu_buttons(
    multiplayer_connection: Res<MultiplayerConnectionState>,
    mut ui_interaction: ResMut<UiInteractionState>,
    mut pause_menu: ResMut<PauseMenuState>,
    mut widgets: Query<(&CssID, &mut UIWidgetState)>,
    mut registry: ResMut<UiRegistry>,
    asset_server: Res<AssetServer>,
    mut world_unload_ui: ResMut<WorldUnloadUiState>,
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut next_state: ResMut<NextState<AppState>>,
    mut open_to_lan_writer: MessageWriter<OpenToLanRequest>,
    mut disconnect_writer: MessageWriter<DisconnectFromServerRequest>,
    mut stop_host_writer: MessageWriter<StopLanHostRequest>,
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
            hide_pause_menu_ui(&mut registry);
            set_pause_menu_cursor(false, &mut cursor_q);
        }
        PauseMenuAction::OpenToLan => {
            open_to_lan_writer.write(OpenToLanRequest);
            pause_menu.open = false;
            ui_interaction.menu_open = false;
            hide_pause_menu_ui(&mut registry);
            set_pause_menu_cursor(false, &mut cursor_q);
        }
        PauseMenuAction::Settings => {
            info!("Settings button clicked (not implemented yet).");
            show_pause_menu_ui(&mut registry, &asset_server, multiplayer_connection.connected);
        }
        PauseMenuAction::ExitToMenu => {
            if multiplayer_connection.connected {
                disconnect_writer.write(DisconnectFromServerRequest);
            }
            stop_host_writer.write(StopLanHostRequest);
            pause_menu.open = false;
            ui_interaction.menu_open = false;
            hide_pause_menu_ui(&mut registry);
            set_pause_menu_cursor(true, &mut cursor_q);
            trigger_world_unload_ui(&mut registry, &asset_server, &mut world_unload_ui);
            next_state.set(AppState::Screen(BeforeUiState::Menu));
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

fn show_pause_menu_ui(
    registry: &mut UiRegistry,
    asset_server: &AssetServer,
    multiplayer_connected: bool,
) {
    let target_key = pause_menu_ui_key(multiplayer_connected);
    let target_path = pause_menu_ui_path(multiplayer_connected);

    if registry.get(target_key).is_none() {
        let handle: Handle<HtmlAsset> = asset_server.load(target_path);
        registry.add(target_key.to_string(), HtmlSource::from_handle(handle));
    }

    activate_pause_menu_ui(registry, target_key);
}

fn hide_pause_menu_ui(registry: &mut UiRegistry) {
    remove_pause_menu_ui(registry, PAUSE_MENU_UI_KEY);
    remove_pause_menu_ui(registry, PAUSE_MENU_MULTIPLAYER_UI_KEY);
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

fn activate_pause_menu_ui(registry: &mut UiRegistry, key: &str) {
    if registry.get(key).is_none() {
        return;
    }

    let mut changed = false;
    if let Some(current) = registry.current.as_mut() {
        let original_len = current.len();
        current.retain(|name| {
            name != PAUSE_MENU_UI_KEY && name != PAUSE_MENU_MULTIPLAYER_UI_KEY
        });
        changed |= current.len() != original_len;

        if current.iter().all(|name| name != key) {
            current.push(key.to_string());
            changed = true;
        }

        if changed {
            registry.ui_update = true;
        }
        return;
    }

    registry.current = Some(vec![key.to_string()]);
    registry.ui_update = true;
}

fn remove_pause_menu_ui(registry: &mut UiRegistry, key: &str) {
    let mut clear_current = false;

    if let Some(current) = registry.current.as_mut() {
        let original_len = current.len();
        current.retain(|name| name != key);
        if current.len() != original_len {
            registry.ui_update = true;
        }
        clear_current = current.is_empty();
    }

    if clear_current {
        registry.current = None;
    }
}

fn pause_menu_ui_in_stack(registry: &UiRegistry, multiplayer_connected: bool) -> bool {
    let expected = pause_menu_ui_key(multiplayer_connected);
    registry
        .current
        .as_ref()
        .is_some_and(|current| current.iter().any(|name| name.as_str() == expected))
}

fn pause_menu_ui_key(multiplayer_connected: bool) -> &'static str {
    if multiplayer_connected {
        PAUSE_MENU_MULTIPLAYER_UI_KEY
    } else {
        PAUSE_MENU_UI_KEY
    }
}

fn pause_menu_ui_path(multiplayer_connected: bool) -> &'static str {
    if multiplayer_connected {
        PAUSE_MENU_MULTIPLAYER_UI_PATH
    } else {
        PAUSE_MENU_UI_PATH
    }
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
            PAUSE_PLAY_ID => Some(PauseMenuAction::BackToGame),
            PAUSE_CONNECT_ID => Some(PauseMenuAction::OpenToLan),
            PAUSE_SETTINGS_ID => Some(PauseMenuAction::Settings),
            PAUSE_CLOSE_ID => Some(PauseMenuAction::ExitToMenu),
            _ => None,
        }
    })
}
