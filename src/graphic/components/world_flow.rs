fn show_world_gen_ui(mut visibility: Query<&mut Visibility, With<WorldGenRoot>>) {
    if let Ok(mut visible) = visibility.single_mut() {
        *visible = Visibility::Inherited;
    }
}

fn hide_world_gen_ui(mut visibility: Query<&mut Visibility, With<WorldGenRoot>>) {
    if let Ok(mut visible) = visibility.single_mut() {
        *visible = Visibility::Hidden;
    }
}

fn sync_world_gen_progress(
    time: Res<Time>,
    app_state: Res<State<AppState>>,
    mut loading_progress: ResMut<LoadingProgress>,
    mut animation: ResMut<WorldGenUiAnimation>,
    mut progress_bars: Query<(&CssID, &mut ProgressBar)>,
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

    for (css_id, mut progress_bar) in &mut progress_bars {
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

fn trigger_world_unload_ui(
    mut root: Query<&mut Visibility, With<WorldUnloadRoot>>,
    mut state: ResMut<WorldUnloadUiState>,
) {
    state.active = true;
    state.timer.reset();
    if let Ok(mut visible) = root.single_mut() {
        *visible = Visibility::Inherited;
    }
}

fn tick_world_unload_ui(
    time: Res<Time>,
    app_state: Res<State<AppState>>,
    mut root: Query<&mut Visibility, With<WorldUnloadRoot>>,
    mut state: ResMut<WorldUnloadUiState>,
) {
    if !state.active {
        return;
    }

    if matches!(
        app_state.get(),
        AppState::Loading(LoadingStates::BaseGen)
            | AppState::Loading(LoadingStates::WaterGen)
            | AppState::Loading(LoadingStates::CaveGen)
    ) {
        if let Ok(mut visible) = root.single_mut() {
            *visible = Visibility::Hidden;
        }
        state.active = false;
        return;
    }

    if let Ok(mut visible) = root.single_mut() {
        *visible = Visibility::Inherited;
    }

    state.timer.tick(time.delta());
    if !state.timer.is_finished() {
        return;
    }

    if let Ok(mut visible) = root.single_mut() {
        *visible = Visibility::Hidden;
    }
    state.active = false;
}

fn reset_world_unload_ui(
    mut root: Query<&mut Visibility, With<WorldUnloadRoot>>,
    mut state: ResMut<WorldUnloadUiState>,
) {
    if let Ok(mut visible) = root.single_mut() {
        *visible = Visibility::Hidden;
    }
    state.active = false;
    state.timer.reset();
}

fn world_unload_ui_should_tick(state: Res<WorldUnloadUiState>) -> bool {
    state.active
}

fn hide_menu_roots_for_ingame(
    mut roots: ParamSet<(
        Query<&mut Visibility, With<MainMenuRoot>>,
        Query<&mut Visibility, With<SinglePlayerRoot>>,
        Query<&mut Visibility, With<CreateWorldRoot>>,
        Query<&mut Visibility, With<MultiplayerRoot>>,
    )>,
    mut ui_interaction: ResMut<UiInteractionState>,
    mut single_player_state: ResMut<SinglePlayerUiState>,
    mut multiplayer_state: ResMut<MultiplayerUiState>,
) {
    if let Ok(mut visible) = roots.p0().single_mut() {
        *visible = Visibility::Hidden;
    }
    if let Ok(mut visible) = roots.p1().single_mut() {
        *visible = Visibility::Hidden;
    }
    if let Ok(mut visible) = roots.p2().single_mut() {
        *visible = Visibility::Hidden;
    }
    if let Ok(mut visible) = roots.p3().single_mut() {
        *visible = Visibility::Hidden;
    }

    ui_interaction.menu_open = false;
    single_player_state.closing_for_world_load = false;
    single_player_state.pending_delete_index = None;
    multiplayer_state.form_dialog = None;
    multiplayer_state.pending_delete_key = None;
    multiplayer_state.joining_key = None;
}
