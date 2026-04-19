use crate::core::inventory::items::{ItemId, ItemRegistry};
use crate::core::world::block::BlockStats;
use bevy::prelude::{Resource, UVec3, Vec3};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const BUILDING_SHAPED_RECIPE_KIND: &str = "building_shaped";
const STRUCTURE_COLLIDER_MIN_SIZE_METERS: f32 = 0.01;
const STRUCTURE_COLLIDER_MAX_SIZE_METERS: f32 = 128.0;
const STRUCTURE_COLLIDER_MAX_OFFSET_METERS: f32 = 128.0;

/// Material requirement source entry for one building recipe.
#[derive(Clone, Debug)]
pub enum BuildingMaterialRequirementSource {
    Item {
        item_id: ItemId,
        item_localized_name: String,
    },
    Group {
        group: String,
    },
}

/// Material requirement entry for one building recipe.
#[derive(Clone, Debug)]
pub struct BuildingMaterialRequirement {
    pub source: BuildingMaterialRequirementSource,
    pub count: u16,
}

impl BuildingMaterialRequirement {
    #[inline]
    pub fn item(item_id: ItemId, item_localized_name: String, count: u16) -> Self {
        Self {
            source: BuildingMaterialRequirementSource::Item {
                item_id,
                item_localized_name,
            },
            count,
        }
    }

    #[inline]
    pub fn group(group: String, count: u16) -> Self {
        Self {
            source: BuildingMaterialRequirementSource::Group { group },
            count,
        }
    }
}

/// Runtime definition of one structure building recipe.
#[derive(Clone, Debug)]
pub struct BuildingStructureRecipe {
    pub name: String,
    pub source_path: String,
    pub model_asset_path: String,
    pub model_meta: BuildingStructureModelMeta,
    pub space: UVec3,
    pub build_time_secs: f32,
    pub requirements: Vec<BuildingMaterialRequirement>,
}

/// Registry for all structure building recipes.
#[derive(Resource, Default, Clone, Debug)]
pub struct BuildingStructureRecipeRegistry {
    pub recipes: Vec<BuildingStructureRecipe>,
}

impl BuildingStructureRecipeRegistry {
    /// Returns one recipe by normalized name.
    #[inline]
    pub fn recipe_by_name(&self, name: &str) -> Option<&BuildingStructureRecipe> {
        let normalized = normalize_recipe_name(name);
        self.recipes.iter().find(|recipe| recipe.name == normalized)
    }
}

#[derive(Deserialize)]
struct BuildingRecipeJson {
    #[serde(default, rename = "type")]
    recipe_kind: String,
    #[serde(default)]
    space: String,
    #[serde(default)]
    model: String,
    #[serde(default = "default_build_time_secs")]
    build_time: f32,
    #[serde(default)]
    requirements: Vec<BuildingRequirementJson>,
}

/// Anchor mode used when placing structure models from recipes.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BuildingModelAnchor {
    /// Treat model origin as the center of the occupied volume.
    #[default]
    Center,
    /// Treat model origin as minimum corner (x=0,y=0,z=0) of occupied volume.
    #[serde(alias = "corner")]
    MinCorner,
}

/// Runtime metadata loaded from `assets/models/{name}/data.meta.json`.
#[derive(Clone, Debug, Default)]
pub struct BuildingStructureModelMeta {
    pub animated: bool,
    pub model_anchor: BuildingModelAnchor,
    pub model_rotation_quarters: u8,
    pub model_offset: Vec3,
    pub stats: BlockStats,
    pub colliders: BuildingStructureColliderSource,
    pub block_registration: Option<BuildingStructureBlockRegistration>,
    pub textures: Vec<BuildingStructureTextureBinding>,
}

/// Texture source selector for one named model part.
#[derive(Clone, Debug)]
pub enum BuildingStructureTextureSource {
    /// Uses material from one item group (optionally one atlas tile).
    Group {
        group: String,
        tile: Option<[u32; 2]>,
    },
    /// Uses one direct texture path loaded through the asset server.
    DirectPath { asset_path: String },
}

/// Runtime texture binding loaded from `data.meta.json`.
#[derive(Clone, Debug)]
pub struct BuildingStructureTextureBinding {
    pub mesh_name_contains: String,
    pub source: BuildingStructureTextureSource,
    pub uv_repeat: Option<[f32; 2]>,
}

