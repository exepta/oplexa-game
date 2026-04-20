use crate::core::world::chunk::*;
use crate::core::world::chunk_dimension::*;
use crate::core::world::fluid::FluidMap;
use crate::core::world::prop::PropDefinition;
use crate::core::world::world_access_mut;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use serde::Deserialize;
use serde::de::{self, Deserializer};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use oplexa_shared::utils::key_utils::{is_name_key, last_was_separator};
/* ---------------- constants ---------------- */

pub const VOXEL_SIZE: f32 = 1.0;
const ATLAS_PAD_PX: f32 = 0.5;

pub const BASE_BREAK_TIME: f32 = 0.55;
pub const PER_HARDNESS: f32 = 0.45;

pub const MIN_BREAK_TIME: f32 = 0.2;
pub const MAX_BREAK_TIME: f32 = 60.0;
const BLOCK_COLLIDER_MIN_SIZE_METERS: f32 = 0.01;
const BLOCK_COLLIDER_MAX_SIZE_METERS: f32 = 1.0;
const BLOCK_COLLIDER_MAX_OFFSET_METERS: f32 = 1.0;

/* ---------------- types ---------------- */

/// Type alias for block id used by the `core::world::block` module.
pub type BlockId = u16;

/// Defines the possible face variants in the `core::world::block` module.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Face {
    Top,
    Bottom,
    North,
    East,
    South,
    West,
}

/// Represents uv rect used by the `core::world::block` module.
#[derive(Clone, Copy)]
pub struct UvRect {
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
}

/// Connected-texture runtime mapping (mask4 / 16 variants).
#[derive(Clone)]
pub struct ConnectedTextureDef {
    pub group: String,
    pub mask4_tiles: [UvRect; 16],
    pub edge_clip_uv: f32,
}

/// Optional fluid-flow runtime settings for one source block.
#[derive(Clone, Copy, Debug)]
pub struct FluidFlowDef {
    pub step_ms: f32,
}

/// Placement/runtime environment restrictions for one block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockEnvironment {
    Water,
    Overworld,
    Cave,
}

/// Represents a block def used by the `core::world::block` module.
#[derive(Clone)]
pub struct BlockDef {
    pub localized_name: String,
    pub name: String,
    pub mesh_visible: bool,
    pub overridable: bool,
    pub stats: BlockStats,
    pub mining_wobble: MiningWobbleConfig,
    pub prop: Option<PropDefinition>,
    pub collider: BlockColliderDefinition,
    pub uv_top: UvRect,
    pub uv_bottom: UvRect,
    pub uv_north: UvRect,
    pub uv_east: UvRect,
    pub uv_south: UvRect,
    pub uv_west: UvRect,
    pub connected_texture: Option<ConnectedTextureDef>,
    pub fluid_flow: Option<FluidFlowDef>,
    pub water_logged: bool,
    pub allowed_environments: Vec<BlockEnvironment>,
    pub image: Handle<Image>,
    pub material: Handle<StandardMaterial>,
}

/// Represents block stats used by the `core::world::block` module.
#[derive(Deserialize, Clone, Debug, Default)]
#[allow(dead_code)]
pub struct BlockStats {
    #[serde(default)]
    pub hardness: f32,
    #[serde(default)]
    pub blast_resistance: f32,
    #[serde(default, deserialize_with = "deserialize_block_level")]
    pub level: u8,
    #[serde(default = "d_true")]
    pub opaque: bool,
    #[serde(default)]
    pub fluid: bool,
    #[serde(default)]
    pub fluid_level: u8,
    #[serde(default)]
    pub foliage: bool,
    #[serde(default = "d_true")]
    pub solid: bool,
    #[serde(default)]
    pub emissive: f32,
}

/// Per-block configuration for mining hit wobble in terrain shader.
#[derive(Deserialize, Clone, Copy, Debug)]
pub struct MiningWobbleConfig {
    #[serde(default = "d_true")]
    pub enabled: bool,
    #[serde(default = "default_mining_wobble_amplitude")]
    pub amplitude: f32,
    #[serde(default = "default_mining_wobble_frequency")]
    pub frequency: f32,
    #[serde(default = "default_mining_wobble_vertical_scale")]
    pub vertical_scale: f32,
}

impl Default for MiningWobbleConfig {
    fn default() -> Self {
        Self {
            enabled: d_true(),
            amplitude: default_mining_wobble_amplitude(),
            frequency: default_mining_wobble_frequency(),
            vertical_scale: default_mining_wobble_vertical_scale(),
        }
    }
}

impl MiningWobbleConfig {
    pub fn sanitized(mut self) -> Self {
        self.amplitude = self.amplitude.clamp(0.0, 0.20);
        self.frequency = self.frequency.clamp(0.1, 120.0);
        self.vertical_scale = self.vertical_scale.clamp(0.0, 2.0);
        self
    }
}

/// Defines collider behavior for one block.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BlockColliderKind {
    /// Legacy/default behavior: solid non-fluid blocks collide with their meshed geometry.
    #[default]
    Auto,
    /// No collider.
    None,
    /// Full block collider (1x1x1) regardless of render mesh.
    FullBlock,
    /// Axis-aligned box collider with explicit size/offset.
    Box,
}

/// Runtime collider definition attached to one block entry.
#[derive(Clone, Debug, Deserialize)]
pub struct BlockColliderDefinition {
    #[serde(default)]
    pub kind: BlockColliderKind,
    #[serde(default = "d_true")]
    pub block_entities: bool,
    #[serde(default = "default_block_collider_size_m")]
    pub size_m: [f32; 3],
    #[serde(default)]
    pub offset_m: [f32; 3],
}

impl Default for BlockColliderDefinition {
    fn default() -> Self {
        Self {
            kind: BlockColliderKind::Auto,
            block_entities: true,
            size_m: default_block_collider_size_m(),
            offset_m: [0.0, 0.0, 0.0],
        }
    }
}

impl BlockColliderDefinition {
    pub fn sanitized(mut self) -> Self {
        self.size_m = [
            self.size_m[0].clamp(
                BLOCK_COLLIDER_MIN_SIZE_METERS,
                BLOCK_COLLIDER_MAX_SIZE_METERS,
            ),
            self.size_m[1].clamp(
                BLOCK_COLLIDER_MIN_SIZE_METERS,
                BLOCK_COLLIDER_MAX_SIZE_METERS,
            ),
            self.size_m[2].clamp(
                BLOCK_COLLIDER_MIN_SIZE_METERS,
                BLOCK_COLLIDER_MAX_SIZE_METERS,
            ),
        ];
        self.offset_m = [
            self.offset_m[0].clamp(
                -BLOCK_COLLIDER_MAX_OFFSET_METERS,
                BLOCK_COLLIDER_MAX_OFFSET_METERS,
            ),
            self.offset_m[1].clamp(
                -BLOCK_COLLIDER_MAX_OFFSET_METERS,
                BLOCK_COLLIDER_MAX_OFFSET_METERS,
            ),
            self.offset_m[2].clamp(
                -BLOCK_COLLIDER_MAX_OFFSET_METERS,
                BLOCK_COLLIDER_MAX_OFFSET_METERS,
            ),
        ];
        self
    }
}

/// Represents the selected block used by the `core::world::block` module.
#[derive(Resource, Clone, Debug)]
pub struct SelectedBlock {
    pub id: u16,
    pub name: String,
}

impl Default for SelectedBlock {
    /// Runs the `default` routine for default in the `core::world::block` module.
    fn default() -> Self {
        Self {
            id: 0,
            name: "air".to_string(),
        }
    }
}

struct LoadedBlockCommon {
    localized_name: String,
    display_name: String,
    overridable: bool,
    stats: BlockStats,
    mining_wobble: MiningWobbleConfig,
    prop: Option<PropDefinition>,
    collider: BlockColliderDefinition,
    fluid_flow: Option<FluidFlowDef>,
    water_logged: bool,
    allowed_environments: Vec<BlockEnvironment>,
}

