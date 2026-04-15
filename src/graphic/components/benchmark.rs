use bevy::camera::visibility::ViewVisibility;
use bevy::ecs::archetype::Archetypes;
use bevy::mesh::Mesh3d;
use chrono::Local;
use std::cmp::Ordering;
use std::collections::VecDeque;

const BENCHMARK_RECENT_WINDOW_SECS: f64 = 10.0;
const BENCHMARK_SPIKE_16_MS: f32 = 16.0;
const BENCHMARK_SPIKE_33_MS: f32 = 33.0;

#[derive(Resource, Debug)]
struct BenchmarkRuntime {
    active: bool,
    started_human: String,
    started_unix_secs: u64,
    started_elapsed_secs: f64,
    frame_times_ms: Vec<f32>,
    frame_count: u64,
    frame_time_sum_ms: f64,
    frame_time_min_ms: f32,
    frame_time_max_ms: f32,
    cpu_frame_time_sum_ms: f64,
    cpu_frame_time_min_ms: f32,
    cpu_frame_time_max_ms: f32,
    gpu_frame_time_estimate_sum_ms: f64,
    gpu_frame_time_estimate_samples: u64,
    gpu_frame_time_estimate_min_ms: f32,
    gpu_frame_time_estimate_max_ms: f32,
    spike_16_count: u64,
    spike_33_count: u64,
    recent_frames: VecDeque<(f64, f32)>,
    worst_frame_recent_window_ms: f32,
    process_cpu_percent_latest: f32,
    process_mem_bytes_latest: u64,
    process_mem_peak_bytes: u64,
    vram_bytes_latest: Option<u64>,
    vram_total_bytes: Option<u64>,
    vram_peak_bytes: u64,
    gpu_load_percent_latest: Option<f32>,
    gpu_clock_hz_latest: Option<u64>,
    last_entity_count: Option<usize>,
    current_entity_count: usize,
    peak_entity_count: usize,
    current_archetype_count: usize,
    peak_archetype_count: usize,
    query_iterations_total: u64,
    spawn_count_total: u64,
    despawn_count_total: u64,
    current_mesh_assets: usize,
    peak_mesh_assets: usize,
    current_texture_assets: usize,
    peak_texture_assets: usize,
    approx_draw_calls_latest: u64,
    approx_draw_calls_peak: u64,
    approx_vertices_latest: u64,
    approx_vertices_peak: u64,
    approx_triangles_latest: u64,
    approx_triangles_peak: u64,
    chunk_gen_collect_sum_ms: f64,
    chunk_mesh_apply_sum_ms: f64,
    chunk_collider_schedule_sum_ms: f64,
    chunk_collider_apply_sum_ms: f64,
    chunk_ready_latency_sum_ms: f64,
    chunk_ready_latency_p95_sum_ms: f64,
    chunk_ready_latency_latest_ms: f32,
    chunk_ready_latency_peak_ms: f32,
    chunk_ready_latency_p95_peak_ms: f32,
    chunk_samples: u64,
    cpu_name: String,
    gpu_name: String,
    os_name: String,
    desktop_manager: Option<String>,
    sys_poll_timer: Timer,
}

