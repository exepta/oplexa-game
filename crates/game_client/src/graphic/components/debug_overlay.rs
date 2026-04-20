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
        convert_input(global_config.input.debug_overlay.as_str()).expect("Invalid debug overlay key");

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
    mut gpu_clock_state: ResMut<DebugGpuClockState>,
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
    vram_state.total_bytes = v_ram_utils::detect_v_ram_total_best_effort();
    vram_state.source = vram.map(|value| value.source);
    vram_state.scope = vram.map(|value| value.scope);

    let gpu_load = v_ram_utils::detect_gpu_load_best_effort();
    gpu_load_state.percent = gpu_load.map(|value| value.percent);
    gpu_load_state.source = gpu_load.map(|value| value.source);
    gpu_load_state.scope = gpu_load.map(|value| value.scope);

    let gpu_clock = v_ram_utils::detect_gpu_clock_best_effort();
    gpu_clock_state.hz = gpu_clock.map(|value| value.hz);
    gpu_clock_state.source = gpu_clock.map(|value| value.source);
    gpu_clock_state.scope = gpu_clock.map(|value| value.scope);
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
    perf.fps_direct = raw;

    sample_state.fps_window_secs += dt;
    sample_state.fps_window_sum += raw;
    sample_state.fps_window_count = sample_state.fps_window_count.saturating_add(1);
    if sample_state.fps_window_secs >= 1.0 {
        let count = sample_state.fps_window_count.max(1) as f32;
        perf.fps = sample_state.fps_window_sum / count;
        sample_state.fps_window_secs = 0.0;
        sample_state.fps_window_sum = 0.0;
        sample_state.fps_window_count = 0;
    } else if perf.fps <= 0.0 {
        perf.fps = raw;
    }

    let alpha = 0.15;

    sample_state.low_window_secs += dt;
    sample_state.low_window_fps_samples.push(raw);
    if sample_state.low_window_secs >= 2.0 {
        if !sample_state.low_window_fps_samples.is_empty() {
            sample_state
                .low_window_fps_samples
                .sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let low_bucket = ((sample_state.low_window_fps_samples.len() as f32) * 0.01)
                .ceil()
                .max(1.0) as usize;
            let bucket_len = low_bucket.min(sample_state.low_window_fps_samples.len());
            let low_sum: f32 = sample_state.low_window_fps_samples[..bucket_len]
                .iter()
                .copied()
                .sum();
            perf.fps_low_1p = low_sum / bucket_len as f32;
        }
        sample_state.low_window_secs = 0.0;
        sample_state.low_window_fps_samples.clear();
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

    chunk_debug.loaded_chunks = chunk_map.chunks.len();
    chunk_debug.queue_chunks = pending_gen.0.len()
        + pending_mesh_chunks
        + mesh_backlog_chunks
        + pending_collider_chunks
        + collider_backlog_chunks;
    chunk_debug.dirty_chunks = 0;
    chunk_debug.dirty_subchunks = 0;
    chunk_debug.base_gen_inflight = pending_gen.0.len();
    chunk_debug.base_mesh_inflight = pending_mesh.0.len();
    chunk_debug.base_mesh_queue = mesh_backlog.0.len();
    chunk_debug.collider_inflight = pending_collider.as_ref().map_or(0, |p| p.len());
    chunk_debug.collider_queue = collider_backlog.as_ref().map_or(0, |b| b.len());
    chunk_debug.water_gen_inflight = 0;
    chunk_debug.water_mesh_inflight = 0;
    chunk_debug.water_mesh_queue = 0;
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

#[derive(SystemParam)]
struct LookedBlockDeps<'w, 's> {
    selection_state: Res<'w, SelectionState>,
    block_registry: Res<'w, BlockRegistry>,
    language: Res<'w, ClientLanguageState>,
    q_structures:
        Query<'w, 's, &'static crate::logic::events::block_event_handler::PlacedStructureMetadata>,
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
    chunk_debug: Res<ChunkDebugStats>,
    vram_state: Res<DebugVramState>,
    gpu_load_state: Res<DebugGpuLoadState>,
    gpu_clock_state: Res<DebugGpuClockState>,
    gpu_adapter: Option<Res<RenderAdapterInfo>>,
    world_gen_config: Res<WorldGenConfig>,
    biomes: Res<BiomeRegistry>,
    looked_block_deps: LookedBlockDeps,
    player: Query<(&Transform, Option<&FpsController>), With<Player>>,
    mut paragraphs: Query<(&CssID, &mut Paragraph)>,
) {
    let LookedBlockDeps {
        selection_state,
        block_registry,
        language,
        q_structures,
    } = looked_block_deps;
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
    let vram_text = if let Some(bytes) = vram_state.bytes {
        if let Some(total_bytes) = vram_state.total_bytes {
            format!(
                "{} / {}",
                v_ram_utils::fmt_bytes(bytes),
                v_ram_utils::fmt_bytes(total_bytes)
            )
        } else {
            v_ram_utils::fmt_bytes(bytes)
        }
    } else if cfg!(target_os = "macos") {
        format!("~{}", v_ram_utils::fmt_bytes(stats.app_mem_bytes))
    } else {
        "n/a".to_string()
    };
    let gpu_load_text = if let Some(percent) = gpu_load_state.percent {
        format!("{percent:.1}%")
    } else if cfg!(any(target_os = "macos", windows)) {
        let fps = perf.fps.max(1.0);
        let estimated_percent = (60.0 / fps * 100.0).clamp(0.0, 100.0);
        format!("~{estimated_percent:.1}%")
    } else {
        "n/a".to_string()
    };
    let gpu_clock_text = if let Some(hz) = gpu_clock_state.hz {
        v_ram_utils::fmt_hz(hz)
    } else {
        "n/a".to_string()
    };
    let has_remote_stream_activity =
        chunk_debug.remote_decode_queue_peak > 0 || chunk_debug.remote_remesh_queue_peak > 0;
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
    let not_available = language.localize_name_key("KEY_UI_NOT_AVAILABLE");
    let state_on = language.localize_name_key("KEY_UI_ON");
    let state_off = language.localize_name_key("KEY_UI_OFF");
    let biome_name = biome
        .map(|b| b.name.as_str())
        .unwrap_or(not_available.as_str());
    let biome_climate = biome
        .map(|b| {
            if b.climate.is_empty() {
                not_available.clone()
            } else {
                b.climate.join(", ")
            }
        })
        .unwrap_or_else(|| not_available.clone());
    let looked_block_name = if let Some(hit) = selection_state.hit {
        let id = hit.block_id;
        if id == 0 {
            language.as_ref().localize_name_key("KEY_AIR")
        } else {
            localize_block_name_for_id(language.as_ref(), &block_registry, id)
        }
    } else if let Some(structure_hit) = selection_state.structure_hit {
        q_structures
            .get(structure_hit.entity)
            .ok()
            .map(|meta| {
                if let Some(registration) = meta.registration.as_ref() {
                    if let Some(block_id) = registration.block_id {
                        localize_block_name_for_id(language.as_ref(), &block_registry, block_id)
                    } else {
                        language
                            .as_ref()
                            .localize_name_key(registration.name.as_str())
                    }
                } else {
                    meta.recipe_name.clone()
                }
            })
            .unwrap_or_else(|| not_available.clone())
    } else {
        not_available.clone()
    };

    for (css_id, mut paragraph) in &mut paragraphs {
        paragraph.text = match css_id.0.as_str() {
            ID_BUILD => format!(
                "{}: {} v{} | Bevy {}",
                language.localize_name_key("KEY_UI_DEBUG_GAME_VERSION"),
                build.app_name,
                build.app_version,
                build.bevy_version
            ),
            ID_FPS => format!(
                "{}: {:.1} / {:.1}s",
                language.localize_name_key("KEY_UI_DEBUG_FPS"),
                perf.fps_direct,
                perf.fps
            ),
            ID_FPS_LOW => {
                if perf.fps_low_1p > 0.0 {
                    format!("Fps (Low): {:.1}", perf.fps_low_1p)
                } else {
                    "Fps (Low): n/a".to_string()
                }
            }
            ID_STREAM_DECODE_QUEUE => {
                if !has_remote_stream_activity {
                    format!("Decode Queue: {}", chunk_debug.queue_chunks)
                } else {
                    format!(
                        "Decode Queue: {} (peak {})",
                        chunk_debug.remote_decode_queue, chunk_debug.remote_decode_queue_peak
                    )
                }
            }
            ID_STREAM_REMESH_QUEUE => {
                if !has_remote_stream_activity {
                    format!(
                        "Remesh Queue: {}",
                        chunk_debug.base_mesh_inflight + chunk_debug.base_mesh_queue
                    )
                } else {
                    format!(
                        "Remesh Queue: {} (peak {})",
                        chunk_debug.remote_remesh_queue, chunk_debug.remote_remesh_queue_peak
                    )
                }
            }
            ID_TICK_SPEED => format!(
                "{}: {:.1} t/s",
                language.localize_name_key("KEY_UI_DEBUG_TICKS"),
                perf.tick_speed
            ),
            ID_CPU_NAME => format!(
                "{}: {}",
                language.localize_name_key("KEY_UI_DEBUG_CPU_NAME"),
                cpu_name
            ),
            ID_APP_CPU => format!(
                "{}: {:.1}% / {:.1}%",
                language.localize_name_key("KEY_UI_DEBUG_CPU_LAST_GAME_SYSTEM"),
                app_cpu_normalized,
                stats.cpu_percent
            ),
            ID_GLOBAL_CPU => format!(
                "{}: {:.1}%",
                language.localize_name_key("KEY_UI_DEBUG_SYSTEM_CPU"),
                stats.cpu_percent
            ),
            ID_APP_MEM => format!(
                "{}: {:.1} MiB",
                language.localize_name_key("KEY_UI_DEBUG_RAM"),
                bytes_to_mib(stats.app_mem_bytes)
            ),
            ID_GPU_NAME => format!(
                "{}: {}",
                language.localize_name_key("KEY_UI_DEBUG_GPU_NAME"),
                gpu_name
            ),
            ID_GPU_LOAD => format!("GPU (Last): {}", gpu_load_text),
            ID_GPU_CLOCK => format!("GPU (Takt): {}", gpu_clock_text),
            ID_VRAM => format!("VRAM: {}", vram_text),
            ID_BIOME => format!(
                "{}: {}",
                language.localize_name_key("KEY_UI_DEBUG_BIOME_NAME"),
                biome_name
            ),
            ID_BIOME_CLIMATE => format!(
                "{}: {}",
                language.localize_name_key("KEY_UI_DEBUG_BIOME_CLIMATE"),
                biome_climate
            ),
            ID_LOOK_BLOCK => format!(
                "{}: {}",
                language.localize_name_key("KEY_UI_DEBUG_BLOCK_NAME"),
                looked_block_name
            ),
            ID_PLAYER_POS => {
                if let (Some(pos), Some((yaw, pitch))) = (player_pos, player_yaw_pitch) {
                    format!(
                        "{}: {:.2} / {:.2} / {:.2} (yaw {:.1} / pitch {:.1})",
                        language.localize_name_key("KEY_UI_DEBUG_LOCATION_PLAYER"),
                        pos.x,
                        pos.y,
                        pos.z,
                        yaw,
                        pitch
                    )
                } else {
                    format!(
                        "{}: {}",
                        language.localize_name_key("KEY_UI_DEBUG_LOCATION_PLAYER"),
                        not_available
                    )
                }
            }
            ID_CHUNK_COORD => {
                if let Some(c) = player_chunk {
                    format!(
                        "{}: {} / {}",
                        language.localize_name_key("KEY_UI_DEBUG_CHUNK"),
                        c.x,
                        c.y
                    )
                } else {
                    format!(
                        "{}: {}",
                        language.localize_name_key("KEY_UI_DEBUG_CHUNK"),
                        not_available
                    )
                }
            }
            ID_GRID => format!(
                "{}: {} (Y={:.1})",
                language.localize_name_key("KEY_UI_DEBUG_CHUNK_GRID_STATE"),
                grid_mode_label(grid.mode, language.as_ref()),
                grid.plane_y
            ),
            ID_INSPECTOR => format!(
                "{}: {}",
                language.localize_name_key("KEY_UI_DEBUG_WORLD_INSPECTOR_STATE"),
                if world_inspector.0 {
                    state_on.as_str()
                } else {
                    state_off.as_str()
                }
            ),
            ID_OVERLAY => format!(
                "{}: {}",
                language.localize_name_key("KEY_UI_DEBUG_OVERLAY_STATE"),
                if overlay.show {
                    state_on.as_str()
                } else {
                    state_off.as_str()
                }
            ),
            _ => continue,
        };
    }
}

fn grid_mode_label(mode: DebugGridMode, language: &ClientLanguageState) -> String {
    match mode {
        DebugGridMode::Off => language.localize_name_key("KEY_UI_DEBUG_GRID_OFF"),
        DebugGridMode::Chunks => language.localize_name_key("KEY_UI_DEBUG_GRID_ON"),
        DebugGridMode::AllSubchunks => language.localize_name_key("KEY_UI_DEBUG_GRID_ALL"),
    }
}
