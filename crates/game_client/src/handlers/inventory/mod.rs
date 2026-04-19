use crate::core::entities::player::inventory::PlayerInventory;
use crate::core::entities::player::{GameMode, GameModeState};
use crate::core::inventory::items::{ItemId, ItemRegistry};

/// Defines the possible creative panel click result variants in the `handlers::inventory` module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreativePanelClickResult {
    Ignored,
    Added {
        item_id: ItemId,
        inserted: u16,
        requested: u16,
    },
}

/// Applies creative panel click for the `handlers::inventory` module.
pub fn apply_creative_panel_click(
    game_mode: &GameModeState,
    clicked_item_id: ItemId,
    grant_full_stack: bool,
    inventory: &mut PlayerInventory,
    item_registry: &ItemRegistry,
) -> CreativePanelClickResult {
    if !matches!(game_mode.0, GameMode::Creative) {
        return CreativePanelClickResult::Ignored;
    }
    if clicked_item_id == 0 || item_registry.def_opt(clicked_item_id).is_none() {
        return CreativePanelClickResult::Ignored;
    }

    let requested = if grant_full_stack {
        item_registry.stack_limit(clicked_item_id).max(1)
    } else {
        1
    };
    let leftover = inventory.add_item(clicked_item_id, requested, item_registry);
    let inserted = requested.saturating_sub(leftover);

    if inserted == 0 {
        CreativePanelClickResult::Ignored
    } else {
        CreativePanelClickResult::Added {
            item_id: clicked_item_id,
            inserted,
            requested,
        }
    }
}
