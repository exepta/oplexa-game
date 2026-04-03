use bevy::prelude::*;

/// Defines the possible multiplayer connection phase variants in the `core::multiplayer` module.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum MultiplayerConnectionPhase {
    #[default]
    Idle,
    Connecting,
}

/// Defines the possible world data mode variants in the `core::multiplayer` module.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum WorldDataMode {
    #[default]
    Local,
    Remote,
}

/// Represents multiplayer connection state used by the `core::multiplayer` module.
#[derive(Resource, Debug, Clone)]
pub struct MultiplayerConnectionState {
    pub client_uuid: Option<String>,
    pub connected: bool,
    pub phase: MultiplayerConnectionPhase,
    pub world_data_mode: WorldDataMode,
    pub active_session_url: Option<String>,
    pub server_name: Option<String>,
    pub world_name: Option<String>,
    pub world_seed: Option<i32>,
    pub spawn_translation: Option<[f32; 3]>,
    pub known_player_names: Vec<String>,
    pub last_error: Option<String>,
}

impl Default for MultiplayerConnectionState {
    /// Runs the `default` routine for default in the `core::multiplayer` module.
    fn default() -> Self {
        Self {
            client_uuid: None,
            connected: false,
            phase: MultiplayerConnectionPhase::Idle,
            world_data_mode: WorldDataMode::Local,
            active_session_url: None,
            server_name: None,
            world_name: None,
            world_seed: None,
            spawn_translation: None,
            known_player_names: Vec::new(),
            last_error: None,
        }
    }
}

impl MultiplayerConnectionState {
    /// Builds connection state with client uuid for the `core::multiplayer` module.
    pub fn with_client_uuid(client_uuid: impl Into<String>) -> Self {
        Self {
            client_uuid: Some(client_uuid.into()),
            ..Self::default()
        }
    }

    /// Runs the `uses_local_save_data` routine for uses local save data in the `core::multiplayer` module.
    pub fn uses_local_save_data(&self) -> bool {
        self.world_data_mode == WorldDataMode::Local
    }

    /// Sets world data mode remote for the `core::multiplayer` module.
    pub fn set_world_data_mode_remote(&mut self) {
        self.world_data_mode = WorldDataMode::Remote;
    }

    /// Sets world data mode local for the `core::multiplayer` module.
    pub fn set_world_data_mode_local(&mut self) {
        self.world_data_mode = WorldDataMode::Local;
    }

    /// Clears session for the `core::multiplayer` module.
    pub fn clear_session(&mut self) {
        self.connected = false;
        self.phase = MultiplayerConnectionPhase::Idle;
        self.active_session_url = None;
        self.server_name = None;
        self.world_name = None;
        self.world_seed = None;
        self.spawn_translation = None;
        self.known_player_names.clear();
        self.last_error = None;
    }
}
