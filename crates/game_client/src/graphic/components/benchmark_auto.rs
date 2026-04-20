const BENCHMARK_AUTOMATION_WORLD_NAME: &str = "bench_tmp";
const BENCHMARK_AUTOMATION_SEED: i32 = 1337;
const BENCHMARK_AUTOMATION_WARMUP_SECS: f64 = 20.0;
const BENCHMARK_AUTOMATION_DURATION_SECS: f64 = 60.0;
const BENCHMARK_AUTOMATION_HEIGHT_Y: f32 = 100.0;
const BENCHMARK_AUTOMATION_SPEED_X: f32 = 18.0;

fn toggle_benchmark_menu_dialog(
    keyboard: Res<ButtonInput<KeyCode>>,
    global_config: Res<GlobalConfig>,
    mut benchmark_automation: ResMut<BenchmarkAutomationState>,
) {
    let benchmark_key = convert_input(global_config.input.benchmark.as_str()).unwrap_or(KeyCode::KeyB);
    if keyboard.just_pressed(benchmark_key) {
        benchmark_automation.dialog_open = !benchmark_automation.dialog_open;
    }
}

fn handle_benchmark_menu_dialog_buttons(
    mut commands: Commands,
    mut benchmark_automation: ResMut<BenchmarkAutomationState>,
    mut world_gen_config: ResMut<WorldGenConfig>,
    mut multiplayer_connection: ResMut<MultiplayerConnectionState>,
    mut integrated_server: ResMut<crate::integrated_server::IntegratedServerSession>,
    mut ui_interaction: ResMut<UiInteractionState>,
    mut widgets: Query<(&CssID, &mut UIWidgetState), With<Button>>,
    mut next_state: ResMut<NextState<AppState>>,
    mut world_load_deps: SinglePlayerWorldLoadDeps,
) {
    if !benchmark_automation.dialog_open {
        return;
    }

    let mut start_clicked = false;
    let mut abort_clicked = false;
    for (css_id, mut state) in &mut widgets {
        if !state.checked {
            continue;
        }
        state.checked = false;
        match css_id.0.as_str() {
            BENCHMARK_DIALOG_START_ID => start_clicked = true,
            BENCHMARK_DIALOG_ABORT_ID => abort_clicked = true,
            _ => {}
        }
    }

    if abort_clicked {
        benchmark_automation.dialog_open = false;
        return;
    }

    if !start_clicked {
        return;
    }

    let Some(world_entry) =
        prepare_benchmark_temp_world(BENCHMARK_AUTOMATION_WORLD_NAME, BENCHMARK_AUTOMATION_SEED)
    else {
        return;
    };

    multiplayer_connection.clear_session();
    ui_interaction.menu_open = false;
    ui_interaction.benchmark_input_lock = true;
    benchmark_automation.dialog_open = false;
    benchmark_automation.active_world = Some(world_entry.clone());
    benchmark_automation.session_started_elapsed_secs = None;
    benchmark_automation.measure_started_elapsed_secs = None;
    benchmark_automation.warmup_duration_secs = BENCHMARK_AUTOMATION_WARMUP_SECS;
    benchmark_automation.run_duration_secs = BENCHMARK_AUTOMATION_DURATION_SECS;
    benchmark_automation.abort_requested = false;
    benchmark_automation.cleanup_pending_world_path = None;

    let _ = load_world_and_start(
        &world_entry,
        &mut multiplayer_connection,
        &mut integrated_server,
        &mut world_gen_config,
        &mut commands,
        &mut next_state,
        &mut world_load_deps.region_cache,
        &mut world_load_deps.chunk_map,
        &mut world_load_deps.fluid_map,
        &mut world_load_deps.water_mesh_index,
    );
}

