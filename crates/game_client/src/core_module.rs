use crate::core::chat::ChatLog;
use crate::core::config::{CrosshairConfig, WorldGenConfig};
use crate::core::debug::BlockColliderGizmoState;
use crate::core::inventory::recipe::{
    ActiveStructurePlacementState, ActiveStructureRecipeState, HandCraftedState,
    WorkTableCraftingState,
};
use crate::core::multiplayer::MultiplayerConnectionState;
use crate::core::ui::{HotbarSelectionState, UiInteractionState};
use crate::core::world::save::RegionCache;
use bevy::prelude::*;
use oplexa_core::entities::EntitiesModule;
use oplexa_core::events::EventModule;
use oplexa_core::world::biome::registry::BiomeRegistry;
use oplexa_core::world::block::{MiningOverlayRoot, MiningState, SelectedBlock};

pub struct CoreModule;

impl Plugin for CoreModule {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldGenConfig>();
        app.init_resource::<CrosshairConfig>();
        app.init_resource::<BlockColliderGizmoState>();
        app.init_resource::<ChatLog>();
        app.init_resource::<RegionCache>();
        app.init_resource::<SelectedBlock>();
        app.init_resource::<MiningState>();
        app.init_resource::<MiningOverlayRoot>();
        app.init_resource::<MultiplayerConnectionState>();
        app.init_resource::<UiInteractionState>();
        app.init_resource::<HotbarSelectionState>();
        app.init_resource::<BiomeRegistry>();
        app.init_resource::<HandCraftedState>();
        app.init_resource::<WorkTableCraftingState>();
        app.init_resource::<ActiveStructureRecipeState>();
        app.init_resource::<ActiveStructurePlacementState>();
        app.add_plugins((EventModule, EntitiesModule));
    }
}