/// Runtime collider definition attached to one placed structure model.
#[derive(Clone, Copy, Debug)]
pub struct BuildingStructureColliderDefinition {
    pub block_entities: bool,
    pub size_m: [f32; 3],
    pub offset_m: [f32; 3],
}

/// Collider source for one structure model.
#[derive(Clone, Debug)]
pub enum BuildingStructureColliderSource {
    Boxes(Vec<BuildingStructureColliderDefinition>),
    Mesh,
}

/// Optional runtime registration data for structure models that should also exist as blocks/items.
#[derive(Clone, Debug)]
pub struct BuildingStructureBlockRegistration {
    pub localized_name: String,
    pub name: String,
    pub item_view: bool,
    pub block_id: Option<u16>,
    pub item_id: Option<ItemId>,
}

/// Registration mode loaded from one structure model meta file.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BuildingModelRegisterAs {
    #[default]
    None,
    Block,
}

impl Default for BuildingStructureColliderSource {
    fn default() -> Self {
        Self::Boxes(Vec::new())
    }
}

#[derive(Deserialize, Default)]
struct BuildingModelMetaJson {
    #[serde(default)]
    animated: bool,
    #[serde(default)]
    model_anchor: BuildingModelAnchor,
    #[serde(default)]
    model_rotation_quarters: i32,
    #[serde(default = "default_model_offset")]
    model_offset: [f32; 3],
    #[serde(default)]
    stats: BlockStats,
    #[serde(default)]
    collider: Option<BuildingModelColliderJson>,
    #[serde(default)]
    colliders: Option<BuildingModelCollidersJson>,
    #[serde(default)]
    register_as: BuildingModelRegisterAs,
    #[serde(default)]
    localized_name: String,
    #[serde(default)]
    name: String,
    #[serde(default = "default_item_view")]
    item_view: bool,
    #[serde(default)]
    textures: HashMap<String, BuildingModelTextureBindingJson>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
enum BuildingModelTextureBindingJson {
    Source(String),
    Detailed(BuildingModelTextureBindingObjectJson),
}

#[derive(Deserialize, Clone, Debug, Default)]
struct BuildingModelTextureBindingObjectJson {
    #[serde(default)]
    source: String,
    #[serde(default)]
    uv_repeat: Option<[f32; 2]>,
}

#[derive(Deserialize, Clone, Debug)]
struct BuildingModelColliderJson {
    #[serde(default = "d_true")]
    block_entities: bool,
    #[serde(default = "default_structure_collider_size_m")]
    size_m: [f32; 3],
    #[serde(default)]
    offset_m: [f32; 3],
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
enum BuildingModelCollidersJson {
    Mode(String),
    Boxes(Vec<BuildingModelColliderJson>),
}

#[derive(Deserialize)]
struct BuildingRequirementJson {
    #[serde(default)]
    item: String,
    #[serde(default)]
    group: String,
    #[serde(default = "default_required_count")]
    count: u16,
}

/// Loads all structure recipes from `assets/recipes/structures`.
pub fn load_building_structure_recipe_registry(
    recipes_dir: &str,
    item_registry: &ItemRegistry,
) -> BuildingStructureRecipeRegistry {
    let mut recipes = Vec::new();
    let mut paths = recipe_json_paths_recursively(Path::new(recipes_dir));
    paths.sort_unstable();

    for path in paths {
        let source_path = path.to_string_lossy().to_string();
        let recipe_name = path
            .file_stem()
            .and_then(|value| value.to_str())
            .map(normalize_recipe_name)
            .unwrap_or_default();
        if recipe_name.is_empty() {
            continue;
        }

        let Ok(raw_json) = fs::read_to_string(path.as_path()) else {
            continue;
        };
        let Ok(recipe_json) = serde_json::from_str::<BuildingRecipeJson>(raw_json.as_str()) else {
            continue;
        };
        if !recipe_json
            .recipe_kind
            .trim()
            .eq_ignore_ascii_case(BUILDING_SHAPED_RECIPE_KIND)
        {
            continue;
        }

        let Some(space) = parse_space(recipe_json.space.as_str()) else {
            continue;
        };
        let Some((model_asset_path, model_meta_path)) =
            resolve_model_asset_and_meta_path(recipe_json.model.as_str())
        else {
            continue;
        };
        let Some(model_meta) = load_structure_model_meta(model_meta_path.as_str()) else {
            continue;
        };

        let mut requirements = Vec::new();
        let mut has_invalid_requirement = false;
        for requirement in recipe_json.requirements {
            let item_name = requirement.item.trim();
            let group_name = normalize_recipe_group(requirement.group.as_str());
            let count = requirement.count.max(1);
            if !item_name.is_empty() {
                let Some(item_id) = item_registry.id_opt(item_name) else {
                    has_invalid_requirement = true;
                    break;
                };
                requirements.push(BuildingMaterialRequirement::item(
                    item_id,
                    item_name.to_string(),
                    count,
                ));
                continue;
            }
            if !group_name.is_empty() {
                requirements.push(BuildingMaterialRequirement::group(group_name, count));
                continue;
            }
            has_invalid_requirement = true;
            break;
        }
        if has_invalid_requirement {
            continue;
        }

        recipes.push(BuildingStructureRecipe {
            name: recipe_name,
            source_path,
            model_asset_path,
            model_meta,
            space,
            build_time_secs: recipe_json.build_time.max(0.0),
            requirements,
        });
    }

    BuildingStructureRecipeRegistry { recipes }
}

fn recipe_json_paths_recursively(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    collect_recipe_json_paths(root, &mut paths);
    paths.sort_unstable();
    paths
}

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

fn parse_space(raw: &str) -> Option<UVec3> {
    let mut tokens = raw
        .trim()
        .split('x')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .take(4);

    let first = tokens.next()?.parse::<u32>().ok()?.max(1);
    let second = tokens.next()?.parse::<u32>().ok()?.max(1);
    let third = tokens.next().and_then(|value| value.parse::<u32>().ok());
    if tokens.next().is_some() {
        return None;
    }

    let (size_x, size_z, size_y) = if let Some(height) = third {
        (first, second, height.max(1))
    } else {
        (first, second, 1)
    };
    Some(UVec3::new(size_x, size_y, size_z))
}

fn resolve_model_asset_and_meta_path(raw: &str) -> Option<(String, String)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Preferred format: `model: "work_table"` => assets/models/work_table/data.glb + data.meta.json
    if !trimmed.contains('/') && !trimmed.ends_with(".glb") {
        let model_name = trimmed.trim_matches('/');
        if model_name.is_empty() {
            return None;
        }
        return Some((
            format!("models/{model_name}/data.glb#Scene0"),
            format!("assets/models/{model_name}/data.meta.json"),
        ));
    }

