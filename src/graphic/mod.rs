mod hardcoded_ui;

use crate::graphic::hardcoded_ui::HardcodedUiPlugin;
use bevy::prelude::*;

pub struct GraphicModule;

impl Plugin for GraphicModule {
    fn build(&self, app: &mut App) {
        app.add_plugins(HardcodedUiPlugin);
    }
}
