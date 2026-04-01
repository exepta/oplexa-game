use crate::core::inventory::items::types::{
    DEFAULT_ITEM_STACK_SIZE, EMPTY_ITEM_ID, ItemDef, ItemId, ItemWorldDropConfig,
};
use crate::core::world::block::{BlockId, BlockRegistry};
use bevy::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Registry that stores all item definitions plus block↔item relations.
#[derive(Resource, Clone, Debug)]
pub struct ItemRegistry {
    /// Indexed item definitions (`index == ItemId`).
    pub defs: Vec<ItemDef>,
    /// Maps item key to numeric item id.
    pub key_to_id: HashMap<String, ItemId>,
    /// Maps block id to the corresponding item id.
    pub block_to_item: Vec<Option<ItemId>>,
    /// Maps item id to a placeable block id when this is a block-item.
    pub item_to_block: Vec<Option<BlockId>>,
}

impl ItemRegistry {
    /// Returns the item definition for a known item id.
    #[inline]
    pub fn def(&self, id: ItemId) -> &ItemDef {
        &self.defs[id as usize]
    }

    /// Returns an optional item definition.
    #[inline]
    pub fn def_opt(&self, id: ItemId) -> Option<&ItemDef> {
        self.defs.get(id as usize)
    }

    /// Returns the item display name for a known item id.
    #[inline]
    pub fn name(&self, id: ItemId) -> &str {
        self.def(id).name.as_str()
    }

    /// Returns the item display name when the id is valid.
    #[inline]
    pub fn name_opt(&self, id: ItemId) -> Option<&str> {
        self.def_opt(id).map(|item| item.name.as_str())
    }

    /// Returns the numeric id for a key, if present.
    #[inline]
    pub fn id_opt(&self, key: &str) -> Option<ItemId> {
        self.key_to_id.get(key).copied()
    }

    /// Returns `EMPTY_ITEM_ID` when the key is unknown.
    #[inline]
    pub fn id_or_empty(&self, key: &str) -> ItemId {
        self.id_opt(key).unwrap_or(EMPTY_ITEM_ID)
    }

    /// Returns true when the item id represents no item.
    #[inline]
    pub fn is_empty(&self, item_id: ItemId) -> bool {
        item_id == EMPTY_ITEM_ID
    }

    /// Returns the effective stack limit for an item id.
    #[inline]
    pub fn stack_limit(&self, item_id: ItemId) -> u16 {
        self.def_opt(item_id)
            .map(|item| item.max_stack_size.max(1))
            .unwrap_or(DEFAULT_ITEM_STACK_SIZE)
    }

    /// Returns whether an item can be picked up as a world entity.
    #[inline]
    pub fn is_pickupable(&self, item_id: ItemId) -> bool {
        self.def_opt(item_id)
            .map(|item| item.world_drop.pickupable)
            .unwrap_or(false)
    }

    /// Resolves the item id that should drop for a given block id.
    #[inline]
    pub fn item_for_block(&self, block_id: BlockId) -> Option<ItemId> {
        self.block_to_item
            .get(block_id as usize)
            .and_then(|entry| *entry)
    }

    /// Resolves the placeable block id for a given item id.
    #[inline]
    pub fn block_for_item(&self, item_id: ItemId) -> Option<BlockId> {
        self.item_to_block
            .get(item_id as usize)
            .and_then(|entry| *entry)
    }

    /// Returns true if the item id maps to a placeable block.
    #[inline]
    pub fn is_placeable_block_item(&self, item_id: ItemId) -> bool {
        self.block_for_item(item_id).is_some()
            && self
                .def_opt(item_id)
                .map(|item| item.placeable)
                .unwrap_or(false)
    }

    /// Resolves the icon asset path for UI icon widgets.
    pub fn icon_path(&self, asset_server: &AssetServer, item_id: ItemId) -> Option<String> {
        let item = self.def_opt(item_id)?;
        let path = asset_server.get_path(item.image.id())?;
        Some(path.path().to_string_lossy().to_string())
    }

    /// Loads all JSON item definitions and automatically registers all blocks as items.
    pub fn load_all(
        asset_server: &AssetServer,
        materials: &mut Assets<StandardMaterial>,
        items_dir: &str,
        block_registry: &BlockRegistry,
    ) -> Self {
        let mut registry = Self::new_with_empty(block_registry);

        for path in item_json_paths(items_dir) {
            let item_json: ItemJson = read_json(path.to_str().unwrap_or_default());
            registry.insert_json_item(item_json, asset_server, materials, block_registry);
        }

        registry.ensure_block_items(asset_server, block_registry);
        registry
    }

    /// Headless variant used where rendering assets are not required.
    pub fn load_headless(items_dir: &str, block_registry: &BlockRegistry) -> Self {
        let mut registry = Self::new_with_empty(block_registry);
        for path in item_json_paths(items_dir) {
            let item_json: ItemJson = read_json(path.to_str().unwrap_or_default());
            registry.insert_json_item_headless(item_json, block_registry);
        }
        registry.ensure_block_items_headless(block_registry);
        registry
    }

