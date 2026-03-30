use crate::core::states::states::{AppState, InGameStates, LoadingStates};
use bevy::prelude::*;
use bevy_extended_ui::html::HtmlSource;
use bevy_extended_ui::io::HtmlAsset;
use bevy_extended_ui::registry::UiRegistry;

const WORLD_UNLOAD_UI_KEY: &str = "world-unload";
const WORLD_UNLOAD_UI_PATH: &str = "ui/html/world_unload.html";
const WORLD_UNLOAD_HOLD_SECS: f32 = 0.35;

pub struct WorldUnloadUiPlugin;

#[derive(Resource, Debug, Clone)]
pub(crate) struct WorldUnloadUiState {
    pub(crate) active: bool,
    pub(crate) timer: Timer,
}

impl Default for WorldUnloadUiState {
    fn default() -> Self {
        Self {
            active: false,
            timer: Timer::from_seconds(WORLD_UNLOAD_HOLD_SECS, TimerMode::Once),
        }
    }
}

impl Plugin for WorldUnloadUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldUnloadUiState>()
            .add_systems(Startup, register_world_unload_ui)
            .add_systems(
                OnExit(AppState::InGame(InGameStates::Game)),
                show_world_unload_ui,
            )
            .add_systems(
                Update,
                tick_world_unload_ui.run_if(world_unload_ui_should_tick),
            )
            .add_systems(
                OnEnter(AppState::Loading(LoadingStates::BaseGen)),
                reset_world_unload_ui,
            )
            .add_systems(
                OnEnter(AppState::InGame(InGameStates::Game)),
                reset_world_unload_ui,
            );
    }
}

fn register_world_unload_ui(mut registry: ResMut<UiRegistry>, asset_server: Res<AssetServer>) {
    if registry.get(WORLD_UNLOAD_UI_KEY).is_some() {
        return;
    }

    let handle: Handle<HtmlAsset> = asset_server.load(WORLD_UNLOAD_UI_PATH);
    registry.add(
        WORLD_UNLOAD_UI_KEY.to_string(),
        HtmlSource::from_handle(handle),
    );
}

pub(crate) fn trigger_world_unload_ui(
    registry: &mut UiRegistry,
    asset_server: &AssetServer,
    state: &mut WorldUnloadUiState,
) {
    if registry.get(WORLD_UNLOAD_UI_KEY).is_none() {
        let handle: Handle<HtmlAsset> = asset_server.load(WORLD_UNLOAD_UI_PATH);
        registry.add(
            WORLD_UNLOAD_UI_KEY.to_string(),
            HtmlSource::from_handle(handle),
        );
    }

    state.active = true;
    state.timer.reset();
    activate_world_unload_ui(registry);
}

fn show_world_unload_ui(
    mut registry: ResMut<UiRegistry>,
    asset_server: Res<AssetServer>,
    mut state: ResMut<WorldUnloadUiState>,
) {
    trigger_world_unload_ui(&mut registry, &asset_server, &mut state);
}

fn tick_world_unload_ui(
    time: Res<Time>,
    app_state: Res<State<AppState>>,
    mut registry: ResMut<UiRegistry>,
    mut state: ResMut<WorldUnloadUiState>,
) {
    if !state.active {
        return;
    }

    if is_loading_state(app_state.get()) {
        hide_world_unload_ui(&mut registry);
        state.active = false;
        return;
    }

    activate_world_unload_ui(&mut registry);

    state.timer.tick(time.delta());
    if !state.timer.is_finished() {
        return;
    }

    hide_world_unload_ui(&mut registry);
    state.active = false;
}

fn reset_world_unload_ui(mut registry: ResMut<UiRegistry>, mut state: ResMut<WorldUnloadUiState>) {
    hide_world_unload_ui(&mut registry);
    state.active = false;
    state.timer.reset();
}

fn world_unload_ui_should_tick(state: Res<WorldUnloadUiState>) -> bool {
    state.active
}

fn is_loading_state(app_state: &AppState) -> bool {
    matches!(
        app_state,
        AppState::Loading(LoadingStates::BaseGen)
            | AppState::Loading(LoadingStates::WaterGen)
            | AppState::Loading(LoadingStates::CaveGen)
    )
}

fn activate_world_unload_ui(registry: &mut UiRegistry) {
    if let Some(current) = registry.current.as_mut() {
        if current.iter().any(|name| name == WORLD_UNLOAD_UI_KEY) {
            return;
        }

        current.push(WORLD_UNLOAD_UI_KEY.to_string());
        registry.ui_update = true;
        return;
    }

    registry.current = Some(vec![WORLD_UNLOAD_UI_KEY.to_string()]);
    registry.ui_update = true;
}

fn hide_world_unload_ui(registry: &mut UiRegistry) {
    let mut clear_current = false;

    if let Some(current) = registry.current.as_mut() {
        let original_len = current.len();
        current.retain(|name| name != WORLD_UNLOAD_UI_KEY);
        if current.len() != original_len {
            registry.ui_update = true;
        }
        clear_current = current.is_empty();
    }

    if clear_current {
        registry.current = None;
    }
}
