use naia_shared::Message;

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
    pub world_name: String,
    pub world_seed: i32,
    pub spawn_translation: [f32; 3],
}

impl ServerWelcome {
    pub fn new(
        player_id: u64,
        server_name: impl Into<String>,
        motd: impl Into<String>,
        world_name: impl Into<String>,
        world_seed: i32,
        spawn_translation: [f32; 3],
    ) -> Self {
        Self {
            player_id,
            server_name: server_name.into(),
            motd: motd.into(),
            world_name: world_name.into(),
            world_seed,
            spawn_translation,
        }
    }
}