    fn new_with_empty(block_registry: &BlockRegistry) -> Self {
        let mut defs = Vec::with_capacity(64);
        let mut key_to_id = HashMap::with_capacity(64);
        let mut item_to_block = Vec::with_capacity(64);

        defs.push(ItemDef {
            key: "empty".to_string(),
            name: "Empty".to_string(),
            max_stack_size: DEFAULT_ITEM_STACK_SIZE,
            category: "internal".to_string(),
            texture_path: String::new(),
            image: Handle::default(),
            material: Handle::default(),
            block_item: false,
            placeable: false,
            tags: Vec::new(),
            rarity: "common".to_string(),
            world_drop: ItemWorldDropConfig::default(),
        });
        key_to_id.insert("empty".to_string(), EMPTY_ITEM_ID);
        item_to_block.push(None);

        Self {
            defs,
            key_to_id,
            block_to_item: vec![None; block_registry.defs.len()],
            item_to_block,
        }
    }

    fn insert_json_item(
        &mut self,
        item_json: ItemJson,
        asset_server: &AssetServer,
        materials: &mut Assets<StandardMaterial>,
        block_registry: &BlockRegistry,
    ) {
        if item_json.id.trim().is_empty() {
            return;
        }
        let mapped_block = if item_json.block_item {
            resolve_mapped_block_id(&item_json, block_registry)
        } else {
            None
        };

        let texture_path = normalize_item_texture_path(&item_json.texture);
        let image: Handle<Image> = asset_server.load(texture_path.clone());
        let material = materials.add(StandardMaterial {
            base_color_texture: Some(image.clone()),
            base_color: Color::WHITE,
            alpha_mode: AlphaMode::Blend,
            unlit: false,
            metallic: 0.0,
            perceptual_roughness: 1.0,
            reflectance: 0.0,
            cull_mode: None,
            ..Default::default()
        });

        let item_id = self.push_item(ItemDef {
            key: item_json.id.clone(),
            name: if item_json.name.trim().is_empty() {
                prettify_key(&item_json.id)
            } else {
                item_json.name
            },
            max_stack_size: normalize_stack_size(item_json.max_stack_size),
            category: item_json.category,
            texture_path,
            image,
            material,
            block_item: item_json.block_item,
            placeable: item_json.placeable,
            tags: item_json.tags,
            rarity: item_json.rarity,
            world_drop: ItemWorldDropConfig {
                pickupable: item_json.world_drop.pickupable,
            },
        });

        if let Some(block_id) = mapped_block {
            self.bind_block_item(block_id, item_id);
        }
    }

    fn insert_json_item_headless(&mut self, item_json: ItemJson, block_registry: &BlockRegistry) {
        if item_json.id.trim().is_empty() {
            return;
        }
        let mapped_block = if item_json.block_item {
            resolve_mapped_block_id(&item_json, block_registry)
        } else {
            None
        };

        let item_id = self.push_item(ItemDef {
            key: item_json.id.clone(),
            name: if item_json.name.trim().is_empty() {
                prettify_key(&item_json.id)
            } else {
                item_json.name
            },
            max_stack_size: normalize_stack_size(item_json.max_stack_size),
            category: item_json.category,
            texture_path: normalize_item_texture_path(&item_json.texture),
            image: Handle::default(),
            material: Handle::default(),
            block_item: item_json.block_item,
            placeable: item_json.placeable,
            tags: item_json.tags,
            rarity: item_json.rarity,
            world_drop: ItemWorldDropConfig {
                pickupable: item_json.world_drop.pickupable,
            },
        });

        if let Some(block_id) = mapped_block {
            self.bind_block_item(block_id, item_id);
        }
    }

    fn ensure_block_items(&mut self, asset_server: &AssetServer, block_registry: &BlockRegistry) {
        for block_id in 1..block_registry.defs.len() {
            if self.block_to_item[block_id].is_some() {
                continue;
            }

            let block_id = block_id as BlockId;
            let block_def = block_registry.def(block_id);
            let base_key = block_def.name.clone();
            let item_key = unique_key(&self.key_to_id, &base_key);
            let texture_path = asset_server
                .get_path(block_def.image.id())
                .map(|path| path.path().to_string_lossy().to_string())
                .unwrap_or_default();

            let item_id = self.push_item(ItemDef {
                key: item_key,
                name: prettify_block_name(&block_def.name),
                max_stack_size: DEFAULT_ITEM_STACK_SIZE,
                category: "block".to_string(),
                texture_path,
                image: block_def.image.clone(),
                material: block_def.material.clone(),
                block_item: true,
                placeable: true,
                tags: vec!["block".to_string()],
                rarity: "common".to_string(),
                world_drop: ItemWorldDropConfig { pickupable: true },
            });
            self.bind_block_item(block_id, item_id);
        }
    }

