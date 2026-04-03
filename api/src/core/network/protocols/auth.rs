use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Auth {
    pub username: String,
    pub client_uuid: String,
}

impl Auth {
    pub fn new(username: impl Into<String>, client_uuid: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            client_uuid: client_uuid.into(),
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerWelcome {
    pub player_id: u64,
    pub server_name: String,
    pub motd: String,
    pub world_name: String,
    pub world_seed: i32,
    pub spawn_translation: [f32; 3],
    pub block_palette: Vec<String>,
}

impl ServerWelcome {
    pub fn new(
        player_id: u64,
        server_name: impl Into<String>,
        motd: impl Into<String>,
        world_name: impl Into<String>,
        world_seed: i32,
        spawn_translation: [f32; 3],
        block_palette: Vec<String>,
    ) -> Self {
        Self {
            player_id,
            server_name: server_name.into(),
            motd: motd.into(),
            world_name: world_name.into(),
            world_seed,
            spawn_translation,
            block_palette,
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerAuthRejected {
    pub reason: String,
}

impl ServerAuthRejected {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}