struct LoadedBlockVisual {
    uv_top: UvRect,
    uv_bottom: UvRect,
    uv_north: UvRect,
    uv_east: UvRect,
    uv_south: UvRect,
    uv_west: UvRect,
    connected_texture: Option<ConnectedTextureDef>,
    image: Handle<Image>,
    material: Handle<StandardMaterial>,
}

/* ---------------- registry ---------------- */

/// Represents a block registry used by the `core::world::block` module.
#[derive(Resource, Clone)]
pub struct BlockRegistry {
    pub defs: Vec<BlockDef>,
    pub name_to_id: HashMap<String, BlockId>,
}

impl BlockRegistry {
    /// Runs the `def` routine for def in the `core::world::block` module.
    #[inline]
    pub fn def(&self, id: BlockId) -> &BlockDef {
        &self.defs[id as usize]
    }
    /// Returns an optional block definition.
    #[inline]
    pub fn def_opt(&self, id: BlockId) -> Option<&BlockDef> {
        self.defs.get(id as usize)
    }
    /// Runs the `name` routine for name in the `core::world::block` module.
    #[inline]
    pub fn name(&self, id: BlockId) -> &str {
        self.def(id).localized_name.as_str()
    }

    /// Runs the `name_opt` routine for name opt in the `core::world::block` module.
    #[inline]
    pub fn name_opt(&self, id: BlockId) -> Option<&str> {
        self.defs
            .get(id as usize)
            .map(|d| d.localized_name.as_str())
    }

    /// Runs the `display_name` routine for display name in the `core::world::block` module.
    #[inline]
    pub fn display_name(&self, id: BlockId) -> &str {
        self.def(id).name.as_str()
    }

    /// Runs the `display_name_opt` routine for display name opt in the `core::world::block` module.
    #[inline]
    pub fn display_name_opt(&self, id: BlockId) -> Option<&str> {
        self.defs.get(id as usize).map(|d| d.name.as_str())
    }

    /// Runs the `id_opt` routine for id opt in the `core::world::block` module.
    #[inline]
    pub fn id_opt(&self, name: &str) -> Option<BlockId> {
        self.name_to_id.get(name).copied()
    }

    /// Runs the `id` routine for id in the `core::world::block` module.
    #[inline]
    pub fn id(&self, name: &str) -> BlockId {
        *self.name_to_id.get(name).expect("unknown block name")
    }

    /// Runs the `id_or_air` routine for id or air in the `core::world::block` module.
    #[inline]
    pub fn id_or_air(&self, name: &str) -> BlockId {
        self.id_opt(name).unwrap_or(0)
    }

    /// Runs the `material` routine for material in the `core::world::block` module.
    #[inline]
    pub fn material(&self, id: BlockId) -> Handle<StandardMaterial> {
        self.def(id).material.clone()
    }

    /// Runs the `stats` routine for stats in the `core::world::block` module.
    #[inline]
    pub fn stats(&self, id: BlockId) -> &BlockStats {
        &self.def(id).stats
    }
    /// Returns prop metadata for one block id.
    #[inline]
    pub fn prop(&self, id: BlockId) -> Option<&PropDefinition> {
        self.def(id).prop.as_ref()
    }
    /// Checks whether air in the `core::world::block` module.
    #[inline]
    pub fn is_air(&self, id: BlockId) -> bool {
        id == 0
    }
    /// Checks whether opaque in the `core::world::block` module.
    #[inline]
    pub fn is_opaque(&self, id: BlockId) -> bool {
        self.stats(id).opaque
    }
    /// Checks whether fluid in the `core::world::block` module.
    #[inline]
    pub fn is_fluid(&self, id: BlockId) -> bool {
        self.stats(id).fluid
    }
    /// Returns fluid fill level (0..10). Non-fluid blocks always return 0.
    #[inline]
    pub fn fluid_level(&self, id: BlockId) -> u8 {
        self.stats(id).fluid_level
    }
    /// Returns optional fluid-flow settings for one source block.
    #[inline]
    pub fn fluid_flow(&self, id: BlockId) -> Option<FluidFlowDef> {
        self.def(id).fluid_flow
    }
    /// Checks whether this block has a prop render definition.
    #[inline]
    pub fn is_prop(&self, id: BlockId) -> bool {
        self.prop(id).is_some()
    }
    /// Returns whether this block can be replaced directly by placement.
    #[inline]
    pub fn is_overridable(&self, id: BlockId) -> bool {
        self.def(id).overridable
    }
    /// Returns true when the given prop block can be placed on the given ground block.
    #[inline]
    pub fn prop_allows_ground(&self, prop_id: BlockId, ground_id: BlockId) -> bool {
        let Some(prop) = self.prop(prop_id) else {
            return true;
        };
        let Some(ground_name) = self.name_opt(ground_id) else {
            return false;
        };
        prop.allows_ground_name(ground_name)
    }
    /// Returns whether this block can coexist with water inside one voxel.
    #[inline]
    pub fn is_water_logged(&self, id: BlockId) -> bool {
        self.def(id).water_logged
    }
    /// Returns configured allowed environments for this block.
    #[inline]
    pub fn allowed_environments(&self, id: BlockId) -> &[BlockEnvironment] {
        self.def(id).allowed_environments.as_slice()
    }
    /// Returns true when this block should use the render mesh for collision.
    #[inline]
    pub fn collision_uses_render_mesh(&self, id: BlockId) -> bool {
        let collider = &self.def(id).collider;
        if !collider.block_entities {
            return false;
        }
        match collider.kind {
            BlockColliderKind::Auto => {
                !self.is_air(id) && !self.is_fluid(id) && self.stats(id).solid
            }
            BlockColliderKind::None | BlockColliderKind::FullBlock | BlockColliderKind::Box => {
                false
            }
        }
    }
    /// Returns optional box collider size/offset (both in block units).
    #[inline]
    pub fn collision_box(&self, id: BlockId) -> Option<([f32; 3], [f32; 3])> {
        let collider = &self.def(id).collider;
        if !collider.block_entities {
            return None;
        }
        match collider.kind {
            BlockColliderKind::Auto | BlockColliderKind::None => None,
            BlockColliderKind::FullBlock => Some(([1.0, 1.0, 1.0], [0.0, 0.0, 0.0])),
            BlockColliderKind::Box => Some((collider.size_m, collider.offset_m)),
        }
    }
    /// Returns true when this block should participate in world collision meshes.
    #[inline]
    pub fn is_solid_for_collision(&self, id: BlockId) -> bool {
        self.collision_uses_render_mesh(id) || self.collision_box(id).is_some()
    }
    /// Returns selection hitbox size/offset (both in block units), independent from physics passability.
    #[inline]
    pub fn selection_box(&self, id: BlockId) -> Option<([f32; 3], [f32; 3])> {
        let collider = &self.def(id).collider;
        match collider.kind {
            BlockColliderKind::FullBlock => return Some(([1.0, 1.0, 1.0], [0.0, 0.0, 0.0])),
            BlockColliderKind::Box => return Some((collider.size_m, collider.offset_m)),
            BlockColliderKind::Auto => {
                if !self.is_air(id) && !self.is_fluid(id) && self.stats(id).solid {
                    return Some(([1.0, 1.0, 1.0], [0.0, 0.0, 0.0]));
                }
            }
            BlockColliderKind::None => {}
        }
        if let Some(prop) = self.prop(id) {
            let height = prop.height_m;
            let width = prop.width_m;
            let offset_y = (height * 0.5) - 0.5;
            return Some(([width, height, width], [0.0, offset_y, 0.0]));
        }
        None
    }
    /// Runs the `emissive` routine for emissive in the `core::world::block` module.
    #[inline]
    pub fn emissive(&self, id: BlockId) -> f32 {
        self.stats(id).emissive
    }
    /// Runs the `hardness` routine for hardness in the `core::world::block` module.
    #[inline]
    pub fn hardness(&self, id: BlockId) -> f32 {
        self.stats(id).hardness.max(0.0)
    }

