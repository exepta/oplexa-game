use crate::core::states::states::{AppState, InGameStates, LoadingStates};
use crate::core::states::world_gen::{LoadingPhase, LoadingProgress};
use bevy::prelude::*;
use bevy_extended_ui::html::HtmlSource;
use bevy_extended_ui::io::HtmlAsset;
use bevy_extended_ui::registry::UiRegistry;
use bevy_extended_ui::styles::CssID;
use bevy_extended_ui::widgets::ProgressBar;
use bevy_extended_ui::{ExtendedUiConfiguration, ExtendedUiPlugin};

const WORLD_GEN_UI_KEY: &str = "world-gen";
const WORLD_GEN_UI_PATH: &str = "ui/html/world_gen.html";
const WORLD_GEN_PROGRESS_ID: &str = "world-gen-progress";

pub struct WorldGenScreenPlugin;

#[derive(Resource, Debug, Clone)]
struct WorldGenUiAnimation {
    displayed_pct: f32,
}

impl Default for WorldGenUiAnimation {
    fn default() -> Self {
        Self { displayed_pct: 0.0 }
    }
}

impl Plugin for WorldGenScreenPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadingProgress>()
            .init_resource::<WorldGenUiAnimation>()
            .add_plugins(ExtendedUiPlugin)
            .add_systems(Startup, (configure_extended_ui, register_world_gen_ui))
            .add_systems(
                OnEnter(AppState::Loading(LoadingStates::BaseGen)),
                (reset_world_gen_ui_animation, show_world_gen_ui),
            )
            .add_systems(
                OnExit(AppState::Loading(LoadingStates::CaveGen)),
                hide_world_gen_ui,
            )
            .add_systems(
                OnEnter(AppState::InGame(InGameStates::Game)),
                hide_world_gen_ui,
            )
            .add_systems(Update, sync_world_gen_progress.run_if(is_loading_state));
    }
}

fn configure_extended_ui(mut config: ResMut<ExtendedUiConfiguration>) {
    config.order = 25;
}

fn register_world_gen_ui(mut registry: ResMut<UiRegistry>, asset_server: Res<AssetServer>) {
    if registry.get(WORLD_GEN_UI_KEY).is_some() {
        return;
    }

    let handle: Handle<HtmlAsset> = asset_server.load(WORLD_GEN_UI_PATH);
    registry.add(
        WORLD_GEN_UI_KEY.to_string(),
        HtmlSource::from_handle(handle),
    );
}

fn show_world_gen_ui(mut registry: ResMut<UiRegistry>, asset_server: Res<AssetServer>) {
    if registry.get(WORLD_GEN_UI_KEY).is_none() {
        let handle: Handle<HtmlAsset> = asset_server.load(WORLD_GEN_UI_PATH);
        registry.add(
            WORLD_GEN_UI_KEY.to_string(),
            HtmlSource::from_handle(handle),
        );
    }

    activate_world_gen_ui(&mut registry);
}

fn hide_world_gen_ui(mut registry: ResMut<UiRegistry>) {
    let mut clear_current = false;

    if let Some(current) = registry.current.as_mut() {
        current.retain(|name| name != WORLD_GEN_UI_KEY);
        clear_current = current.is_empty();
        registry.ui_update = true;
    }

    if clear_current {
        registry.current = None;
    }
}

fn activate_world_gen_ui(registry: &mut UiRegistry) {
    if registry.get(WORLD_GEN_UI_KEY).is_none() {
        return;
    }

    if let Some(current) = registry.current.as_mut() {
        if current.iter().any(|name| name == WORLD_GEN_UI_KEY) {
            return;
        }
        current.push(WORLD_GEN_UI_KEY.to_string());
        registry.ui_update = true;
        return;
    }

    registry.current = Some(vec![WORLD_GEN_UI_KEY.to_string()]);
    registry.ui_update = true;
}

fn sync_world_gen_progress(
    time: Res<Time>,
    app_state: Res<State<AppState>>,
    mut loading_progress: ResMut<LoadingProgress>,
    mut animation: ResMut<WorldGenUiAnimation>,
    mut progress_bars: Query<(&mut ProgressBar, &CssID)>,
) {
    let (phase, target_pct) = match app_state.get() {
        AppState::Loading(LoadingStates::BaseGen) => (LoadingPhase::BaseGen, 34.0),
        AppState::Loading(LoadingStates::WaterGen) => (LoadingPhase::WaterGen, 72.0),
        AppState::Loading(LoadingStates::CaveGen) => (LoadingPhase::CaveGen, 97.0),
        _ => (LoadingPhase::Done, 100.0),
    };

    loading_progress.phase = phase;
    animation.displayed_pct =
        smooth_progress(animation.displayed_pct, target_pct, time.delta_secs());
    loading_progress.overall_pct = animation.displayed_pct;

    for (mut progress_bar, css_id) in &mut progress_bars {
        if css_id.0 != WORLD_GEN_PROGRESS_ID {
            continue;
        }

        progress_bar.min = 0.0;
        progress_bar.max = 100.0;
        progress_bar.value = animation.displayed_pct;
    }
}

fn is_loading_state(app_state: Res<State<AppState>>) -> bool {
    matches!(
        app_state.get(),
        AppState::Loading(LoadingStates::BaseGen)
            | AppState::Loading(LoadingStates::WaterGen)
            | AppState::Loading(LoadingStates::CaveGen)
    )
}

fn reset_world_gen_ui_animation(mut animation: ResMut<WorldGenUiAnimation>) {
    animation.displayed_pct = 0.0;
}

fn smooth_progress(current: f32, target: f32, delta_secs: f32) -> f32 {
    if current >= target {
        return current;
    }

    let step = (delta_secs * 32.0).clamp(0.8, 6.0);
    (current + step).min(target)
}
