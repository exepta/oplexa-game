use bevy::prelude::*;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum MultiplayerConnectionPhase {
    #[default]
    Idle,
    Connecting,
}

#[derive(Resource, Debug, Default, Clone)]
pub struct MultiplayerConnectionState {
    pub connected: bool,
    pub phase: MultiplayerConnectionPhase,
    pub active_session_url: Option<String>,
    pub server_name: Option<String>,
    pub world_name: Option<String>,
    pub world_seed: Option<i32>,
    pub spawn_translation: Option<[f32; 3]>,
    pub last_error: Option<String>,
}

impl MultiplayerConnectionState {
    pub fn uses_local_save_data(&self) -> bool {
        self.active_session_url.is_none()
    }

    pub fn clear_session(&mut self) {
        self.connected = false;
        self.phase = MultiplayerConnectionPhase::Idle;
        self.active_session_url = None;
        self.server_name = None;
        self.world_name = None;
        self.world_seed = None;
        self.spawn_translation = None;
        self.last_error = None;
    }
}
