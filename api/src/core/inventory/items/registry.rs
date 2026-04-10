use crate::core::inventory::items::tools::{
    ToolDef, ToolLevel, ToolType, infer_tool_from_item_key,
};
use crate::core::inventory::items::types::{
    DEFAULT_ITEM_STACK_SIZE, EMPTY_ITEM_ID, ItemDef, ItemId, ItemWorldDropConfig,
};
use crate::core::world::block::{BlockId, BlockRegistry, UvRect};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use image::imageops::FilterType;
use image::{Rgba, RgbaImage};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Prefix used by virtual UI icon keys for block items.
pub const BLOCK_ICON_CACHE_PREFIX: &str = "block-icon://";
/// Default provider used when an item JSON omits an explicit `provider:`.
pub const DEFAULT_ITEM_PROVIDER: &str = "oplexa";

/// Registry that stores all item definitions plus block↔item relations.
#[derive(Resource, Clone, Debug)]
pub struct ItemRegistry {
    /// Indexed item definitions (`index == ItemId`).
    pub defs: Vec<ItemDef>,
    /// Maps localized item name (`provider:key`) to numeric item id.
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

    /// Returns the numeric id for a localized item name, if present.
    ///
    /// For compatibility, non-namespaced values (for example `stick`) are
    /// interpreted as `{DEFAULT_ITEM_PROVIDER}:stick`.
    #[inline]
    pub fn id_opt(&self, key: &str) -> Option<ItemId> {
        self.key_to_id.get(key).copied().or_else(|| {
            if key.contains(':') {
                return None;
            }
            self.key_to_id
                .get(format!("{DEFAULT_ITEM_PROVIDER}:{key}").as_str())
                .copied()
        })
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

    /// Returns tool metadata for an item id, if this item is a tool.
    #[inline]
    pub fn tool_for_item(&self, item_id: ItemId) -> Option<ToolDef> {
        self.def_opt(item_id).and_then(|item| item.tool)
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
        if !item.texture_path.is_empty() {
            return Some(item.texture_path.clone());
        }
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

    /// Creates with empty for the `core::inventory::items::registry` module.
    fn new_with_empty(block_registry: &BlockRegistry) -> Self {
        let mut defs = Vec::with_capacity(64);
        let mut key_to_id = HashMap::with_capacity(64);
        let mut item_to_block = Vec::with_capacity(64);

        defs.push(ItemDef {
            provider: "internal".to_string(),
            key: "empty".to_string(),
            localized_name: "internal:empty".to_string(),
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
            tool: None,
            world_drop: ItemWorldDropConfig::default(),
        });
        key_to_id.insert("internal:empty".to_string(), EMPTY_ITEM_ID);
        key_to_id.insert("empty".to_string(), EMPTY_ITEM_ID);
        item_to_block.push(None);

        Self {
            defs,
            key_to_id,
            block_to_item: vec![None; block_registry.defs.len()],
            item_to_block,
        }
    }

    /// Inserts json item for the `core::inventory::items::registry` module.
    fn insert_json_item(
        &mut self,
        item_json: ItemJson,
        asset_server: &AssetServer,
        materials: &mut Assets<StandardMaterial>,
        block_registry: &BlockRegistry,
    ) {
        let Some(item_identity) = parse_item_identity(item_json.localized_name.as_str()) else {
            return;
        };
        let mapped_block = if item_json.block_item {
            resolve_mapped_block_id(&item_json, item_identity.key.as_str(), block_registry)
        } else {
            None
        };
        let render_kind = resolve_item_render_kind(&item_json);
        let texture_path = resolve_item_texture_path(
            &item_json,
            item_identity.key.as_str(),
            mapped_block,
            block_registry,
        );
        let tool = resolve_item_tool_def(&item_json, item_identity.key.as_str());
        let (image, material) = match render_kind {
            ItemRenderKind::Flat => {
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
                (image, material)
            }
            ItemRenderKind::Block => (Handle::default(), Handle::default()),
        };

        let item_id = self.push_item(ItemDef {
            provider: item_identity.provider,
            key: item_identity.key.clone(),
            localized_name: item_identity.localized_name.clone(),
            name: if item_json.name.trim().is_empty() {
                prettify_key(item_identity.key.as_str())
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
            tool,
            world_drop: ItemWorldDropConfig {
                pickupable: item_json.world_drop.pickupable,
            },
        });

        if let Some(block_id) = mapped_block {
            self.bind_block_item(block_id, item_id);
        }
    }

    /// Inserts json item headless for the `core::inventory::items::registry` module.
    fn insert_json_item_headless(&mut self, item_json: ItemJson, block_registry: &BlockRegistry) {
        let Some(item_identity) = parse_item_identity(item_json.localized_name.as_str()) else {
            return;
        };
        let mapped_block = if item_json.block_item {
            resolve_mapped_block_id(&item_json, item_identity.key.as_str(), block_registry)
        } else {
            None
        };
        let texture_path = resolve_item_texture_path_headless(
            &item_json,
            item_identity.key.as_str(),
            mapped_block,
        );
        let tool = resolve_item_tool_def(&item_json, item_identity.key.as_str());

        let item_id = self.push_item(ItemDef {
            provider: item_identity.provider,
            key: item_identity.key.clone(),
            localized_name: item_identity.localized_name.clone(),
            name: if item_json.name.trim().is_empty() {
                prettify_key(item_identity.key.as_str())
            } else {
                item_json.name
            },
            max_stack_size: normalize_stack_size(item_json.max_stack_size),
            category: item_json.category,
            texture_path,
            image: Handle::default(),
            material: Handle::default(),
            block_item: item_json.block_item,
            placeable: item_json.placeable,
            tags: item_json.tags,
            rarity: item_json.rarity,
            tool,
            world_drop: ItemWorldDropConfig {
                pickupable: item_json.world_drop.pickupable,
            },
        });

        if let Some(block_id) = mapped_block {
            self.bind_block_item(block_id, item_id);
        }
    }

    /// Runs the `ensure_block_items` routine for ensure block items in the `core::inventory::items::registry` module.
    fn ensure_block_items(&mut self, asset_server: &AssetServer, block_registry: &BlockRegistry) {
        for block_id in 1..block_registry.defs.len() {
            if self.block_to_item[block_id].is_some() {
                continue;
            }

            let block_id = block_id as BlockId;
            let canonical_id = canonical_block_item_id(block_registry, block_id);
            if canonical_id != block_id {
                continue;
            }
            let block_def = block_registry.def(block_id);
            let item_key = block_def.localized_name.clone();
            let localized_name = format!("{DEFAULT_ITEM_PROVIDER}:{item_key}");
            let texture_path = block_icon_cache_key(block_id);
            let image: Handle<Image> = asset_server.load("textures/items/missing.png");

            let item_id = self.push_item(ItemDef {
                provider: DEFAULT_ITEM_PROVIDER.to_string(),
                key: item_key,
                localized_name,
                name: block_def.name.clone(),
                max_stack_size: DEFAULT_ITEM_STACK_SIZE,
                category: "block".to_string(),
                texture_path,
                image,
                material: block_def.material.clone(),
                block_item: true,
                placeable: true,
                tags: vec!["block".to_string()],
                rarity: "common".to_string(),
                tool: None,
                world_drop: ItemWorldDropConfig { pickupable: true },
            });
            self.bind_block_item(block_id, item_id);
        }

        for block_id in 1..block_registry.defs.len() {
            if self.block_to_item[block_id].is_some() {
                continue;
            }
            let block_id = block_id as BlockId;
            let canonical_id = canonical_block_item_id(block_registry, block_id);
            if canonical_id == block_id {
                continue;
            }
            let Some(item_id) = self
                .block_to_item
                .get(canonical_id as usize)
                .and_then(|entry| *entry)
            else {
                continue;
            };
            if let Some(slot) = self.block_to_item.get_mut(block_id as usize) {
                *slot = Some(item_id);
            }
        }
    }

    /// Runs the `ensure_block_items_headless` routine for ensure block items headless in the `core::inventory::items::registry` module.
    fn ensure_block_items_headless(&mut self, block_registry: &BlockRegistry) {
        for block_id in 1..block_registry.defs.len() {
            if self.block_to_item[block_id].is_some() {
                continue;
            }

            let block_id = block_id as BlockId;
            let canonical_id = canonical_block_item_id(block_registry, block_id);
            if canonical_id != block_id {
                continue;
            }
            let block_def = block_registry.def(block_id);
            let item_key = block_def.localized_name.clone();
            let localized_name = format!("{DEFAULT_ITEM_PROVIDER}:{item_key}");
            let item_id = self.push_item(ItemDef {
                provider: DEFAULT_ITEM_PROVIDER.to_string(),
                key: item_key,
                localized_name,
                name: block_def.name.clone(),
                max_stack_size: DEFAULT_ITEM_STACK_SIZE,
                category: "block".to_string(),
                texture_path: String::from("textures/items/missing.png"),
                image: Handle::default(),
                material: Handle::default(),
                block_item: true,
                placeable: true,
                tags: vec!["block".to_string()],
                rarity: "common".to_string(),
                tool: None,
                world_drop: ItemWorldDropConfig { pickupable: true },
            });
            self.bind_block_item(block_id, item_id);
        }

        for block_id in 1..block_registry.defs.len() {
            if self.block_to_item[block_id].is_some() {
                continue;
            }
            let block_id = block_id as BlockId;
            let canonical_id = canonical_block_item_id(block_registry, block_id);
            if canonical_id == block_id {
                continue;
            }
            let Some(item_id) = self
                .block_to_item
                .get(canonical_id as usize)
                .and_then(|entry| *entry)
            else {
                continue;
            };
            if let Some(slot) = self.block_to_item.get_mut(block_id as usize) {
                *slot = Some(item_id);
            }
        }
    }

    /// Runs the `push_item` routine for push item in the `core::inventory::items::registry` module.
    fn push_item(&mut self, def: ItemDef) -> ItemId {
        if let Some(existing) = self.key_to_id.get(&def.localized_name).copied() {
            debug!(
                "Duplicate item '{}' ignored; keeping existing id {}",
                def.localized_name, existing
            );
            return existing;
        }

        let id = self.defs.len() as ItemId;
        self.key_to_id.insert(def.localized_name.clone(), id);
        self.defs.push(def);
        self.item_to_block.push(None);
        id
    }

    /// Runs the `bind_block_item` routine for bind block item in the `core::inventory::items::registry` module.
    fn bind_block_item(&mut self, block_id: BlockId, item_id: ItemId) {
        if let Some(slot) = self.block_to_item.get_mut(block_id as usize) {
            *slot = Some(item_id);
        }
        if let Some(slot) = self.item_to_block.get_mut(item_id as usize) {
            if slot.is_none() {
                *slot = Some(block_id);
            }
        }
    }
}

/// Represents item json used by the `core::inventory::items::registry` module.
#[derive(Deserialize)]
struct ItemJson {
    #[serde(default, alias = "id")]
    localized_name: String,
    #[serde(default)]
    name: String,
    #[serde(default = "default_json_stack_size")]
    max_stack_size: i32,
    #[serde(default)]
    category: String,
    #[serde(default)]
    texture: String,
    #[serde(default)]
    render: ItemRenderJson,
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
    tool: ItemToolJson,
    #[serde(default)]
    world_drop: ItemWorldDropJson,
}

/// Represents item world drop json used by the `core::inventory::items::registry` module.
#[derive(Deserialize, Default)]
struct ItemWorldDropJson {
    #[serde(default = "default_true")]
    pickupable: bool,
}

/// Represents item render json used by the `core::inventory::items::registry` module.
#[derive(Deserialize, Default)]
struct ItemRenderJson {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    texture: String,
    #[serde(default)]
    block: Option<String>,
    #[serde(default)]
    projection: String,
}

/// Represents item tool json used by the `core::inventory::items::registry` module.
#[derive(Deserialize, Default)]
struct ItemToolJson {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default = "default_json_tool_level")]
    level: u8,
}

/// Defines the possible item render kind variants in the `core::inventory::items::registry` module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ItemRenderKind {
    Flat,
    Block,
}