impl Default for BenchmarkRuntime {
    fn default() -> Self {
        Self {
            active: false,
            started_human: String::new(),
            started_unix_secs: 0,
            started_elapsed_secs: 0.0,
            frame_times_ms: Vec::new(),
            frame_count: 0,
            frame_time_sum_ms: 0.0,
            frame_time_min_ms: f32::MAX,
            frame_time_max_ms: 0.0,
            cpu_frame_time_sum_ms: 0.0,
            cpu_frame_time_min_ms: f32::MAX,
            cpu_frame_time_max_ms: 0.0,
            gpu_frame_time_estimate_sum_ms: 0.0,
            gpu_frame_time_estimate_samples: 0,
            gpu_frame_time_estimate_min_ms: f32::MAX,
            gpu_frame_time_estimate_max_ms: 0.0,
            spike_16_count: 0,
            spike_33_count: 0,
            recent_frames: VecDeque::new(),
            worst_frame_recent_window_ms: 0.0,
            process_cpu_percent_latest: 0.0,
            process_mem_bytes_latest: 0,
            process_mem_peak_bytes: 0,
            vram_bytes_latest: None,
            vram_total_bytes: None,
            vram_peak_bytes: 0,
            gpu_load_percent_latest: None,
            gpu_clock_hz_latest: None,
            last_entity_count: None,
            current_entity_count: 0,
            peak_entity_count: 0,
            current_archetype_count: 0,
            peak_archetype_count: 0,
            query_iterations_total: 0,
            spawn_count_total: 0,
            despawn_count_total: 0,
            current_mesh_assets: 0,
            peak_mesh_assets: 0,
            current_texture_assets: 0,
            peak_texture_assets: 0,
            approx_draw_calls_latest: 0,
            approx_draw_calls_peak: 0,
            approx_vertices_latest: 0,
            approx_vertices_peak: 0,
            approx_triangles_latest: 0,
            approx_triangles_peak: 0,
            chunk_gen_collect_sum_ms: 0.0,
            chunk_mesh_apply_sum_ms: 0.0,
            chunk_collider_schedule_sum_ms: 0.0,
            chunk_collider_apply_sum_ms: 0.0,
            chunk_ready_latency_sum_ms: 0.0,
            chunk_ready_latency_p95_sum_ms: 0.0,
            chunk_ready_latency_latest_ms: 0.0,
            chunk_ready_latency_peak_ms: 0.0,
            chunk_ready_latency_p95_peak_ms: 0.0,
            chunk_samples: 0,
            cpu_name: "Unknown CPU".to_string(),
            gpu_name: "Unknown GPU".to_string(),
            os_name: "Unknown OS".to_string(),
            desktop_manager: None,
            sys_poll_timer: Timer::from_seconds(0.25, TimerMode::Repeating),
        }
    }
}

impl BenchmarkRuntime {
    fn reset_for_start(&mut self) {
        self.frame_times_ms.clear();
        self.frame_count = 0;
        self.frame_time_sum_ms = 0.0;
        self.frame_time_min_ms = f32::MAX;
        self.frame_time_max_ms = 0.0;
        self.cpu_frame_time_sum_ms = 0.0;
        self.cpu_frame_time_min_ms = f32::MAX;
        self.cpu_frame_time_max_ms = 0.0;
        self.gpu_frame_time_estimate_sum_ms = 0.0;
        self.gpu_frame_time_estimate_samples = 0;
        self.gpu_frame_time_estimate_min_ms = f32::MAX;
        self.gpu_frame_time_estimate_max_ms = 0.0;
        self.spike_16_count = 0;
        self.spike_33_count = 0;
        self.recent_frames.clear();
        self.worst_frame_recent_window_ms = 0.0;
        self.process_cpu_percent_latest = 0.0;
        self.process_mem_bytes_latest = 0;
        self.process_mem_peak_bytes = 0;
        self.vram_bytes_latest = None;
        self.vram_total_bytes = None;
        self.vram_peak_bytes = 0;
        self.gpu_load_percent_latest = None;
        self.gpu_clock_hz_latest = None;
        self.last_entity_count = None;
        self.current_entity_count = 0;
        self.peak_entity_count = 0;
        self.current_archetype_count = 0;
        self.peak_archetype_count = 0;
        self.query_iterations_total = 0;
        self.spawn_count_total = 0;
        self.despawn_count_total = 0;
        self.current_mesh_assets = 0;
        self.peak_mesh_assets = 0;
        self.current_texture_assets = 0;
        self.peak_texture_assets = 0;
        self.approx_draw_calls_latest = 0;
        self.approx_draw_calls_peak = 0;
        self.approx_vertices_latest = 0;
        self.approx_vertices_peak = 0;
        self.approx_triangles_latest = 0;
        self.approx_triangles_peak = 0;
        self.chunk_gen_collect_sum_ms = 0.0;
        self.chunk_mesh_apply_sum_ms = 0.0;
        self.chunk_collider_schedule_sum_ms = 0.0;
        self.chunk_collider_apply_sum_ms = 0.0;
        self.chunk_ready_latency_sum_ms = 0.0;
        self.chunk_ready_latency_p95_sum_ms = 0.0;
        self.chunk_ready_latency_latest_ms = 0.0;
        self.chunk_ready_latency_peak_ms = 0.0;
        self.chunk_ready_latency_p95_peak_ms = 0.0;
        self.chunk_samples = 0;
        self.sys_poll_timer = Timer::from_seconds(0.25, TimerMode::Repeating);
    }
}

