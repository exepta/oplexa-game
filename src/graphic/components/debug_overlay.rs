fn prime_sys_stats(mut stats: ResMut<SysStats>) {
    stats.sys.refresh_cpu_all();
    stats.sys.refresh_processes(ProcessesToUpdate::All, true);
}

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

fn close_system_last_ui(
    mut root: Query<&mut Visibility, With<DebugOverlayRoot>>,
    mut overlay: ResMut<DebugOverlayState>,
) {
    overlay.show = false;
    if let Ok(mut visible) = root.single_mut() {
        *visible = Visibility::Hidden;
    }
}

fn refresh_sys_stats(
    time: Res<Time>,
    mut stats: ResMut<SysStats>,
    mut vram_state: ResMut<DebugVramState>,
) {
    stats.timer.tick(time.delta());
    if !stats.timer.just_finished() {
        return;
    }

    stats.sys.refresh_cpu_usage();
    stats.sys.refresh_processes(ProcessesToUpdate::All, true);

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
}

#[allow(clippy::too_many_arguments)]
fn sync_system_last_ui(
    build: Res<BuildInfo>,
    overlay: Res<DebugOverlayState>,
    grid: Res<DebugGridState>,
    world_inspector: Res<WorldInspectorState>,
    stats: Res<SysStats>,
    vram_state: Res<DebugVramState>,
    gpu_adapter: Option<Res<RenderAdapterInfo>>,
    world_gen_config: Res<WorldGenConfig>,
    biomes: Res<BiomeRegistry>,
    player: Query<&Transform, With<Player>>,
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
    let vram_text = vram_state
        .bytes
        .map(v_ram_utils::fmt_bytes)
        .unwrap_or_else(|| "n/a".to_string());
    let vram_backend = match (vram_state.source, vram_state.scope) {
        (Some(source), Some(scope)) => format!("{source}/{scope}"),
        (Some(source), None) => source.to_string(),
        _ => "unavailable".to_string(),
    };
    let player_pos = player.iter().next().map(|transform| transform.translation);
    let biome_name = player_pos.map(|pos| {
        let p_chunks = Vec2::new(pos.x / CX as f32, pos.z / CZ as f32);
        dominant_biome_at_p_chunks(&biomes, world_gen_config.seed, p_chunks)
            .name
            .clone()
    });

    for (css_id, mut paragraph) in &mut paragraphs {
        paragraph.text = match css_id.0.as_str() {
            ID_BUILD => format!(
                "{} v{} | Bevy {}",
                build.app_name, build.app_version, build.bevy_version
            ),
            ID_CPU_NAME => format!("CPU: {}", cpu_name),
            ID_GPU_NAME => format!("GPU: {}", gpu_name),
            ID_VRAM => format!("VRAM: {} ({})", vram_text, vram_backend),
            ID_BIOME => format!("Biome: {}", biome_name.as_deref().unwrap_or("n/a")),
            ID_GLOBAL_CPU => format!("System CPU: {:.1}%", stats.cpu_percent),
            ID_APP_CPU => format!("Game CPU: {:.1}%", app_cpu_normalized),
            ID_APP_MEM => format!("Game RAM: {:.1} MiB", bytes_to_mib(stats.app_mem_bytes)),
            ID_PLAYER_POS => {
                if let Some(pos) = player_pos {
                    format!("Player XYZ: {:.2} / {:.2} / {:.2}", pos.x, pos.y, pos.z)
                } else {
                    "Player XYZ: n/a".to_string()
                }
            }
            ID_GRID => format!(
                "Chunk Grid: {} (Y={:.1})",
                bool_label(grid.show),
                grid.plane_y
            ),
            ID_INSPECTOR => format!("World Inspector: {}", bool_label(world_inspector.0)),
            ID_OVERLAY => format!("Debug Overlay: {}", bool_label(overlay.show)),
            _ => continue,
        };
    }
}

