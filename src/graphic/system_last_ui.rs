use crate::core::config::GlobalConfig;
use crate::core::config::WorldGenConfig;
use crate::core::debug::{
    BuildInfo, DebugGridState, DebugOverlayState, SysStats, WorldInspectorState,
};
use crate::core::entities::player::Player;
use crate::core::states::states::{AppState, InGameStates};
use crate::core::world::biome::func::dominant_biome_at_p_chunks;
use crate::core::world::biome::registry::BiomeRegistry;
use crate::core::world::chunk_dimension::{CX, CZ};
use crate::utils::key_utils::convert;
use bevy::prelude::*;
use bevy::render::renderer::RenderAdapterInfo;
use bevy_extended_ui::html::HtmlSource;
use bevy_extended_ui::io::HtmlAsset;
use bevy_extended_ui::registry::UiRegistry;
use bevy_extended_ui::styles::CssID;
use bevy_extended_ui::widgets::Paragraph;
use sysinfo::{ProcessesToUpdate, get_current_pid};

const SYSTEM_LAST_UI_KEY: &str = "system-last";
const SYSTEM_LAST_UI_PATH: &str = "ui/html/system_last.html";

const ID_BUILD: &str = "debug-build";
const ID_CPU_NAME: &str = "debug-cpu-name";
const ID_GPU_NAME: &str = "debug-gpu-name";
const ID_BIOME: &str = "debug-biome";
const ID_GLOBAL_CPU: &str = "debug-global-cpu";
const ID_APP_CPU: &str = "debug-app-cpu";
const ID_APP_MEM: &str = "debug-app-mem";
const ID_PLAYER_POS: &str = "debug-player-pos";
const ID_GRID: &str = "debug-grid";
const ID_INSPECTOR: &str = "debug-world-inspector";
const ID_OVERLAY: &str = "debug-overlay";

pub struct SystemLastUiPlugin;

impl Plugin for SystemLastUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DebugOverlayState>()
            .init_resource::<DebugGridState>()
            .init_resource::<SysStats>()
            .add_systems(Startup, (register_system_last_ui, prime_sys_stats))
            .add_systems(
                Update,
                (
                    toggle_system_last_ui,
                    refresh_sys_stats,
                    sync_system_last_ui,
                )
                    .run_if(in_state(AppState::InGame(InGameStates::Game))),
            );
    }
}

fn register_system_last_ui(mut registry: ResMut<UiRegistry>, asset_server: Res<AssetServer>) {
    if registry.get(SYSTEM_LAST_UI_KEY).is_some() {
        return;
    }

    let handle: Handle<HtmlAsset> = asset_server.load(SYSTEM_LAST_UI_PATH);
    registry.add(
        SYSTEM_LAST_UI_KEY.to_string(),
        HtmlSource::from_handle(handle),
    );
}

fn prime_sys_stats(mut stats: ResMut<SysStats>) {
    stats.sys.refresh_cpu_all();
    stats.sys.refresh_processes(ProcessesToUpdate::All, true);
}

fn toggle_system_last_ui(
    keyboard: Res<ButtonInput<KeyCode>>,
    global_config: Res<GlobalConfig>,
    asset_server: Res<AssetServer>,
    mut registry: ResMut<UiRegistry>,
    mut overlay: ResMut<DebugOverlayState>,
) {
    let key =
        convert(global_config.input.debug_overlay.as_str()).expect("Invalid debug overlay key");

    if !keyboard.just_pressed(key) {
        return;
    }

    overlay.show = !overlay.show;
    if overlay.show {
        show_system_last_ui(&mut registry, &asset_server);
    } else {
        hide_system_last_ui(&mut registry);
    }
}

fn refresh_sys_stats(time: Res<Time>, mut stats: ResMut<SysStats>) {
    stats.timer.tick(time.delta());
    if !stats.timer.just_finished() {
        return;
    }

    stats.sys.refresh_cpu_usage();
    stats.sys.refresh_processes(ProcessesToUpdate::All, true);

    stats.cpu_percent = stats.sys.global_cpu_usage();
    if let Ok(pid) = get_current_pid() {
        if let Some((app_cpu_percent, app_mem_bytes)) = stats
            .sys
            .process(pid)
            .map(|process| (process.cpu_usage(), process.memory()))
        {
            stats.app_cpu_percent = app_cpu_percent;
            stats.app_mem_bytes = app_mem_bytes;
        }
    }
}

fn sync_system_last_ui(
    build: Res<BuildInfo>,
    overlay: Res<DebugOverlayState>,
    grid: Res<DebugGridState>,
    world_inspector: Res<WorldInspectorState>,
    stats: Res<SysStats>,
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

fn show_system_last_ui(registry: &mut UiRegistry, asset_server: &AssetServer) {
    if registry.get(SYSTEM_LAST_UI_KEY).is_none() {
        let handle: Handle<HtmlAsset> = asset_server.load(SYSTEM_LAST_UI_PATH);
        registry.add(
            SYSTEM_LAST_UI_KEY.to_string(),
            HtmlSource::from_handle(handle),
        );
    }

    registry.use_ui(SYSTEM_LAST_UI_KEY);
}

fn hide_system_last_ui(registry: &mut UiRegistry) {
    let mut clear_current = false;

    if let Some(current) = registry.current.as_mut() {
        current.retain(|name| name != SYSTEM_LAST_UI_KEY);
        clear_current = current.is_empty();
        registry.ui_update = true;
    }

    if clear_current {
        registry.current = None;
    }
}

#[inline]
fn bytes_to_mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

#[inline]
fn bool_label(value: bool) -> &'static str {
    if value { "ON" } else { "OFF" }
}
