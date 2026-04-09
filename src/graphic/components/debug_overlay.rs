/// Runs the `prime_sys_stats` routine for prime sys stats in the `graphic::components::debug_overlay` module.
fn prime_sys_stats(mut stats: ResMut<SysStats>) {
    stats.sys.refresh_cpu_all();
    if let Ok(pid) = get_current_pid() {
        stats
            .sys
            .refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    }
}

/// Runs the `toggle_system_last_ui` routine for toggle system last ui in the `graphic::components::debug_overlay` module.
fn toggle_system_last_ui(
    keyboard: Res<ButtonInput<KeyCode>>,
    global_config: Res<GlobalConfig>,
    mut root: Query<&mut Visibility, With<DebugOverlayRoot>>,
    mut overlay: ResMut<DebugOverlayState>,
) {
    let key =
        convert(global_config.input.debug_overlay.as_str()).expect("Invalid debug overlay key");

    if !keyboard.just_pressed(key) {
        return;
    }

    overlay.show = !overlay.show;
    if let Ok(mut visible) = root.single_mut() {
        *visible = if overlay.show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

/// Runs the `close_system_last_ui` routine for close system last ui in the `graphic::components::debug_overlay` module.
fn close_system_last_ui(
    mut root: Query<&mut Visibility, With<DebugOverlayRoot>>,
    mut overlay: ResMut<DebugOverlayState>,
) {
    overlay.show = false;
    if let Ok(mut visible) = root.single_mut() {
        *visible = Visibility::Hidden;
    }
}

/// Refreshes sys stats for the `graphic::components::debug_overlay` module.
fn refresh_sys_stats(
    time: Res<Time>,
    overlay: Res<DebugOverlayState>,
    mut stats: ResMut<SysStats>,
    mut vram_state: ResMut<DebugVramState>,
    mut gpu_load_state: ResMut<DebugGpuLoadState>,
) {
    if !overlay.show {
        return;
    }
    stats.timer.tick(time.delta());
    if !stats.timer.just_finished() {
        return;
    }

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

    let vram = v_ram_utils::detect_v_ram_best_effort();
    vram_state.bytes = vram.map(|value| value.bytes);
    vram_state.source = vram.map(|value| value.source);
    vram_state.scope = vram.map(|value| value.scope);

    let gpu_load = v_ram_utils::detect_gpu_load_best_effort();
    gpu_load_state.percent = gpu_load.map(|value| value.percent);
    gpu_load_state.source = gpu_load.map(|value| value.source);
    gpu_load_state.scope = gpu_load.map(|value| value.scope);
}

/// Samples runtime FPS and update tick speed for the debug overlay.
fn sample_runtime_perf_stats(
    time: Res<Time<bevy::time::Real>>,
    local_timeline: Option<Res<LocalTimeline>>,
    mut perf: ResMut<RuntimePerfStats>,
    mut sample_state: ResMut<RuntimePerfSampleState>,
) {
    let dt = time.delta_secs().max(0.0001);
    let raw = (1.0 / dt).clamp(0.0, 10000.0);
    let alpha = 0.15;

    if perf.fps <= 0.0 {
        perf.fps = raw;
    } else {
        perf.fps += (raw - perf.fps) * alpha;
    }

    if let Some(local_timeline) = local_timeline {
        let now_secs = time.elapsed_secs_f64();
        let current_tick = local_timeline.tick();

        if let (Some(last_tick), Some(last_secs)) = (
            sample_state.last_local_tick,
            sample_state.last_sample_real_secs,
        ) {
            let elapsed = (now_secs - last_secs).max(0.001) as f32;
            let delta_ticks = (current_tick - last_tick).max(0) as f32;
            let raw_ticks = delta_ticks / elapsed;
            if perf.tick_speed <= 0.0 {
                perf.tick_speed = raw_ticks;
            } else {
                perf.tick_speed += (raw_ticks - perf.tick_speed) * alpha;
            }
        }

        sample_state.last_local_tick = Some(current_tick);
        sample_state.last_sample_real_secs = Some(now_secs);
    }
}

fn sample_chunk_debug_stats(
    overlay: Res<DebugOverlayState>,
    chunk_map: Res<ChunkMap>,
    pending_gen: Res<PendingGen>,
    pending_mesh: Res<PendingMesh>,
    mesh_backlog: Res<MeshBacklog>,
    pending_collider: Option<Res<PendingColliderBuild>>,
    collider_backlog: Option<Res<ColliderBacklog>>,
    pending_water_load: Option<Res<PendingWaterLoad>>,
    pending_water_mesh: Option<Res<PendingWaterMesh>>,
    water_backlog: Option<Res<WaterMeshBacklog>>,
    stage_telemetry: Option<Res<ChunkStageTelemetry>>,
    mut chunk_debug: ResMut<ChunkDebugStats>,
) {
    if !overlay.show {
        return;
    }
    let sub_per_chunk = SEC_COUNT.max(1);
    let pending_mesh_chunks = pending_mesh.0.len().div_ceil(sub_per_chunk);
    let mesh_backlog_chunks = mesh_backlog.0.len().div_ceil(sub_per_chunk);
    let pending_collider_chunks = pending_collider
        .as_ref()
        .map_or(0, |p| p.len().div_ceil(sub_per_chunk));
    let collider_backlog_chunks = collider_backlog
        .as_ref()
        .map_or(0, |b| b.len().div_ceil(sub_per_chunk));
    let pending_water_mesh_chunks = pending_water_mesh
        .as_ref()
        .map_or(0, |p| p.0.len().div_ceil(sub_per_chunk));
    let water_backlog_chunks = water_backlog
        .as_ref()
        .map_or(0, |b| b.0.len().div_ceil(sub_per_chunk));

    chunk_debug.loaded_chunks = chunk_map.chunks.len();
    chunk_debug.queue_chunks = pending_gen.0.len()
        + pending_mesh_chunks
        + mesh_backlog_chunks
        + pending_collider_chunks
        + collider_backlog_chunks
        + pending_water_load.as_ref().map_or(0, |p| p.0.len())
        + pending_water_mesh_chunks
        + water_backlog_chunks;
    chunk_debug.dirty_chunks = 0;
    chunk_debug.dirty_subchunks = 0;
    chunk_debug.base_gen_inflight = pending_gen.0.len();
    chunk_debug.base_mesh_inflight = pending_mesh.0.len();
    chunk_debug.base_mesh_queue = mesh_backlog.0.len();
    chunk_debug.collider_inflight = pending_collider.as_ref().map_or(0, |p| p.len());
    chunk_debug.collider_queue = collider_backlog.as_ref().map_or(0, |b| b.len());
    chunk_debug.water_gen_inflight = pending_water_load.as_ref().map_or(0, |p| p.0.len());
    chunk_debug.water_mesh_inflight = pending_water_mesh.as_ref().map_or(0, |p| p.0.len());
    chunk_debug.water_mesh_queue = water_backlog.as_ref().map_or(0, |b| b.0.len());
    chunk_debug.stage_gen_collect_ms = stage_telemetry
        .as_ref()
        .map_or(0.0, |t| t.stage_gen_collect_ms);
    chunk_debug.stage_mesh_apply_ms = stage_telemetry
        .as_ref()
        .map_or(0.0, |t| t.stage_mesh_apply_ms);
    chunk_debug.stage_collider_schedule_ms = stage_telemetry
        .as_ref()
        .map_or(0.0, |t| t.stage_collider_schedule_ms);
    chunk_debug.stage_collider_apply_ms = stage_telemetry
        .as_ref()
        .map_or(0.0, |t| t.stage_collider_apply_ms);
    chunk_debug.chunk_ready_latency_ms = stage_telemetry
        .as_ref()
        .map_or(0.0, |t| t.chunk_ready_latency_ms);
    chunk_debug.chunk_ready_latency_p95_ms = stage_telemetry
        .as_ref()
        .map_or(0.0, |t| t.chunk_ready_latency_p95_ms);
}

/// Synchronizes system last ui for the `graphic::components::debug_overlay` module.
#[allow(clippy::too_many_arguments)]
fn sync_system_last_ui(
    build: Res<BuildInfo>,
    overlay: Res<DebugOverlayState>,
    grid: Res<DebugGridState>,
    world_inspector: Res<WorldInspectorState>,
    stats: Res<SysStats>,
    perf: Res<RuntimePerfStats>,
    vram_state: Res<DebugVramState>,
    gpu_load_state: Res<DebugGpuLoadState>,
    gpu_adapter: Option<Res<RenderAdapterInfo>>,
    world_gen_config: Res<WorldGenConfig>,
    biomes: Res<BiomeRegistry>,
    selection_state: Res<SelectionState>,
    chunk_map: Res<ChunkMap>,
    block_registry: Res<BlockRegistry>,
    player: Query<(&Transform, Option<&FpsController>), With<Player>>,
    mut paragraphs: Query<(&CssID, &mut Paragraph)>,
) {
    if !overlay.show {
        return;
    }

    let cores = stats.sys.cpus().len().max(1);
    let app_cpu_normalized = stats.app_cpu_percent / cores as f32;
    let cpu_name = stats
        .sys
        .cpus()
        .first()
        .map(|cpu| cpu.brand().trim())
        .filter(|name| !name.is_empty())
        .unwrap_or("Unknown CPU");
    let gpu_name = gpu_adapter
        .as_ref()
        .map(|adapter| adapter.name.as_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Unknown GPU");
    let (vram_text, vram_backend) = if let Some(bytes) = vram_state.bytes {
        let backend = match (vram_state.source, vram_state.scope) {
            (Some(source), Some(scope)) => format!("{source}/{scope}"),
            (Some(source), None) => source.to_string(),
            _ => "unknown".to_string(),
        };
        (v_ram_utils::fmt_bytes(bytes), backend)
    } else if cfg!(target_os = "macos") {
        (
            format!("~{}", v_ram_utils::fmt_bytes(stats.app_mem_bytes)),
            "fallback/app-rss".to_string(),
        )
    } else {
        ("n/a".to_string(), "unavailable".to_string())
    };
    let (gpu_load_text, gpu_load_backend) = if let Some(percent) = gpu_load_state.percent {
        let backend = match (gpu_load_state.source, gpu_load_state.scope) {
            (Some(source), Some(scope)) => format!("{source}/{scope}"),
            (Some(source), None) => source.to_string(),
            _ => "unknown".to_string(),
        };
        (format!("{percent:.1}%"), backend)
    } else if cfg!(target_os = "macos") {
        let fps = perf.fps.max(1.0);
        let estimated_percent = (60.0 / fps * 100.0).clamp(0.0, 100.0);
        (
            format!("~{estimated_percent:.1}%"),
            "fallback/fps-estimate".to_string(),
        )
    } else {
        ("n/a".to_string(), "unavailable".to_string())
    };
    let (player_pos, player_yaw_pitch) = player
        .iter()
        .next()
        .map(|(transform, fps)| {
            let yaw_pitch = fps.map_or_else(
                || {
                    let yaw = transform.rotation.to_euler(EulerRot::YXZ).0.to_degrees();
                    (yaw, 0.0)
                },
                |fps| (fps.yaw.to_degrees(), fps.pitch.to_degrees()),
            );
            (Some(transform.translation), Some(yaw_pitch))
        })
        .unwrap_or((None, None));
    let player_chunk = player_pos.map(|pos| {
        let (c, _) = world_to_chunk_xz(
            (pos.x / VOXEL_SIZE).floor() as i32,
            (pos.z / VOXEL_SIZE).floor() as i32,
        );
        c
    });
    let biome = player_pos.map(|pos| {
        let p_chunks = Vec2::new(pos.x / CX as f32, pos.z / CZ as f32);
        dominant_biome_at_p_chunks(&biomes, world_gen_config.seed, p_chunks)
    });
    let biome_name = biome.map(|b| b.name.as_str()).unwrap_or("n/a");
    let biome_climate = biome
        .map(|b| {
            if b.climate.is_empty() {
                "n/a".to_string()
            } else {
                b.climate.join(", ")
            }
        })
        .unwrap_or_else(|| "n/a".to_string());
    let looked_block_name = selection_state
        .hit
        .map(|hit| {
            let id = crate::core::world::block::get_block_world(&chunk_map, hit.block_pos);
            if id == 0 {
                "air".to_string()
            } else {
                block_registry.name_opt(id).unwrap_or("unknown").to_string()
            }
        })
        .unwrap_or_else(|| "n/a".to_string());

    for (css_id, mut paragraph) in &mut paragraphs {
        paragraph.text = match css_id.0.as_str() {
            ID_BUILD => format!(
                "Game Version: {} v{} | Bevy {}",
                build.app_name, build.app_version, build.bevy_version
            ),
            ID_FPS => format!("FPS: {:.1}", perf.fps),
            ID_TICK_SPEED => format!("Ticks: {:.1} t/s", perf.tick_speed),
            ID_CPU_NAME => format!("CPU Name: {}", cpu_name),
            ID_APP_CPU => format!(
                "CPU Last (Game / System): {:.1}% / {:.1}%",
                app_cpu_normalized, stats.cpu_percent
            ),
            ID_GLOBAL_CPU => format!("System CPU: {:.1}%", stats.cpu_percent),
            ID_APP_MEM => format!("RAM: {:.1} MiB", bytes_to_mib(stats.app_mem_bytes)),
            ID_GPU_NAME => format!("Graphic Name: {}", gpu_name),
            ID_GPU_LOAD => format!("Graphic Last: {} ({})", gpu_load_text, gpu_load_backend),
            ID_VRAM => format!("VRAM: {} ({})", vram_text, vram_backend),
            ID_BIOME => format!("Biome Name: {}", biome_name),
            ID_BIOME_CLIMATE => format!("Biome Klima: {}", biome_climate),
            ID_LOOK_BLOCK => format!("Block Name: {}", looked_block_name),
            ID_PLAYER_POS => {
                if let (Some(pos), Some((yaw, pitch))) = (player_pos, player_yaw_pitch) {
                    format!(
                        "Location (Player): {:.2} / {:.2} / {:.2} (yaw {:.1} / pitch {:.1})",
                        pos.x, pos.y, pos.z, yaw, pitch
                    )
                } else {
                    "Location (Player): n/a".to_string()
                }
            }
            ID_CHUNK_COORD => {
                if let Some(c) = player_chunk {
                    format!("Chunk: {} / {}", c.x, c.y)
                } else {
                    "Chunk: n/a".to_string()
                }
            }
            ID_GRID => format!(
                "Chunk Grid State: {} (Y={:.1})",
                grid_mode_label(grid.mode),
                grid.plane_y
            ),
            ID_INSPECTOR => format!("World Inspector State: {}", bool_label(world_inspector.0)),
            ID_OVERLAY => format!("Debug Overlay State: {}", bool_label(overlay.show)),
            _ => continue,
        };
    }
}

fn grid_mode_label(mode: DebugGridMode) -> &'static str {
    match mode {
        DebugGridMode::Off => "Off",
        DebugGridMode::Chunks => "On",
        DebugGridMode::AllSubchunks => "All",
    }
}
