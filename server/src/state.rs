use crate::models::{HostedDrop, HostedPlayer};
use multiplayer::world::NetworkWorld;
use naia_server::UserKey;
use std::collections::HashMap;

pub struct ServerRuntimeConfig {
    pub server_name: String,
    pub motd: String,
    pub max_players: usize,
}

pub struct ServerState {
    pub world: NetworkWorld,
    pub next_player_id: u64,
    pub next_drop_id: u64,
    pub pending_auth: HashMap<UserKey, String>,
    pub players: HashMap<UserKey, HostedPlayer>,
    pub drops: HashMap<u64, HostedDrop>,
}

impl ServerState {
    pub fn new() -> Self {
        Self {
            world: NetworkWorld::default(),
            next_player_id: 1,
            next_drop_id: 1,
            pending_auth: HashMap::new(),
            players: HashMap::new(),
            drops: HashMap::new(),
        }
    }
}
