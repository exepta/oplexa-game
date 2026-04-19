use crate::core::world::block::*;
use crate::core::world::chunk::*;
use crate::core::world::prop::PropDefinition;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::tasks::Task;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

/// Represents mesh backlog used by the `generator::chunk::chunk_struct` module.
#[derive(Resource, Default)]
pub struct MeshBacklog(pub VecDeque<(IVec2, usize)>);

/// Pending Chunk-Generate-Tasks
#[derive(Resource, Default)]
pub struct PendingGen(pub HashMap<IVec2, Task<(IVec2, ChunkData)>>);

/// Pending Mesh-Tasks pro (coord, sub)
#[derive(Resource, Default)]
pub struct PendingMesh(
    pub HashMap<(IVec2, usize), Task<((IVec2, usize), Vec<(BlockId, MeshBuild)>)>>,
);

/// Represents reg lite entry used by the `generator::chunk::chunk_struct` module.
#[derive(Clone)]
pub struct RegLiteEntry {
    pub top: UvRect,
    pub bottom: UvRect,
    pub north: UvRect,
    pub east: UvRect,
    pub south: UvRect,
    pub west: UvRect,
    pub mesh_visible: bool,
    pub opaque: bool,
    pub solid: bool,
    pub fluid: bool,
    pub fluid_level: u8,
    pub foliage: bool,
    pub prop: Option<PropDefinition>,
    pub custom_mesh_box: Option<([f32; 3], [f32; 3])>,
    pub connect_group: u16,
    pub connect_mask4_tiles: Option<[UvRect; 16]>,
    pub connect_edge_clip_uv: f32,
}
/// Represents reg lite used by the `generator::chunk::chunk_struct` module.
#[derive(Clone)]
pub struct RegLite {
    pub map: Arc<HashMap<BlockId, RegLiteEntry>>,
}

