pub mod modding;

pub use modding::*;
pub use oplexa_shared::utils;

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

pub mod protocol {
    pub use oplexa_protocol::{config, discovery, protocols};
}

pub mod shared {
    pub use oplexa_shared::config;
    pub use oplexa_shared::utils;
}