    // Backward-compatible path format.
    let mut asset_path = if let Some(stripped) = trimmed.strip_prefix("assets/") {
        stripped.to_string()
    } else {
        trimmed.to_string()
    };
    if !asset_path.contains('#') {
        asset_path.push_str("#Scene0");
    }
    let glb_path = asset_path
        .split('#')
        .next()
        .map(str::to_string)
        .unwrap_or_default();
    let meta_path = derive_meta_path_from_glb(glb_path.as_str())?;
    Some((asset_path, meta_path))
}

fn derive_meta_path_from_glb(glb_path: &str) -> Option<String> {
    let path = Path::new(glb_path);
    let file_name = path.file_name()?.to_str()?;
    let dir = path.parent()?;
    let dir_str = dir.to_str()?;

    if file_name.eq_ignore_ascii_case("data.glb") {
        return Some(format!("assets/{dir_str}/data.meta.json"));
    }

    let stem = path.file_stem()?.to_str()?;
    Some(format!("assets/{dir_str}/{stem}.meta.json"))
}

fn load_structure_model_meta(path: &str) -> Option<BuildingStructureModelMeta> {
    let raw = fs::read_to_string(path).ok()?;
    let parsed = serde_json::from_str::<BuildingModelMetaJson>(raw.as_str()).ok()?;

    let mut box_colliders = Vec::new();
    if let Some(collider) = parsed.collider {
        box_colliders.push(collider.sanitized());
    }

    let colliders = match parsed.colliders {
        Some(BuildingModelCollidersJson::Mode(mode)) => {
            if mode.trim().eq_ignore_ascii_case("mesh") {
                BuildingStructureColliderSource::Mesh
            } else {
                return None;
            }
        }
        Some(BuildingModelCollidersJson::Boxes(entries)) => {
            box_colliders.extend(entries.into_iter().map(|collider| collider.sanitized()));
            BuildingStructureColliderSource::Boxes(box_colliders)
        }
        None => BuildingStructureColliderSource::Boxes(box_colliders),
    };

    let fallback_localized_name = Path::new(path)
        .parent()
        .and_then(Path::file_name)
        .and_then(|value| value.to_str())
        .map(|value| format!("{value}_block"))
        .unwrap_or_else(|| "structure_block".to_string());
    let block_registration = match parsed.register_as {
        BuildingModelRegisterAs::None => None,
        BuildingModelRegisterAs::Block => {
            let localized_name = normalize_runtime_block_localized_name(
                parsed.localized_name.as_str(),
                fallback_localized_name.as_str(),
            );
            let name =
                normalize_runtime_block_name_key(parsed.name.as_str(), localized_name.as_str());
            Some(BuildingStructureBlockRegistration {
                localized_name,
                name,
                item_view: parsed.item_view,
                block_id: None,
                item_id: None,
            })
        }
    };

    Some(BuildingStructureModelMeta {
        animated: parsed.animated,
        model_anchor: parsed.model_anchor,
        model_rotation_quarters: normalize_rotation_quarters(parsed.model_rotation_quarters),
        model_offset: Vec3::new(
            parsed.model_offset[0],
            parsed.model_offset[1],
            parsed.model_offset[2],
        ),
        stats: parsed.stats,
        colliders,
        block_registration,
        textures: parse_model_texture_bindings(parsed.textures),
    })
}

fn parse_model_texture_bindings(
    raw: HashMap<String, BuildingModelTextureBindingJson>,
) -> Vec<BuildingStructureTextureBinding> {
    let mut bindings = Vec::new();
    for (matcher_raw, source_config_raw) in raw {
        let matcher = matcher_raw.trim().to_ascii_lowercase();
        if matcher.is_empty() {
            continue;
        }
        let (source_raw, uv_repeat_raw) = match source_config_raw {
            BuildingModelTextureBindingJson::Source(source) => (source, None),
            BuildingModelTextureBindingJson::Detailed(source) => (source.source, source.uv_repeat),
        };
        let Some(source) = parse_texture_source(source_raw.as_str()) else {
            continue;
        };
        bindings.push(BuildingStructureTextureBinding {
            mesh_name_contains: matcher,
            source,
            uv_repeat: normalize_uv_repeat(uv_repeat_raw),
        });
    }
    // Prefer more specific matchers first when keys overlap.
    bindings.sort_by(|a, b| {
        b.mesh_name_contains
            .len()
            .cmp(&a.mesh_name_contains.len())
            .then_with(|| a.mesh_name_contains.cmp(&b.mesh_name_contains))
    });
    bindings
}

fn normalize_uv_repeat(raw: Option<[f32; 2]>) -> Option<[f32; 2]> {
    let [u, v] = raw?;
    let u = if u.is_finite() { u } else { 1.0 };
    let v = if v.is_finite() { v } else { 1.0 };
    Some([u.clamp(0.01, 64.0), v.clamp(0.01, 64.0)])
}

fn parse_texture_source(raw: &str) -> Option<BuildingStructureTextureSource> {
    let cleaned = raw.trim().trim_end_matches(',').trim();
    if cleaned.is_empty() {
        return None;
    }

    if cleaned.contains('[') || cleaned.contains(']') {
        let (group, tile) = parse_group_with_optional_tile(cleaned)?;
        return Some(BuildingStructureTextureSource::Group { group, tile });
    }

    if looks_like_asset_path(cleaned) {
        let asset_path = normalize_asset_relative_path(cleaned);
        if asset_path.is_empty() {
            return None;
        }
        return Some(BuildingStructureTextureSource::DirectPath { asset_path });
    }

    let group = normalize_recipe_group(cleaned);
    if group.is_empty() {
        return None;
    }
    Some(BuildingStructureTextureSource::Group { group, tile: None })
}

fn parse_group_with_optional_tile(raw: &str) -> Option<(String, Option<[u32; 2]>)> {
    let Some(open_bracket) = raw.find('[') else {
        return None;
    };

    let Some(close_bracket) = raw.rfind(']') else {
        return None;
    };
    if close_bracket <= open_bracket {
        return None;
    }

    let group_raw = raw[..open_bracket].trim();
    let group = normalize_recipe_group(group_raw);
    if group.is_empty() {
        return None;
    }

    let tile_raw = &raw[(open_bracket + 1)..close_bracket];
    let mut nums = tile_raw
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let col = nums.next()?.parse::<u32>().ok()?;
    let row = nums.next()?.parse::<u32>().ok()?;
    if nums.next().is_some() {
        return None;
    }
    Some((group, Some([col, row])))
}

#[inline]
fn looks_like_asset_path(raw: &str) -> bool {
    raw.contains('/') || raw.contains('\\') || raw.contains('.')
}

#[inline]
fn normalize_asset_relative_path(raw: &str) -> String {
    let mut value = raw.trim().replace('\\', "/");
    if let Some(stripped) = value.strip_prefix("assets/") {
        value = stripped.to_string();
    }
    if let Some(stripped) = value.strip_prefix("./") {
        value = stripped.to_string();
    }
    value
}

#[inline]
fn normalize_recipe_name(raw: &str) -> String {
    raw.trim().to_ascii_lowercase().replace(' ', "_")
}

#[inline]
fn normalize_recipe_group(raw: &str) -> String {
    raw.trim().to_ascii_lowercase().replace(' ', "_")
}

#[inline]
fn default_required_count() -> u16 {
    1
}

#[inline]
fn default_build_time_secs() -> f32 {
    0.0
}

#[inline]
fn default_model_offset() -> [f32; 3] {
    [0.0, 0.0, 0.0]
}

#[inline]
fn default_item_view() -> bool {
    true
}

#[inline]
fn normalize_runtime_block_localized_name(raw: &str, fallback: &str) -> String {
    let mut value = raw.trim().to_ascii_lowercase();
    if let Some((_, suffix)) = value.rsplit_once(':') {
        value = suffix.to_string();
    }
    if value.is_empty() {
        value = fallback.trim().to_ascii_lowercase();
    }
    let mut normalized = String::with_capacity(value.len().max(16));
    let mut last_sep = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch);
            last_sep = false;
        } else if !last_sep {
            normalized.push('_');
            last_sep = true;
        }
    }
    while normalized.ends_with('_') {
        normalized.pop();
    }
    if normalized.is_empty() {
        "structure_block".to_string()
    } else {
        normalized
    }
}

