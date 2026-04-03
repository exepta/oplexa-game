mod hardcoded_ui;

use crate::graphic::hardcoded_ui::HardcodedUiPlugin;
use bevy::prelude::*;

/// Represents graphic module used by the `graphic` module.
pub struct GraphicModule;

impl Plugin for GraphicModule {
    /// Builds this component for the `graphic` module.
    fn build(&self, app: &mut App) {
        app.add_plugins(HardcodedUiPlugin);
    }
}
