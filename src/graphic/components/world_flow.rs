/// Runs the `show_world_gen_ui` routine for show world gen ui in the `graphic::components::world_flow` module.
fn show_world_gen_ui(mut visibility: Query<&mut Visibility, With<WorldGenRoot>>) {
    if let Ok(mut visible) = visibility.single_mut() {
        *visible = Visibility::Inherited;
    }
}

/// Runs the `hide_world_gen_ui` routine for hide world gen ui in the `graphic::components::world_flow` module.
fn hide_world_gen_ui(mut visibility: Query<&mut Visibility, With<WorldGenRoot>>) {
    if let Ok(mut visible) = visibility.single_mut() {
        *visible = Visibility::Hidden;
    }
}

/// Synchronizes world gen progress for the `graphic::components::world_flow` module.
fn sync_world_gen_progress(
    time: Res<Time>,
    app_state: Res<State<AppState>>,
    game_config: Res<GlobalConfig>,
    mut loading_progress: ResMut<LoadingProgress>,
    mut animation: ResMut<WorldGenUiAnimation>,
    mut progress_bars: Query<(&CssID, &mut ProgressBar)>,
    mut paragraphs: Query<(&CssID, &mut Paragraph)>,
) {
    let (phase, target_pct) = match app_state.get() {
        AppState::Loading(LoadingStates::BaseGen) => (LoadingPhase::BaseGen, 34.0),
        AppState::Loading(LoadingStates::CaveGen) => (LoadingPhase::CaveGen, 72.0),
        AppState::Loading(LoadingStates::WaterGen) => (LoadingPhase::WaterGen, 97.0),
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

    let initial_radius =
        loading_preload_radius_for_ui(game_config.graphics.chunk_range).max(0) as usize;
    let side = initial_radius * 2 + 1;
    let target = side * side;
    let pct = (animation.displayed_pct / 100.0).clamp(0.0, 1.0);
    let current = ((target as f32) * pct).round() as usize;

    for (css_id, mut paragraph) in &mut paragraphs {
        if css_id.0 != WORLD_GEN_CHUNKS_ID {
            continue;
        }
        paragraph.text = format!("Chunks Loaded {} / {}", current, target);
    }
}

#[inline]
fn loading_preload_radius_for_ui(chunk_range: i32) -> i32 {
    chunk_range.max(0)
}

/// Checks whether loading state in the `graphic::components::world_flow` module.
fn is_loading_state(app_state: Res<State<AppState>>) -> bool {
    matches!(
        app_state.get(),
        AppState::Loading(LoadingStates::BaseGen)
            | AppState::Loading(LoadingStates::WaterGen)
            | AppState::Loading(LoadingStates::CaveGen)
    )
}

/// Runs the `reset_world_gen_ui_animation` routine for reset world gen ui animation in the `graphic::components::world_flow` module.
fn reset_world_gen_ui_animation(mut animation: ResMut<WorldGenUiAnimation>) {
    animation.displayed_pct = 0.0;
}

/// Runs the `smooth_progress` routine for smooth progress in the `graphic::components::world_flow` module.
fn smooth_progress(current: f32, target: f32, delta_secs: f32) -> f32 {
    if current >= target {
        return current;
    }

    let step = (delta_secs * 32.0).clamp(0.8, 6.0);
    (current + step).min(target)
}

/// Runs the `trigger_world_unload_ui` routine for trigger world unload ui in the `graphic::components::world_flow` module.
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

/// Runs the `tick_world_unload_ui` routine for tick world unload ui in the `graphic::components::world_flow` module.
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

/// Runs the `reset_world_unload_ui` routine for reset world unload ui in the `graphic::components::world_flow` module.
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

/// Runs the `world_unload_ui_should_tick` routine for world unload ui should tick in the `graphic::components::world_flow` module.
fn world_unload_ui_should_tick(state: Res<WorldUnloadUiState>) -> bool {
    state.active
}

/// `sync_scrollbar_from_content` (bevy_extended_ui) sets Scrollbar to `Visibility::Visible`
/// whenever content overflows, even when the containing menu root is Hidden.
/// Because `Visibility::Visible` ignores parent visibility, the scrollbar stays on screen.
/// This PostUpdate system overrides that: if the menu root is Hidden, force all direct
/// Scrollbar children of the list Div to Hidden as well.
fn suppress_stale_scrollbars(
    sp_root: Query<&Visibility, With<SinglePlayerRoot>>,
    mp_root: Query<&Visibility, With<MultiplayerRoot>>,
    sp_list: Query<&Children, With<SinglePlayerWorldList>>,
    mp_list: Query<&Children, With<MultiplayerServerList>>,
    scrollbar_check: Query<(), With<Scrollbar>>,
    mut vis_q: Query<
        &mut Visibility,
        (
            With<Scrollbar>,
            Without<SinglePlayerRoot>,
            Without<MultiplayerRoot>,
        ),
    >,
) {
    if let (Ok(root_vis), Ok(children)) = (sp_root.single(), sp_list.single()) {
        if *root_vis == Visibility::Hidden {
            for child in children.iter() {
                if scrollbar_check.get(child).is_ok() {
                    if let Ok(mut vis) = vis_q.get_mut(child) {
                        if *vis == Visibility::Visible {
                            *vis = Visibility::Hidden;
                        }
                    }
                }
            }
        }
    }

    if let (Ok(root_vis), Ok(children)) = (mp_root.single(), mp_list.single()) {
        if *root_vis == Visibility::Hidden {
            for child in children.iter() {
                if scrollbar_check.get(child).is_ok() {
                    if let Ok(mut vis) = vis_q.get_mut(child) {
                        if *vis == Visibility::Visible {
                            *vis = Visibility::Hidden;
                        }
                    }
                }
            }
        }
    }
}

/// Runs the `hide_menu_roots_for_ingame` routine for hide menu roots for ingame in the `graphic::components::world_flow` module.
fn hide_menu_roots_for_ingame(
    mut commands: Commands,
    mut roots: ParamSet<(
        Query<&mut Visibility, With<MainMenuRoot>>,
        Query<&mut Visibility, With<SinglePlayerRoot>>,
        Query<&mut Visibility, With<CreateWorldRoot>>,
        Query<&mut Visibility, With<MultiplayerRoot>>,
    )>,
    mut ui_interaction: ResMut<UiInteractionState>,
    mut single_player_state: ResMut<SinglePlayerUiState>,
    mut multiplayer_state: ResMut<MultiplayerUiState>,
    sp_items: Query<Entity, With<SinglePlayerListItem>>,
    mp_items: Query<Entity, With<MultiplayerListItem>>,
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

    for entity in sp_items.iter() {
        commands.entity(entity).despawn();
    }
    for entity in mp_items.iter() {
        commands.entity(entity).despawn();
    }

    ui_interaction.menu_open = false;
    single_player_state.closing_for_world_load = false;
    single_player_state.pending_delete_index = None;
    multiplayer_state.form_dialog = None;
    multiplayer_state.pending_delete_key = None;
    multiplayer_state.joining_key = None;
}