impl RegLite {
    /// Runs the `from_reg` routine for from reg in the `generator::chunk::chunk_struct` module.
    pub fn from_reg(reg: &BlockRegistry) -> Self {
        let mut map = HashMap::new();
        let mut connect_groups: HashMap<String, u16> = HashMap::new();
        let mut next_connect_group: u16 = 1;
        for &id in reg.name_to_id.values() {
            if id == 0 {
                continue;
            }
            let def = reg.def(id);
            let (connect_group, connect_mask4_tiles, connect_edge_clip_uv) = if let Some(ctm) =
                def.connected_texture.as_ref()
            {
                let gid = if let Some(existing) = connect_groups.get(ctm.group.as_str()) {
                    *existing
                } else {
                    let created = next_connect_group;
                    next_connect_group = next_connect_group.checked_add(1).unwrap_or_else(|| {
                        panic!("too many connected texture groups (>{})", u16::MAX - 1)
                    });
                    connect_groups.insert(ctm.group.clone(), created);
                    created
                };
                (gid, Some(ctm.mask4_tiles), ctm.edge_clip_uv)
            } else {
                (0, None, 0.0)
            };
            map.insert(
                id,
                RegLiteEntry {
                    top: reg.uv(id, Face::Top),
                    bottom: reg.uv(id, Face::Bottom),
                    north: reg.uv(id, Face::North),
                    east: reg.uv(id, Face::East),
                    south: reg.uv(id, Face::South),
                    west: reg.uv(id, Face::West),
                    mesh_visible: reg.def(id).mesh_visible,
                    opaque: reg.def(id).stats.opaque,
                    solid: reg.def(id).stats.solid,
                    fluid: reg.def(id).stats.fluid,
                    fluid_level: reg.def(id).stats.fluid_level,
                    foliage: reg.def(id).stats.foliage,
                    prop: reg.def(id).prop.clone(),
                    custom_mesh_box: if reg.def(id).prop.is_none() {
                        match reg.def(id).collider.kind {
                            BlockColliderKind::Box => {
                                Some((reg.def(id).collider.size_m, reg.def(id).collider.offset_m))
                            }
                            _ => None,
                        }
                    } else {
                        None
                    },
                    connect_group,
                    connect_mask4_tiles,
                    connect_edge_clip_uv,
                },
            );
        }
        Self { map: Arc::new(map) }
    }
    /// Runs the `uv` routine for uv in the `generator::chunk::chunk_struct` module.
    #[inline]
    pub fn uv(&self, id: BlockId, face: Face) -> UvRect {
        let e = self.map.get(&id).expect("unknown id");
        match face {
            Face::Top => e.top,
            Face::Bottom => e.bottom,
            Face::North => e.north,
            Face::East => e.east,
            Face::South => e.south,
            Face::West => e.west,
        }
    }
    /// Runs the `mesh_visible` routine for mesh visible in the `generator::chunk::chunk_struct` module.
    #[inline]
    pub fn mesh_visible(&self, id: BlockId) -> bool {
        self.map.get(&id).map(|e| e.mesh_visible).unwrap_or(false)
    }
    /// Runs the `opaque` routine for opaque in the `generator::chunk::chunk_struct` module.
    #[inline]
    pub fn opaque(&self, id: BlockId) -> bool {
        self.map.get(&id).map(|e| e.opaque).unwrap_or(false)
    }
    /// Runs the `solid` routine for solid in the `generator::chunk::chunk_struct` module.
    #[inline]
    pub fn solid(&self, id: BlockId) -> bool {
        self.map.get(&id).map(|e| e.solid).unwrap_or(false)
    }
    /// Runs the `fluid` routine for fluid in the `generator::chunk::chunk_struct` module.
    #[inline]
    pub fn fluid(&self, id: BlockId) -> bool {
        self.map.get(&id).map(|e| e.fluid).unwrap_or(false)
    }
    /// Runs the `fluid_level` routine for fluid level in the `generator::chunk::chunk_struct` module.
    #[inline]
    pub fn fluid_level(&self, id: BlockId) -> u8 {
        self.map.get(&id).map(|e| e.fluid_level).unwrap_or(0)
    }
    /// Runs the `foliage` routine for foliage in the `generator::chunk::chunk_struct` module.
    #[inline]
    pub fn foliage(&self, id: BlockId) -> bool {
        self.map.get(&id).map(|e| e.foliage).unwrap_or(false)
    }
    /// Runs the `prop` routine for prop in the `generator::chunk::chunk_struct` module.
    #[inline]
    pub fn prop(&self, id: BlockId) -> Option<&PropDefinition> {
        self.map.get(&id).and_then(|entry| entry.prop.as_ref())
    }
    /// Runs the `custom_mesh_box` routine for custom mesh box in the `generator::chunk::chunk_struct` module.
    #[inline]
    pub fn custom_mesh_box(&self, id: BlockId) -> Option<([f32; 3], [f32; 3])> {
        self.map
            .get(&id)
            .and_then(|entry| entry.custom_mesh_box.as_ref().copied())
    }
    /// Returns connected-texture group id (0 = none).
    #[inline]
    pub fn connect_group(&self, id: BlockId) -> u16 {
        self.map.get(&id).map(|e| e.connect_group).unwrap_or(0)
    }
    /// Returns connected-texture UV by 4-neighbor mask.
    #[inline]
    pub fn connected_mask4_uv(&self, id: BlockId, mask: u8) -> Option<UvRect> {
        let entry = self.map.get(&id)?;
        let tiles = entry.connect_mask4_tiles.as_ref()?;
        tiles.get(mask as usize).copied()
    }
    /// Returns true when block id uses connected-texture mask4 mapping.
    #[inline]
    pub fn has_connected_mask4(&self, id: BlockId) -> bool {
        self.map
            .get(&id)
            .map(|entry| entry.connect_mask4_tiles.is_some())
            .unwrap_or(false)
    }
    /// Returns optional frame edge clip width in local UV-space (0 disables clip).
    #[inline]
    pub fn connected_edge_clip_uv(&self, id: BlockId) -> f32 {
        self.map
            .get(&id)
            .map(|entry| entry.connect_edge_clip_uv)
            .unwrap_or(0.0)
    }
    /// Runs the `is_crossed_prop` routine for is crossed prop in the `generator::chunk::chunk_struct` module.
    #[inline]
    pub fn is_crossed_prop(&self, id: BlockId) -> bool {
        self.prop(id)
            .map(PropDefinition::is_crossed_planes)
            .unwrap_or(false)
    }
}

