#[derive(SystemParam)]
struct LoadingProgressData<'w, 's> {
    load_center: Option<Res<'w, LoadCenter>>,
    chunk_map: Res<'w, ChunkMap>,
    pending_gen: Option<Res<'w, PendingGen>>,
    pending_mesh: Option<Res<'w, PendingMesh>>,
    mesh_backlog: Option<Res<'w, MeshBacklog>>,
    cave_tracker: Option<Res<'w, CaveTracker>>,
    cave_jobs: Option<Res<'w, CaveJobs>>,
    water_ready_set: Option<Res<'w, WaterReadySet>>,
    water_todo: Option<Res<'w, WaterMeshingTodo>>,
    pending_water_load: Option<Res<'w, PendingWaterLoad>>,
    pending_water_mesh: Option<Res<'w, PendingWaterMesh>>,
    water_backlog: Option<Res<'w, WaterMeshBacklog>>,
    _marker: std::marker::PhantomData<&'s ()>,
}

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
    loading_data: LoadingProgressData,
    mut loading_progress: ResMut<LoadingProgress>,
    mut animation: ResMut<WorldGenUiAnimation>,
    mut progress_log_state: ResMut<WorldGenProgressLogState>,
    mut progress_bars: Query<(&CssID, &mut ProgressBar)>,
    mut paragraphs: Query<(&CssID, &mut Paragraph)>,
) {
    let metrics = compute_loading_progress_metrics(
        app_state.get(),
        game_config.graphics.chunk_range,
        loading_data.load_center.as_deref(),
        &loading_data.chunk_map,
        loading_data.pending_gen.as_deref(),
        loading_data.pending_mesh.as_deref(),
        loading_data.mesh_backlog.as_deref(),
        loading_data.cave_tracker.as_deref(),
        loading_data.cave_jobs.as_deref(),
        loading_data.water_ready_set.as_deref(),
        loading_data.water_todo.as_deref(),
        loading_data.pending_water_load.as_deref(),
        loading_data.pending_water_mesh.as_deref(),
        loading_data.water_backlog.as_deref(),
    );

    let phase_floor = phase_floor_percent(metrics.phase);
    let phase_cap = phase_cap_percent(metrics.phase);
    if metrics.phase != progress_log_state.last_phase {
        progress_log_state.last_phase = metrics.phase;
        progress_log_state.phase_peak_percent = phase_floor;
        progress_log_state.phase_peak_chunks = 0;
    }
    progress_log_state.phase_peak_percent = progress_log_state
        .phase_peak_percent
        .max(metrics.overall_pct.clamp(phase_floor, phase_cap))
        .min(phase_cap);
    progress_log_state.phase_peak_chunks = progress_log_state
        .phase_peak_chunks
        .max(metrics.progress_chunks)
        .min(metrics.total_chunks);

    loading_progress.phase = metrics.phase;
    animation.displayed_pct = smooth_progress(
        animation.displayed_pct,
        progress_log_state.phase_peak_percent,
        time.delta_secs(),
    );
    loading_progress.overall_pct = progress_log_state.phase_peak_percent;

    progress_log_state.timer.tick(time.delta());
    if progress_log_state.timer.just_finished() {
        let pct = animation.displayed_pct.round().clamp(0.0, 100.0) as u8;
        if progress_log_state
            .last_logged_percent
            .is_none_or(|last| pct > last)
        {
            info!(
                "[Worler-{}]: progress {}%",
                progress_log_state.world_sequence.max(1),
                pct
            );
            progress_log_state.last_logged_percent = Some(pct);
        }
    }

    for (css_id, mut progress_bar) in &mut progress_bars {
        if css_id.0 != WORLD_GEN_PROGRESS_ID {
            continue;
        }

        progress_bar.min = 0.0;
        progress_bar.max = 100.0;
        progress_bar.value = animation.displayed_pct;
    }

    for (css_id, mut paragraph) in &mut paragraphs {
        if css_id.0 != WORLD_GEN_CHUNKS_ID {
            continue;
        }
        paragraph.text = format!(
            "Chunks Loaded {} / {}",
            progress_log_state.phase_peak_chunks,
            metrics.total_chunks
        );
    }
}

#[derive(Clone, Copy, Debug)]
struct LoadingProgressMetrics {
    phase: LoadingPhase,
    overall_pct: f32,
    progress_chunks: usize,
    total_chunks: usize,
}

