use serde::{Deserialize, Serialize};

/// Level tags used for system chat lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SystemMessageLevel {
    Info,
    Debug,
    Warn,
    Error,
}

impl SystemMessageLevel {
    /// Returns the display tag expected by the UI (`[INFO]`, `[DEBUG]`, ...).
    pub fn as_tag(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

/// Sender kinds that originate from world entities.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntitySender {
    /// Human controlled player.
    Player { player_id: u64, player_name: String },
    /// Reserved for future non-player entity senders.
    Other {
        entity_kind: String,
        entity_id: u64,
        display_name: String,
    },
}

impl EntitySender {
    /// Returns a display name suitable for chat labels.
    pub fn display_name(&self) -> &str {
        match self {
            Self::Player { player_name, .. } => player_name.as_str(),
            Self::Other { display_name, .. } => display_name.as_str(),
        }
    }
}

/// Sender kinds that originate from server/plugin systems.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SystemSender {
    /// Core server status and diagnostics.
    Server { level: SystemMessageLevel },
    /// Plugin specific system sender.
    Plugin {
        plugin_name: String,
        level: SystemMessageLevel,
    },
}

impl SystemSender {
    /// Returns the level associated with this system sender.
    pub fn level(&self) -> SystemMessageLevel {
        match self {
            Self::Server { level } => *level,
            Self::Plugin { level, .. } => *level,
        }
    }
}

/// Unified sender representation used by command execution and chat payloads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandSender {
    Entity(EntitySender),
    System(SystemSender),
}