    /// Runs the `level` routine for level in the `core::world::block` module.
    #[inline]
    pub fn level(&self, id: BlockId) -> u8 {
        self.stats(id).level.min(6)
    }

    /// Runs the `uv` routine for uv in the `core::world::block` module.
    #[inline]
    pub fn uv(&self, id: BlockId, face: Face) -> UvRect {
        let d = self.def(id);
        match face {
            Face::Top => d.uv_top,
            Face::Bottom => d.uv_bottom,
            Face::North => d.uv_north,
            Face::East => d.uv_east,
            Face::South => d.uv_south,
            Face::West => d.uv_west,
        }
    }

    /// Runs the `face_uvs` routine for face uvs in the `core::world::block` module.
    #[inline]
    pub fn face_uvs(&self, id: BlockId) -> (UvRect, UvRect, UvRect, UvRect, UvRect, UvRect) {
        let d = self.def(id);
        (
            d.uv_top,
            d.uv_bottom,
            d.uv_north,
            d.uv_east,
            d.uv_south,
            d.uv_west,
        )
    }

    /// Loads all for the `core::world::block` module.
    pub fn load_all(
        asset_server: &AssetServer,
        materials: &mut Assets<StandardMaterial>,
        blocks_dir: &str,
    ) -> Self {
        Self::load_with_visual_builder(blocks_dir, |block_json, common| {
            let tex_dir = block_json
                .texture_dir
                .clone()
                .unwrap_or_else(|| guess_tex_dir_from_block_name(&common.localized_name));

            // Tileset is read from disk (not via asset server).
            let tileset_path = format!("assets/{}/data.json", tex_dir);
            let tileset: BlockTileset = read_json(&tileset_path);

            // Atlas image is loaded via asset_server.
            let atlas_path = format!("{}/{}", tex_dir, tileset.image);
            let image: Handle<Image> = asset_server.load(atlas_path);

            // Resolve faces (supports: specific keys, 'all', 'vertical', 'horizontal', and 'nord' fallback).
            let faces = block_json.texture.resolve();
            let uv_top = tile_uv(
                &tileset,
                require_face(&faces.top, "top", &common.localized_name),
            )
            .unwrap();
            let uv_bottom = tile_uv(
                &tileset,
                require_face(&faces.bottom, "bottom", &common.localized_name),
            )
            .unwrap();
            let uv_north = tile_uv(
                &tileset,
                require_face(&faces.north, "north", &common.localized_name),
            )
            .unwrap();
            let uv_east = tile_uv(
                &tileset,
                require_face(&faces.east, "east", &common.localized_name),
            )
            .unwrap();
            let uv_south = tile_uv(
                &tileset,
                require_face(&faces.south, "south", &common.localized_name),
            )
            .unwrap();
            let uv_west = tile_uv(
                &tileset,
                require_face(&faces.west, "west", &common.localized_name),
            )
            .unwrap();
            let connected_texture = resolve_connected_texture(
                block_json.connected_texture.as_ref(),
                &tileset,
                faces.north,
                &common.localized_name,
            );
            let material = add_standard_block_material(materials, &image, &common.stats);

            loaded_block_visual_from_parts(
                (uv_top, uv_bottom, uv_north, uv_east, uv_south, uv_west),
                connected_texture,
                image,
                material,
            )
        })
    }

    /// Loads headless for the `core::world::block` module.
    pub fn load_headless(blocks_dir: &str) -> Self {
        Self::load_with_visual_builder(blocks_dir, |_, _| headless_loaded_block_visual())
    }

    /// Ensures that one runtime-defined block exists and returns its id.
    pub fn ensure_runtime_block(
        &mut self,
        asset_server: &AssetServer,
        materials: &mut Assets<StandardMaterial>,
        localized_name: &str,
        name: &str,
        stats: BlockStats,
    ) -> BlockId {
        let normalized_stats = normalize_block_stats(stats);
        let image: Handle<Image> = asset_server.load("textures/items/missing.png");
        let material = add_standard_block_material(materials, &image, &normalized_stats);
        self.ensure_runtime_block_with_visual(
            localized_name,
            name,
            normalized_stats,
            image,
            material,
        )
    }

    /// Ensures that one runtime-defined block exists in headless mode and returns its id.
    pub fn ensure_runtime_block_headless(
        &mut self,
        localized_name: &str,
        name: &str,
        stats: BlockStats,
    ) -> BlockId {
        self.ensure_runtime_block_with_visual(
            localized_name,
            name,
            normalize_block_stats(stats),
            Handle::default(),
            Handle::default(),
        )
    }

    fn ensure_runtime_block_with_visual(
        &mut self,
        localized_name: &str,
        name: &str,
        stats: BlockStats,
        image: Handle<Image>,
        material: Handle<StandardMaterial>,
    ) -> BlockId {
        let normalized_localized_name = normalize_runtime_block_localized_name(localized_name);
        if let Some(existing) = self.id_opt(normalized_localized_name.as_str()) {
            return existing;
        }

        let display_name =
            normalize_runtime_block_name_key(name, normalized_localized_name.as_str());
        let id = self.defs.len() as BlockId;
        self.name_to_id
            .insert(normalized_localized_name.clone(), id);
        self.defs.push(runtime_block_def(
            normalized_localized_name,
            display_name,
            stats,
            image,
            material,
        ));
        id
    }

    fn load_with_visual_builder<F>(blocks_dir: &str, mut visual_builder: F) -> Self
    where
        F: FnMut(&BlockJson, &LoadedBlockCommon) -> LoadedBlockVisual,
    {
        let (mut defs, mut name_to_id) = registry_store_with_air();

        for path in block_json_paths(blocks_dir) {
            let block_json: BlockJson = read_json(path.to_str().unwrap());
            let common = build_loaded_block_common(&block_json);
            let visual = visual_builder(&block_json, &common);
            insert_loaded_block_and_variants(&mut defs, &mut name_to_id, common, visual);
        }

        Self { defs, name_to_id }
    }
}

#[derive(Clone, Copy)]
enum AutoSlabVariant {
    Bottom,
    Top,
    North,
    South,
    East,
    West,
}

fn append_auto_slab_variants(
    defs: &mut Vec<BlockDef>,
    name_to_id: &mut HashMap<String, BlockId>,
    base_id: BlockId,
) {
    let base_index = base_id as usize;
    let Some(base_name_prefix) = defs[base_index]
        .localized_name
        .strip_suffix("_slab_block")
        .map(str::to_string)
    else {
        return;
    };
    if defs[base_index].collider.kind != BlockColliderKind::Box {
        return;
    }

    let source = defs[base_index].clone();
    defs[base_index].collider = slab_variant_collider(&source.collider, AutoSlabVariant::Bottom);

    let variants = [
        ("_slab_top_block", AutoSlabVariant::Top),
        ("_slab_north_block", AutoSlabVariant::North),
        ("_slab_south_block", AutoSlabVariant::South),
        ("_slab_east_block", AutoSlabVariant::East),
        ("_slab_west_block", AutoSlabVariant::West),
    ];

    for (suffix, variant) in variants {
        let localized_name = format!("{base_name_prefix}{suffix}");

        let mut def = source.clone();
        def.name = normalize_block_name_key("", localized_name.as_str());
        def.collider = slab_variant_collider(&source.collider, variant);
        insert_variant_block_if_absent(defs, name_to_id, localized_name, def);
    }
}

