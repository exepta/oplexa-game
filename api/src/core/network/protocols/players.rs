use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerJoined {
    pub player_id: u64,
    pub username: String,
}

impl PlayerJoined {
    pub fn new(player_id: u64, username: impl Into<String>) -> Self {
        Self {
            player_id,
            username: username.into(),
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerLeft {
    pub player_id: u64,
}

impl PlayerLeft {
    pub fn new(player_id: u64) -> Self {
        Self { player_id }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerMove {
    pub translation: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
}

impl PlayerMove {
    pub fn new(translation: [f32; 3], yaw: f32, pitch: f32) -> Self {
        Self {
            translation,
            yaw,
            pitch,
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientKeepAlive {
    pub stamp_ms: u32,
}

impl ClientKeepAlive {
    pub fn new(stamp_ms: u32) -> Self {
        Self { stamp_ms }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerSnapshot {
    pub player_id: u64,
    pub translation: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
}

impl PlayerSnapshot {
    pub fn new(player_id: u64, translation: [f32; 3], yaw: f32, pitch: f32) -> Self {
        Self {
            player_id,
            translation,
            yaw,
            pitch,
        }
    }
}
