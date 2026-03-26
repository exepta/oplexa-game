pub mod player;

use crate::logic::entities::player::PlayerServices;
use bevy::prelude::*;

pub struct EntitiesHandler;

impl Plugin for EntitiesHandler {
    fn build(&self, app: &mut App) {
        app.add_plugins(PlayerServices);
    }
}