fn append_auto_fluid_flow_variants(
    defs: &mut Vec<BlockDef>,
    name_to_id: &mut HashMap<String, BlockId>,
    source_id: BlockId,
) {
    let source = defs[source_id as usize].clone();
    if !source.stats.fluid {
        return;
    }
    if source.fluid_flow.is_none() {
        return;
    }

    for level in 1..=10u8 {
        let localized_name = format!("{}_flow_{level}", source.localized_name);

        let mut def = source.clone();
        def.name = format!("{} Flow {}", source.name, level);
        def.overridable = true;
        def.stats.fluid_level = level;
        def.stats.fluid = true;
        def.stats.solid = false;
        def.stats.opaque = false;
        def.collider = fluid_level_visual_box(level);
        // Variants are render-only levels; the source keeps flow settings.
        def.fluid_flow = None;

        let Some(id) = insert_variant_block_if_absent(defs, name_to_id, localized_name, def) else {
            continue;
        };
        if let Some(alias_prefix) = source.localized_name.strip_suffix("_block") {
            let legacy_alias = format!("{alias_prefix}_flow_{level}");
            name_to_id.entry(legacy_alias).or_insert(id);
        }
    }
}

#[inline]
fn fluid_level_visual_box(level: u8) -> BlockColliderDefinition {
    let h = (level as f32 / 10.0).clamp(0.1, 1.0);
    let offset_y = (h - 1.0) * 0.5;
    BlockColliderDefinition {
        kind: BlockColliderKind::Box,
        block_entities: false,
        size_m: [1.0, h, 1.0],
        offset_m: [0.0, offset_y, 0.0],
    }
}

fn slab_variant_collider(
    source: &BlockColliderDefinition,
    variant: AutoSlabVariant,
) -> BlockColliderDefinition {
    let thickness = source.size_m[1].clamp(
        BLOCK_COLLIDER_MIN_SIZE_METERS,
        BLOCK_COLLIDER_MAX_SIZE_METERS,
    );
    let side_offset = (1.0 - thickness).max(0.0) * 0.5;
    let (size_m, offset_m) = match variant {
        AutoSlabVariant::Bottom => ([1.0, thickness, 1.0], [0.0, -side_offset, 0.0]),
        AutoSlabVariant::Top => ([1.0, thickness, 1.0], [0.0, side_offset, 0.0]),
        AutoSlabVariant::North => ([1.0, 1.0, thickness], [0.0, 0.0, -side_offset]),
        AutoSlabVariant::South => ([1.0, 1.0, thickness], [0.0, 0.0, side_offset]),
        AutoSlabVariant::East => ([thickness, 1.0, 1.0], [side_offset, 0.0, 0.0]),
        AutoSlabVariant::West => ([thickness, 1.0, 1.0], [-side_offset, 0.0, 0.0]),
    };
    BlockColliderDefinition {
        kind: BlockColliderKind::Box,
        block_entities: source.block_entities,
        size_m,
        offset_m,
    }
}

/* ---------------- optional enum helpers ---------------- */

/// Defines the possible blocks variants in the `core::world::block` module.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Blocks {
    Dirt,
    Grass,
    Stone,
    Log,
    Sand,
    Water,
    Glass,
    Border,
    Clay,
    Gravel,
    DeepStone,
    SandStone,
    Snow,
}
impl Blocks {
    /// Runs the `localized_name` routine for localized name in the `core::world::block` module.
    pub const fn localized_name(self) -> &'static str {
        match self {
            Blocks::Dirt => "dirt_block",
            Blocks::Grass => "grass_block",
            Blocks::Stone => "stone_block",
            Blocks::Log => "oak_log_block",
            Blocks::Sand => "sand_block",
            Blocks::Water => "water_block",
            Blocks::Glass => "glass_block",
            Blocks::Border => "border_block",
            Blocks::Clay => "clay_block",
            Blocks::Gravel => "gravel_block",
            Blocks::DeepStone => "deep_stone_block",
            Blocks::SandStone => "sand_stone_block",
            Blocks::Snow => "snow_block",
        }
    }
}
impl AsRef<str> for Blocks {
    /// Runs the `as_ref` routine for as ref in the `core::world::block` module.
    fn as_ref(&self) -> &str {
        self.localized_name()
    }
}

/* ---------------- mining helpers ---------------- */

/// Represents mining overlay root used by the `core::world::block` module.
#[derive(Resource, Default)]
pub struct MiningOverlayRoot(pub Option<Entity>);

/// Represents mining state used by the `core::world::block` module.
#[derive(Resource, Default)]
pub struct MiningState {
    pub target: Option<MiningTarget>,
}

/// Represents mining target used by the `core::world::block` module.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MiningTarget {
    pub loc: IVec3,
    pub id: BlockId,
    pub started_at: f32,
    pub duration: f32,
}

/* ---------------- dirs/offsets ---------------- */

#[allow(dead_code)]
pub const DIR4_XZ: [IVec2; 4] = [
    IVec2::new(1, 0),
    IVec2::new(-1, 0),
    IVec2::new(0, 1),
    IVec2::new(0, -1),
];

#[allow(dead_code)]
pub const DIR6: [IVec3; 6] = [
    IVec3::new(1, 0, 0),  // +X
    IVec3::new(-1, 0, 0), // -X
    IVec3::new(0, 1, 0),  // +Y
    IVec3::new(0, -1, 0), // -Y
    IVec3::new(0, 0, 1),  // +Z
    IVec3::new(0, 0, -1), // -Z
];

/// Runs the `face_offset` routine for face offset in the `core::world::block` module.
#[inline]
pub fn face_offset(f: Face) -> IVec3 {
    match f {
        Face::East => IVec3::new(1, 0, 0),
        Face::West => IVec3::new(-1, 0, 0),
        Face::Top => IVec3::new(0, 1, 0),
        Face::Bottom => IVec3::new(0, -1, 0),
        Face::South => IVec3::new(0, 0, 1),
        Face::North => IVec3::new(0, 0, -1),
    }
}

/// Runs the `neighbor_world` routine for neighbor world in the `core::world::block` module.
#[inline]
pub fn neighbor_world(wp: IVec3, f: Face) -> IVec3 {
    wp + face_offset(f)
}

/* ---------------- space conversions ---------------- */

/// Runs the `to_block_space` routine for to block space in the `core::world::block` module.
#[inline]
pub fn to_block_space(v: Vec3) -> Vec3 {
    v / VOXEL_SIZE
}
/// Runs the `to_world_space` routine for to world space in the `core::world::block` module.
#[inline]
pub fn to_world_space(v: Vec3) -> Vec3 {
    v * VOXEL_SIZE
}

/// Runs the `block_center_world` routine for block center world in the `core::world::block` module.
#[inline]
pub fn block_center_world(wp: IVec3) -> Vec3 {
    let s = VOXEL_SIZE;
    Vec3::new(
        (wp.x as f32 + 0.5) * s,
        (wp.y as f32 + 0.5) * s,
        (wp.z as f32 + 0.5) * s,
    )
}

/// Runs the `block_origin_world` routine for block origin world in the `core::world::block` module.
#[inline]
pub fn block_origin_world(wp: IVec3) -> Vec3 {
    to_world_space(Vec3::new(wp.x as f32, wp.y as f32, wp.z as f32))
}

/// Represents aabb3 used by the `core::world::block` module.
#[derive(Clone, Copy, Debug)]
pub struct Aabb3 {
    pub min: Vec3,
    pub max: Vec3,
}

/// Runs the `block_aabb_world` routine for block aabb world in the `core::world::block` module.
#[inline]
pub fn block_aabb_world(wp: IVec3) -> Aabb3 {
    let o = block_origin_world(wp);
    let s = VOXEL_SIZE;
    Aabb3 {
        min: o,
        max: o + Vec3::splat(s),
    }
}

/* ---------------- chunk lookups ---------------- */

