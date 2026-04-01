use bevy::prelude::*;

/// Numeric identifier for an item definition.
pub type ItemId = u16;

/// Reserved ID that represents an empty inventory slot or "no item".
pub const EMPTY_ITEM_ID: ItemId = 0;

/// Default stack limit used when a JSON item sets no explicit stack size.
pub const DEFAULT_ITEM_STACK_SIZE: u16 = 128;

/// Describes how an item behaves when spawned as a world drop entity.
#[derive(Clone, Debug)]
pub struct ItemWorldDropConfig {
    /// Whether the player is allowed to pick up this item from the world.
    pub pickupable: bool,
}

impl Default for ItemWorldDropConfig {
    fn default() -> Self {
        Self { pickupable: true }
    }
}

/// Runtime item definition used by inventory, UI and world-drop systems.
#[derive(Clone, Debug)]
pub struct ItemDef {
    /// Stable item key (for example: `stick` or `dirt_block`).
    pub key: String,
    /// Display name shown in UI.
    pub name: String,
    /// Maximum number of this item in one inventory slot.
    pub max_stack_size: u16,
    /// Free-form category label from JSON.
    pub category: String,
    /// Resolved item icon texture path (JSON texture or generated block preview).
    pub texture_path: String,
    /// Loaded icon texture handle.
    pub image: Handle<Image>,
    /// Material used for non-block world drop rendering.
    pub material: Handle<StandardMaterial>,
    /// Marks whether this item represents a block.
    pub block_item: bool,
    /// Marks whether right-click placement is allowed for this item.
    pub placeable: bool,
    /// Optional list of tags from JSON.
    pub tags: Vec<String>,
    /// Free-form rarity label from JSON.
    pub rarity: String,
    /// World drop behavior flags.
    pub world_drop: ItemWorldDropConfig,
}