/// Represents item identity used by the `core::inventory::items::registry` module.
#[derive(Clone, Debug)]
struct ItemIdentity {
    provider: String,
    key: String,
    localized_name: String,
}

/// Runs the `item_json_paths` routine for item json paths in the `core::inventory::items::registry` module.
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

/// Reads json for the `core::inventory::items::registry` module.
fn read_json<T: for<'de> Deserialize<'de>>(path: &str) -> T {
    let text = fs::read_to_string(path).expect("failed to read item json");
    serde_json::from_str(&text).expect("invalid item json")
}

/// Runs the `normalize_stack_size` routine for normalize stack size in the `core::inventory::items::registry` module.
fn normalize_stack_size(value: i32) -> u16 {
    if value <= 0 {
        DEFAULT_ITEM_STACK_SIZE
    } else {
        value.min(u16::MAX as i32) as u16
    }
}

/// Parses item identity for the `core::inventory::items::registry` module.
fn parse_item_identity(raw_localized_name: &str) -> Option<ItemIdentity> {
    let trimmed = raw_localized_name.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (provider, key) = if let Some((provider, key)) = trimmed.split_once(':') {
        (provider.trim(), key.trim())
    } else {
        (DEFAULT_ITEM_PROVIDER, trimmed)
    };

    if provider.is_empty() || key.is_empty() {
        return None;
    }

    let provider = provider.to_ascii_lowercase();
    let key = key.to_ascii_lowercase();
    Some(ItemIdentity {
        localized_name: format!("{provider}:{key}"),
        provider,
        key,
    })
}