/// Runs the `chunk_and_local_from_world` routine for chunk and local from world in the `core::world::block` module.
#[inline]
pub fn chunk_and_local_from_world(wp: IVec3) -> (IVec2, usize, usize, usize) {
    let (cc, l) = world_to_chunk_xz(wp.x, wp.z);
    let lx = l.x as usize;
    let lz = l.y as usize;
    let ly = world_y_to_local(wp.y);
    (cc, lx, ly, lz)
}

/// Runs the `face_visible_against` routine for face visible against in the `core::world::block` module.
#[inline]
pub fn face_visible_against(reg: &BlockRegistry, self_id: BlockId, neigh_id: BlockId) -> bool {
    if reg.is_air(self_id) {
        return false;
    }
    if reg.is_air(neigh_id) {
        return true;
    }
    if self_id == neigh_id && !reg.is_opaque(self_id) {
        return false;
    }
    if reg.is_fluid(self_id) && reg.is_fluid(neigh_id) {
        return false;
    }
    !reg.is_opaque(neigh_id)
}

/// Runs the `water_face_visible_against` routine for water face visible against in the `core::world::block` module.
#[inline]
pub fn water_face_visible_against(reg: &BlockRegistry, neigh_id: BlockId) -> bool {
    if reg.is_fluid(neigh_id) {
        return false;
    }
    !reg.is_opaque(neigh_id)
}

/// Returns block world for the `core::world::block` module.
#[inline]
pub fn get_block_world(chunk_map: &ChunkMap, wp: IVec3) -> BlockId {
    get_block_world_impl(chunk_map, wp, false)
}

/// Returns stacked block world for the `core::world::block` module.
#[inline]
pub fn get_stacked_block_world(chunk_map: &ChunkMap, wp: IVec3) -> BlockId {
    get_block_world_impl(chunk_map, wp, true)
}

#[inline]
fn get_block_world_impl(chunk_map: &ChunkMap, wp: IVec3, stacked: bool) -> BlockId {
    if wp.y < Y_MIN || wp.y > Y_MAX {
        return 0;
    }
    let (cc, local) = world_to_chunk_xz(wp.x, wp.z);
    let Some(chunk) = chunk_map.chunks.get(&cc) else {
        return 0;
    };
    let lx = local.x as usize;
    let lz = local.y as usize;
    let ly = world_y_to_local(wp.y);
    if stacked {
        chunk.get_stacked(lx, ly, lz)
    } else {
        chunk.get(lx, ly, lz)
    }
}

/// Returns id world for the `core::world::block` module.
#[inline]
pub fn get_id_world(chunk_map: &ChunkMap, wp: IVec3) -> BlockId {
    get_block_world(chunk_map, wp)
}

/// Sets id world for the `core::world::block` module.
pub fn set_id_world(chunk_map: &mut ChunkMap, wp: IVec3, id: BlockId) -> Option<BlockId> {
    set_id_world_impl(chunk_map, wp, id, false)
}

/// Sets stacked id world for the `core::world::block` module.
pub fn set_stacked_id_world(chunk_map: &mut ChunkMap, wp: IVec3, id: BlockId) -> Option<BlockId> {
    set_id_world_impl(chunk_map, wp, id, true)
}

fn set_id_world_impl(
    chunk_map: &mut ChunkMap,
    wp: IVec3,
    id: BlockId,
    stacked: bool,
) -> Option<BlockId> {
    let Some(mut access) = world_access_mut(chunk_map, wp) else {
        return None;
    };
    let old = if stacked {
        access.get_stacked()
    } else {
        access.get()
    };
    if stacked {
        access.set_stacked(id);
    } else {
        access.set(id);
    }
    Some(old)
}

/// Runs the `place_if_air` routine for place if air in the `core::world::block` module.
pub fn place_if_air(chunk_map: &mut ChunkMap, wp: IVec3, id: BlockId) -> Result<(), ()> {
    if get_id_world(chunk_map, wp) == 0 {
        set_id_world(chunk_map, wp, id);
        Ok(())
    } else {
        Err(())
    }
}

/// Runs the `fluid_at_world` routine for fluid at world in the `core::world::block` module.
#[inline]
pub fn fluid_at_world(fluids: &FluidMap, wx: i32, wy: i32, wz: i32) -> bool {
    if wy < Y_MIN || wy > Y_MAX {
        return false;
    }
    let (cc, local) = world_to_chunk_xz(wx, wz);
    let lx = local.x as usize;
    let lz = local.y as usize;
    let ly = (wy - Y_MIN) as usize;
    match fluids.0.get(&cc) {
        Some(fc) => fc.get(lx, ly, lz),
        None => false,
    }
}

/* ---------------- misc helpers ---------------- */

/// Runs the `block_name_from_registry` routine for block name from registry in the `core::world::block` module.
#[inline]
pub fn block_name_from_registry(reg: &BlockRegistry, id: BlockId) -> String {
    reg.name_to_id
        .iter()
        .find(|&(_, &bid)| bid == id)
        .map(|(name, _)| name.clone())
        .unwrap_or_else(|| format!("#{id}"))
}

/// Runs the `break_time_for` routine for break time for in the `core::world::block` module.
#[inline]
pub fn break_time_for(id: BlockId, registry: &BlockRegistry) -> f32 {
    let time = registry.hardness(id);
    (BASE_BREAK_TIME + PER_HARDNESS * time).clamp(MIN_BREAK_TIME, MAX_BREAK_TIME)
}

/// Runs the `mining_progress` routine for mining progress in the `core::world::block` module.
#[inline]
pub fn mining_progress(now: f32, target: &MiningTarget) -> f32 {
    ((now - target.started_at) / target.duration).clamp(0.0, 1.0)
}

/* ---------------- spawning ---------------- */

/// Spawns block by id for the `core::world::block` module.
pub fn spawn_block_by_id(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    reg: &BlockRegistry,
    id: BlockId,
    world_pos: Vec3,
    size: f32,
) {
    // Uses deduplicated mesh builder
    let mesh = build_block_cube_mesh(reg, id, size);
    commands.spawn((
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(reg.material(id)),
        Transform::from_translation(world_pos + Vec3::splat(size * 0.5)),
        Name::new(reg.name(id).to_string()),
    ));
}

/// Spawns block by name for the `core::world::block` module.
pub fn spawn_block_by_name<P: AsRef<str>>(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    reg: &BlockRegistry,
    block_ref: P,
    world_pos: Vec3,
    size: f32,
) {
    let id = reg.id(block_ref.as_ref());
    spawn_block_by_id(commands, meshes, reg, id, world_pos, size);
}

/// Runs the `id_any` routine for id any in the `core::world::block` module.
#[inline]
pub fn id_any(reg: &BlockRegistry, names: &[&str]) -> BlockId {
    for n in names {
        if let Some(&id) = reg.name_to_id.get(*n) {
            return id;
        }
    }
    0
}

/* ---------------- internal structs ---------------- */

const Z: UvRect = UvRect {
    u0: 0.0,
    v0: 0.0,
    u1: 0.0,
    v1: 0.0,
};

/// Runs the `block_json_paths` routine for block json paths in the `core::world::block` module.
fn block_json_paths(blocks_dir: &str) -> Vec<PathBuf> {
    let dir = Path::new(blocks_dir);
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            paths.push(path);
        }
    }
    paths.sort_unstable();
    paths
}

/// Represents block tileset used by the `core::world::block` module.
#[derive(Deserialize)]
struct BlockTileset {
    pub image: String,
    pub tile_size: u32,
    pub columns: u32,
    pub rows: u32,
    pub tiles: HashMap<String, [u32; 2]>,
}