fn sync_benchmark_menu_dialog(
    benchmark_automation: Res<BenchmarkAutomationState>,
    language: Res<ClientLanguageState>,
    mut root_q: Query<&mut Visibility, With<BenchmarkMenuDialogRoot>>,
    mut dialog_text_q: Query<&mut Paragraph, With<BenchmarkMenuDialogText>>,
    mut buttons: Query<(&CssID, &mut Button)>,
) {
    if let Ok(mut visibility) = root_q.single_mut() {
        *visibility = if benchmark_automation.dialog_open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for mut paragraph in &mut dialog_text_q {
        paragraph.text = language.localize_name_key("KEY_UI_BENCHMARK_START_QUESTION");
    }

    for (css_id, mut button) in &mut buttons {
        if css_id.0 == BENCHMARK_DIALOG_START_ID {
            button.text = language.localize_name_key("KEY_UI_BENCHMARK_START");
            continue;
        }
        if css_id.0 == BENCHMARK_DIALOG_ABORT_ID {
            button.text = language.localize_name_key("KEY_UI_ABORT");
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_benchmark_automation(
    time: Res<Time<bevy::time::Real>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    global_config: Res<GlobalConfig>,
    mut benchmark_automation: ResMut<BenchmarkAutomationState>,
    mut ui_interaction: ResMut<UiInteractionState>,
    mut game_mode_state: ResMut<GameModeState>,
    mut next_state: ResMut<NextState<AppState>>,
    mut benchmark: ResMut<BenchmarkRuntime>,
    mut stats: ResMut<SysStats>,
    mut vram_state: ResMut<DebugVramState>,
    mut gpu_load_state: ResMut<DebugGpuLoadState>,
    mut gpu_clock_state: ResMut<DebugGpuClockState>,
    gpu_adapter: Option<Res<RenderAdapterInfo>>,
    mut q_player: Query<(Entity, &mut Transform, &mut FpsController, &mut FlightState), With<Player>>,
    mut q_cam: Query<(&ChildOf, &mut Transform), (With<PlayerCamera>, Without<Player>)>,
) {
    if benchmark_automation.active_world.is_none() {
        ui_interaction.benchmark_input_lock = false;
        return;
    }

    let close_key = convert_input(global_config.input.ui_close_back.as_str()).unwrap_or(KeyCode::Escape);
    if keyboard.just_pressed(close_key) {
        benchmark_automation.abort_requested = true;
    }

    let Ok((player_entity, mut player_tf, mut controller, mut flight_state)) = q_player.single_mut()
    else {
        ui_interaction.benchmark_input_lock = true;
        return;
    };

    if benchmark_automation.session_started_elapsed_secs.is_none() {
        benchmark_automation.session_started_elapsed_secs = Some(time.elapsed_secs_f64());
        benchmark_automation.measure_started_elapsed_secs = None;
        if benchmark_automation.warmup_duration_secs <= 0.0 {
            benchmark_automation.warmup_duration_secs = BENCHMARK_AUTOMATION_WARMUP_SECS;
        }
        if benchmark_automation.run_duration_secs <= 0.0 {
            benchmark_automation.run_duration_secs = BENCHMARK_AUTOMATION_DURATION_SECS;
        }
        player_tf.translation.y = BENCHMARK_AUTOMATION_HEIGHT_Y;
    }

    ui_interaction.benchmark_input_lock = true;
    game_mode_state.0 = GameMode::Creative;
    flight_state.flying = true;
    controller.yaw = 0.0;
    controller.pitch = 0.0;
    player_tf.rotation = Quat::IDENTITY;
    player_tf.translation.y = BENCHMARK_AUTOMATION_HEIGHT_Y;
    player_tf.translation.x += BENCHMARK_AUTOMATION_SPEED_X * time.delta_secs();

    for (parent, mut cam_tf) in &mut q_cam {
        if parent.parent() == player_entity {
            cam_tf.rotation = Quat::IDENTITY;
        }
    }

    let session_started = benchmark_automation
        .session_started_elapsed_secs
        .unwrap_or_else(|| time.elapsed_secs_f64());
    let session_elapsed = (time.elapsed_secs_f64() - session_started).max(0.0);
    let warmup_secs = benchmark_automation
        .warmup_duration_secs
        .max(BENCHMARK_AUTOMATION_WARMUP_SECS);
    if benchmark_automation.measure_started_elapsed_secs.is_none() && session_elapsed >= warmup_secs {
        benchmark_automation.measure_started_elapsed_secs = Some(time.elapsed_secs_f64());
        start_benchmark(
            &mut benchmark,
            &time,
            &mut stats,
            &mut vram_state,
            &mut gpu_load_state,
            &mut gpu_clock_state,
            gpu_adapter.as_deref(),
        );
        info!("Benchmark: true");
    }
    let finished = benchmark_automation
        .measure_started_elapsed_secs
        .map(|measure_started| {
            let measure_elapsed = (time.elapsed_secs_f64() - measure_started).max(0.0);
            measure_elapsed >= benchmark_automation.run_duration_secs
        })
        .unwrap_or(false);

    if !(finished || benchmark_automation.abort_requested) {
        return;
    }

    if benchmark.active {
        stop_benchmark(&mut benchmark, Some(time.elapsed_secs_f64()));
        info!("Benchmark: false");
    }

    reset_benchmark_automation_runtime(&mut benchmark_automation);
    ui_interaction.benchmark_input_lock = false;
    next_state.set(AppState::Screen(BeforeUiState::Menu));
}

fn sync_benchmark_automation_timer(
    time: Res<Time<bevy::time::Real>>,
    benchmark_automation: Res<BenchmarkAutomationState>,
    language: Res<ClientLanguageState>,
    mut root_q: Query<&mut Visibility, With<BenchmarkAutomationTimerRoot>>,
    mut text_q: Query<&mut Paragraph, With<BenchmarkAutomationTimerText>>,
) {
    let Some(session_started_secs) = benchmark_automation.session_started_elapsed_secs else {
        if let Ok(mut visibility) = root_q.single_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    };

    if let Ok(mut visibility) = root_q.single_mut() {
        *visibility = Visibility::Inherited;
    }

    let session_elapsed = (time.elapsed_secs_f64() - session_started_secs).max(0.0);
    let warmup_duration = benchmark_automation
        .warmup_duration_secs
        .max(BENCHMARK_AUTOMATION_WARMUP_SECS);
    let run_duration = benchmark_automation
        .run_duration_secs
        .max(BENCHMARK_AUTOMATION_DURATION_SECS);
    let (prefix, left, total) = if let Some(measure_started_secs) =
        benchmark_automation.measure_started_elapsed_secs
    {
        let measure_elapsed = (time.elapsed_secs_f64() - measure_started_secs).max(0.0);
        let remaining = (run_duration - measure_elapsed).max(0.0);
        (
            language.localize_name_key("KEY_UI_BENCHMARK_TIME_LEFT"),
            format_mmss(remaining),
            format_mmss(run_duration),
        )
    } else {
        let remaining = (warmup_duration - session_elapsed).max(0.0);
        (
            language.localize_name_key("KEY_UI_BENCHMARK_WARMUP"),
            format_mmss(remaining),
            format_mmss(warmup_duration),
        )
    };

    for mut paragraph in &mut text_q {
        paragraph.text = format!("{prefix}: {left} / {total}");
    }
}

fn cleanup_benchmark_temp_world_if_needed(mut benchmark_automation: ResMut<BenchmarkAutomationState>) {
    let Some(world_path) = benchmark_automation.cleanup_pending_world_path.clone() else {
        return;
    };
    let world_name = world_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    if world_name != BENCHMARK_AUTOMATION_WORLD_NAME {
        warn!(
            "Benchmark temp cleanup skipped for unexpected path: {:?}",
            world_path
        );
        benchmark_automation.cleanup_pending_world_path = None;
        return;
    }

    match fs::remove_dir_all(&world_path) {
        Ok(_) => {
            info!("Benchmark temp world deleted: {:?}", world_path);
            benchmark_automation.cleanup_pending_world_path = None;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            benchmark_automation.cleanup_pending_world_path = None;
        }
        Err(error) => {
            warn!(
                "Benchmark temp world delete failed for {:?}: {}",
                world_path, error
            );
        }
    }
}

fn reset_benchmark_automation_on_world_exit(
    mut benchmark_automation: ResMut<BenchmarkAutomationState>,
    mut ui_interaction: ResMut<UiInteractionState>,
    mut timer_root_q: Query<&mut Visibility, With<BenchmarkAutomationTimerRoot>>,
) {
    reset_benchmark_automation_runtime(&mut benchmark_automation);
    benchmark_automation.dialog_open = false;
    ui_interaction.benchmark_input_lock = false;

    if let Ok(mut visibility) = timer_root_q.single_mut() {
        *visibility = Visibility::Hidden;
    }
}

#[inline]
fn reset_benchmark_automation_runtime(benchmark_automation: &mut BenchmarkAutomationState) {
    if let Some(world) = benchmark_automation.active_world.take() {
        benchmark_automation.cleanup_pending_world_path = Some(world.path);
    }
    benchmark_automation.session_started_elapsed_secs = None;
    benchmark_automation.measure_started_elapsed_secs = None;
    benchmark_automation.abort_requested = false;
}

fn prepare_benchmark_temp_world(world_name: &str, seed: i32) -> Option<SavedWorldEntry> {
    let root = saves_root();
    if let Err(error) = fs::create_dir_all(&root) {
        warn!("Failed to create saves directory {:?}: {}", root, error);
        return None;
    }

    let world_path = root.join(world_name);
    if world_path.exists()
        && let Err(error) = fs::remove_dir_all(&world_path)
    {
        warn!(
            "Failed to reset benchmark temp world {:?}: {}",
            world_path, error
        );
        return None;
    }
    if let Err(error) = fs::create_dir_all(world_path.join("region")) {
        warn!(
            "Failed to create benchmark temp world {:?}: {}",
            world_path, error
        );
        return None;
    }

    let (anchor_x, anchor_z) = api::core::world::spawn::spawn_anchor_from_seed(seed);
    let default_spawn = [anchor_x as f32 + 0.5, 180.0, anchor_z as f32 + 0.5];
    if let Err(error) = write_world_meta(&world_path, seed, Some(default_spawn)) {
        warn!(
            "Failed to write benchmark world meta for {:?}: {}",
            world_path, error
        );
    }

    Some(SavedWorldEntry {
        folder_name: world_name.to_string(),
        seed,
        path: world_path,
    })
}

#[inline]
fn format_mmss(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let mm = total / 60;
    let ss = total % 60;
    format!("{mm:02}:{ss:02}")
}
