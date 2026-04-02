use crate::core::inventory::items::ItemId;
use serde_json::Value;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct NamespacedKey {
    pub provider: String,
    pub key: String,
}

impl NamespacedKey {
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

    #[inline]
    pub fn localized_name(&self) -> String {
        format!("{}:{}", self.provider, self.key)
    }
}

impl fmt::Display for NamespacedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.provider, self.key)
    }
}

#[derive(Clone, Debug)]
pub struct RecipeCraftingEntry {
    pub recipe_type: NamespacedKey,
    pub data: Value,
}

#[derive(Clone, Debug)]
pub struct RecipeResultDef {
    pub item_id: ItemId,
    pub item_localized_name: String,
    pub count: u16,
}

#[derive(Clone, Debug)]
pub struct RecipeDefinition {
    pub source_path: String,
    pub recipe_kind: String,
    pub crafting: Vec<RecipeCraftingEntry>,
    pub result: RecipeResultDef,
}

#[derive(Clone, Copy, Debug)]
pub struct RecipeInputRequirement {
    pub slot_index: usize,
    pub item_id: ItemId,
    pub count: u16,
}

#[derive(Clone, Debug)]
pub struct ResolvedRecipe {
    pub source_path: String,
    pub recipe_kind: String,
    pub recipe_type: NamespacedKey,
    pub required_inputs: Vec<RecipeInputRequirement>,
    pub result: RecipeResultDef,
}