/// Runs the `resolve_mapped_block_id` routine for resolve mapped block id in the `core::inventory::items::registry` module.
fn resolve_mapped_block_id(
    item_json: &ItemJson,
    item_key: &str,
    block_registry: &BlockRegistry,
) -> Option<BlockId> {
    if let Some(block_name) = item_json
        .block
        .as_deref()
        .or(item_json.render.block.as_deref())
    {
        return block_registry.id_opt(block_name);
    }

    guess_block_name_from_item_key(item_key).and_then(|name| block_registry.id_opt(name.as_str()))
}

/// Runs the `resolve_item_tool_def` routine for resolve item tool def in the `core::inventory::items::registry` module.
fn resolve_item_tool_def(item_json: &ItemJson, item_key: &str) -> Option<ToolDef> {
    let kind = item_json.tool.kind.trim();
    if !kind.is_empty() {
        if let Ok(tool_type) = kind.parse::<ToolType>() {
            return Some(ToolDef::new(
                tool_type,
                ToolLevel::from_u8_clamped(item_json.tool.level),
            ));
        }
    }

    infer_tool_from_item_key(item_key)
}

/// Resolves the UI texture path for one item definition in graphics mode.
fn resolve_item_texture_path(
    item_json: &ItemJson,
    item_key: &str,
    mapped_block: Option<BlockId>,
    block_registry: &BlockRegistry,
) -> String {
    match resolve_item_render_kind(item_json) {
        ItemRenderKind::Flat => normalize_item_texture_path(
            first_non_empty(&[
                item_json.render.texture.as_str(),
                item_json.texture.as_str(),
                "textures/items/missing.png",
            ])
            .unwrap_or("textures/items/missing.png"),
        ),
        ItemRenderKind::Block => {
            if !is_supported_block_projection(item_json) {
                return String::from("textures/items/missing.png");
            }
            let Some(block_id) =
                resolve_item_render_block_id(item_json, item_key, mapped_block, block_registry)
            else {
                return String::from("textures/items/missing.png");
            };
            block_icon_cache_key(block_id)
        }
    }
}

