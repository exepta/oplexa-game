use crate::core::inventory::items::ItemId;
use serde_json::Value;
use std::fmt;

/// Represents namespaced key used by the `core::inventory::recipe::types` module.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct NamespacedKey {
    pub provider: String,
    pub key: String,
}

impl NamespacedKey {
    /// Parses the requested data for the `core::inventory::recipe::types` module.
    pub fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        let (provider, key) = trimmed.split_once(':')?;
        let provider = provider.trim().to_ascii_lowercase();
        let key = key.trim().to_ascii_lowercase();
        if provider.is_empty() || key.is_empty() {
            return None;
        }
        Some(Self { provider, key })
    }

    /// Runs the `localized_name` routine for localized name in the `core::inventory::recipe::types` module.
    #[inline]
    pub fn localized_name(&self) -> String {
        format!("{}:{}", self.provider, self.key)
    }
}

impl fmt::Display for NamespacedKey {
    /// Runs the `fmt` routine for fmt in the `core::inventory::recipe::types` module.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.provider, self.key)
    }
}

/// Represents recipe crafting entry used by the `core::inventory::recipe::types` module.
#[derive(Clone, Debug)]
pub struct RecipeCraftingEntry {
    pub recipe_type: NamespacedKey,
    pub data: Value,
}

/// Represents recipe result def used by the `core::inventory::recipe::types` module.
#[derive(Clone, Debug)]
pub struct RecipeResultDef {
    pub item_id: ItemId,
    pub item_localized_name: String,
    pub count: u16,
}

/// Represents recipe result template used by loaded recipe JSON definitions.
#[derive(Clone, Debug)]
pub enum RecipeResultTemplateDef {
    Static {
        item_id: ItemId,
        item_localized_name: String,
        count: u16,
    },
    ByGroupFromSlot {
        slot_index: usize,
        group: String,
        count: u16,
    },
}

impl RecipeResultTemplateDef {
    /// Returns the configured result stack size for this template.
    #[inline]
    pub fn count(&self) -> u16 {
        match self {
            Self::Static { count, .. } | Self::ByGroupFromSlot { count, .. } => *count,
        }
    }
}

/// Represents recipe definition used by the `core::inventory::recipe::types` module.
#[derive(Clone, Debug)]
pub struct RecipeDefinition {
    pub source_path: String,
    pub recipe_kind: String,
    pub crafting: Vec<RecipeCraftingEntry>,
    pub result: RecipeResultTemplateDef,
}

/// Represents recipe input requirement used by the `core::inventory::recipe::types` module.
#[derive(Clone, Copy, Debug)]
pub struct RecipeInputRequirement {
    pub slot_index: usize,
    pub item_id: ItemId,
    pub count: u16,
}

/// Represents resolved recipe used by the `core::inventory::recipe::types` module.
#[derive(Clone, Debug)]
pub struct ResolvedRecipe {
    pub source_path: String,
    pub recipe_kind: String,
    pub recipe_type: NamespacedKey,
    pub required_inputs: Vec<RecipeInputRequirement>,
    pub result: RecipeResultDef,
}
