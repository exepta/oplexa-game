use bevy::prelude::*;

#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct MultiplayerConnectionState {
    pub connected: bool,
}