/// Resolves the UI texture path for one item definition in headless mode.
fn resolve_item_texture_path_headless(
    item_json: &ItemJson,
    _item_key: &str,
    mapped_block: Option<BlockId>,
) -> String {
    match resolve_item_render_kind(item_json) {
        ItemRenderKind::Flat => normalize_item_texture_path(
            first_non_empty(&[
                item_json.render.texture.as_str(),
                item_json.texture.as_str(),
                "textures/items/missing.png",
            ])
            .unwrap_or("textures/items/missing.png"),
        ),
        ItemRenderKind::Block => mapped_block
            .map(block_icon_cache_key)
            .unwrap_or_else(|| String::from("textures/items/missing.png")),
    }
}

/// Returns whether the configured block-item projection is currently supported.
///
/// At the moment only `isometric` is implemented; empty values default to this.
fn is_supported_block_projection(item_json: &ItemJson) -> bool {
    let projection = item_json.render.projection.trim();
    projection.is_empty() || projection.eq_ignore_ascii_case("isometric")
}

/// Resolves whether an item should be rendered as a flat icon or a block preview.
fn resolve_item_render_kind(item_json: &ItemJson) -> ItemRenderKind {
    let explicit = item_json.render.kind.trim().to_ascii_lowercase();
    if explicit == "block" {
        return ItemRenderKind::Block;
    }
    if explicit == "flat" {
        return ItemRenderKind::Flat;
    }
    if item_json.render.block.is_some() {
        return ItemRenderKind::Block;
    }
    if item_json.block_item {
        return ItemRenderKind::Block;
    }
    ItemRenderKind::Flat
}

