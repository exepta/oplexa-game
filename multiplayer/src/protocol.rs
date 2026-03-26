use naia_shared::{LinkConditionerConfig, Message, Protocol};
use std::time::Duration;

#[derive(Message)]
pub struct Auth {
    pub username: String,
}

impl Auth {
    pub fn new(username: impl Into<String>) -> Self {
        Self {
            username: username.into(),
        }
    }
}

#[derive(Message)]
pub struct ServerWelcome {
    pub player_id: u64,
    pub server_name: String,
    pub motd: String,
}

impl ServerWelcome {
    pub fn new(player_id: u64, server_name: impl Into<String>, motd: impl Into<String>) -> Self {
        Self {
            player_id,
            server_name: server_name.into(),
            motd: motd.into(),
        }
    }
}

#[derive(Message)]
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

#[derive(Message)]
pub struct PlayerLeft {
    pub player_id: u64,
}

impl PlayerLeft {
    pub fn new(player_id: u64) -> Self {
        Self { player_id }
    }
}

#[derive(Message)]
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

#[derive(Message)]
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

pub fn protocol() -> Protocol {
    Protocol::builder()
        .tick_interval(Duration::from_millis(50))
        .link_condition(LinkConditionerConfig::good_condition())
        .add_default_channels()
        .add_message::<Auth>()
        .add_message::<ServerWelcome>()
        .add_message::<PlayerJoined>()
        .add_message::<PlayerLeft>()
        .add_message::<PlayerMove>()
        .add_message::<PlayerSnapshot>()
        .build()
}
