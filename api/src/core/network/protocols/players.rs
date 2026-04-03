use serde::{Deserialize, Serialize};

/// Represents player joined used by the `core::network::protocols::players` module.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerJoined {
    pub player_id: u64,
    pub username: String,
}

impl PlayerJoined {
    /// Creates a new instance for the `core::network::protocols::players` module.
    pub fn new(player_id: u64, username: impl Into<String>) -> Self {
        Self {
            player_id,
            username: username.into(),
        }
    }
}

/// Represents player left used by the `core::network::protocols::players` module.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerLeft {
    pub player_id: u64,
}

impl PlayerLeft {
    /// Creates a new instance for the `core::network::protocols::players` module.
    pub fn new(player_id: u64) -> Self {
        Self { player_id }
    }
}

/// Represents player move used by the `core::network::protocols::players` module.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerMove {
    pub translation: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
}

impl PlayerMove {
    /// Creates a new instance for the `core::network::protocols::players` module.
    pub fn new(translation: [f32; 3], yaw: f32, pitch: f32) -> Self {
        Self {
            translation,
            yaw,
            pitch,
        }
    }
}

/// Represents client keep alive used by the `core::network::protocols::players` module.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientKeepAlive {
    pub stamp_ms: u32,
}

impl ClientKeepAlive {
    /// Creates a new instance for the `core::network::protocols::players` module.
    pub fn new(stamp_ms: u32) -> Self {
        Self { stamp_ms }
    }
}

/// Represents player snapshot used by the `core::network::protocols::players` module.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerSnapshot {
    pub player_id: u64,
    pub translation: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
}

impl PlayerSnapshot {
    /// Creates a new instance for the `core::network::protocols::players` module.
    pub fn new(player_id: u64, translation: [f32; 3], yaw: f32, pitch: f32) -> Self {
        Self {
            player_id,
            translation,
            yaw,
            pitch,
        }
    }
}
