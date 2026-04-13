mod bootstrap;
mod models;
mod services;
mod state;
mod types;

use crate::{
    bootstrap::{ServerBootstrapConfig, load_bootstrap, spawn_server},
    services::{
        ServerCommandRegistry, cleanup_orphaned_players, create_server_console_input,
        flush_chunk_streaming, handle_auth_messages, handle_block_break_messages,
        handle_block_place_messages, handle_chat_messages, handle_chunk_interest_messages,
        handle_client_connected, handle_client_disconnected, handle_console_commands,
        handle_drop_item_messages, handle_drop_pickup_messages, handle_inventory_sync_messages,
        handle_keepalive_messages, handle_new_client, handle_player_move_messages,
        persist_online_player_positions, poll_lan_discovery, purge_stale_players,
    },
    state::ServerState,
};
use api::core::network::{discovery::LanDiscoveryServer, protocols::ProtocolPlugin};
use bevy::app::ScheduleRunnerPlugin;
use bevy::log::{Level, LogPlugin};
use bevy::prelude::*;
use lightyear::prelude::server::ServerPlugins;
use std::time::Duration;

/// Runs the `main` routine for main in the `project` module.
fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(2))))
        .add_plugins(LogPlugin {
            level: Level::DEBUG,
            filter: "info,oplexa_game_server=debug,api=debug,tokio_tungstenite=warn,tungstenite=warn,lightyear=warn,wgpu=error,naga=warn".to_string(),
            ..default()
        });

    let bootstrap = load_bootstrap();

    app.add_plugins(ServerPlugins {
        tick_duration: Duration::from_millis(50),
    })
    .add_plugins(ProtocolPlugin)
    .insert_resource(ServerBootstrapConfig {
        bind_addr: bootstrap.bind_addr,
    })
    .insert_resource(ServerState::new(
        bootstrap.world_root,
        bootstrap.runtime_config.world_seed,
    ))
    .insert_resource(ServerCommandRegistry::default())
    .insert_resource(create_server_console_input())
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
            handle_block_break_messages,
            handle_block_place_messages,
            handle_drop_item_messages,
            handle_drop_pickup_messages,
            flush_chunk_streaming,
            purge_stale_players,
            cleanup_orphaned_players,
            poll_lan_discovery,
        ),
    )
    .run();
}

/// Represents lan discovery resource used by the `project` module.
#[derive(Resource)]
pub struct LanDiscoveryResource(pub Option<LanDiscoveryServer>);