fn toggle_benchmark(
    keyboard: Res<ButtonInput<KeyCode>>,
    global_config: Res<GlobalConfig>,
    time: Res<Time<bevy::time::Real>>,
    mut benchmark: ResMut<BenchmarkRuntime>,
    mut stats: ResMut<SysStats>,
    mut vram_state: ResMut<DebugVramState>,
    mut gpu_load_state: ResMut<DebugGpuLoadState>,
    mut gpu_clock_state: ResMut<DebugGpuClockState>,
    gpu_adapter: Option<Res<RenderAdapterInfo>>,
    ui_interaction: Option<Res<UiInteractionState>>,
) {
    if ui_interaction
        .as_ref()
        .is_some_and(|state| state.benchmark_input_lock)
    {
        return;
    }

    let benchmark_key = convert(global_config.input.benchmark.as_str()).unwrap_or(KeyCode::KeyB);
    if !keyboard.just_pressed(benchmark_key) {
        return;
    }

    if benchmark.active {
        stop_benchmark(&mut benchmark, Some(time.elapsed_secs_f64()));
        info!("Benchmark: false");
        return;
    }

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

fn start_benchmark(
    benchmark: &mut BenchmarkRuntime,
    time: &Time<bevy::time::Real>,
    stats: &mut SysStats,
    vram_state: &mut DebugVramState,
    gpu_load_state: &mut DebugGpuLoadState,
    gpu_clock_state: &mut DebugGpuClockState,
    gpu_adapter: Option<&RenderAdapterInfo>,
) {
    if benchmark.active {
        return;
    }

    benchmark.reset_for_start();
    benchmark.active = true;
    benchmark.started_human = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    benchmark.started_unix_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    benchmark.started_elapsed_secs = time.elapsed_secs_f64();
    benchmark.os_name = detect_os_name();
    benchmark.desktop_manager = detect_desktop_manager();
    benchmark.gpu_name = gpu_adapter
        .map(|adapter| adapter.name.clone())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "Unknown GPU".to_string());

    stats.sys.refresh_cpu_all();
    benchmark.cpu_name = stats
        .sys
        .cpus()
        .first()
        .map(|cpu| cpu.brand().trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Unknown CPU".to_string());

    poll_benchmark_system_metrics(
        benchmark,
        stats,
        vram_state,
        gpu_load_state,
        gpu_clock_state,
    );
}

#[allow(clippy::too_many_arguments)]
fn sample_benchmark_runtime(
    time: Res<Time<bevy::time::Real>>,
    mut benchmark: ResMut<BenchmarkRuntime>,
    mut stats: ResMut<SysStats>,
    mut vram_state: ResMut<DebugVramState>,
    mut gpu_load_state: ResMut<DebugGpuLoadState>,
    mut gpu_clock_state: ResMut<DebugGpuClockState>,
    q_entities: Query<Entity>,
    q_meshes: Query<Option<&ViewVisibility>, With<Mesh3d>>,
    archetypes: &Archetypes,
    mesh_assets: Res<Assets<Mesh>>,
    image_assets: Res<Assets<Image>>,
    stage_telemetry: Option<Res<ChunkStageTelemetry>>,
) {
    if !benchmark.active {
        return;
    }

    benchmark.sys_poll_timer.tick(time.delta());
    if benchmark.sys_poll_timer.just_finished() || benchmark.frame_count == 0 {
        poll_benchmark_system_metrics(
            &mut benchmark,
            &mut stats,
            &mut vram_state,
            &mut gpu_load_state,
            &mut gpu_clock_state,
        );
    }

    let frame_ms = (time.delta_secs() * 1000.0).max(0.0);
    benchmark.frame_count = benchmark.frame_count.saturating_add(1);
    benchmark.frame_times_ms.push(frame_ms);
    benchmark.frame_time_sum_ms += frame_ms as f64;
    benchmark.frame_time_min_ms = benchmark.frame_time_min_ms.min(frame_ms);
    benchmark.frame_time_max_ms = benchmark.frame_time_max_ms.max(frame_ms);

    if frame_ms > BENCHMARK_SPIKE_16_MS {
        benchmark.spike_16_count = benchmark.spike_16_count.saturating_add(1);
    }
    if frame_ms > BENCHMARK_SPIKE_33_MS {
        benchmark.spike_33_count = benchmark.spike_33_count.saturating_add(1);
    }

    let now_secs = time.elapsed_secs_f64();
    benchmark.recent_frames.push_back((now_secs, frame_ms));
    while let Some((sample_secs, _)) = benchmark.recent_frames.front() {
        if now_secs - *sample_secs > BENCHMARK_RECENT_WINDOW_SECS {
            benchmark.recent_frames.pop_front();
        } else {
            break;
        }
    }
    benchmark.worst_frame_recent_window_ms = benchmark
        .recent_frames
        .iter()
        .map(|(_, ms)| *ms)
        .fold(0.0, f32::max);

    let cpu_frame_ms = frame_ms * (benchmark.process_cpu_percent_latest / 100.0).clamp(0.0, 2.0);
    benchmark.cpu_frame_time_sum_ms += cpu_frame_ms as f64;
    benchmark.cpu_frame_time_min_ms = benchmark.cpu_frame_time_min_ms.min(cpu_frame_ms);
    benchmark.cpu_frame_time_max_ms = benchmark.cpu_frame_time_max_ms.max(cpu_frame_ms);

    if let Some(gpu_load_percent) = benchmark.gpu_load_percent_latest {
        let estimated_gpu_ms = frame_ms * (gpu_load_percent / 100.0).clamp(0.0, 2.0);
        benchmark.gpu_frame_time_estimate_sum_ms += estimated_gpu_ms as f64;
        benchmark.gpu_frame_time_estimate_samples =
            benchmark.gpu_frame_time_estimate_samples.saturating_add(1);
        benchmark.gpu_frame_time_estimate_min_ms = benchmark
            .gpu_frame_time_estimate_min_ms
            .min(estimated_gpu_ms);
        benchmark.gpu_frame_time_estimate_max_ms = benchmark
            .gpu_frame_time_estimate_max_ms
            .max(estimated_gpu_ms);
    }

    let entity_count = q_entities.iter().count();
    benchmark.current_entity_count = entity_count;
    benchmark.peak_entity_count = benchmark.peak_entity_count.max(entity_count);

    if let Some(last_entity_count) = benchmark.last_entity_count {
        if entity_count > last_entity_count {
            benchmark.spawn_count_total = benchmark
                .spawn_count_total
                .saturating_add((entity_count - last_entity_count) as u64);
        } else if last_entity_count > entity_count {
            benchmark.despawn_count_total = benchmark
                .despawn_count_total
                .saturating_add((last_entity_count - entity_count) as u64);
        }
    }
    benchmark.last_entity_count = Some(entity_count);

    let archetype_count = archetypes.len();
    benchmark.current_archetype_count = archetype_count;
    benchmark.peak_archetype_count = benchmark.peak_archetype_count.max(archetype_count);

    benchmark.query_iterations_total = benchmark
        .query_iterations_total
        .saturating_add(entity_count as u64);

    benchmark.current_mesh_assets = mesh_assets.len();
    benchmark.peak_mesh_assets = benchmark.peak_mesh_assets.max(benchmark.current_mesh_assets);
    benchmark.current_texture_assets = image_assets.len();
    benchmark.peak_texture_assets = benchmark
        .peak_texture_assets
        .max(benchmark.current_texture_assets);

    let mut draw_calls = 0u64;
    let vertices = 0u64;
    let triangles = 0u64;
    for view_visibility in &q_meshes {
        if let Some(view_visibility) = view_visibility && !view_visibility.get() {
            continue;
        }

        draw_calls = draw_calls.saturating_add(1);
    }

    benchmark.approx_draw_calls_latest = draw_calls;
    benchmark.approx_draw_calls_peak = benchmark.approx_draw_calls_peak.max(draw_calls);
    benchmark.approx_vertices_latest = vertices;
    benchmark.approx_vertices_peak = benchmark.approx_vertices_peak.max(vertices);
    benchmark.approx_triangles_latest = triangles;
    benchmark.approx_triangles_peak = benchmark.approx_triangles_peak.max(triangles);

    if let Some(stage) = stage_telemetry.as_ref() {
        benchmark.chunk_samples = benchmark.chunk_samples.saturating_add(1);
        benchmark.chunk_gen_collect_sum_ms += stage.stage_gen_collect_ms as f64;
        benchmark.chunk_mesh_apply_sum_ms += stage.stage_mesh_apply_ms as f64;
        benchmark.chunk_collider_schedule_sum_ms += stage.stage_collider_schedule_ms as f64;
        benchmark.chunk_collider_apply_sum_ms += stage.stage_collider_apply_ms as f64;
        benchmark.chunk_ready_latency_sum_ms += stage.chunk_ready_latency_ms as f64;
        benchmark.chunk_ready_latency_p95_sum_ms += stage.chunk_ready_latency_p95_ms as f64;
        benchmark.chunk_ready_latency_latest_ms = stage.chunk_ready_latency_ms;
        benchmark.chunk_ready_latency_peak_ms = benchmark
            .chunk_ready_latency_peak_ms
            .max(stage.chunk_ready_latency_ms);
        benchmark.chunk_ready_latency_p95_peak_ms = benchmark
            .chunk_ready_latency_p95_peak_ms
            .max(stage.chunk_ready_latency_p95_ms);
    }
}

fn sync_benchmark_border(
    benchmark: Res<BenchmarkRuntime>,
    mut border_root: Query<&mut Visibility, With<BenchmarkBorderRoot>>,
) {
    if let Ok(mut visibility) = border_root.single_mut() {
        *visibility = if benchmark.active {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

fn force_stop_benchmark_on_game_exit(
    time: Res<Time<bevy::time::Real>>,
    mut benchmark: ResMut<BenchmarkRuntime>,
    mut border_root: Query<&mut Visibility, With<BenchmarkBorderRoot>>,
) {
    if benchmark.active {
        stop_benchmark(&mut benchmark, Some(time.elapsed_secs_f64()));
        info!("Benchmark: false");
    }

    if let Ok(mut visibility) = border_root.single_mut() {
        *visibility = Visibility::Hidden;
    }
}

fn stop_benchmark(benchmark: &mut BenchmarkRuntime, end_elapsed_secs: Option<f64>) {
    if !benchmark.active {
        return;
    }
    benchmark.active = false;

    let report = build_benchmark_report(benchmark, end_elapsed_secs);
    let file_name = format!(
        "benchmark-{}.txt",
        Local::now().format("%Y-%m-%d_%H-%M-%S")
    );
    if let Err(error) = fs::write(&file_name, report) {
        error!("Failed to write benchmark report '{}': {}", file_name, error);
    } else {
        info!("Benchmark report written: {}", file_name);
    }
}

fn build_benchmark_report(benchmark: &BenchmarkRuntime, end_elapsed_secs: Option<f64>) -> String {
    let duration_secs = end_elapsed_secs
        .map(|end| (end - benchmark.started_elapsed_secs).max(0.0))
        .unwrap_or_else(|| (benchmark.frame_time_sum_ms / 1000.0).max(0.0));
    let safe_duration_secs = duration_secs.max(0.0001);

    let frame_avg_ms = avg_ms(benchmark.frame_time_sum_ms, benchmark.frame_count);
    let cpu_time_avg_ms = avg_ms(benchmark.cpu_frame_time_sum_ms, benchmark.frame_count);
    let fps_avg = benchmark.frame_count as f64 / safe_duration_secs;
    let fps_avg_text = format!("{fps_avg:.2}");

    let frame_1p_low_ms = worst_percentile_avg_ms(&benchmark.frame_times_ms, 1.0);
    let frame_01p_low_ms = worst_percentile_avg_ms(&benchmark.frame_times_ms, 0.1);

    let gpu_estimate_avg_ms = avg_ms(
        benchmark.gpu_frame_time_estimate_sum_ms,
        benchmark.gpu_frame_time_estimate_samples,
    );

    let chunk_gen_collect_avg_ms = avg_ms(benchmark.chunk_gen_collect_sum_ms, benchmark.chunk_samples);
    let chunk_mesh_apply_avg_ms = avg_ms(benchmark.chunk_mesh_apply_sum_ms, benchmark.chunk_samples);
    let chunk_collider_schedule_avg_ms =
        avg_ms(benchmark.chunk_collider_schedule_sum_ms, benchmark.chunk_samples);
    let chunk_collider_apply_avg_ms =
        avg_ms(benchmark.chunk_collider_apply_sum_ms, benchmark.chunk_samples);
    let chunk_ready_latency_avg_ms =
        avg_ms(benchmark.chunk_ready_latency_sum_ms, benchmark.chunk_samples);
    let chunk_ready_latency_p95_avg_ms =
        avg_ms(benchmark.chunk_ready_latency_p95_sum_ms, benchmark.chunk_samples);
    let async_task_avg_ms = if benchmark.chunk_samples > 0 {
        chunk_gen_collect_avg_ms + chunk_mesh_apply_avg_ms + chunk_collider_schedule_avg_ms
            + chunk_collider_apply_avg_ms
    } else {
        0.0
    };

    let spawn_rate = benchmark.spawn_count_total as f64 / safe_duration_secs;
    let despawn_rate = benchmark.despawn_count_total as f64 / safe_duration_secs;

    let desktop_manager_text = if cfg!(target_os = "linux") {
        benchmark
            .desktop_manager
            .clone()
            .unwrap_or_else(|| "Unknown".to_string())
    } else {
        "n/a (non-linux)".to_string()
    };

    let mut text = String::new();
    text.push_str("Benchmark Report\n");
    text.push_str("================\n\n");

    text.push_str("Meta\n");
    text.push_str("----\n");
    text.push_str(&format!("Started: {}\n", benchmark.started_human));
    text.push_str(&format!("Start Unix: {}\n", benchmark.started_unix_secs));
    text.push_str(&format!("Duration: {:.3} s\n", duration_secs));
    text.push_str(&format!("Frames: {}\n", benchmark.frame_count));
    text.push_str(&format!("Average FPS: {}\n\n", fps_avg_text));

    text.push_str("System\n");
    text.push_str("------\n");
    text.push_str(&format!("OS: {}\n", benchmark.os_name));
    text.push_str(&format!("Desktop Manager: {}\n", desktop_manager_text));
    text.push_str(&format!("CPU: {}\n", benchmark.cpu_name));
    text.push_str(&format!("GPU: {}\n\n", benchmark.gpu_name));

    text.push_str("CPU\n");
    text.push_str("---\n");
    text.push_str(&format!(
        "Frame Time (ms): min {:.3} | avg {:.3} | max {:.3}\n",
        safe_min_ms(benchmark.frame_time_min_ms),
        frame_avg_ms,
        benchmark.frame_time_max_ms
    ));
    text.push_str(&format!(
        "1% Low Frame Time (ms): {}\n",
        fmt_optional_ms(frame_1p_low_ms)
    ));
    text.push_str(&format!(
        "0.1% Low Frame Time (ms): {}\n",
        fmt_optional_ms(frame_01p_low_ms)
    ));
    text.push_str(&format!(
        "CPU Time / Frame (ms, estimated): min {:.3} | avg {:.3} | max {:.3}\n",
        safe_min_ms(benchmark.cpu_frame_time_min_ms),
        cpu_time_avg_ms,
        benchmark.cpu_frame_time_max_ms
    ));
    text.push_str(&format!(
        "Process CPU (latest): {:.2}%\n\n",
        benchmark.process_cpu_percent_latest
    ));

    text.push_str("System Times (Bevy/Runtime)\n");
    text.push_str("---------------------------\n");
    text.push_str(&format!(
        "Physics (collider apply, ms): avg {:.3}\n",
        chunk_collider_apply_avg_ms
    ));
    text.push_str(&format!(
        "Rendering Prep (mesh apply, ms): avg {:.3}\n",
        chunk_mesh_apply_avg_ms
    ));
    text.push_str("AI (ms): n/a\n");
    text.push_str("UI (ms): n/a\n");
    text.push_str(&format!(
        "Chunk Gen Collect (ms): avg {:.3}\n",
        chunk_gen_collect_avg_ms
    ));
    text.push_str(&format!(
        "Collider Schedule (ms): avg {:.3}\n\n",
        chunk_collider_schedule_avg_ms
    ));

    text.push_str("GPU\n");
    text.push_str("---\n");
    text.push_str(&format!(
        "GPU Frame Time (ms, estimated from GPU load): {}\n",
        if benchmark.gpu_frame_time_estimate_samples > 0 {
            format!(
                "min {:.3} | avg {:.3} | max {:.3}",
                safe_min_ms(benchmark.gpu_frame_time_estimate_min_ms),
                gpu_estimate_avg_ms,
                benchmark.gpu_frame_time_estimate_max_ms
            )
        } else {
            "n/a".to_string()
        }
    ));
    text.push_str(&format!(
        "Draw Calls (approx): current {} | peak {}\n",
        benchmark.approx_draw_calls_latest, benchmark.approx_draw_calls_peak
    ));
    if benchmark.approx_vertices_latest > 0 || benchmark.approx_vertices_peak > 0 {
        text.push_str(&format!(
            "Vertices (approx): current {} | peak {}\n",
            benchmark.approx_vertices_latest, benchmark.approx_vertices_peak
        ));
    } else {
        text.push_str("Vertices (approx): n/a (mesh vertex data unavailable in MainWorld)\n");
    }
    if benchmark.approx_triangles_latest > 0 || benchmark.approx_triangles_peak > 0 {
        text.push_str(&format!(
            "Triangles (approx): current {} | peak {}\n",
            benchmark.approx_triangles_latest, benchmark.approx_triangles_peak
        ));
    } else {
        text.push_str("Triangles (approx): n/a (mesh index data unavailable in MainWorld)\n");
    }
    text.push_str(&format!(
        "VRAM Usage: current {} | peak {}{}\n",
        fmt_optional_bytes(benchmark.vram_bytes_latest),
        v_ram_utils::fmt_bytes(benchmark.vram_peak_bytes),
        benchmark
            .vram_total_bytes
            .map(|total| format!(" | total {}", v_ram_utils::fmt_bytes(total)))
            .unwrap_or_default()
    ));
    text.push_str("Shader / Pipeline Time: n/a\n\n");

    text.push_str("ECS (Bevy)\n");
    text.push_str("----------\n");
    text.push_str("Time per System: n/a (requires dedicated per-system instrumentation)\n");
    text.push_str(&format!(
        "Entity Count: current {} | peak {}\n",
        benchmark.current_entity_count, benchmark.peak_entity_count
    ));
    text.push_str(&format!(
        "Archetype Count: current {} | peak {}\n",
        benchmark.current_archetype_count, benchmark.peak_archetype_count
    ));
    text.push_str("Query Count: 1 (global entity query)\n");
    text.push_str(&format!(
        "Query Iterations (total): {}\n",
        benchmark.query_iterations_total
    ));
    text.push_str(&format!(
        "Spawn / Despawn: total {} / {} | rate {:.2} / {:.2} per s\n\n",
        benchmark.spawn_count_total, benchmark.despawn_count_total, spawn_rate, despawn_rate
    ));

    text.push_str("RAM\n");
    text.push_str("---\n");
    text.push_str(&format!(
        "RAM Usage (process): current {:.2} MiB | peak {:.2} MiB\n",
        bytes_to_mib(benchmark.process_mem_bytes_latest),
        bytes_to_mib(benchmark.process_mem_peak_bytes)
    ));
    text.push_str(&format!(
        "VRAM Usage: current {} | peak {}\n",
        fmt_optional_bytes(benchmark.vram_bytes_latest),
        v_ram_utils::fmt_bytes(benchmark.vram_peak_bytes)
    ));
    text.push_str("Allocations / Frame: n/a\n");
    text.push_str(&format!(
        "Peak Memory (process): {:.2} MiB\n",
        bytes_to_mib(benchmark.process_mem_peak_bytes)
    ));
    text.push_str(&format!(
        "Asset Count: Textures current {} / peak {}, Meshes current {} / peak {}\n\n",
        benchmark.current_texture_assets,
        benchmark.peak_texture_assets,
        benchmark.current_mesh_assets,
        benchmark.peak_mesh_assets
    ));

    text.push_str("IO\n");
    text.push_str("--\n");
    text.push_str("Asset Load Time: n/a\n");
    text.push_str(&format!(
        "Chunk Load Time (ms): latest {:.3} | avg {:.3} | peak {:.3}\n",
        benchmark.chunk_ready_latency_latest_ms,
        chunk_ready_latency_avg_ms,
        benchmark.chunk_ready_latency_peak_ms
    ));
    text.push_str("Disk IO: n/a\n");
    text.push_str(&format!(
        "Async Task Duration (ms, chunk stages avg sum): {:.3}\n\n",
        async_task_avg_ms
    ));

    text.push_str("Stutter\n");
    text.push_str("-------\n");
    text.push_str(&format!(
        "Spike Detection: frames >16ms = {} | >33ms = {}\n",
        benchmark.spike_16_count, benchmark.spike_33_count
    ));
    text.push_str(&format!(
        "Worst Frame (last {:.0} s): {:.3} ms\n",
        BENCHMARK_RECENT_WINDOW_SECS, benchmark.worst_frame_recent_window_ms
    ));
    text.push_str(&format!(
        "Chunk Latency P95 (ms): avg {:.3} | peak {:.3}\n",
        chunk_ready_latency_p95_avg_ms, benchmark.chunk_ready_latency_p95_peak_ms
    ));

    text
}

fn poll_benchmark_system_metrics(
    benchmark: &mut BenchmarkRuntime,
    stats: &mut SysStats,
    vram_state: &mut DebugVramState,
    gpu_load_state: &mut DebugGpuLoadState,
    gpu_clock_state: &mut DebugGpuClockState,
) {
    stats.sys.refresh_cpu_usage();
    if let Ok(pid) = get_current_pid() {
        stats
            .sys
            .refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    }

    stats.cpu_percent = stats.sys.global_cpu_usage();
    if let Ok(pid) = get_current_pid()
        && let Some((app_cpu_percent, app_mem_bytes)) = stats
            .sys
            .process(pid)
            .map(|process| (process.cpu_usage(), process.memory()))
    {
        stats.app_cpu_percent = app_cpu_percent;
        stats.app_mem_bytes = app_mem_bytes;
    }

    let cores = stats.sys.cpus().len().max(1) as f32;
    benchmark.process_cpu_percent_latest = stats.app_cpu_percent / cores;
    benchmark.process_mem_bytes_latest = stats.app_mem_bytes;
    benchmark.process_mem_peak_bytes = benchmark
        .process_mem_peak_bytes
        .max(benchmark.process_mem_bytes_latest);

    let vram = v_ram_utils::detect_v_ram_best_effort();
    vram_state.bytes = vram.map(|value| value.bytes);
    vram_state.total_bytes = v_ram_utils::detect_v_ram_total_best_effort();
    vram_state.source = vram.map(|value| value.source);
    vram_state.scope = vram.map(|value| value.scope);

    benchmark.vram_bytes_latest = vram_state.bytes;
    benchmark.vram_total_bytes = vram_state.total_bytes;
    if let Some(bytes) = benchmark.vram_bytes_latest {
        benchmark.vram_peak_bytes = benchmark.vram_peak_bytes.max(bytes);
    }

    let gpu_load = v_ram_utils::detect_gpu_load_best_effort();
    gpu_load_state.percent = gpu_load.map(|value| value.percent);
    gpu_load_state.source = gpu_load.map(|value| value.source);
    gpu_load_state.scope = gpu_load.map(|value| value.scope);
    benchmark.gpu_load_percent_latest = gpu_load_state.percent;

    let gpu_clock = v_ram_utils::detect_gpu_clock_best_effort();
    gpu_clock_state.hz = gpu_clock.map(|value| value.hz);
    gpu_clock_state.source = gpu_clock.map(|value| value.source);
    gpu_clock_state.scope = gpu_clock.map(|value| value.scope);
    benchmark.gpu_clock_hz_latest = gpu_clock_state.hz;
}

fn detect_os_name() -> String {
    let os_name = sysinfo::System::name().unwrap_or_else(|| std::env::consts::OS.to_string());
    let version = sysinfo::System::long_os_version()
        .or_else(sysinfo::System::os_version)
        .or_else(sysinfo::System::kernel_version);
    if let Some(version) = version {
        format!("{os_name} ({version})")
    } else {
        os_name
    }
}

fn detect_desktop_manager() -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }

    let candidates = [
        "XDG_CURRENT_DESKTOP",
        "XDG_SESSION_DESKTOP",
        "DESKTOP_SESSION",
        "GDMSESSION",
    ];

    for key in candidates {
        if let Ok(value) = std::env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(format!("{trimmed} ({key})"));
            }
        }
    }

    None
}