/// Represents block json used by the `core::world::block` module.
#[derive(Deserialize)]
struct BlockJson {
    #[serde(default)]
    pub localized_name: String,
    #[serde(default)]
    pub name: String,
    pub texture_dir: Option<String>,
    pub texture: TextureFacesJson,
    #[serde(default)]
    pub stats: BlockStats,
    #[serde(default)]
    pub mining_wobble: MiningWobbleConfig,
    #[serde(default)]
    pub overridable: bool,
    #[serde(default)]
    pub prop: Option<PropDefinition>,
    #[serde(default)]
    pub collider: BlockColliderDefinition,
    #[serde(default)]
    pub connected_texture: Option<ConnectedTextureJson>,
    #[serde(default)]
    pub fluid_flow: Option<FluidFlowJson>,
    #[serde(default)]
    pub water_logged: bool,
    #[serde(default)]
    pub allowed_environments: Vec<String>,
}

/// Connected texture JSON schema for block defs.
#[derive(Deserialize, Default)]
struct ConnectedTextureJson {
    #[serde(default)]
    pub group: String,
    #[serde(default = "default_connected_texture_mode")]
    pub mode: String,
    #[serde(default)]
    pub mask_tiles: HashMap<String, String>,
    #[serde(default)]
    pub edge_clip_px: f32,
}

#[derive(Deserialize, Clone, Copy, Debug)]
struct FluidFlowJson {
    #[serde(default = "d_true")]
    pub enabled: bool,
    #[serde(default = "default_fluid_flow_step_ms")]
    pub step_ms: f32,
}

/// Represents texture faces json used by the `core::world::block` module.
#[derive(Deserialize)]
struct TextureFacesJson {
    // direct faces
    #[serde(default)]
    pub top: String,
    #[serde(default)]
    pub bottom: String,
    #[serde(default)]
    pub west: String,
    #[serde(default)]
    pub east: String,
    #[serde(default)]
    pub south: String,

    // north + legacy alias "nord"
    #[serde(default)]
    pub north: String,
    #[serde(default)]
    pub nord: String,

    // groups
    #[serde(default)]
    pub all: String,
    #[serde(default)]
    pub vertical: String,
    #[serde(default)]
    pub horizontal: String,
}

impl TextureFacesJson {
    /// Runs the `resolve` routine for resolve in the `core::world::block` module.
    fn resolve(&self) -> ResolvedFaces<'_> {
        /// Picks the requested data for the `core::world::block` module.
        #[inline]
        fn pick<'a>(specific: &'a str, group: &'a str, all: &'a str) -> &'a str {
            if !specific.is_empty() {
                specific
            } else if !group.is_empty() {
                group
            } else {
                all
            }
        }

        let north_name = if !self.north.is_empty() {
            self.north.as_str()
        } else {
            self.nord.as_str()
        };

        ResolvedFaces {
            top: pick(&self.top, &self.vertical, &self.all),
            bottom: pick(&self.bottom, &self.vertical, &self.all),
            north: pick(north_name, &self.horizontal, &self.all),
            east: pick(&self.east, &self.horizontal, &self.all),
            south: pick(&self.south, &self.horizontal, &self.all),
            west: pick(&self.west, &self.horizontal, &self.all),
        }
    }
}

/// Represents resolved faces used by the `core::world::block` module.
struct ResolvedFaces<'a> {
    top: &'a str,
    bottom: &'a str,
    north: &'a str,
    east: &'a str,
    south: &'a str,
    west: &'a str,
}

/* ---------------- defaults + io ---------------- */

/// Runs the `d_true` routine for d true in the `core::world::block` module.
fn d_true() -> bool {
    true
}

/// Deserializes block level for the `core::world::block` module.
fn deserialize_block_level<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: Deserializer<'de>,
{
    /// Defines the possible level repr variants in the `core::world::block` module.
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum LevelRepr {
        Num(u8),
        Str(String),
    }

    let value = LevelRepr::deserialize(deserializer)?;
    let parsed = match value {
        LevelRepr::Num(num) => num,
        LevelRepr::Str(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Ok(0);
            }
            let base = trimmed.split('-').next().map(str::trim).unwrap_or(trimmed);
            base.parse::<u8>()
                .map_err(|_| de::Error::custom("invalid block level"))?
        }
    };

    Ok(parsed.min(6))
}

/// Runs the `require_face` routine for require face in the `core::world::block` module.
#[inline]
fn require_face<'a>(name: &'a str, face: &str, block_name: &str) -> &'a str {
    if name.is_empty() {
        panic!(
            "block '{}': missing texture for face '{}'. Provide '{}' or use 'all'/'vertical'/'horizontal'.",
            block_name, face, face
        );
    }
    name
}

/// Reads json for the `core::world::block` module.
fn read_json<T: for<'de> Deserialize<'de>>(path: &str) -> T {
    let s = fs::read_to_string(path).unwrap_or_else(|_| panic!("missing file: {path}"));
    serde_json::from_str(&s).unwrap_or_else(|e| panic!("invalid JSON '{path}': {e}"))
}

/// Guesses tex dir from block name for the `core::world::block` module.
fn guess_tex_dir_from_block_name(block_name: &str) -> String {
    let base = block_name.strip_suffix("_block").unwrap_or(block_name);
    format!("textures/blocks/{}", base)
}

#[inline]
fn default_block_collider_size_m() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

#[inline]
fn air_block_def() -> BlockDef {
    let mut air = runtime_block_def(
        "air".into(),
        "air".into(),
        BlockStats::default(),
        Handle::default(),
        Handle::default(),
    );
    air.overridable = false;
    air.collider = BlockColliderDefinition::default();
    air
}

#[inline]
fn registry_store_with_air() -> (Vec<BlockDef>, HashMap<String, BlockId>) {
    let mut defs = Vec::new();
    let mut name_to_id = HashMap::new();
    defs.push(air_block_def());
    name_to_id.insert("air".into(), 0);
    (defs, name_to_id)
}

#[inline]
fn insert_loaded_block_and_variants(
    defs: &mut Vec<BlockDef>,
    name_to_id: &mut HashMap<String, BlockId>,
    common: LoadedBlockCommon,
    visual: LoadedBlockVisual,
) {
    let id = defs.len() as BlockId;
    name_to_id.insert(common.localized_name.clone(), id);
    defs.push(build_loaded_block_def(common, visual));
    append_auto_slab_variants(defs, name_to_id, id);
    append_auto_fluid_flow_variants(defs, name_to_id, id);
}

#[inline]
fn empty_face_uvs() -> (UvRect, UvRect, UvRect, UvRect, UvRect, UvRect) {
    (Z, Z, Z, Z, Z, Z)
}

#[inline]
fn build_loaded_block_common(block_json: &BlockJson) -> LoadedBlockCommon {
    let (localized_name, display_name) =
        normalize_block_identity(&block_json.localized_name, &block_json.name);
    let stats = normalize_block_stats(block_json.stats.clone());
    let fluid_flow = resolve_fluid_flow(block_json.fluid_flow.as_ref(), &stats);
    let allowed_environments =
        sanitize_allowed_environments(block_json.allowed_environments.as_slice());

    LoadedBlockCommon {
        localized_name,
        display_name,
        overridable: block_json.overridable,
        stats,
        mining_wobble: block_json.mining_wobble.sanitized(),
        prop: block_json.prop.clone().map(PropDefinition::sanitized),
        collider: block_json.collider.clone().sanitized(),
        fluid_flow,
        water_logged: block_json.water_logged,
        allowed_environments,
    }
}

#[inline]
fn headless_loaded_block_visual() -> LoadedBlockVisual {
    loaded_block_visual_from_parts(empty_face_uvs(), None, Handle::default(), Handle::default())
}

#[inline]
fn build_loaded_block_def(common: LoadedBlockCommon, visual: LoadedBlockVisual) -> BlockDef {
    BlockDef {
        localized_name: common.localized_name,
        name: common.display_name,
        mesh_visible: true,
        overridable: common.overridable,
        stats: common.stats,
        mining_wobble: common.mining_wobble,
        prop: common.prop,
        collider: common.collider,
        uv_top: visual.uv_top,
        uv_bottom: visual.uv_bottom,
        uv_north: visual.uv_north,
        uv_east: visual.uv_east,
        uv_south: visual.uv_south,
        uv_west: visual.uv_west,
        connected_texture: visual.connected_texture,
        fluid_flow: common.fluid_flow,
        water_logged: common.water_logged,
        allowed_environments: common.allowed_environments,
        image: visual.image,
        material: visual.material,
    }
}

