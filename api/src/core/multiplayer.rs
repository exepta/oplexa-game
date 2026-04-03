use bevy::prelude::*;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum MultiplayerConnectionPhase {
    #[default]
    Idle,
    Connecting,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum WorldDataMode {
    #[default]
    Local,
    Remote,
}

#[derive(Resource, Debug, Clone)]
pub struct MultiplayerConnectionState {
    pub connected: bool,
    pub phase: MultiplayerConnectionPhase,
    pub world_data_mode: WorldDataMode,
    pub active_session_url: Option<String>,
    pub server_name: Option<String>,
    pub world_name: Option<String>,
    pub world_seed: Option<i32>,
    pub spawn_translation: Option<[f32; 3]>,
    pub last_error: Option<String>,
}

impl Default for MultiplayerConnectionState {
    fn default() -> Self {
        Self {
            connected: false,
            phase: MultiplayerConnectionPhase::Idle,
            world_data_mode: WorldDataMode::Local,
            active_session_url: None,
            server_name: None,
            world_name: None,
            world_seed: None,
            spawn_translation: None,
            last_error: None,
        }
    }
}

impl MultiplayerConnectionState {
    pub fn uses_local_save_data(&self) -> bool {
        self.world_data_mode == WorldDataMode::Local
    }

    pub fn set_world_data_mode_remote(&mut self) {
        self.world_data_mode = WorldDataMode::Remote;
    }

    pub fn set_world_data_mode_local(&mut self) {
        self.world_data_mode = WorldDataMode::Local;
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
