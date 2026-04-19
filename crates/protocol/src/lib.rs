pub mod network;

pub use network::{config, discovery, protocols};

pub mod core {
    pub use oplexa_core::chat;
    pub use oplexa_core::commands;
    pub use oplexa_core::entities;
    pub use oplexa_core::events;
    pub use oplexa_core::inventory;
    pub use oplexa_core::world;
}