#[allow(clippy::too_many_arguments)]
fn compute_loading_progress_metrics(
    app_state: &AppState,
    chunk_range: i32,
    load_center: Option<&LoadCenter>,
    chunk_map: &ChunkMap,
    pending_gen: Option<&PendingGen>,
    pending_mesh: Option<&PendingMesh>,
    mesh_backlog: Option<&MeshBacklog>,
    cave_tracker: Option<&CaveTracker>,
    cave_jobs: Option<&CaveJobs>,
    water_ready_set: Option<&WaterReadySet>,
    water_todo: Option<&WaterMeshingTodo>,
    pending_water_load: Option<&PendingWaterLoad>,
    pending_water_mesh: Option<&PendingWaterMesh>,
    water_backlog: Option<&WaterMeshBacklog>,
) -> LoadingProgressMetrics {
    let phase = match app_state {
        AppState::Loading(LoadingStates::BaseGen) => LoadingPhase::BaseGen,
        AppState::Loading(LoadingStates::CaveGen) => LoadingPhase::CaveGen,
        AppState::Loading(LoadingStates::WaterGen) => LoadingPhase::WaterGen,
        _ => LoadingPhase::Done,
    };

    match phase {
        LoadingPhase::BaseGen => {
            let radius = loading_preload_radius_for_ui(chunk_range).max(0);
            let center = load_center.map(|lc| lc.world_xz).unwrap_or(IVec2::ZERO);
            let side = (radius as usize).saturating_mul(2).saturating_add(1);
            let total = side.saturating_mul(side).max(1);

            let mut busy_mesh: HashSet<IVec2> = HashSet::new();
            if let Some(pending_mesh) = pending_mesh {
                busy_mesh.extend(pending_mesh.0.keys().map(|(coord, _)| *coord));
            }
            if let Some(mesh_backlog) = mesh_backlog {
                busy_mesh.extend(mesh_backlog.0.iter().map(|(coord, _)| *coord));
            }

            const SCORE_PENDING_GEN: f32 = 0.25;
            const SCORE_LOADED_GEN: f32 = 0.70;
            const SCORE_READY: f32 = 1.0;

            let mut equivalent_done = 0.0f32;
            for dz in -radius..=radius {
                for dx in -radius..=radius {
                    let coord = IVec2::new(center.x + dx, center.y + dz);
                    if pending_gen.is_some_and(|pending| pending.0.contains_key(&coord)) {
                        equivalent_done += SCORE_PENDING_GEN;
                        continue;
                    }
                    if !chunk_map.chunks.contains_key(&coord) {
                        continue;
                    }
                    if busy_mesh.contains(&coord) {
                        equivalent_done += SCORE_LOADED_GEN;
                        continue;
                    }
                    equivalent_done += SCORE_READY;
                }
            }

            let ratio = (equivalent_done / total as f32).clamp(0.0, 1.0);
            let progress_chunks = equivalent_done.round().clamp(0.0, total as f32) as usize;
            LoadingProgressMetrics {
                phase,
                overall_pct: ratio * 72.0,
                progress_chunks,
                total_chunks: total,
            }
        }
        LoadingPhase::CaveGen => {
            let (done, total) = match (cave_tracker, cave_jobs) {
                (Some(cave_tracker), Some(cave_jobs)) => {
                    let total = cave_tracker
                        .done
                        .len()
                        .saturating_add(cave_tracker.pending.len())
                        .saturating_add(cave_jobs.running.len())
                        .max(1);
                    (cave_tracker.done.len(), total)
                }
                _ => (0usize, 1usize),
            };
            let ratio = (done as f32 / total as f32).clamp(0.0, 1.0);
            LoadingProgressMetrics {
                phase,
                overall_pct: 72.0 + ratio * (84.0 - 72.0),
                progress_chunks: done,
                total_chunks: total,
            }
        }
        LoadingPhase::WaterGen => {
            let total = chunk_map.chunks.len().max(1);
            let ready = water_ready_set.map_or(0usize, |ready_set| {
                chunk_map
                    .chunks
                    .keys()
                    .filter(|coord| ready_set.0.contains(coord))
                    .count()
            });

            let mut busy = 0usize;
            if let Some(todo) = water_todo {
                busy = busy.saturating_add(todo.0.len());
            }
            if let Some(pending_load) = pending_water_load {
                busy = busy.saturating_add(pending_load.0.len());
            }
            if let Some(pending_mesh) = pending_water_mesh {
                busy = busy.saturating_add(pending_mesh.0.len().div_ceil(SEC_COUNT.max(1)));
            }
            if let Some(backlog) = water_backlog {
                busy = busy.saturating_add(backlog.0.len().div_ceil(SEC_COUNT.max(1)));
            }

            let ready_clamped = ready.min(total);
            let ratio = (ready_clamped as f32 / total as f32).clamp(0.0, 1.0);
            let effective_done = total.saturating_sub(busy.min(total.saturating_sub(ready_clamped)));
            LoadingProgressMetrics {
                phase,
                overall_pct: 72.0 + ratio * (97.0 - 72.0),
                progress_chunks: effective_done.min(total),
                total_chunks: total,
            }
        }
        LoadingPhase::Done => LoadingProgressMetrics {
            phase,
            overall_pct: 100.0,
            progress_chunks: 1,
            total_chunks: 1,
        },
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
fn reset_world_gen_ui_animation(
    mut animation: ResMut<WorldGenUiAnimation>,
    mut progress_log_state: ResMut<WorldGenProgressLogState>,
) {
    animation.displayed_pct = 0.0;
    progress_log_state.world_sequence = progress_log_state.world_sequence.saturating_add(1);
    progress_log_state.last_logged_percent = None;
    progress_log_state.last_phase = LoadingPhase::BaseGen;
    progress_log_state.phase_peak_percent = 0.0;
    progress_log_state.phase_peak_chunks = 0;
    progress_log_state.timer.reset();
}

#[inline]
fn phase_floor_percent(phase: LoadingPhase) -> f32 {
    match phase {
        LoadingPhase::BaseGen => 0.0,
        LoadingPhase::CaveGen => 72.0,
        LoadingPhase::WaterGen => 72.0,
        LoadingPhase::Done => 100.0,
    }
}

#[inline]
fn phase_cap_percent(phase: LoadingPhase) -> f32 {
    match phase {
        LoadingPhase::BaseGen => 72.0,
        LoadingPhase::CaveGen => 84.0,
        LoadingPhase::WaterGen => 97.0,
        LoadingPhase::Done => 100.0,
    }
}

/// Runs the `log_task_pool_worker_counts_on_world_start` routine for worker pool logging in the `graphic::components::world_flow` module.
fn log_task_pool_worker_counts_on_world_start() {
    let total_cores = bevy::tasks::available_parallelism();
    let gameplay_workers = ComputeTaskPool::get().thread_num();
    let chunk_workers = AsyncComputeTaskPool::get().thread_num();
    let io_workers = IoTaskPool::get().thread_num();

    info!(
        "[WORKERS] gameplay={} | chunks={} | io={} | cores={}",
        gameplay_workers, chunk_workers, io_workers, total_cores
    );
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