#[inline]
fn normalize_runtime_block_name_key(raw_name: &str, fallback_localized_name: &str) -> String {
    let trimmed = raw_name.trim();
    if trimmed.starts_with("KEY_")
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
    {
        return trimmed.to_string();
    }
    let mut key = String::with_capacity(fallback_localized_name.len() + 10);
    key.push_str("KEY_");
    let mut last_sep = false;
    for ch in fallback_localized_name.chars() {
        if ch.is_ascii_alphanumeric() {
            key.push(ch.to_ascii_uppercase());
            last_sep = false;
        } else if !last_sep {
            key.push('_');
            last_sep = true;
        }
    }
    while key.ends_with('_') {
        key.pop();
    }
    key
}

#[inline]
fn normalize_rotation_quarters(raw: i32) -> u8 {
    raw.rem_euclid(4) as u8
}

#[inline]
fn default_structure_collider_size_m() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

#[inline]
fn d_true() -> bool {
    true
}

impl BuildingModelColliderJson {
    fn sanitized(self) -> BuildingStructureColliderDefinition {
        BuildingStructureColliderDefinition {
            block_entities: self.block_entities,
            size_m: [
                self.size_m[0].clamp(
                    STRUCTURE_COLLIDER_MIN_SIZE_METERS,
                    STRUCTURE_COLLIDER_MAX_SIZE_METERS,
                ),
                self.size_m[1].clamp(
                    STRUCTURE_COLLIDER_MIN_SIZE_METERS,
                    STRUCTURE_COLLIDER_MAX_SIZE_METERS,
                ),
                self.size_m[2].clamp(
                    STRUCTURE_COLLIDER_MIN_SIZE_METERS,
                    STRUCTURE_COLLIDER_MAX_SIZE_METERS,
                ),
            ],
            offset_m: [
                self.offset_m[0].clamp(
                    -STRUCTURE_COLLIDER_MAX_OFFSET_METERS,
                    STRUCTURE_COLLIDER_MAX_OFFSET_METERS,
                ),
                self.offset_m[1].clamp(
                    -STRUCTURE_COLLIDER_MAX_OFFSET_METERS,
                    STRUCTURE_COLLIDER_MAX_OFFSET_METERS,
                ),
                self.offset_m[2].clamp(
                    -STRUCTURE_COLLIDER_MAX_OFFSET_METERS,
                    STRUCTURE_COLLIDER_MAX_OFFSET_METERS,
                ),
            ],
        }
    }
}
