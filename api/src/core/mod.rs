pub mod chat;
pub mod commands;
pub mod config;
pub mod debug;
pub mod entities;
pub mod events;
pub mod inventory;
pub mod multiplayer;
pub mod network;
pub mod shader;
pub mod states;
pub mod ui;
pub mod world;

use crate::core::chat::ChatLog;
use crate::core::config::*;
use crate::core::debug::BlockColliderGizmoState;
use crate::core::entities::EntitiesModule;
use crate::core::events::EventModule;
use crate::core::inventory::recipe::{
    ActiveStructurePlacementState, ActiveStructureRecipeState, HandCraftedState,
    WorkTableCraftingState,
};
use crate::core::multiplayer::MultiplayerConnectionState;
use crate::core::ui::{HotbarSelectionState, UiInteractionState};
use crate::core::world::biome::registry::BiomeRegistry;
use crate::core::world::block::*;
use crate::generator::chunk::trees::registry::TreeRegistry;
use bevy::prelude::*;

/// Represents core module used by the `core` module.
pub struct CoreModule;

impl Plugin for CoreModule {
    /// Builds this component for the `core` module.
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldGenConfig>();
        app.init_resource::<CrosshairConfig>();
        app.init_resource::<BlockColliderGizmoState>();
        app.init_resource::<ChatLog>();
        app.init_resource::<SelectedBlock>();
        app.init_resource::<MiningState>();
        app.init_resource::<MiningOverlayRoot>();
        app.init_resource::<MultiplayerConnectionState>();
        app.init_resource::<UiInteractionState>();
        app.init_resource::<HotbarSelectionState>();
        app.init_resource::<BiomeRegistry>();
        app.init_resource::<TreeRegistry>();
        app.init_resource::<HandCraftedState>();
        app.init_resource::<WorkTableCraftingState>();
        app.init_resource::<ActiveStructureRecipeState>();
        app.init_resource::<ActiveStructurePlacementState>();
        app.add_plugins((EventModule, EntitiesModule));
    }
}
