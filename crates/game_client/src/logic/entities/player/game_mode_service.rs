use crate::core::config::GlobalConfig;
use crate::core::entities::player::{FlightState, GameMode, GameModeState};
use crate::core::states::states::{AppState, InGameStates};
use crate::utils::key_utils::convert_input;
use bevy::prelude::*;

/// Represents change game mode handler used by the `logic::entities::player::game_mode_service` module.
pub struct ChangeGameModeHandler;

impl Plugin for ChangeGameModeHandler {
    /// Builds this component for the `logic::entities::player::game_mode_service` module.
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            change_mode.run_if(in_state(AppState::InGame(InGameStates::Game))),
        );
    }
}

/// Runs the `change_mode` routine for change mode in the `logic::entities::player::game_mode_service` module.
fn change_mode(
    mut game_mode: ResMut<GameModeState>,
    keys: Res<ButtonInput<KeyCode>>,
    game_config: Res<GlobalConfig>,
    mut fly_state: Query<&mut FlightState>,
) {
    let key = convert_input(game_config.input.toggle_game_mode.as_str()).expect("Invalid key");
    if keys.just_pressed(key) {
        game_mode.0 = match game_mode.0 {
            GameMode::Survival => GameMode::Creative,
            GameMode::Creative => GameMode::Spectator,
            GameMode::Spectator => GameMode::Survival,
        };

        let mut fly_state = fly_state.single_mut().unwrap();
        fly_state.flying = game_mode.0 == GameMode::Creative || game_mode.0 == GameMode::Spectator;
    }
}
