use crate::core::commands::{CommandSender, GameModeKind};
use serde::{Deserialize, Serialize};

/// Client to server chat payload.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientChatMessage {
    pub text: String,
}

impl ClientChatMessage {
    /// Creates a new instance for the `core::network::protocols::chat` module.
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

/// Server to client chat payload.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerChatMessage {
    pub sender: CommandSender,
    pub message: String,
}

impl ServerChatMessage {
    /// Creates a new instance for the `core::network::protocols::chat` module.
    pub fn new(sender: CommandSender, message: impl Into<String>) -> Self {
        Self {
            sender,
            message: message.into(),
        }
    }
}

/// Server to client game mode sync payload.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerGameModeChanged {
    pub player_id: u64,
    pub mode: GameModeKind,
}

impl ServerGameModeChanged {
    /// Creates a new instance for the `core::network::protocols::chat` module.
    pub fn new(player_id: u64, mode: GameModeKind) -> Self {
        Self { player_id, mode }
    }
}
