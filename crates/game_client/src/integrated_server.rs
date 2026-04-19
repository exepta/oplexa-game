use bevy::prelude::Resource;
use oplexa_game_server::{IntegratedServerConfig, IntegratedServerHandle, start_integrated_server};
use std::io;
#[cfg(feature = "integrated")]
use std::net::SocketAddr;
#[cfg(not(feature = "integrated"))]
use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;

#[derive(Resource, Default)]
pub struct IntegratedServerSession {
    handle: Option<IntegratedServerHandle>,
    #[cfg(feature = "integrated")]
    pending_client_io: Option<lightyear::crossbeam::CrossbeamIo>,
}

impl IntegratedServerSession {
    pub fn is_active(&self) -> bool {
        self.handle.is_some()
    }

    pub fn start(
        &mut self,
        world_root: PathBuf,
        world_name: String,
        world_seed: i32,
        spawn_translation: [f32; 3],
    ) -> io::Result<String> {
        self.shutdown();
        #[cfg(feature = "integrated")]
        let (client_io, server_io) = oplexa_game_server::create_integrated_io_pair();

        let handle = start_integrated_server(IntegratedServerConfig {
            bind_addr: pick_loopback_bind_addr()?,
            world_root,
            world_name,
            world_seed,
            spawn_translation,
            #[cfg(feature = "integrated")]
            server_io: Some(server_io),
        })?;

        #[cfg(feature = "integrated")]
        {
            self.pending_client_io = Some(client_io);
        }

        let session_url = handle.session_url().to_string();
        self.handle = Some(handle);
        Ok(session_url)
    }

    pub fn shutdown(&mut self) {
        #[cfg(feature = "integrated")]
        {
            self.pending_client_io = None;
        }

        if let Some(mut handle) = self.handle.take() {
            handle.shutdown();
        }
    }

    pub fn shutdown_blocking(&mut self) {
        #[cfg(feature = "integrated")]
        {
            self.pending_client_io = None;
        }

        if let Some(mut handle) = self.handle.take() {
            handle.shutdown_blocking();
        }
    }

    #[cfg(feature = "integrated")]
    pub fn take_client_io(&mut self) -> Option<lightyear::crossbeam::CrossbeamIo> {
        self.pending_client_io.take()
    }
}

#[cfg(not(feature = "integrated"))]
fn pick_loopback_bind_addr() -> io::Result<SocketAddr> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let addr = listener.local_addr()?;
    drop(listener);
    Ok(addr)
}

#[cfg(feature = "integrated")]
fn pick_loopback_bind_addr() -> io::Result<SocketAddr> {
    Ok(SocketAddr::from(([127, 0, 0, 1], 0)))
}
