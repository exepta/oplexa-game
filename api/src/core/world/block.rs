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
/* ---------------- constants ---------------- */

pub const VOXEL_SIZE: f32 = 1.0;
const ATLAS_PAD_PX: f32 = 0.5;

pub const BASE_BREAK_TIME: f32 = 0.55;
pub const PER_HARDNESS: f32 = 0.45;

pub const MIN_BREAK_TIME: f32 = 0.2;
pub const MAX_BREAK_TIME: f32 = 60.0;

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

/// Represents block def used by the `core::world::block` module.
#[derive(Clone)]
pub struct BlockDef {
    pub name: String,
    pub stats: BlockStats,
    pub prop: Option<PropDefinition>,
    pub uv_top: UvRect,
    pub uv_bottom: UvRect,
    pub uv_north: UvRect,
    pub uv_east: UvRect,
    pub uv_south: UvRect,
    pub uv_west: UvRect,
    pub image: Handle<Image>,
    pub material: Handle<StandardMaterial>,
}

/// Represents block stats used by the `core::world::block` module.
#[derive(Deserialize, Clone, Default)]
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
    pub foliage: bool,
    #[serde(default = "d_true")]
    pub solid: bool,
    #[serde(default)]
    pub emissive: f32,
}

/// Represents selected block used by the `core::world::block` module.
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

/* ---------------- registry ---------------- */

/// Represents block registry used by the `core::world::block` module.
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
    /// Runs the `name` routine for name in the `core::world::block` module.
    #[inline]
    pub fn name(&self, id: BlockId) -> &str {
        self.def(id).name.as_str()
    }

    /// Runs the `name_opt` routine for name opt in the `core::world::block` module.
    #[inline]
    pub fn name_opt(&self, id: BlockId) -> Option<&str> {
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
    /// Checks whether this block has a prop render definition.
    #[inline]
    pub fn is_prop(&self, id: BlockId) -> bool {
        self.prop(id).is_some()
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
    /// Returns true when this block should participate in world collision meshes.
    #[inline]
    pub fn is_solid_for_collision(&self, id: BlockId) -> bool {
        !self.is_air(id) && !self.is_fluid(id) && self.stats(id).solid
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
        // 0 = air
        let mut defs: Vec<BlockDef> = Vec::new();
        let mut name_to_id = HashMap::new();

        defs.push(BlockDef {
            name: "air".into(),
            stats: BlockStats::default(),
            prop: None,
            uv_top: Z,
            uv_bottom: Z,
            uv_north: Z,
            uv_east: Z,
            uv_south: Z,
            uv_west: Z,
            image: Handle::default(),
            material: Handle::default(),
        });
        name_to_id.insert("air".into(), 0);

        for path in block_json_paths(blocks_dir) {
            let block_json: BlockJson = read_json(path.to_str().unwrap());
            let tex_dir = block_json
                .texture_dir
                .clone()
                .unwrap_or_else(|| guess_tex_dir_from_block_name(&block_json.name));

            // tileset is read from disk (not via asset server)
            let tileset_path = format!("assets/{}/data.json", tex_dir);
            let tileset: BlockTileset = read_json(&tileset_path);

            // atlas image is loaded via asset_server
            let atlas_path = format!("{}/{}", tex_dir, tileset.image);
            let image: Handle<Image> = asset_server.load(atlas_path);

            // resolve faces (supports: specific keys, 'all', 'vertical', 'horizontal', and 'nord' fallback)
            let faces = block_json.texture.resolve();
            let uv_top =
                tile_uv(&tileset, require_face(&faces.top, "top", &block_json.name)).unwrap();
            let uv_bottom = tile_uv(
                &tileset,
                require_face(&faces.bottom, "bottom", &block_json.name),
            )
            .unwrap();
            let uv_north = tile_uv(
                &tileset,
                require_face(&faces.north, "north", &block_json.name),
            )
            .unwrap();
            let uv_east = tile_uv(
                &tileset,
                require_face(&faces.east, "east", &block_json.name),
            )
            .unwrap();
            let uv_south = tile_uv(
                &tileset,
                require_face(&faces.south, "south", &block_json.name),
            )
            .unwrap();
            let uv_west = tile_uv(
                &tileset,
                require_face(&faces.west, "west", &block_json.name),
            )
            .unwrap();

            let (alpha_mode, base_color) = material_policy_from_stats(&block_json.stats);

            let material = materials.add(StandardMaterial {
                base_color_texture: Some(image.clone()),
                base_color,
                alpha_mode,
                unlit: false,
                metallic: 0.0,
                perceptual_roughness: 1.0,
                reflectance: 0.0,
                ..Default::default()
            });

            let id = defs.len() as BlockId;
            name_to_id.insert(block_json.name.clone(), id);
            defs.push(BlockDef {
                name: block_json.name,
                stats: block_json.stats,
                prop: block_json.prop.map(PropDefinition::sanitized),
                uv_top,
                uv_bottom,
                uv_north,
                uv_east,
                uv_south,
                uv_west,
                image,
                material,
            });
        }

        Self { defs, name_to_id }
    }

    /// Loads headless for the `core::world::block` module.
    pub fn load_headless(blocks_dir: &str) -> Self {
        let mut defs: Vec<BlockDef> = Vec::new();
        let mut name_to_id = HashMap::new();

        defs.push(BlockDef {
            name: "air".into(),
            stats: BlockStats::default(),
            prop: None,
            uv_top: Z,
            uv_bottom: Z,
            uv_north: Z,
            uv_east: Z,
            uv_south: Z,
            uv_west: Z,
            image: Handle::default(),
            material: Handle::default(),
        });
        name_to_id.insert("air".into(), 0);

        for path in block_json_paths(blocks_dir) {
            let block_json: BlockJson = read_json(path.to_str().unwrap());
            let id = defs.len() as BlockId;
            name_to_id.insert(block_json.name.clone(), id);
            defs.push(BlockDef {
                name: block_json.name,
                stats: block_json.stats,
                prop: block_json.prop.map(PropDefinition::sanitized),
                uv_top: Z,
                uv_bottom: Z,
                uv_north: Z,
                uv_east: Z,
                uv_south: Z,
                uv_west: Z,
                image: Handle::default(),
                material: Handle::default(),
            });
        }

        Self { defs, name_to_id }
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
    chunk.get(lx, ly, lz)
}

/// Returns id world for the `core::world::block` module.
#[inline]
pub fn get_id_world(chunk_map: &ChunkMap, wp: IVec3) -> BlockId {
    get_block_world(chunk_map, wp)
}

/// Sets id world for the `core::world::block` module.
pub fn set_id_world(chunk_map: &mut ChunkMap, wp: IVec3, id: BlockId) -> Option<BlockId> {
    let Some(mut access) = world_access_mut(chunk_map, wp) else {
        return None;
    };
    let old = access.get();
    access.set(id);
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
    pub name: String,
    pub texture_dir: Option<String>,
    pub texture: TextureFacesJson,
    #[serde(default)]
    pub stats: BlockStats,
    #[serde(default)]
    pub prop: Option<PropDefinition>,
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

/// Runs the `material_policy_from_stats` routine for material policy from stats in the `core::world::block` module.
#[inline]
fn material_policy_from_stats(stats: &BlockStats) -> (AlphaMode, Color) {
    if stats.opaque {
        (AlphaMode::Opaque, Color::WHITE)
    } else {
        (AlphaMode::Blend, Color::srgba(1.0, 1.0, 1.0, 0.8))
    }
}