fn worst_percentile_avg_ms(frame_times_ms: &[f32], percent: f32) -> Option<f32> {
    if frame_times_ms.is_empty() {
        return None;
    }

    let mut samples = frame_times_ms.to_vec();
    samples.sort_unstable_by(|a, b| b.partial_cmp(a).unwrap_or(Ordering::Equal));

    let bucket_size = ((samples.len() as f32) * (percent / 100.0))
        .ceil()
        .max(1.0) as usize;
    let bucket_size = bucket_size.min(samples.len());
    let sum: f32 = samples[..bucket_size].iter().copied().sum();
    Some(sum / bucket_size as f32)
}

#[inline]
fn avg_ms(sum_ms: f64, count: u64) -> f32 {
    if count == 0 {
        0.0
    } else {
        (sum_ms / count as f64) as f32
    }
}

#[inline]
fn safe_min_ms(value: f32) -> f32 {
    if value == f32::MAX { 0.0 } else { value }
}

#[inline]
fn fmt_optional_ms(value: Option<f32>) -> String {
    value
        .map(|inner| format!("{inner:.3}"))
        .unwrap_or_else(|| "n/a".to_string())
}

#[inline]
fn fmt_optional_bytes(value: Option<u64>) -> String {
    value
        .map(v_ram_utils::fmt_bytes)
        .unwrap_or_else(|| "n/a".to_string())
}