#[inline]
fn runtime_none_collider() -> BlockColliderDefinition {
    BlockColliderDefinition {
        kind: BlockColliderKind::None,
        block_entities: false,
        size_m: default_block_collider_size_m(),
        offset_m: [0.0, 0.0, 0.0],
    }
}

#[inline]
fn runtime_block_def(
    localized_name: String,
    display_name: String,
    stats: BlockStats,
    image: Handle<Image>,
    material: Handle<StandardMaterial>,
) -> BlockDef {
    let (top_face_uv, bottom_face_uv, north_face_uv, east_face_uv, south_face_uv, west_face_uv) =
        empty_face_uvs();
    BlockDef {
        localized_name,
        name: display_name,
        mesh_visible: false,
        overridable: !stats.solid,
        stats,
        mining_wobble: MiningWobbleConfig::default(),
        prop: None,
        collider: runtime_none_collider(),
        uv_top: top_face_uv,
        uv_bottom: bottom_face_uv,
        uv_north: north_face_uv,
        uv_east: east_face_uv,
        uv_south: south_face_uv,
        uv_west: west_face_uv,
        connected_texture: None,
        fluid_flow: None,
        water_logged: false,
        allowed_environments: Vec::new(),
        image,
        material,
    }
}

#[inline]
fn default_mining_wobble_amplitude() -> f32 {
    0.055
}

#[inline]
fn default_mining_wobble_frequency() -> f32 {
    28.0
}

#[inline]
fn default_mining_wobble_vertical_scale() -> f32 {
    0.35
}

fn default_connected_texture_mode() -> String {
    "mask4".to_string()
}

fn default_fluid_flow_step_ms() -> f32 {
    750.0
}

#[inline]
fn parse_block_environment(raw: &str) -> Option<BlockEnvironment> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "water" => Some(BlockEnvironment::Water),
        "overworld" => Some(BlockEnvironment::Overworld),
        "cave" => Some(BlockEnvironment::Cave),
        _ => None,
    }
}

fn sanitize_allowed_environments(raw_values: &[String]) -> Vec<BlockEnvironment> {
    let mut out = Vec::new();
    for raw in raw_values {
        let Some(env) = parse_block_environment(raw.as_str()) else {
            continue;
        };
        if !out.contains(&env) {
            out.push(env);
        }
    }
    out
}

#[inline]
fn insert_variant_block_if_absent(
    defs: &mut Vec<BlockDef>,
    name_to_id: &mut HashMap<String, BlockId>,
    localized_name: String,
    mut def: BlockDef,
) -> Option<BlockId> {
    if name_to_id.contains_key(localized_name.as_str()) {
        return None;
    }
    def.localized_name = localized_name.clone();
    let id = defs.len() as BlockId;
    defs.push(def);
    name_to_id.insert(localized_name, id);
    Some(id)
}

/// Normalizes block identity fields loaded from JSON.
///
/// Supports both schemas:
/// - legacy: `name = "stone_block"`
/// - new: `localized_name = "stone_block", name = "Stone Block"`
fn normalize_block_identity(raw_localized_name: &str, raw_name: &str) -> (String, String) {
    let mut localized_name = raw_localized_name.trim().to_string();
    if localized_name.is_empty() {
        localized_name = raw_name.trim().to_ascii_lowercase();
    }
    if localized_name.is_empty() {
        panic!("block JSON identity is invalid: missing both 'localized_name' and 'name'");
    }
    let name_key = normalize_block_name_key(raw_name, localized_name.as_str());

    (localized_name, name_key)
}

fn normalize_block_name_key(raw_name: &str, fallback_localized_name: &str) -> String {
    let trimmed = raw_name.trim();
    if is_name_key(trimmed) {
        return trimmed.to_string();
    }
    block_name_key_from_localized_name(fallback_localized_name)
}

fn block_name_key_from_localized_name(localized_name: &str) -> String {
    let mut key = String::with_capacity(localized_name.len() + 4);
    key.push_str("KEY_");
    last_was_separator(localized_name, &mut key);
    while key.ends_with('_') {
        key.pop();
    }
    key
}

#[inline]
fn normalize_runtime_block_localized_name(raw: &str) -> String {
    let mut localized_name = raw.trim().to_ascii_lowercase();
    if let Some((_, suffix)) = localized_name.rsplit_once(':') {
        localized_name = suffix.to_string();
    }
    let mut out = String::with_capacity(localized_name.len().max(16));
    last_was_separator(&localized_name, &mut out);
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        "structure_block".to_string()
    } else {
        out
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
    block_name_key_from_localized_name(fallback_localized_name)
}

fn normalize_connected_texture_group(raw: &str, fallback_block_name: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        fallback_block_name.to_string()
    } else {
        trimmed.to_ascii_lowercase()
    }
}

fn parse_connected_texture_mask_index(raw_key: &str) -> Option<usize> {
    let key = raw_key.trim().to_ascii_lowercase();
    let numeric = key
        .strip_prefix("mask_")
        .or_else(|| key.strip_prefix('m'))
        .unwrap_or(key.as_str());
    let idx = numeric.parse::<usize>().ok()?;
    (idx < 16).then_some(idx)
}

fn resolve_connected_texture(
    cfg_opt: Option<&ConnectedTextureJson>,
    tileset: &BlockTileset,
    fallback_tile_name: &str,
    block_name: &str,
) -> Option<ConnectedTextureDef> {
    let cfg = cfg_opt?;
    if !cfg.mode.eq_ignore_ascii_case("mask4") {
        panic!(
            "block '{}': unsupported connected_texture.mode '{}', expected 'mask4'",
            block_name, cfg.mode
        );
    }

    let base_uv = tile_uv(tileset, fallback_tile_name).unwrap_or_else(|err| {
        panic!(
            "block '{}': invalid connected_texture fallback tile '{}': {}",
            block_name, fallback_tile_name, err
        )
    });
    let mut mask4_tiles = [base_uv; 16];
    for (raw_key, tile_name) in &cfg.mask_tiles {
        let idx = parse_connected_texture_mask_index(raw_key).unwrap_or_else(|| {
            panic!(
                "block '{}': invalid connected_texture.mask_tiles key '{}', expected 0..15, m0..m15 or mask_0..mask_15",
                block_name, raw_key
            )
        });
        let uv = tile_uv(tileset, tile_name).unwrap_or_else(|err| {
            panic!(
                "block '{}': invalid connected_texture tile '{}' for key '{}': {}",
                block_name, tile_name, raw_key, err
            )
        });
        mask4_tiles[idx] = uv;
    }

    Some(ConnectedTextureDef {
        group: normalize_connected_texture_group(cfg.group.as_str(), block_name),
        mask4_tiles,
        edge_clip_uv: (cfg.edge_clip_px / tileset.tile_size.max(1) as f32).clamp(0.0, 0.45),
    })
}

/* ---------------- uv helpers ---------------- */

/// Runs the `tile_uv` routine for tile uv in the `core::world::block` module.
fn tile_uv(ts: &BlockTileset, name: &str) -> Result<UvRect, String> {
    let [col, row] = *ts
        .tiles
        .get(name)
        .ok_or_else(|| format!("tile '{}' not in data.json", name))?;

    if col >= ts.columns || row >= ts.rows {
        return Err(format!(
            "tile '{}' out of bounds ({}x{})",
            name, ts.columns, ts.rows
        ));
    }

    let img_w = (ts.columns * ts.tile_size) as f32;
    let img_h = (ts.rows * ts.tile_size) as f32;

    let ([u0, v0], [u1, v1]) = atlas_uv(
        col as usize,
        row as usize,
        ts.columns as usize,
        ts.rows as usize,
        ATLAS_PAD_PX,
        img_w,
        img_h,
    );

    Ok(UvRect { u0, v0, u1, v1 })
}

