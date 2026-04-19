mod app;
mod bootstrap;
mod models;
mod services;
mod state;
mod types;
mod world_spawn;

pub use app::LanDiscoveryResource;

use crate::bootstrap::BootstrapResult;
use crate::state::ServerRuntimeConfig;
use std::io;
use std::net::SocketAddr;
#[cfg(not(feature = "integrated"))]
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub mod core {
    pub use oplexa_core::chat;
    pub use oplexa_core::commands;
    pub mod config {
        pub use oplexa_shared::config::*;
    }
    pub use oplexa_core::entities;
    pub use oplexa_core::events;
    pub use oplexa_core::inventory;
    pub mod network {
        pub use oplexa_protocol::network::{config, discovery, protocols};
    }
    pub use oplexa_core::world;
}

pub mod generator;

#[cfg(feature = "integrated")]
pub const INTEGRATED_SESSION_URL: &str = "integrated://local";

#[cfg(feature = "integrated")]
pub fn integrated_server_addr() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 35565))
}

#[cfg(feature = "integrated")]
pub fn integrated_client_addr() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 35566))
}

#[cfg(feature = "integrated")]
pub fn create_integrated_io_pair() -> (
    lightyear::crossbeam::CrossbeamIo,
    lightyear::crossbeam::CrossbeamIo,
) {
    lightyear::crossbeam::CrossbeamIo::new_pair()
}

pub struct IntegratedServerConfig {
    pub bind_addr: SocketAddr,
    pub world_root: PathBuf,
    pub world_name: String,
    pub world_seed: i32,
    pub spawn_translation: [f32; 3],
    #[cfg(feature = "integrated")]
    pub server_io: Option<lightyear::crossbeam::CrossbeamIo>,
}

pub struct IntegratedServerHandle {
    session_url: String,
    shutdown_signal: Arc<AtomicBool>,
    join_handle: Option<JoinHandle<()>>,
}

impl IntegratedServerHandle {
    pub fn session_url(&self) -> &str {
        &self.session_url
    }

    pub fn shutdown(&mut self) {
        self.shutdown_signal.store(true, Ordering::SeqCst);
        // Do not block the caller thread (UI/main thread) on server shutdown.
        // Dropping the join handle detaches the thread; the shutdown signal lets
        // the integrated server exit on its own next frame.
        let _detached = self.join_handle.take();
    }

    pub fn shutdown_blocking(&mut self) {
        self.shutdown_signal.store(true, Ordering::SeqCst);
        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
    }
}

impl Drop for IntegratedServerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn start_integrated_server(
    config: IntegratedServerConfig,
) -> io::Result<IntegratedServerHandle> {
    let IntegratedServerConfig {
        bind_addr: _configured_bind_addr,
        world_root,
        world_name,
        world_seed,
        spawn_translation,
        #[cfg(feature = "integrated")]
        server_io,
    } = config;

    #[cfg(feature = "integrated")]
    let bind_addr = integrated_server_addr();
    #[cfg(not(feature = "integrated"))]
    let bind_addr = _configured_bind_addr;
    #[cfg(feature = "integrated")]
    let session_url = INTEGRATED_SESSION_URL.to_string();
    #[cfg(not(feature = "integrated"))]
    let session_url = format!("http://127.0.0.1:{}", bind_addr.port());
    let shutdown_signal = Arc::new(AtomicBool::new(false));
    let thread_shutdown_signal = Arc::clone(&shutdown_signal);

    let bootstrap = BootstrapResult {
        discovery: None,
        runtime_config: ServerRuntimeConfig {
            server_name: format!("{} (Integrated)", world_name),
            motd: "Integrated singleplayer server".to_string(),
            max_players: 1,
            client_timeout: 60,
            world_name,
            world_seed,
            spawn_translation,
            chunk_stream_sends_per_tick_base: 22,
            chunk_stream_sends_per_tick_per_client: 8,
            chunk_stream_sends_per_tick_max: 240,
            chunk_stream_inflight_per_client: 42,
            chunk_flight_timeout_ms: 120,
            chunk_stream_gen_max_inflight: 96,
            max_stream_radius: 16,
            locate_search_radius: 512,
            dead_entity_check_interval_secs: 3,
        },
        world_root,
        bind_addr,
    };

    let join_handle = thread::Builder::new()
        .name("oplexa-integrated-server".to_string())
        .spawn(move || {
            app::run_server_app(
                bootstrap,
                Some(thread_shutdown_signal),
                false,
                #[cfg(feature = "integrated")]
                server_io,
            )
        })
        .map_err(|error| io::Error::other(error.to_string()))?;

    #[cfg(feature = "integrated")]
    wait_for_integrated_server_ready();
    #[cfg(not(feature = "integrated"))]
    wait_for_server_ready(bind_addr)?;

    Ok(IntegratedServerHandle {
        session_url,
        shutdown_signal,
        join_handle: Some(join_handle),
    })
}

#[cfg(not(feature = "integrated"))]
fn wait_for_server_ready(bind_addr: SocketAddr) -> io::Result<()> {
    let timeout = Duration::from_secs(3);
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if TcpStream::connect_timeout(&bind_addr, Duration::from_millis(50)).is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(30));
    }

    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        format!("integrated server did not start on {bind_addr} within {timeout:?}"),
    ))
}

#[cfg(feature = "integrated")]
fn wait_for_integrated_server_ready() {
    // Give the integrated server app one frame to run startup systems.
    thread::sleep(Duration::from_millis(16));
}

pub fn run_dedicated() {
    if let Err(error) = oplexa_shared::paths::ensure_workspace_cwd() {
        panic!("failed to set current dir to workspace root: {error}");
    }
    app::run_dedicated();
}
