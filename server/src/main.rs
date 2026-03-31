mod bootstrap;
mod models;
mod services;
mod state;
mod types;

use crate::{
    bootstrap::{ServerBootstrapConfig, load_bootstrap, spawn_server},
    services::{
        flush_chunk_streaming, handle_auth_messages, handle_block_break_messages,
        handle_block_place_messages, handle_chunk_interest_messages, handle_drop_item_messages,
        handle_drop_pickup_messages, handle_keepalive_messages, handle_new_client,
        handle_client_connected, handle_client_disconnected, handle_player_move_messages,
        purge_stale_players, poll_lan_discovery,
    },
    state::ServerState,
};
use api::core::network::{discovery::LanDiscoveryServer, protocols::ProtocolPlugin};
use bevy::app::ScheduleRunnerPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use lightyear::prelude::server::ServerPlugins;
use simple_logger::SimpleLogger;
use std::time::Duration;

fn main() {
    SimpleLogger::new()
        .init()
        .expect("Logger initialization failed");

    let bootstrap = load_bootstrap();

    App::new()
        .add_plugins(
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(5))),
        )
        .add_plugins(LogPlugin::default())
        .add_plugins(ServerPlugins {
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
                handle_keepalive_messages,
                handle_chunk_interest_messages,
                handle_block_break_messages,
                handle_block_place_messages,
                handle_drop_item_messages,
                handle_drop_pickup_messages,
                flush_chunk_streaming,
                purge_stale_players,
                poll_lan_discovery,
            ),
        )
        .run();
}

#[derive(Resource)]
pub struct LanDiscoveryResource(pub Option<LanDiscoveryServer>);
