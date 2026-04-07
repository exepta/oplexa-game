mod cross_hair_service;
mod game_mode_service;
mod init_service;
mod leaves_fx_service;
mod look_at_service;
mod water_hud_service;

use crate::logic::entities::player::cross_hair_service::CrosshairHandler;
use crate::logic::entities::player::game_mode_service::ChangeGameModeHandler;
use crate::logic::entities::player::init_service::PlayerInitialize;
use crate::logic::entities::player::leaves_fx_service::LeavesAmbientFxPlugin;
use crate::logic::entities::player::look_at_service::LookAtService;
use crate::logic::entities::player::water_hud_service::UnderwaterFxPlugin;
use bevy::prelude::*;

/// Represents player services used by the `logic::entities::player` module.
pub struct PlayerServices;

impl Plugin for PlayerServices {
    /// Builds this component for the `logic::entities::player` module.
    fn build(&self, app: &mut App) {
        app.add_plugins((
            PlayerInitialize,
            LookAtService,
            CrosshairHandler,
            UnderwaterFxPlugin,
            LeavesAmbientFxPlugin,
            ChangeGameModeHandler,
        ));
    }
}