/// Resolves the block id for an item using render JSON and legacy mapping fields.
fn resolve_item_render_block_id(
    item_json: &ItemJson,
    item_key: &str,
    mapped_block: Option<BlockId>,
    block_registry: &BlockRegistry,
) -> Option<BlockId> {
    if let Some(block_name) = item_json.render.block.as_deref()
        && let Some(block_id) = block_registry.id_opt(block_name)
    {
        return Some(block_id);
    }
    if let Some(block_id) = mapped_block {
        return Some(block_id);
    }
    guess_block_name_from_item_key(item_key).and_then(|name| block_registry.id_opt(name.as_str()))
}

/// Runs the `first_non_empty` routine for first non empty in the `core::inventory::items::registry` module.
#[inline]
fn first_non_empty<'a>(candidates: &[&'a str]) -> Option<&'a str> {
    candidates
        .iter()
        .copied()
        .find(|value| !value.trim().is_empty())
}

/// Runs the `block_icon_cache_key` routine for block icon cache key in the `core::inventory::items::registry` module.
fn block_icon_cache_key(block_id: BlockId) -> String {
    format!("{BLOCK_ICON_CACHE_PREFIX}{block_id}")
}

/// Parses a virtual block-icon cache key and returns its block id.
pub fn parse_block_icon_cache_key(path: &str) -> Option<BlockId> {
    path.strip_prefix(BLOCK_ICON_CACHE_PREFIX)
        .and_then(|raw| raw.parse::<u16>().ok())
}

#[inline]
fn canonical_block_item_id(block_registry: &BlockRegistry, block_id: BlockId) -> BlockId {
    let name = block_registry.def(block_id).localized_name.as_str();
    let Some(base_name) = slab_base_name_for_variant(name) else {
        return block_id;
    };
    block_registry
        .id_opt(base_name.as_str())
        .unwrap_or(block_id)
}

#[inline]
fn slab_base_name_for_variant(name: &str) -> Option<String> {
    const ORIENTED_SUFFIXES: [&str; 5] = [
        "_slab_top_block",
        "_slab_north_block",
        "_slab_south_block",
        "_slab_east_block",
        "_slab_west_block",
    ];
    ORIENTED_SUFFIXES.iter().find_map(|suffix| {
        name.strip_suffix(suffix)
            .map(|prefix| format!("{prefix}_slab_block"))
    })
}

/// Guesses block name from item key for the `core::inventory::items::registry` module.
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

/// Runs the `normalize_item_texture_path` routine for normalize item texture path in the `core::inventory::items::registry` module.
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

/// Runs the `prettify_key` routine for prettify key in the `core::inventory::items::registry` module.
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

/// Runs the `default_true` routine for default true in the `core::inventory::items::registry` module.
fn default_true() -> bool {
    true
}

/// Runs the `default_rarity` routine for default rarity in the `core::inventory::items::registry` module.
fn default_rarity() -> String {
    String::from("common")
}

/// Runs the `default_json_stack_size` routine for default json stack size in the `core::inventory::items::registry` module.
fn default_json_stack_size() -> i32 {
    DEFAULT_ITEM_STACK_SIZE as i32
}

/// Runs the `default_json_tool_level` routine for default json tool level in the `core::inventory::items::registry` module.
fn default_json_tool_level() -> u8 {
    1
}