/// Represents mesh build used by the `generator::chunk::chunk_struct` module.
pub struct MeshBuild {
    pub pos: Vec<[f32; 3]>,
    pub nrm: Vec<[f32; 3]>,
    pub uv: Vec<[f32; 2]>,
    pub ctm: Vec<[f32; 2]>,
    pub tile_rect: Vec<[f32; 4]>,
    pub idx: Vec<u32>,
}

impl MeshBuild {
    /// Creates a new instance for the `generator::chunk::chunk_struct` module.
    pub fn new() -> Self {
        Self {
            pos: vec![],
            nrm: vec![],
            uv: vec![],
            ctm: vec![],
            tile_rect: vec![],
            idx: vec![],
        }
    }
    /// Runs the `quad` routine for quad in the `generator::chunk::chunk_struct` module.
    pub fn quad(&mut self, q: [[f32; 3]; 4], n: [f32; 3], uv: [[f32; 2]; 4], tile_rect: [f32; 4]) {
        self.quad_with_ctm(q, n, uv, tile_rect, [-1.0, 0.0]);
    }
    /// Runs the `quad_with_ctm` routine for quad with ctm in the `generator::chunk::chunk_struct` module.
    pub fn quad_with_ctm(
        &mut self,
        q: [[f32; 3]; 4],
        n: [f32; 3],
        uv: [[f32; 2]; 4],
        tile_rect: [f32; 4],
        ctm: [f32; 2],
    ) {
        let base = self.pos.len() as u32;
        self.pos.extend_from_slice(&q);
        self.nrm.extend_from_slice(&[n; 4]);
        self.uv.extend_from_slice(&uv);
        self.ctm.extend_from_slice(&[ctm; 4]);
        self.tile_rect.extend_from_slice(&[tile_rect; 4]);
        self.idx
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    /// Runs the `into_mesh` routine for into mesh in the `generator::chunk::chunk_struct` module.
    pub fn into_mesh(self) -> Mesh {
        let mut m = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        );
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.pos);
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.nrm);
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uv);
        m.insert_attribute(Mesh::ATTRIBUTE_UV_1, self.ctm);
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.tile_rect);

        // <= 65k Vertices? -> U16-Indices
        if self.idx.len() <= u16::MAX as usize {
            let idx_u16: Vec<u16> = self.idx.into_iter().map(|i| i as u16).collect();
            m.insert_indices(Indices::U16(idx_u16));
        } else {
            m.insert_indices(Indices::U32(self.idx));
        }

        m
    }

    /// Runs the `mesh_is_empty` routine for mesh is empty in the `generator::chunk::chunk_struct` module.
    #[allow(dead_code)]
    pub fn mesh_is_empty(m: &Mesh) -> bool {
        match m.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(v)) => v.is_empty(),
            Some(VertexAttributeValues::Float32(v)) => v.is_empty(),
            _ => true,
        }
    }
}

/// Represents border snapshot used by the `generator::chunk::chunk_struct` module.
#[derive(Clone)]
pub struct BorderSnapshot {
    pub y0: usize,
    pub y1: usize,
    pub east: Option<Vec<BlockId>>,
    pub west: Option<Vec<BlockId>>,
    pub south: Option<Vec<BlockId>>,
    pub north: Option<Vec<BlockId>>,
    pub east_stacked: Option<Vec<BlockId>>,
    pub west_stacked: Option<Vec<BlockId>>,
    pub south_stacked: Option<Vec<BlockId>>,
    pub north_stacked: Option<Vec<BlockId>>,
}