/// Runs the `atlas_uv` routine for atlas uv in the `core::world::block` module.
fn atlas_uv(
    tile_x: usize,
    tile_y: usize,
    tiles_x: usize,
    tiles_y: usize,
    pad_px: f32,
    image_w: f32,
    image_h: f32,
) -> ([f32; 2], [f32; 2]) {
    let tw = image_w / tiles_x as f32;
    let th = image_h / tiles_y as f32;

    let u0 = (tile_x as f32 * tw + pad_px) / image_w;
    let v0 = (tile_y as f32 * th + pad_px) / image_h;
    let u1 = ((tile_x as f32 + 1.0) * tw - pad_px) / image_w;
    let v1 = ((tile_y as f32 + 1.0) * th - pad_px) / image_h;

    ([u0, v0], [u1, v1])
}

/* ---------------- cube mesh builder (de-duplicated) ---------------- */

/// Build a cube mesh from per-face UVs given as a tuple.
/// Order: (Top, Bottom, North, East, South, West).
pub fn cube_mesh_from_faces_tuple(
    faces: (UvRect, UvRect, UvRect, UvRect, UvRect, UvRect),
    size: f32,
) -> Mesh {
    /// Runs the `quad_uv` routine for quad uv in the `core::world::block` module.
    #[inline]
    fn quad_uv(uv: &UvRect, flip_v: bool) -> [[f32; 2]; 4] {
        if !flip_v {
            [
                [uv.u0, uv.v0],
                [uv.u1, uv.v0],
                [uv.u1, uv.v1],
                [uv.u0, uv.v1],
            ]
        } else {
            [
                [uv.u0, uv.v1],
                [uv.u1, uv.v1],
                [uv.u1, uv.v0],
                [uv.u0, uv.v0],
            ]
        }
    }

    let (top, bottom, north, east, south, west) = faces;
    let s = size;

    let mut pos: Vec<[f32; 3]> = Vec::with_capacity(24);
    let mut nrm: Vec<[f32; 3]> = Vec::with_capacity(24);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(24);
    let mut idx: Vec<u32> = Vec::with_capacity(36);

    /// Runs the `append_quad` routine for append quad in the `core::world::block` module.
    #[inline]
    fn append_quad(
        pos: &mut Vec<[f32; 3]>,
        nrm: &mut Vec<[f32; 3]>,
        uvs: &mut Vec<[f32; 2]>,
        idx: &mut Vec<u32>,
        quad: [[f32; 3]; 4],
        normal: [f32; 3],
        uv: &UvRect,
        flip_v: bool,
    ) {
        let base = pos.len() as u32;
        pos.extend_from_slice(&quad);
        nrm.extend_from_slice(&[normal; 4]);
        uvs.extend_from_slice(&quad_uv(uv, flip_v));
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    // +X (East)
    append_quad(
        &mut pos,
        &mut nrm,
        &mut uvs,
        &mut idx,
        [[s, 0.0, s], [s, 0.0, 0.0], [s, s, 0.0], [s, s, s]],
        [1.0, 0.0, 0.0],
        &east,
        true,
    );
    // -X (West)
    append_quad(
        &mut pos,
        &mut nrm,
        &mut uvs,
        &mut idx,
        [[0.0, 0.0, 0.0], [0.0, 0.0, s], [0.0, s, s], [0.0, s, 0.0]],
        [-1.0, 0.0, 0.0],
        &west,
        true,
    );
    // +Y (Top)
    append_quad(
        &mut pos,
        &mut nrm,
        &mut uvs,
        &mut idx,
        [[0.0, s, s], [s, s, s], [s, s, 0.0], [0.0, s, 0.0]],
        [0.0, 1.0, 0.0],
        &top,
        false,
    );
    // -Y (Bottom)
    append_quad(
        &mut pos,
        &mut nrm,
        &mut uvs,
        &mut idx,
        [[0.0, 0.0, 0.0], [s, 0.0, 0.0], [s, 0.0, s], [0.0, 0.0, s]],
        [0.0, -1.0, 0.0],
        &bottom,
        false,
    );
    // +Z (South)
    append_quad(
        &mut pos,
        &mut nrm,
        &mut uvs,
        &mut idx,
        [[0.0, 0.0, s], [s, 0.0, s], [s, s, s], [0.0, s, s]],
        [0.0, 0.0, 1.0],
        &south,
        true,
    );
    // -Z (North)
    append_quad(
        &mut pos,
        &mut nrm,
        &mut uvs,
        &mut idx,
        [[s, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, s, 0.0], [s, s, 0.0]],
        [0.0, 0.0, -1.0],
        &north,
        true,
    );

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(idx));
    mesh
}

/// Convenience: build a cube mesh for a given block id using the registry UVs.
pub fn build_block_cube_mesh(reg: &BlockRegistry, id: BlockId, size: f32) -> Mesh {
    cube_mesh_from_faces_tuple(reg.face_uvs(id), size)
}

/* ---------------- small utils ---------------- */

/// Runs the `world_y_to_local` routine for world y to local in the `core::world::block` module.
#[inline]
fn world_y_to_local(wy: i32) -> usize {
    (wy - Y_MIN) as usize
}

#[inline]
fn normalize_block_stats(mut stats: BlockStats) -> BlockStats {
    if stats.fluid {
        if stats.fluid_level == 0 {
            stats.fluid_level = 10;
        } else {
            stats.fluid_level = stats.fluid_level.clamp(1, 10);
        }
    } else {
        stats.fluid_level = 0;
    }
    stats
}

#[inline]
fn resolve_fluid_flow(cfg: Option<&FluidFlowJson>, stats: &BlockStats) -> Option<FluidFlowDef> {
    if !stats.fluid {
        return None;
    }
    let Some(cfg) = cfg else {
        return None;
    };
    if !cfg.enabled {
        return None;
    }
    Some(FluidFlowDef {
        step_ms: cfg.step_ms.clamp(50.0, 60_000.0),
    })
}

/// Runs the `material_policy_from_stats` routine for material policy from stats in the `core::world::block` module.
#[inline]
fn material_policy_from_stats(stats: &BlockStats) -> (AlphaMode, Color) {
    if stats.opaque {
        (AlphaMode::Opaque, Color::WHITE)
    } else {
        (AlphaMode::Blend, Color::srgba(1.0, 1.0, 1.0, 0.8))
    }
}

#[inline]
fn add_standard_block_material(
    materials: &mut Assets<StandardMaterial>,
    image: &Handle<Image>,
    stats: &BlockStats,
) -> Handle<StandardMaterial> {
    let (alpha_mode, base_color) = material_policy_from_stats(stats);
    materials.add(StandardMaterial {
        base_color_texture: Some(image.clone()),
        base_color,
        alpha_mode,
        unlit: false,
        metallic: 0.0,
        perceptual_roughness: 1.0,
        reflectance: 0.0,
        ..Default::default()
    })
}

#[inline]
fn loaded_block_visual_from_parts(
    face_uvs: (UvRect, UvRect, UvRect, UvRect, UvRect, UvRect),
    connected_texture: Option<ConnectedTextureDef>,
    image: Handle<Image>,
    material: Handle<StandardMaterial>,
) -> LoadedBlockVisual {
    let (uv_top, uv_bottom, uv_north, uv_east, uv_south, uv_west) = face_uvs;
    LoadedBlockVisual {
        uv_top,
        uv_bottom,
        uv_north,
        uv_east,
        uv_south,
        uv_west,
        connected_texture,
        image,
        material,
    }
}