/// Builds an in-memory icon image for one block item using block atlas UV faces.
///
/// The returned image can be inserted into a UI image cache under a custom key.
pub fn build_block_item_icon_image(
    block_registry: &BlockRegistry,
    asset_server: &AssetServer,
    block_id: BlockId,
) -> Option<Image> {
    let block = block_registry.def(block_id);
    let height_ratio = block_icon_height_ratio(block);
    let atlas_rel = asset_server
        .get_path(block.image.id())
        .map(|path| path.path().to_string_lossy().to_string())?;
    let atlas_fs = Path::new("assets").join(atlas_rel.as_str());
    let icon = render_isometric_block_icon(
        atlas_fs.as_path(),
        block.uv_top,
        block.uv_west,
        block.uv_north,
        height_ratio,
    )
    .ok()?;

    let width = icon.width();
    let height = icon.height();
    let data = icon.into_raw();
    Some(Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    ))
}

/// Renders isometric block icon for the `core::inventory::items::registry` module.
fn render_isometric_block_icon(
    atlas_path: &Path,
    top_uv: UvRect,
    left_uv: UvRect,
    right_uv: UvRect,
    height_ratio: f32,
) -> Result<RgbaImage, String> {
    /// Icon output width/height in pixels.
    const ICON_SIZE: u32 = 64;
    /// Transparent border kept around the rendered block silhouette.
    const ICON_PADDING: u32 = 2;

    let atlas = image::open(atlas_path)
        .map_err(|err| {
            format!(
                "failed to open block atlas '{}': {err}",
                atlas_path.display()
            )
        })?
        .to_rgba8();

    let mut canvas = RgbaImage::from_pixel(ICON_SIZE, ICON_SIZE, Rgba([0, 0, 0, 0]));
    let side_height = 20.0 * height_ratio.clamp(0.2, 1.0);
    let side_origin_y = 40.0 - side_height;
    let top_origin_y = side_origin_y - 12.0;

    // Draw sides first, then top so the top edge stays crisp.
    draw_textured_parallelogram(
        &mut canvas,
        &atlas,
        left_uv,
        [12.0, side_origin_y],
        [20.0, 12.0],
        [0.0, side_height],
        0.78,
    );
    draw_textured_parallelogram(
        &mut canvas,
        &atlas,
        right_uv,
        [52.0, side_origin_y],
        [0.0, side_height],
        [-20.0, 12.0],
        0.66,
    );
    draw_textured_parallelogram(
        &mut canvas,
        &atlas,
        top_uv,
        [32.0, top_origin_y],
        [20.0, 12.0],
        [-20.0, 12.0],
        1.0,
    );

    fit_icon_to_canvas(&mut canvas, ICON_PADDING);
    Ok(canvas)
}

#[inline]
fn block_icon_height_ratio(block: &crate::core::world::block::BlockDef) -> f32 {
    match block.collider.kind {
        crate::core::world::block::BlockColliderKind::Box => block.collider.size_m[1],
        _ => 1.0,
    }
}

/// Fits the non-transparent icon area into the canvas while keeping aspect ratio.
///
/// This removes excessive transparent margins from generated block icons so they
/// appear larger in UI slots.
fn fit_icon_to_canvas(canvas: &mut RgbaImage, padding: u32) {
    let Some((min_x, min_y, max_x, max_y)) = non_transparent_bounds(canvas) else {
        return;
    };

    let src_w = max_x - min_x + 1;
    let src_h = max_y - min_y + 1;
    if src_w == 0 || src_h == 0 {
        return;
    }

    let target_w = canvas
        .width()
        .saturating_sub(padding.saturating_mul(2))
        .max(1);
    let target_h = canvas
        .height()
        .saturating_sub(padding.saturating_mul(2))
        .max(1);

    let scale = (target_w as f32 / src_w as f32).min(target_h as f32 / src_h as f32);
    let scaled_w = ((src_w as f32 * scale).round() as u32).clamp(1, canvas.width());
    let scaled_h = ((src_h as f32 * scale).round() as u32).clamp(1, canvas.height());

    let cropped = image::imageops::crop_imm(canvas, min_x, min_y, src_w, src_h).to_image();
    let scaled = image::imageops::resize(&cropped, scaled_w, scaled_h, FilterType::Nearest);

    let mut normalized = RgbaImage::from_pixel(canvas.width(), canvas.height(), Rgba([0, 0, 0, 0]));
    let offset_x = (canvas.width() - scaled_w) / 2;
    let offset_y = (canvas.height() - scaled_h) / 2;
    for y in 0..scaled_h {
        for x in 0..scaled_w {
            let px = *scaled.get_pixel(x, y);
            normalized.put_pixel(offset_x + x, offset_y + y, px);
        }
    }

    *canvas = normalized;
}

