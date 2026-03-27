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

#[derive(Message)]
pub struct ClientBlockBreak {
    pub location: [i32; 3],
    pub drop_block_id: u16,
    pub drop_id: u64,
}

impl ClientBlockBreak {
    pub fn new(location: [i32; 3], drop_block_id: u16, drop_id: u64) -> Self {
        Self {
            location,
            drop_block_id,
            drop_id,
        }
    }
}

#[derive(Message)]
pub struct ClientBlockPlace {
    pub location: [i32; 3],
    pub block_id: u16,
}

impl ClientBlockPlace {
    pub fn new(location: [i32; 3], block_id: u16) -> Self {
        Self { location, block_id }
    }
}

#[derive(Message)]
pub struct ServerBlockBreak {
    pub player_id: u64,
    pub location: [i32; 3],
}

impl ServerBlockBreak {
    pub fn new(player_id: u64, location: [i32; 3]) -> Self {
        Self {
            player_id,
            location,
        }
    }
}

#[derive(Message)]
pub struct ServerBlockPlace {
    pub player_id: u64,
    pub location: [i32; 3],
    pub block_id: u16,
}

impl ServerBlockPlace {
    pub fn new(player_id: u64, location: [i32; 3], block_id: u16) -> Self {
        Self {
            player_id,
            location,
            block_id,
        }
    }
}

#[derive(Message)]
pub struct ServerDropSpawn {
    pub drop_id: u64,
    pub location: [i32; 3],
    pub block_id: u16,
}

impl ServerDropSpawn {
    pub fn new(drop_id: u64, location: [i32; 3], block_id: u16) -> Self {
        Self {
            drop_id,
            location,
            block_id,
        }
    }
}

#[derive(Message)]
pub struct ClientDropPickup {
    pub drop_id: u64,
}

impl ClientDropPickup {
    pub fn new(drop_id: u64) -> Self {
        Self { drop_id }
    }
}

#[derive(Message)]
pub struct ServerDropPicked {
    pub drop_id: u64,
    pub player_id: u64,
    pub block_id: u16,
}

impl ServerDropPicked {
    pub fn new(drop_id: u64, player_id: u64, block_id: u16) -> Self {
        Self {
            drop_id,
            player_id,
            block_id,
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
        .add_message::<ClientBlockBreak>()
        .add_message::<ClientBlockPlace>()
        .add_message::<ServerBlockBreak>()
        .add_message::<ServerBlockPlace>()
        .add_message::<ServerDropSpawn>()
        .add_message::<ClientDropPickup>()
        .add_message::<ServerDropPicked>()
        .build()
}
