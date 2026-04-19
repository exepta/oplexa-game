pub mod client;
mod core_module;
pub mod debug;
pub mod generator;
pub mod graphic;
pub mod handlers;
pub mod integrated_server;
pub mod logic;
pub mod multiplayer;
pub mod shader;
pub mod shader_assets;
pub mod states;
pub mod ui;

pub use oplexa_shared::utils;

pub mod core {
    pub use oplexa_core::chat;
    pub use oplexa_core::commands;
    pub mod config {
        pub use oplexa_shared::config::*;
    }
    pub use crate::core_module::CoreModule;
    pub mod debug {
        pub use crate::debug::*;
    }
    pub use oplexa_core::entities;
    pub use oplexa_core::events;
    pub use oplexa_core::inventory;
    pub mod multiplayer {
        pub use crate::multiplayer::*;
    }
    pub mod network {
        pub use oplexa_protocol::network::{config, discovery, protocols};
    }
    pub mod shader {
        pub use crate::shader_assets::*;
    }
    pub mod states {
        pub use crate::states::*;
    }
    pub mod ui {
        pub use crate::ui::*;
    }
    pub use oplexa_core::world;
}

pub fn run() {
    if let Err(error) = oplexa_shared::paths::ensure_workspace_cwd() {
        panic!("failed to set current dir to workspace root: {error}");
    }
    client::run();
}