/// Returns the inclusive non-transparent bounds of an RGBA image.
fn non_transparent_bounds(image: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let mut min_x = image.width();
    let mut min_y = image.height();
    let mut max_x = 0_u32;
    let mut max_y = 0_u32;
    let mut found = false;

    for (x, y, px) in image.enumerate_pixels() {
        if px[3] == 0 {
            continue;
        }
        found = true;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }

    if found {
        Some((min_x, min_y, max_x, max_y))
    } else {
        None
    }
}

/// Draws textured parallelogram for the `core::inventory::items::registry` module.
fn draw_textured_parallelogram(
    canvas: &mut RgbaImage,
    atlas: &RgbaImage,
    uv: UvRect,
    origin: [f32; 2],
    vx: [f32; 2],
    vy: [f32; 2],
    shade: f32,
) {
    let corners = [
        origin,
        [origin[0] + vx[0], origin[1] + vx[1]],
        [origin[0] + vy[0], origin[1] + vy[1]],
        [origin[0] + vx[0] + vy[0], origin[1] + vx[1] + vy[1]],
    ];

    let min_x = corners
        .iter()
        .map(|point| point[0])
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as i32;
    let max_x = corners
        .iter()
        .map(|point| point[0])
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .min((canvas.width() - 1) as f32) as i32;
    let min_y = corners
        .iter()
        .map(|point| point[1])
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as i32;
    let max_y = corners
        .iter()
        .map(|point| point[1])
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .min((canvas.height() - 1) as f32) as i32;

    let det = vx[0] * vy[1] - vx[1] * vy[0];
    if det.abs() <= f32::EPSILON {
        return;
    }

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = (x as f32 + 0.5) - origin[0];
            let dy = (y as f32 + 0.5) - origin[1];
            let a = (dx * vy[1] - dy * vy[0]) / det;
            let b = (dy * vx[0] - dx * vx[1]) / det;

            if !(0.0..=1.0).contains(&a) || !(0.0..=1.0).contains(&b) {
                continue;
            }

            let src_u = uv.u0 + a * (uv.u1 - uv.u0);
            let src_v = uv.v0 + b * (uv.v1 - uv.v0);
            let sx = ((src_u * atlas.width().saturating_sub(1) as f32).round() as u32)
                .min(atlas.width().saturating_sub(1));
            let sy = ((src_v * atlas.height().saturating_sub(1) as f32).round() as u32)
                .min(atlas.height().saturating_sub(1));

            let src_px = atlas.get_pixel(sx, sy);
            if src_px[3] == 0 {
                continue;
            }

            let tinted = [
                ((src_px[0] as f32 * shade).round() as u8),
                ((src_px[1] as f32 * shade).round() as u8),
                ((src_px[2] as f32 * shade).round() as u8),
                src_px[3],
            ];

            alpha_blend_pixel(canvas, x as u32, y as u32, tinted);
        }
    }
}

/// Runs the `alpha_blend_pixel` routine for alpha blend pixel in the `core::inventory::items::registry` module.
fn alpha_blend_pixel(canvas: &mut RgbaImage, x: u32, y: u32, src: [u8; 4]) {
    let dst = canvas.get_pixel_mut(x, y);
    let src_a = src[3] as f32 / 255.0;
    let dst_a = dst[3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);

    if out_a <= f32::EPSILON {
        *dst = Rgba([0, 0, 0, 0]);
        return;
    }

    let blend = |src_c: u8, dst_c: u8| -> u8 {
        let src_c = src_c as f32 / 255.0;
        let dst_c = dst_c as f32 / 255.0;
        (((src_c * src_a + dst_c * dst_a * (1.0 - src_a)) / out_a) * 255.0).round() as u8
    };

    *dst = Rgba([
        blend(src[0], dst[0]),
        blend(src[1], dst[1]),
        blend(src[2], dst[2]),
        (out_a * 255.0).round() as u8,
    ]);
}
