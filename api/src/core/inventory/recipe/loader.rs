use crate::core::inventory::items::ItemRegistry;
use crate::core::inventory::recipe::registry::{RecipeRegistry, RecipeTypeRegistry};
use crate::core::inventory::recipe::types::{
    NamespacedKey, RecipeCraftingEntry, RecipeDefinition, RecipeResultTemplateDef,
};
use bevy::prelude::*;
use serde::Deserialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// Represents recipe json used by the `core::inventory::recipe::loader` module.
#[derive(Deserialize)]
struct RecipeJson {
    #[serde(default, rename = "type")]
    recipe_kind: String,
    #[serde(default)]
    crafting: Vec<RecipeCraftingEntryJson>,
    result: RecipeResultJson,
}

/// Represents recipe crafting entry json used by the `core::inventory::recipe::loader` module.
#[derive(Deserialize)]
struct RecipeCraftingEntryJson {
    #[serde(rename = "type")]
    recipe_type: String,
    #[serde(default)]
    data: Value,
}

/// Represents recipe result json used by the `core::inventory::recipe::loader` module.
#[derive(Deserialize)]
struct RecipeResultJson {
    #[serde(default)]
    item: String,
    #[serde(default)]
    slot: Option<usize>,
    #[serde(default)]
    group: String,
    #[serde(default = "default_result_count")]
    count: u16,
}

/// Loads recipe registry for the `core::inventory::recipe::loader` module.
pub fn load_recipe_registry(
    recipes_dir: &str,
    item_registry: &ItemRegistry,
    recipe_type_registry: &RecipeTypeRegistry,
) -> RecipeRegistry {
    let mut recipes = Vec::new();

    let mut paths = recipe_json_paths_recursively(Path::new(recipes_dir));
    paths.sort_unstable();

    for path in paths {
        let source_path = path.to_string_lossy().to_string();
        let Ok(raw_json) = fs::read_to_string(path.as_path()) else {
            warn!("Could not read recipe JSON '{}'", source_path);
            continue;
        };

        let Ok(recipe_json) = serde_json::from_str::<RecipeJson>(raw_json.as_str()) else {
            warn!("Invalid recipe JSON '{}'", source_path);
            continue;
        };

        let mut crafting = Vec::with_capacity(recipe_json.crafting.len());
        for raw_entry in recipe_json.crafting {
            let Some(recipe_type) = NamespacedKey::parse(raw_entry.recipe_type.as_str()) else {
                warn!(
                    "Skipping recipe entry in '{}': invalid crafting type '{}'",
                    source_path, raw_entry.recipe_type
                );
                continue;
            };
            if !recipe_type_registry.has_handler(&recipe_type) {
                debug!(
                    "Recipe '{}' references unknown recipe type '{}'; it will remain inactive until a plugin registers that type.",
                    source_path, recipe_type
                );
            }
            crafting.push(RecipeCraftingEntry {
                recipe_type,
                data: raw_entry.data,
            });
        }

        if crafting.is_empty() {
            warn!(
                "Skipping recipe '{}': no valid crafting entries after validation",
                source_path
            );
            continue;
        }

        let result_count = recipe_json.result.count.max(1);
        let result = if !recipe_json.result.item.trim().is_empty() {
            let Some(result_item_id) = item_registry.id_opt(recipe_json.result.item.as_str())
            else {
                warn!(
                    "Skipping recipe '{}': unknown result item '{}'",
                    source_path, recipe_json.result.item
                );
                continue;
            };
            RecipeResultTemplateDef::Static {
                item_id: result_item_id,
                item_localized_name: recipe_json.result.item,
                count: result_count,
            }
        } else if let Some(slot_index) = recipe_json.result.slot {
            let result_group = normalize_recipe_group(recipe_json.result.group.as_str());
            if result_group.is_empty() {
                warn!(
                    "Skipping recipe '{}': dynamic result requires non-empty `result.group`",
                    source_path
                );
                continue;
            }
            RecipeResultTemplateDef::ByGroupFromSlot {
                slot_index,
                group: result_group,
                count: result_count,
            }
        } else {
            warn!(
                "Skipping recipe '{}': result requires either `item` or `slot+group`",
                source_path
            );
            continue;
        };

        recipes.push(RecipeDefinition {
            source_path,
            recipe_kind: recipe_json.recipe_kind,
            crafting,
            result,
        });
    }

    info!("Loaded {} recipe(s) from '{}'", recipes.len(), recipes_dir);
    RecipeRegistry { recipes }
}

/// Runs the `recipe_json_paths_recursively` routine for recipe json paths recursively in the `core::inventory::recipe::loader` module.
fn recipe_json_paths_recursively(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    collect_recipe_json_paths(root, &mut paths);
    paths
}

/// Runs the `collect_recipe_json_paths` routine for collect recipe json paths in the `core::inventory::recipe::loader` module.
fn collect_recipe_json_paths(dir: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_recipe_json_paths(path.as_path(), paths);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            paths.push(path);
        }
    }
}

/// Runs the `default_result_count` routine for default result count in the `core::inventory::recipe::loader` module.
#[inline]
fn default_result_count() -> u16 {
    1
}

#[inline]
fn normalize_recipe_group(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}
