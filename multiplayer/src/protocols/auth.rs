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
