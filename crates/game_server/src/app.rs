use crate::{
    bootstrap::{ServerBootstrapConfig, load_bootstrap, spawn_server},
    services::{
        ServerCommandRegistry, cleanup_orphaned_players, create_server_console_input,
        flush_chunk_streaming, flush_deferred_world_edit_messages, flush_pending_chunk_saves,
        handle_auth_messages, handle_block_break_messages, handle_block_place_messages,
        handle_chat_messages, handle_chest_inventory_open_messages,
        handle_chest_inventory_persist_messages, handle_chunk_interest_messages,
        handle_client_connected, handle_client_disconnected, handle_console_commands,
        handle_drop_item_messages, handle_drop_pickup_messages, handle_inventory_sync_messages,
        handle_keepalive_messages, handle_new_client, handle_player_move_messages,
        persist_online_player_positions, poll_lan_discovery, purge_stale_players,
    },
    state::ServerState,
};
use api::core::network::{discovery::LanDiscoveryServer, protocols::ProtocolPlugin};
use bevy::app::{
    ScheduleRunnerPlugin, TaskPoolOptions, TaskPoolPlugin, TaskPoolThreadAssignmentPolicy,
};
use bevy::log::{Level, LogPlugin};
use bevy::prelude::*;
use lightyear::prelude::server::ServerPlugins;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub fn run_dedicated() {
    run_server_app(
        load_bootstrap(),
        None,
        true,
        #[cfg(feature = "integrated")]
        None,
    );
}

pub(crate) fn run_server_app(
    bootstrap: crate::bootstrap::BootstrapResult,
    shutdown_signal: Option<Arc<AtomicBool>>,
    with_console_input: bool,
    #[cfg(feature = "integrated")] integrated_server_io: Option<lightyear::crossbeam::CrossbeamIo>,
) {
    let cpu_cores = bevy::tasks::available_parallelism().max(1);
    let (gameplay_workers, chunk_workers, io_workers) = task_pool_workers_for_cores(cpu_cores);
    let mut app = App::new();
    app.set_error_handler(bevy::ecs::error::warn);
    app.add_plugins(
        MinimalPlugins
            .set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(2)))
            .set(TaskPoolPlugin {
                task_pool_options: TaskPoolOptions {
                    io: TaskPoolThreadAssignmentPolicy {
                        min_threads: io_workers,
                        max_threads: io_workers,
                        percent: (io_workers as f32 / cpu_cores as f32).clamp(0.0, 1.0),
                        on_thread_spawn: None,
                        on_thread_destroy: None,
                    },
                    async_compute: TaskPoolThreadAssignmentPolicy {
                        min_threads: chunk_workers,
                        max_threads: chunk_workers,
                        percent: (chunk_workers as f32 / cpu_cores as f32).clamp(0.0, 1.0),
                        on_thread_spawn: None,
                        on_thread_destroy: None,
                    },
                    compute: TaskPoolThreadAssignmentPolicy {
                        min_threads: gameplay_workers,
                        max_threads: gameplay_workers,
                        percent: (gameplay_workers as f32 / cpu_cores as f32).clamp(0.0, 1.0),
                        on_thread_spawn: None,
                        on_thread_destroy: None,
                    },
                    ..default()
                },
            }),
    )
        .add_plugins(LogPlugin {
            level: Level::DEBUG,
            filter: "info,oplexa_game_server=debug,api=debug,tokio_tungstenite=warn,tungstenite=warn,lightyear=warn,wgpu=error,naga=warn".to_string(),
            ..default()
        });

    info!(
        "Server task pools: gameplay={}, chunks={}, io={} (logical_cores={})",
        gameplay_workers, chunk_workers, io_workers, cpu_cores
    );

    let netcode_client_timeout_secs = bootstrap
        .runtime_config
        .client_timeout
        .clamp(1, i32::MAX as u64) as i32;

    app.add_plugins(ServerPlugins {
        tick_duration: Duration::from_millis(16),
    })
    .add_plugins(ProtocolPlugin)
    .insert_resource(ServerBootstrapConfig {
        bind_addr: bootstrap.bind_addr,
        netcode_client_timeout_secs,
        #[cfg(feature = "integrated")]
        integrated_server_io,
    })
    .insert_resource(ServerState::new(
        bootstrap.world_root,
        bootstrap.runtime_config.world_seed,
    ))
    .insert_resource(ServerCommandRegistry::default())
    .insert_resource(if with_console_input {
        create_server_console_input()
    } else {
        create_silent_server_console_input()
    })
    .insert_resource(bootstrap.runtime_config)
    .insert_resource(LanDiscoveryResource(bootstrap.discovery))
    .add_systems(Startup, spawn_server)
    .add_observer(handle_new_client)
    .add_observer(handle_client_connected)
    .add_observer(handle_client_disconnected)
    .add_systems(
        Update,
        (
            handle_auth_messages,
            handle_player_move_messages,
            handle_inventory_sync_messages,
            persist_online_player_positions,
            handle_keepalive_messages,
            handle_console_commands,
            handle_chat_messages,
            handle_chunk_interest_messages,
        ),
    )
    .add_systems(
        Update,
        (
            handle_block_break_messages,
            handle_block_place_messages,
            handle_chest_inventory_open_messages,
            handle_chest_inventory_persist_messages,
            handle_drop_item_messages,
            handle_drop_pickup_messages,
            flush_chunk_streaming,
            flush_deferred_world_edit_messages.after(flush_chunk_streaming),
            flush_pending_chunk_saves,
            purge_stale_players,
            cleanup_orphaned_players,
            poll_lan_discovery,
        ),
    );

    if let Some(shutdown_signal) = shutdown_signal {
        app.insert_resource(IntegratedShutdownSignal(shutdown_signal))
            .add_systems(Update, check_integrated_shutdown);
    }

    app.run();
}

#[derive(Resource)]
pub struct LanDiscoveryResource(pub Option<LanDiscoveryServer>);

#[derive(Resource)]
struct IntegratedShutdownSignal(pub Arc<AtomicBool>);

fn create_silent_server_console_input() -> crate::services::ServerConsoleInput {
    let (_tx, rx) = std::sync::mpsc::channel::<String>();
    crate::services::ServerConsoleInput(std::sync::Mutex::new(rx))
}

fn check_integrated_shutdown(
    shutdown_signal: Option<Res<IntegratedShutdownSignal>>,
    mut state: ResMut<ServerState>,
    mut app_exit: MessageWriter<AppExit>,
) {
    let Some(shutdown_signal) = shutdown_signal else {
        return;
    };
    if shutdown_signal.0.load(Ordering::SeqCst) {
        state.flush_pending_chunk_saves(usize::MAX);
        app_exit.write(AppExit::Success);
    }
}

fn task_pool_workers_for_cores(logical_cores: usize) -> (usize, usize, usize) {
    match logical_cores {
        0..=4 => (1, 2, 1),
        5..=8 => (2, 5, 1),
        9..=12 => (2, 8, 1),
        13..=16 => (3, 10, 2),
        17..=20 => (3, 14, 2),
        21..=24 => (4, 16, 3),
        _ => {
            let gameplay = 4 + ((logical_cores - 24) / 8);
            let io = 3 + ((logical_cores - 24) / 12);
            let reserved_main = 1usize;
            let chunks = logical_cores
                .saturating_sub(gameplay + io + reserved_main)
                .max(1);
            (gameplay.max(1), chunks, io.max(1))
        }
    }
}