    fn ensure_block_items_headless(&mut self, block_registry: &BlockRegistry) {
        for block_id in 1..block_registry.defs.len() {
            if self.block_to_item[block_id].is_some() {
                continue;
            }

            let block_id = block_id as BlockId;
            let block_def = block_registry.def(block_id);
            let item_key = unique_key(&self.key_to_id, &block_def.name);
            let item_id = self.push_item(ItemDef {
                key: item_key,
                name: prettify_block_name(&block_def.name),
                max_stack_size: DEFAULT_ITEM_STACK_SIZE,
                category: "block".to_string(),
                texture_path: String::new(),
                image: Handle::default(),
                material: Handle::default(),
                block_item: true,
                placeable: true,
                tags: vec!["block".to_string()],
                rarity: "common".to_string(),
                world_drop: ItemWorldDropConfig { pickupable: true },
            });
            self.bind_block_item(block_id, item_id);
        }
    }

    fn push_item(&mut self, def: ItemDef) -> ItemId {
        if let Some(existing) = self.key_to_id.get(&def.key).copied() {
            return existing;
        }

        let id = self.defs.len() as ItemId;
        self.key_to_id.insert(def.key.clone(), id);
        self.defs.push(def);
        self.item_to_block.push(None);
        id
    }

    fn bind_block_item(&mut self, block_id: BlockId, item_id: ItemId) {
        if let Some(slot) = self.block_to_item.get_mut(block_id as usize) {
            *slot = Some(item_id);
        }
        if let Some(slot) = self.item_to_block.get_mut(item_id as usize) {
            *slot = Some(block_id);
        }
    }
}

#[derive(Deserialize)]
struct ItemJson {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default = "default_json_stack_size")]
    max_stack_size: i32,
    #[serde(default)]
    category: String,
    #[serde(default)]
    texture: String,
    #[serde(default)]
    block_item: bool,
    #[serde(default)]
    placeable: bool,
    #[serde(default)]
    block: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default = "default_rarity")]
    rarity: String,
    #[serde(default)]
    world_drop: ItemWorldDropJson,
}

#[derive(Deserialize, Default)]
struct ItemWorldDropJson {
    #[serde(default = "default_true")]
    pickupable: bool,
}

fn item_json_paths(items_dir: &str) -> Vec<PathBuf> {
    let dir = Path::new(items_dir);
    let mut paths = Vec::new();
    let Ok(read_dir) = fs::read_dir(dir) else {
        return paths;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            paths.push(path);
        }
    }

    paths.sort_unstable();
    paths
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &str) -> T {
    let text = fs::read_to_string(path).expect("failed to read item json");
    serde_json::from_str(&text).expect("invalid item json")
}

fn normalize_stack_size(value: i32) -> u16 {
    if value <= 0 {
        DEFAULT_ITEM_STACK_SIZE
    } else {
        value.min(u16::MAX as i32) as u16
    }
}

fn resolve_mapped_block_id(
    item_json: &ItemJson,
    block_registry: &BlockRegistry,
) -> Option<BlockId> {
    if let Some(block_name) = item_json.block.as_deref() {
        return block_registry.id_opt(block_name);
    }

    guess_block_name_from_item_key(&item_json.id)
        .and_then(|name| block_registry.id_opt(name.as_str()))
}

fn guess_block_name_from_item_key(item_key: &str) -> Option<String> {
    let key = item_key.trim();
    if key.is_empty() {
        return None;
    }

    if key.ends_with("_block") {
        return Some(key.to_string());
    }
    if let Some(base) = key.strip_suffix("_item") {
        return Some(format!("{base}_block"));
    }
    Some(format!("{key}_block"))
}

fn normalize_item_texture_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::from("textures/items/missing.png");
    }
    if let Some(stripped) = trimmed.strip_prefix("assets/") {
        return stripped.to_string();
    }
    trimmed.to_string()
}

fn prettify_key(key: &str) -> String {
    key.replace('_', " ")
        .split_whitespace()
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first
                    .to_uppercase()
                    .chain(chars.flat_map(|ch| ch.to_lowercase()))
                    .collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn prettify_block_name(block_name: &str) -> String {
    if let Some(base) = block_name.strip_suffix("_block") {
        return prettify_key(base);
    }
    prettify_key(block_name)
}

fn unique_key(existing: &HashMap<String, ItemId>, preferred: &str) -> String {
    if !existing.contains_key(preferred) {
        return preferred.to_string();
    }

    let mut index = 1usize;
    loop {
        let candidate = format!("{preferred}_item_{index}");
        if !existing.contains_key(&candidate) {
            return candidate;
        }
        index += 1;
    }
}

fn default_true() -> bool {
    true
}

fn default_rarity() -> String {
    String::from("common")
}

fn default_json_stack_size() -> i32 {
    DEFAULT_ITEM_STACK_SIZE as i32
}
