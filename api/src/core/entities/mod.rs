pub mod player;

use crate::core::entities::player::PlayerModule;
use bevy::prelude::*;

pub struct EntitiesModule;

impl Plugin for EntitiesModule {
    fn build(&self, app: &mut App) {
        app.add_plugins(PlayerModule);
    }
}
