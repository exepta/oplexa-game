use crate::core::config::WorldGenConfig;
use crate::core::states::states::{AppState, LoadingStates};
use crate::core::world::biome::registry::BiomeRegistry;
use crate::core::world::block::{BlockId, BlockRegistry, Face};
use crate::core::world::chunk::{ChunkData, ChunkMap, ChunkMeshIndex};
use crate::core::world::chunk_dimension::*;
use crate::core::world::save::*;
use crate::generator::chunk::chunk_gen::generate_chunk_async_biome;
use crate::generator::chunk::chunk_struct::*;
use crate::generator::chunk::trees::registry::TreeRegistry;
use bevy::prelude::*;
use lz4_flex::{compress_prepend_size, decompress_size_prepended};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const MAX_INFLIGHT_MESH: usize = 32;
pub const MAX_INFLIGHT_GEN: usize = 32;

pub(crate) const DIR4: [IVec2; 4] = [
    IVec2::new(1, 0),
    IVec2::new(-1, 0),
    IVec2::new(0, 1),
    IVec2::new(0, -1),
];

#[inline]
fn greedy_merge_mask<F>(mask: &[BlockId], w: usize, h: usize, mut emit: F)
where
    F: FnMut(usize, usize, usize, usize, BlockId),
{
    let mut used = vec![false; mask.len()];
    for v in 0..h {
        for u in 0..w {
            let i = v * w + u;
            if used[i] {
                continue;
            }
            let id = mask[i];
            if id == 0 {
                continue;
            }

            let mut rw = 1usize;
            while u + rw < w {
                let ii = v * w + (u + rw);
                if used[ii] || mask[ii] != id {
                    break;
                }
                rw += 1;
            }

            let mut rh = 1usize;
            'grow: while v + rh < h {
                for du in 0..rw {
                    let ii = (v + rh) * w + (u + du);
                    if used[ii] || mask[ii] != id {
                        break 'grow;
                    }
                }
                rh += 1;
            }

            for dv in 0..rh {
                for du in 0..rw {
                    used[(v + dv) * w + (u + du)] = true;
                }
            }

            emit(u, v, rw, rh, id);
        }
    }
}

/// Runs the `mesh_subchunk_async` routine for mesh subchunk async in the `generator::chunk::chunk_utils` module.
pub async fn mesh_subchunk_async(
    chunk: &ChunkData,
    reg: &RegLite,
    sub: usize,
    block_size: f32,
    borders: Option<BorderSnapshot>,
) -> Vec<(BlockId, MeshBuild)> {
    let mut by_block: HashMap<BlockId, MeshBuild> = HashMap::new();
    let s = block_size;
    let y0 = sub * SEC_H;
    let y1 = (y0 + SEC_H).min(CY);
    let yh = y1 - y0;

    let (
        east,
        west,
        south,
        north,
        east_stacked,
        west_stacked,
        south_stacked,
        north_stacked,
        snap_y0,
        snap_y1,
    ) = if let Some(b) = borders {
        debug_assert_eq!(b.y0, y0, "BorderSnapshot.y0 != sub y0");
        debug_assert_eq!(b.y1, y1, "BorderSnapshot.y1 != sub y1");
        (
            b.east,
            b.west,
            b.south,
            b.north,
            b.east_stacked,
            b.west_stacked,
            b.south_stacked,
            b.north_stacked,
            b.y0,
            b.y1,
        )
    } else {
        (None, None, None, None, None, None, None, None, y0, y1)
    };

    let sample_opt =
        |opt: &Option<Vec<BlockId>>, y: usize, i: usize, stride: usize| -> Option<BlockId> {
            if y < snap_y0 || y >= snap_y1 {
                return None;
            }
            opt.as_ref().map(|v| {
                let iy = y - snap_y0;
                v[iy * stride + i]
            })
        };

    let east_at_opt = |y: usize, z: usize| sample_opt(&east, y, z, CZ);
    let west_at_opt = |y: usize, z: usize| sample_opt(&west, y, z, CZ);
    let south_at_opt = |y: usize, x: usize| sample_opt(&south, y, x, CX);
    let north_at_opt = |y: usize, x: usize| sample_opt(&north, y, x, CX);
    let east_stacked_at_opt = |y: usize, z: usize| sample_opt(&east_stacked, y, z, CZ);
    let west_stacked_at_opt = |y: usize, z: usize| sample_opt(&west_stacked, y, z, CZ);
    let south_stacked_at_opt = |y: usize, x: usize| sample_opt(&south_stacked, y, x, CX);
    let north_stacked_at_opt = |y: usize, x: usize| sample_opt(&north_stacked, y, x, CX);

    let get = |x: isize, y: isize, z: isize| -> BlockId {
        if x < 0 || y < 0 || z < 0 || x >= CX as isize || y >= CY as isize || z >= CZ as isize {
            0
        } else {
            chunk.blocks[((y as usize) * CZ + (z as usize)) * CX + (x as usize)]
        }
    };
    let sample_with_borders = |x: i32, y: i32, z: i32| -> BlockId {
        if y < 0 || y >= CY as i32 {
            return 0;
        }
        let yu = y as usize;

        if (0..CX as i32).contains(&x) && (0..CZ as i32).contains(&z) {
            return chunk.get(x as usize, yu, z as usize);
        }

        if x == -1 && (0..CZ as i32).contains(&z) {
            return west_at_opt(yu, z as usize).unwrap_or(0);
        }
        if x == CX as i32 && (0..CZ as i32).contains(&z) {
            return east_at_opt(yu, z as usize).unwrap_or(0);
        }
        if z == -1 && (0..CX as i32).contains(&x) {
            return north_at_opt(yu, x as usize).unwrap_or(0);
        }
        if z == CZ as i32 && (0..CX as i32).contains(&x) {
            return south_at_opt(yu, x as usize).unwrap_or(0);
        }
        0
    };
    let uvq_tiled = |ur: f32, vr: f32, flip_v: bool| -> [[f32; 2]; 4] {
        if !flip_v {
            [[0.0, 0.0], [ur, 0.0], [ur, vr], [0.0, vr]]
        } else {
            [[0.0, vr], [ur, vr], [ur, 0.0], [0.0, 0.0]]
        }
    };
    let face_visible = |self_id: BlockId, neigh_id: BlockId| -> bool {
        if self_id == 0 {
            return false;
        }
        if neigh_id == 0 {
            return true;
        }
        // Connected transparent blocks (e.g. water/glass) should not render inner faces.
        if self_id == neigh_id && !reg.opaque(self_id) {
            return false;
        }

        // Treat neighboring foliage blocks as connected canopy: don't render inner faces.
        if reg.foliage(self_id) && reg.foliage(neigh_id) {
            return false;
        }

        let self_connect_group = reg.connect_group(self_id);
        if self_connect_group != 0 && self_connect_group == reg.connect_group(neigh_id) {
            return false;
        }

        let self_fluid = reg.fluid(self_id);
        let neigh_fluid = reg.fluid(neigh_id);

        if self_fluid && neigh_fluid {
            return false;
        }

        // Fluids should not render side planes against opaque solids.
        // This avoids coplanar transparent-vs-solid overlap (z-fighting/flicker).
        if self_fluid && !neigh_fluid {
            return !reg.opaque(neigh_id);
        }

        if reg.custom_mesh_box(neigh_id).is_some() {
            return true;
        }

        !reg.opaque(neigh_id)
    };
    let is_cube_voxel = |id: BlockId| {
        id != 0
            && reg.mesh_visible(id)
            && !reg.is_crossed_prop(id)
            && reg.custom_mesh_box(id).is_none()
    };
    let is_connected_cube_voxel = |id: BlockId| is_cube_voxel(id) && reg.has_connected_mask4(id);
    let face_uv_axes = |face: Face| -> (IVec3, IVec3) {
        match face {
            Face::Top => (IVec3::new(1, 0, 0), IVec3::new(0, 0, -1)),
            Face::Bottom => (IVec3::new(1, 0, 0), IVec3::new(0, 0, 1)),
            Face::East => (IVec3::new(0, 0, -1), IVec3::new(0, -1, 0)),
            Face::West => (IVec3::new(0, 0, 1), IVec3::new(0, -1, 0)),
            Face::South => (IVec3::new(1, 0, 0), IVec3::new(0, -1, 0)),
            Face::North => (IVec3::new(-1, 0, 0), IVec3::new(0, -1, 0)),
        }
    };
    let connected_mask4 = |id: BlockId, face: Face, x: usize, y: usize, z: usize| -> u8 {
        let group = reg.connect_group(id);
        if group == 0 {
            return 0;
        }
        let pos = IVec3::new(x as i32, y as i32, z as i32);
        let (u_pos, v_pos) = face_uv_axes(face);
        let mut mask = 0u8;
        for (bit, off) in [(1u8, u_pos), (2u8, -u_pos), (4u8, v_pos), (8u8, -v_pos)] {
            let nid = sample_with_borders(pos.x + off.x, pos.y + off.y, pos.z + off.z);
            if reg.connect_group(nid) == group {
                mask |= bit;
            }
        }
        mask
    };
    let local_box_bounds = |size_m: [f32; 3], offset_m: [f32; 3]| -> ([f32; 3], [f32; 3]) {
        let half_x = (size_m[0] * 0.5).max(0.0005);
        let half_y = (size_m[1] * 0.5).max(0.0005);
        let half_z = (size_m[2] * 0.5).max(0.0005);
        let cx = 0.5 + offset_m[0];
        let cy = 0.5 + offset_m[1];
        let cz = 0.5 + offset_m[2];
        (
            [cx - half_x, cy - half_y, cz - half_z],
            [cx + half_x, cy + half_y, cz + half_z],
        )
    };
    let neighbor_stacked = |x: i32, y: i32, z: i32| -> BlockId {
        if y < 0 || y >= CY as i32 {
            return 0;
        }
        let yu = y as usize;
        if (0..CX as i32).contains(&x) && (0..CZ as i32).contains(&z) {
            return chunk.get_stacked(x as usize, yu, z as usize);
        }
        if x == -1 && (0..CZ as i32).contains(&z) {
            return west_stacked_at_opt(yu, z as usize).unwrap_or(0);
        }
        if x == CX as i32 && (0..CZ as i32).contains(&z) {
            return east_stacked_at_opt(yu, z as usize).unwrap_or(0);
        }
        if z == -1 && (0..CX as i32).contains(&x) {
            return north_stacked_at_opt(yu, x as usize).unwrap_or(0);
        }
        if z == CZ as i32 && (0..CX as i32).contains(&x) {
            return south_stacked_at_opt(yu, x as usize).unwrap_or(0);
        }
        0
    };
    let overlap = |a0: f32, a1: f32, b0: f32, b1: f32| -> bool {
        const EPS: f32 = 0.001;
        (a1.min(b1) - a0.max(b0)) > EPS
    };
    let face_occluded_by_bounds = |self_min: [f32; 3],
                                   self_max: [f32; 3],
                                   other_min: [f32; 3],
                                   other_max: [f32; 3],
                                   face: Face| {
        const EPS: f32 = 0.001;
        match face {
            Face::Top => {
                overlap(self_min[0], self_max[0], other_min[0], other_max[0])
                    && overlap(self_min[2], self_max[2], other_min[2], other_max[2])
                    && other_min[1] <= self_max[1] + EPS
                    && other_max[1] > self_max[1] + EPS
            }
            Face::Bottom => {
                overlap(self_min[0], self_max[0], other_min[0], other_max[0])
                    && overlap(self_min[2], self_max[2], other_min[2], other_max[2])
                    && other_max[1] >= self_min[1] - EPS
                    && other_min[1] < self_min[1] - EPS
            }
            Face::East => {
                overlap(self_min[1], self_max[1], other_min[1], other_max[1])
                    && overlap(self_min[2], self_max[2], other_min[2], other_max[2])
                    && other_min[0] <= self_max[0] + EPS
                    && other_max[0] > self_max[0] + EPS
            }
            Face::West => {
                overlap(self_min[1], self_max[1], other_min[1], other_max[1])
                    && overlap(self_min[2], self_max[2], other_min[2], other_max[2])
                    && other_max[0] >= self_min[0] - EPS
                    && other_min[0] < self_min[0] - EPS
            }
            Face::South => {
                overlap(self_min[0], self_max[0], other_min[0], other_max[0])
                    && overlap(self_min[1], self_max[1], other_min[1], other_max[1])
                    && other_min[2] <= self_max[2] + EPS
                    && other_max[2] > self_max[2] + EPS
            }
            Face::North => {
                overlap(self_min[0], self_max[0], other_min[0], other_max[0])
                    && overlap(self_min[1], self_max[1], other_min[1], other_max[1])
                    && other_max[2] >= self_min[2] - EPS
                    && other_min[2] < self_min[2] - EPS
            }
        }
    };
    let same_cell_connected_occludes_face = |self_id: BlockId,
                                             self_size_m: [f32; 3],
                                             self_offset_m: [f32; 3],
                                             other_id: BlockId,
                                             face: Face|
     -> bool {
        let self_group = reg.connect_group(self_id);
        if other_id == 0 || self_group == 0 || self_group != reg.connect_group(other_id) {
            return false;
        }
        let Some((other_size_m, other_offset_m)) = reg.custom_mesh_box(other_id) else {
            return false;
        };

        let ([smin_x, smin_y, smin_z], [smax_x, smax_y, smax_z]) =
            local_box_bounds(self_size_m, self_offset_m);
        let ([omin_x, omin_y, omin_z], [omax_x, omax_y, omax_z]) =
            local_box_bounds(other_size_m, other_offset_m);
        face_occluded_by_bounds(
            [smin_x, smin_y, smin_z],
            [smax_x, smax_y, smax_z],
            [omin_x, omin_y, omin_z],
            [omax_x, omax_y, omax_z],
            face,
        )
    };
    let same_cell_connected_edge_mask = |self_id: BlockId,
                                         self_size_m: [f32; 3],
                                         self_offset_m: [f32; 3],
                                         other_id: BlockId,
                                         face: Face|
     -> u8 {
        const EPS: f32 = 0.001;
        let self_group = reg.connect_group(self_id);
        if other_id == 0 || self_group == 0 || self_group != reg.connect_group(other_id) {
            return 0;
        }
        let Some((other_size_m, other_offset_m)) = reg.custom_mesh_box(other_id) else {
            return 0;
        };

        let (self_min, self_max) = local_box_bounds(self_size_m, self_offset_m);
        let (other_min, other_max) = local_box_bounds(other_size_m, other_offset_m);
        let (u_pos, v_pos) = face_uv_axes(face);
        let face_normal = match face {
            Face::Top => IVec3::new(0, 1, 0),
            Face::Bottom => IVec3::new(0, -1, 0),
            Face::East => IVec3::new(1, 0, 0),
            Face::West => IVec3::new(-1, 0, 0),
            Face::South => IVec3::new(0, 0, 1),
            Face::North => IVec3::new(0, 0, -1),
        };
        let axis_idx = |v: IVec3| -> usize {
            if v.x != 0 {
                0
            } else if v.y != 0 {
                1
            } else {
                2
            }
        };
        let axis_sign = |v: IVec3, axis: usize| -> i32 {
            match axis {
                0 => v.x.signum(),
                1 => v.y.signum(),
                _ => v.z.signum(),
            }
        };
        let overlap_axis = |axis: usize| -> bool {
            overlap(
                self_min[axis],
                self_max[axis],
                other_min[axis],
                other_max[axis],
            )
        };

        // Require overlap on face normal axis so only this face-plane is considered.
        if !overlap_axis(axis_idx(face_normal)) {
            return 0;
        }

        let mut out = 0u8;
        for (bit, dir, orth) in [
            (1u8, u_pos, v_pos),
            (2u8, -u_pos, v_pos),
            (4u8, v_pos, u_pos),
            (8u8, -v_pos, u_pos),
        ] {
            let axis = axis_idx(dir);
            let sign = axis_sign(dir, axis);
            let orth_axis = axis_idx(orth);
            if !overlap_axis(orth_axis) {
                continue;
            }
            let touches = if sign > 0 {
                other_min[axis] <= self_max[axis] + EPS && other_max[axis] > self_max[axis] + EPS
            } else {
                other_max[axis] >= self_min[axis] - EPS && other_min[axis] < self_min[axis] - EPS
            };
            if touches {
                out |= bit;
            }
        }
        out
    };
    let connected_neighbor_edge_mask = |self_id: BlockId,
                                        self_size_m: [f32; 3],
                                        self_offset_m: [f32; 3],
                                        face: Face,
                                        x: usize,
                                        y: usize,
                                        z: usize|
     -> u8 {
        const EPS: f32 = 0.001;
        let self_group = reg.connect_group(self_id);
        if self_group == 0 {
            return 0;
        }
        let (self_min, self_max) = local_box_bounds(self_size_m, self_offset_m);
        let (u_pos, v_pos) = face_uv_axes(face);
        let face_normal = match face {
            Face::Top => IVec3::new(0, 1, 0),
            Face::Bottom => IVec3::new(0, -1, 0),
            Face::East => IVec3::new(1, 0, 0),
            Face::West => IVec3::new(-1, 0, 0),
            Face::South => IVec3::new(0, 0, 1),
            Face::North => IVec3::new(0, 0, -1),
        };
        let axis_idx = |v: IVec3| -> usize {
            if v.x != 0 {
                0
            } else if v.y != 0 {
                1
            } else {
                2
            }
        };
        let axis_sign = |v: IVec3, axis: usize| -> i32 {
            match axis {
                0 => v.x.signum(),
                1 => v.y.signum(),
                _ => v.z.signum(),
            }
        };

        let mut out = 0u8;
        for (bit, dir, orth) in [
            (1u8, u_pos, v_pos),
            (2u8, -u_pos, v_pos),
            (4u8, v_pos, u_pos),
            (8u8, -v_pos, u_pos),
        ] {
            let nx = x as i32 + dir.x;
            let ny = y as i32 + dir.y;
            let nz = z as i32 + dir.z;
            let candidates = [
                sample_with_borders(nx, ny, nz),
                neighbor_stacked(nx, ny, nz),
            ];
            let shift = [dir.x as f32, dir.y as f32, dir.z as f32];
            let edge_axis = axis_idx(dir);
            let edge_sign = axis_sign(dir, edge_axis);
            let orth_axis = axis_idx(orth);
            let normal_axis = axis_idx(face_normal);

            let mut connected = false;
            for neigh_id in candidates {
                if neigh_id == 0 || reg.connect_group(neigh_id) != self_group {
                    continue;
                }
                let (other_min, other_max) =
                    if let Some((other_size_m, other_offset_m)) = reg.custom_mesh_box(neigh_id) {
                        local_box_bounds(other_size_m, other_offset_m)
                    } else {
                        ([0.0, 0.0, 0.0], [1.0, 1.0, 1.0])
                    };
                let shifted_min = [
                    other_min[0] + shift[0],
                    other_min[1] + shift[1],
                    other_min[2] + shift[2],
                ];
                let shifted_max = [
                    other_max[0] + shift[0],
                    other_max[1] + shift[1],
                    other_max[2] + shift[2],
                ];
                if !overlap(
                    self_min[orth_axis],
                    self_max[orth_axis],
                    shifted_min[orth_axis],
                    shifted_max[orth_axis],
                ) {
                    continue;
                }
                if !overlap(
                    self_min[normal_axis],
                    self_max[normal_axis],
                    shifted_min[normal_axis],
                    shifted_max[normal_axis],
                ) {
                    continue;
                }
                let touches = if edge_sign > 0 {
                    shifted_min[edge_axis] <= self_max[edge_axis] + EPS
                        && shifted_max[edge_axis] > self_max[edge_axis] + EPS
                } else {
                    shifted_max[edge_axis] >= self_min[edge_axis] - EPS
                        && shifted_min[edge_axis] < self_min[edge_axis] - EPS
                };
                if touches {
                    connected = true;
                    break;
                }
            }
            if connected {
                out |= bit;
            }
        }
        out
    };
    let connected_neighbor_occludes_face = |self_id: BlockId,
                                            self_size_m: [f32; 3],
                                            self_offset_m: [f32; 3],
                                            face: Face,
                                            x: usize,
                                            y: usize,
                                            z: usize|
     -> bool {
        let self_group = reg.connect_group(self_id);
        if self_group == 0 {
            return false;
        }

        let cell_off = match face {
            Face::Top => IVec3::new(0, 1, 0),
            Face::Bottom => IVec3::new(0, -1, 0),
            Face::East => IVec3::new(1, 0, 0),
            Face::West => IVec3::new(-1, 0, 0),
            Face::South => IVec3::new(0, 0, 1),
            Face::North => IVec3::new(0, 0, -1),
        };

        let nx = x as i32 + cell_off.x;
        let ny = y as i32 + cell_off.y;
        let nz = z as i32 + cell_off.z;
        let neigh_primary = sample_with_borders(nx, ny, nz);
        let neigh_stacked = neighbor_stacked(nx, ny, nz);

        let (self_min, self_max) = local_box_bounds(self_size_m, self_offset_m);
        let shift = [cell_off.x as f32, cell_off.y as f32, cell_off.z as f32];

        for neigh_id in [neigh_primary, neigh_stacked] {
            if neigh_id == 0 || reg.connect_group(neigh_id) != self_group {
                continue;
            }

            let (other_min, other_max) =
                if let Some((other_size_m, other_offset_m)) = reg.custom_mesh_box(neigh_id) {
                    local_box_bounds(other_size_m, other_offset_m)
                } else {
                    ([0.0, 0.0, 0.0], [1.0, 1.0, 1.0])
                };
            let shifted_min = [
                other_min[0] + shift[0],
                other_min[1] + shift[1],
                other_min[2] + shift[2],
            ];
            let shifted_max = [
                other_max[0] + shift[0],
                other_max[1] + shift[1],
                other_max[2] + shift[2],
            ];
            if face_occluded_by_bounds(self_min, self_max, shifted_min, shifted_max, face) {
                return true;
            }
        }
        false
    };

    // +Y (Top): greedy in XZ plane for each Y slice.
    let mut top_mask = vec![0u16; CX * CZ];
    for y in y0..y1 {
        top_mask.fill(0);
        for z in 0..CZ {
            for x in 0..CX {
                let id = chunk.get(x, y, z);
                if !is_cube_voxel(id) || is_connected_cube_voxel(id) {
                    continue;
                }
                let n_up = get(x as isize, y as isize + 1, z as isize);
                if face_visible(id, n_up) {
                    top_mask[z * CX + x] = id;
                }
            }
        }
        greedy_merge_mask(&top_mask, CX, CZ, |x, z, rw, rh, id| {
            let u = reg.uv(id, Face::Top);
            let b = by_block.entry(id).or_insert_with(MeshBuild::new);
            let x0 = x as f32 * s;
            let x1 = (x + rw) as f32 * s;
            let z0 = z as f32 * s;
            let z1 = (z + rh) as f32 * s;
            let yp = (y + 1) as f32 * s;
            b.quad(
                [[x0, yp, z1], [x1, yp, z1], [x1, yp, z0], [x0, yp, z0]],
                [0.0, 1.0, 0.0],
                uvq_tiled(rw as f32, rh as f32, false),
                [u.u0, u.v0, u.u1, u.v1],
            );
        });
    }

    // -Y (Bottom): greedy in XZ plane for each Y slice.
    let mut bot_mask = vec![0u16; CX * CZ];
    for y in y0..y1 {
        bot_mask.fill(0);
        for z in 0..CZ {
            for x in 0..CX {
                let id = chunk.get(x, y, z);
                if !is_cube_voxel(id) || is_connected_cube_voxel(id) {
                    continue;
                }
                let n_down = get(x as isize, y as isize - 1, z as isize);
                if face_visible(id, n_down) {
                    bot_mask[z * CX + x] = id;
                }
            }
        }
        greedy_merge_mask(&bot_mask, CX, CZ, |x, z, rw, rh, id| {
            let u = reg.uv(id, Face::Bottom);
            let b = by_block.entry(id).or_insert_with(MeshBuild::new);
            let x0 = x as f32 * s;
            let x1 = (x + rw) as f32 * s;
            let z0 = z as f32 * s;
            let z1 = (z + rh) as f32 * s;
            let yp = y as f32 * s;
            b.quad(
                [[x0, yp, z0], [x1, yp, z0], [x1, yp, z1], [x0, yp, z1]],
                [0.0, -1.0, 0.0],
                uvq_tiled(rw as f32, rh as f32, false),
                [u.u0, u.v0, u.u1, u.v1],
            );
        });
    }

    // +X (East): greedy in ZY plane for each X slice.
    let mut east_mask = vec![0u16; CZ * yh];
    for x in 0..CX {
        east_mask.fill(0);
        for y in y0..y1 {
            let yr = y - y0;
            for z in 0..CZ {
                let id = chunk.get(x, y, z);
                if !is_cube_voxel(id) || is_connected_cube_voxel(id) {
                    continue;
                }
                let n_east = if x + 1 < CX {
                    Some(chunk.get(x + 1, y, z))
                } else {
                    east_at_opt(y, z)
                };
                if let Some(nei) = n_east {
                    if face_visible(id, nei) {
                        east_mask[yr * CZ + z] = id;
                    }
                }
            }
        }
        greedy_merge_mask(&east_mask, CZ, yh, |z, yr, rz, ry, id| {
            let u = reg.uv(id, Face::East);
            let b = by_block.entry(id).or_insert_with(MeshBuild::new);
            let z0 = z as f32 * s;
            let z1 = (z + rz) as f32 * s;
            let y0p = (y0 + yr) as f32 * s;
            let y1p = (y0 + yr + ry) as f32 * s;
            let xp = (x + 1) as f32 * s;
            b.quad(
                [[xp, y0p, z1], [xp, y0p, z0], [xp, y1p, z0], [xp, y1p, z1]],
                [1.0, 0.0, 0.0],
                uvq_tiled(rz as f32, ry as f32, true),
                [u.u0, u.v0, u.u1, u.v1],
            );
        });
    }

    // -X (West): greedy in ZY plane for each X slice.
    let mut west_mask = vec![0u16; CZ * yh];
    for x in 0..CX {
        west_mask.fill(0);
        for y in y0..y1 {
            let yr = y - y0;
            for z in 0..CZ {
                let id = chunk.get(x, y, z);
                if !is_cube_voxel(id) || is_connected_cube_voxel(id) {
                    continue;
                }
                let n_west = if x > 0 {
                    Some(chunk.get(x - 1, y, z))
                } else {
                    west_at_opt(y, z)
                };
                if let Some(nei) = n_west {
                    if face_visible(id, nei) {
                        west_mask[yr * CZ + z] = id;
                    }
                }
            }
        }
        greedy_merge_mask(&west_mask, CZ, yh, |z, yr, rz, ry, id| {
            let u = reg.uv(id, Face::West);
            let b = by_block.entry(id).or_insert_with(MeshBuild::new);
            let z0 = z as f32 * s;
            let z1 = (z + rz) as f32 * s;
            let y0p = (y0 + yr) as f32 * s;
            let y1p = (y0 + yr + ry) as f32 * s;
            let xp = x as f32 * s;
            b.quad(
                [[xp, y0p, z0], [xp, y0p, z1], [xp, y1p, z1], [xp, y1p, z0]],
                [-1.0, 0.0, 0.0],
                uvq_tiled(rz as f32, ry as f32, true),
                [u.u0, u.v0, u.u1, u.v1],
            );
        });
    }

    // +Z (South): greedy in XY plane for each Z slice.
    let mut south_mask = vec![0u16; CX * yh];
    for z in 0..CZ {
        south_mask.fill(0);
        for y in y0..y1 {
            let yr = y - y0;
            for x in 0..CX {
                let id = chunk.get(x, y, z);
                if !is_cube_voxel(id) || is_connected_cube_voxel(id) {
                    continue;
                }
                let n_south = if z + 1 < CZ {
                    Some(chunk.get(x, y, z + 1))
                } else {
                    south_at_opt(y, x)
                };
                if let Some(nei) = n_south {
                    if face_visible(id, nei) {
                        south_mask[yr * CX + x] = id;
                    }
                }
            }
        }
        greedy_merge_mask(&south_mask, CX, yh, |x, yr, rx, ry, id| {
            let u = reg.uv(id, Face::South);
            let b = by_block.entry(id).or_insert_with(MeshBuild::new);
            let x0 = x as f32 * s;
            let x1 = (x + rx) as f32 * s;
            let y0p = (y0 + yr) as f32 * s;
            let y1p = (y0 + yr + ry) as f32 * s;
            let zp = (z + 1) as f32 * s;
            b.quad(
                [[x0, y0p, zp], [x1, y0p, zp], [x1, y1p, zp], [x0, y1p, zp]],
                [0.0, 0.0, 1.0],
                uvq_tiled(rx as f32, ry as f32, true),
                [u.u0, u.v0, u.u1, u.v1],
            );
        });
    }

    // -Z (North): greedy in XY plane for each Z slice.
    let mut north_mask = vec![0u16; CX * yh];
    for z in 0..CZ {
        north_mask.fill(0);
        for y in y0..y1 {
            let yr = y - y0;
            for x in 0..CX {
                let id = chunk.get(x, y, z);
                if !is_cube_voxel(id) || is_connected_cube_voxel(id) {
                    continue;
                }
                let n_north = if z > 0 {
                    Some(chunk.get(x, y, z - 1))
                } else {
                    north_at_opt(y, x)
                };
                if let Some(nei) = n_north {
                    if face_visible(id, nei) {
                        north_mask[yr * CX + x] = id;
                    }
                }
            }
        }
        greedy_merge_mask(&north_mask, CX, yh, |x, yr, rx, ry, id| {
            let u = reg.uv(id, Face::North);
            let b = by_block.entry(id).or_insert_with(MeshBuild::new);
            let x0 = x as f32 * s;
            let x1 = (x + rx) as f32 * s;
            let y0p = (y0 + yr) as f32 * s;
            let y1p = (y0 + yr + ry) as f32 * s;
            let zp = z as f32 * s;
            b.quad(
                [[x1, y0p, zp], [x0, y0p, zp], [x0, y1p, zp], [x1, y1p, zp]],
                [0.0, 0.0, -1.0],
                uvq_tiled(rx as f32, ry as f32, true),
                [u.u0, u.v0, u.u1, u.v1],
            );
        });
    }

    // Connected-texture cube pass (mask4, non-greedy per face).
    let connected_tile_rect = |id: BlockId, face: Face, x: usize, y: usize, z: usize| -> [f32; 4] {
        let uv = reg
            .connected_mask4_uv(id, connected_mask4(id, face, x, y, z))
            .unwrap_or_else(|| reg.uv(id, face));
        [uv.u0, uv.v0, uv.u1, uv.v1]
    };
    let connected_ctm = |id: BlockId, face: Face, x: usize, y: usize, z: usize| -> [f32; 2] {
        let mask = connected_mask4(id, face, x, y, z);
        [mask as f32, reg.connected_edge_clip_uv(id)]
    };
    let face_neighbor = |face: Face, x: usize, y: usize, z: usize| -> BlockId {
        let p = IVec3::new(x as i32, y as i32, z as i32);
        let off = match face {
            Face::Top => IVec3::new(0, 1, 0),
            Face::Bottom => IVec3::new(0, -1, 0),
            Face::North => IVec3::new(0, 0, -1),
            Face::East => IVec3::new(1, 0, 0),
            Face::South => IVec3::new(0, 0, 1),
            Face::West => IVec3::new(-1, 0, 0),
        };
        sample_with_borders(p.x + off.x, p.y + off.y, p.z + off.z)
    };
    for y in y0..y1 {
        for z in 0..CZ {
            for x in 0..CX {
                let id = chunk.get(x, y, z);
                if !is_connected_cube_voxel(id) {
                    continue;
                }
                let b = by_block.entry(id).or_insert_with(MeshBuild::new);

                let x0 = x as f32 * s;
                let x1 = (x + 1) as f32 * s;
                let y0p = y as f32 * s;
                let y1p = (y + 1) as f32 * s;
                let z0 = z as f32 * s;
                let z1 = (z + 1) as f32 * s;

                if face_visible(id, face_neighbor(Face::Top, x, y, z)) {
                    b.quad_with_ctm(
                        [[x0, y1p, z1], [x1, y1p, z1], [x1, y1p, z0], [x0, y1p, z0]],
                        [0.0, 1.0, 0.0],
                        uvq_tiled(1.0, 1.0, false),
                        connected_tile_rect(id, Face::Top, x, y, z),
                        connected_ctm(id, Face::Top, x, y, z),
                    );
                }
                if face_visible(id, face_neighbor(Face::Bottom, x, y, z)) {
                    b.quad_with_ctm(
                        [[x0, y0p, z0], [x1, y0p, z0], [x1, y0p, z1], [x0, y0p, z1]],
                        [0.0, -1.0, 0.0],
                        uvq_tiled(1.0, 1.0, false),
                        connected_tile_rect(id, Face::Bottom, x, y, z),
                        connected_ctm(id, Face::Bottom, x, y, z),
                    );
                }
                if face_visible(id, face_neighbor(Face::East, x, y, z)) {
                    b.quad_with_ctm(
                        [[x1, y0p, z1], [x1, y0p, z0], [x1, y1p, z0], [x1, y1p, z1]],
                        [1.0, 0.0, 0.0],
                        uvq_tiled(1.0, 1.0, true),
                        connected_tile_rect(id, Face::East, x, y, z),
                        connected_ctm(id, Face::East, x, y, z),
                    );
                }
                if face_visible(id, face_neighbor(Face::West, x, y, z)) {
                    b.quad_with_ctm(
                        [[x0, y0p, z0], [x0, y0p, z1], [x0, y1p, z1], [x0, y1p, z0]],
                        [-1.0, 0.0, 0.0],
                        uvq_tiled(1.0, 1.0, true),
                        connected_tile_rect(id, Face::West, x, y, z),
                        connected_ctm(id, Face::West, x, y, z),
                    );
                }
                if face_visible(id, face_neighbor(Face::South, x, y, z)) {
                    b.quad_with_ctm(
                        [[x0, y0p, z1], [x1, y0p, z1], [x1, y1p, z1], [x0, y1p, z1]],
                        [0.0, 0.0, 1.0],
                        uvq_tiled(1.0, 1.0, true),
                        connected_tile_rect(id, Face::South, x, y, z),
                        connected_ctm(id, Face::South, x, y, z),
                    );
                }
                if face_visible(id, face_neighbor(Face::North, x, y, z)) {
                    b.quad_with_ctm(
                        [[x1, y0p, z0], [x0, y0p, z0], [x0, y1p, z0], [x1, y1p, z0]],
                        [0.0, 0.0, -1.0],
                        uvq_tiled(1.0, 1.0, true),
                        connected_tile_rect(id, Face::North, x, y, z),
                        connected_ctm(id, Face::North, x, y, z),
                    );
                }
            }
        }
    }

    // Custom block box pass (non-prop blocks with collider.kind = box).
    for y in y0..y1 {
        for z in 0..CZ {
            for x in 0..CX {
                let id = chunk.get(x, y, z);
                if !reg.mesh_visible(id) {
                    continue;
                }
                let Some((size_m, offset_m)) = reg.custom_mesh_box(id) else {
                    continue;
                };

                let u_top = reg.uv(id, Face::Top);
                let u_bottom = reg.uv(id, Face::Bottom);
                let u_east = reg.uv(id, Face::East);
                let u_west = reg.uv(id, Face::West);
                let u_south = reg.uv(id, Face::South);
                let u_north = reg.uv(id, Face::North);
                let b = by_block.entry(id).or_insert_with(MeshBuild::new);

                let half_x = (size_m[0] * s * 0.5).max(0.0005);
                let half_y = (size_m[1] * s * 0.5).max(0.0005);
                let half_z = (size_m[2] * s * 0.5).max(0.0005);
                let cx = (x as f32 + 0.5 + offset_m[0]) * s;
                let cy = (y as f32 + 0.5 + offset_m[1]) * s;
                let cz = (z as f32 + 0.5 + offset_m[2]) * s;
                let min_x = cx - half_x;
                let max_x = cx + half_x;
                let min_y = cy - half_y;
                let max_y = cy + half_y;
                let min_z = cz - half_z;
                let max_z = cz + half_z;
                let is_fluid_box = reg.fluid(id);
                let fluid_base_y = y as f32 * s;
                let fluid_top_from_level =
                    |level: u8| fluid_base_y + (level as f32 / 10.0).clamp(0.0, 1.0) * s;
                // Small inset against non-fluid neighbors to avoid coplanar transparency flicker.
                let side_plane_inset = 0.003 * s;
                // Stronger inset for transparent-solid neighbors (e.g. glass) so water
                // stays visible behind glass without coplanar flicker.
                let side_plane_inset_transparent_solid = 0.04 * s;
                // Lift bottom face of water slightly to reduce transparent depth fighting.
                let fluid_bottom_face_lift = 0.1 * s;
                let top_neigh = face_neighbor(Face::Top, x, y, z);
                let bottom_neigh = face_neighbor(Face::Bottom, x, y, z);
                let fluid_above = is_fluid_box && reg.fluid(top_neigh);
                let fluid_below = is_fluid_box && reg.fluid(bottom_neigh);
                let connected = reg.has_connected_mask4(id);
                let requires_face_visibility = connected || is_fluid_box;
                let framed_slab = connected
                    && reg.connected_edge_clip_uv(id) > 0.0
                    && (size_m[0] < 0.999 || size_m[1] < 0.999 || size_m[2] < 0.999);
                let uv_span =
                    |dim: f32| -> f32 { if framed_slab && dim < 0.999 { 1.0 } else { dim } };
                let same_cell_other = chunk.get_stacked(x, y, z);
                let connected_mask_for_face = |face: Face| -> u8 {
                    let neighbor_mask =
                        connected_neighbor_edge_mask(id, size_m, offset_m, face, x, y, z);
                    let same_cell_mask =
                        same_cell_connected_edge_mask(id, size_m, offset_m, same_cell_other, face);
                    neighbor_mask | same_cell_mask
                };
                let water_flow_vec = if is_fluid_box {
                    let xi = x as i32;
                    let yi = y as i32;
                    let zi = z as i32;
                    let self_level = reg.fluid_level(id) as f32;
                    let flow_weight = |neigh_id: BlockId| -> f32 {
                        if reg.fluid(neigh_id) {
                            (self_level - reg.fluid_level(neigh_id) as f32).max(0.0)
                        } else if neigh_id == 0 || !reg.opaque(neigh_id) {
                            (self_level * 0.35).max(0.5)
                        } else {
                            0.0
                        }
                    };
                    let w_e = flow_weight(sample_with_borders(xi + 1, yi, zi));
                    let w_w = flow_weight(sample_with_borders(xi - 1, yi, zi));
                    let w_s = flow_weight(sample_with_borders(xi, yi, zi + 1));
                    let w_n = flow_weight(sample_with_borders(xi, yi, zi - 1));
                    if fluid_above || fluid_below {
                        // Falling columns should use stable vertical shader flow.
                        [0.0, 0.0]
                    } else {
                        let mut dir = Vec2::new(w_e - w_w, w_s - w_n);
                        if dir.length_squared() > 1e-8 {
                            dir = dir.normalize();
                            [dir.x, dir.y]
                        } else {
                            [0.0, 0.0]
                        }
                    }
                } else {
                    [-1.0, 0.0]
                };
                let face_ctm = |mask: u8| -> [f32; 2] {
                    if connected {
                        [mask as f32, reg.connected_edge_clip_uv(id)]
                    } else if is_fluid_box {
                        water_flow_vec
                    } else {
                        [-1.0, 0.0]
                    }
                };
                let mut top_nw = max_y;
                let mut top_ne = max_y;
                let mut top_sw = max_y;
                let mut top_se = max_y;
                if is_fluid_box {
                    if fluid_above {
                        // Falling columns should be visually continuous without top-edge gaps.
                        let full_top = fluid_base_y + s;
                        top_nw = full_top;
                        top_ne = full_top;
                        top_sw = full_top;
                        top_se = full_top;
                    } else {
                        let yi = y as i32;
                        let fluid_height_from_pos = |sx: i32, sz: i32| -> Option<f32> {
                            let nid = sample_with_borders(sx, yi, sz);
                            if reg.fluid(nid) {
                                Some(fluid_top_from_level(reg.fluid_level(nid)))
                            } else if reg.fluid(sample_with_borders(sx, yi + 1, sz)) {
                                Some((yi as f32 + 1.0) * s)
                            } else {
                                None
                            }
                        };
                        // Weighted corner sampling keeps diagonals smooth while preserving local level.
                        // Weight current cell stronger than cardinal/diagonal neighbors.
                        let fluid_corner_height = |samples: [(i32, i32, f32); 4]| -> f32 {
                            let mut sum = 0.0f32;
                            let mut count = 0.0f32;
                            for (sx, sz, w) in samples {
                                if let Some(h) = fluid_height_from_pos(sx, sz) {
                                    sum += h * w;
                                    count += w;
                                }
                            }
                            if count <= 0.0 {
                                max_y
                            } else {
                                (sum / count).clamp(min_y, fluid_base_y + s)
                            }
                        };
                        let xi = x as i32;
                        let zi = z as i32;
                        top_nw = fluid_corner_height([
                            (xi, zi, 1.0),
                            (xi - 1, zi, 1.0),
                            (xi, zi - 1, 1.0),
                            (xi - 1, zi - 1, 1.0),
                        ]);
                        top_ne = fluid_corner_height([
                            (xi, zi, 1.0),
                            (xi + 1, zi, 1.0),
                            (xi, zi - 1, 1.0),
                            (xi + 1, zi - 1, 1.0),
                        ]);
                        top_sw = fluid_corner_height([
                            (xi, zi, 1.0),
                            (xi - 1, zi, 1.0),
                            (xi, zi + 1, 1.0),
                            (xi - 1, zi + 1, 1.0),
                        ]);
                        top_se = fluid_corner_height([
                            (xi, zi, 1.0),
                            (xi + 1, zi, 1.0),
                            (xi, zi + 1, 1.0),
                            (xi + 1, zi + 1, 1.0),
                        ]);
                    }
                }
                let top_y_min = top_nw.min(top_ne).min(top_sw).min(top_se);
                let bottom_y = if is_fluid_box {
                    let lift = if bottom_neigh != 0 && !reg.fluid(bottom_neigh) {
                        fluid_bottom_face_lift
                    } else {
                        0.0
                    };
                    (min_y + lift).min(top_y_min - 0.0001).max(min_y)
                } else {
                    min_y
                };
                let top_normal = if is_fluid_box {
                    let e1 = Vec3::new(max_x - min_x, top_se - top_sw, 0.0);
                    let e2 = Vec3::new(0.0, top_nw - top_sw, min_z - max_z);
                    let n = e1.cross(e2);
                    if n.length_squared() > 1e-8 {
                        let nn = n.normalize();
                        [nn.x, nn.y, nn.z]
                    } else {
                        [0.0, 1.0, 0.0]
                    }
                } else {
                    [0.0, 1.0, 0.0]
                };

                if (!is_fluid_box || !reg.fluid(top_neigh))
                    && (!requires_face_visibility
                        || face_visible(id, face_neighbor(Face::Top, x, y, z)))
                    && !(connected
                        && connected_neighbor_occludes_face(
                            id,
                            size_m,
                            offset_m,
                            Face::Top,
                            x,
                            y,
                            z,
                        ))
                    && !same_cell_connected_occludes_face(
                        id,
                        size_m,
                        offset_m,
                        same_cell_other,
                        Face::Top,
                    )
                {
                    let mask = connected_mask_for_face(Face::Top);
                    b.quad_with_ctm(
                        [
                            [min_x, top_sw, max_z],
                            [max_x, top_se, max_z],
                            [max_x, top_ne, min_z],
                            [min_x, top_nw, min_z],
                        ],
                        top_normal,
                        uvq_tiled(uv_span(size_m[0]), uv_span(size_m[2]), false),
                        if connected {
                            let uv = reg
                                .connected_mask4_uv(id, mask)
                                .unwrap_or_else(|| reg.uv(id, Face::Top));
                            [uv.u0, uv.v0, uv.u1, uv.v1]
                        } else {
                            [u_top.u0, u_top.v0, u_top.u1, u_top.v1]
                        },
                        face_ctm(mask),
                    );
                }
                if (!is_fluid_box || (!fluid_below && !fluid_above))
                    && (!requires_face_visibility
                        || face_visible(id, face_neighbor(Face::Bottom, x, y, z)))
                    && !(connected
                        && connected_neighbor_occludes_face(
                            id,
                            size_m,
                            offset_m,
                            Face::Bottom,
                            x,
                            y,
                            z,
                        ))
                    && !same_cell_connected_occludes_face(
                        id,
                        size_m,
                        offset_m,
                        same_cell_other,
                        Face::Bottom,
                    )
                {
                    let mask = connected_mask_for_face(Face::Bottom);
                    b.quad_with_ctm(
                        [
                            [min_x, bottom_y, min_z],
                            [max_x, bottom_y, min_z],
                            [max_x, bottom_y, max_z],
                            [min_x, bottom_y, max_z],
                        ],
                        [0.0, -1.0, 0.0],
                        uvq_tiled(uv_span(size_m[0]), uv_span(size_m[2]), false),
                        if connected {
                            let uv = reg
                                .connected_mask4_uv(id, mask)
                                .unwrap_or_else(|| reg.uv(id, Face::Bottom));
                            [uv.u0, uv.v0, uv.u1, uv.v1]
                        } else {
                            [u_bottom.u0, u_bottom.v0, u_bottom.u1, u_bottom.v1]
                        },
                        face_ctm(mask),
                    );
                }
                let east_neigh = face_neighbor(Face::East, x, y, z);
                if (!requires_face_visibility || face_visible(id, east_neigh))
                    && !(connected
                        && connected_neighbor_occludes_face(
                            id,
                            size_m,
                            offset_m,
                            Face::East,
                            x,
                            y,
                            z,
                        ))
                    && !same_cell_connected_occludes_face(
                        id,
                        size_m,
                        offset_m,
                        same_cell_other,
                        Face::East,
                    )
                {
                    let mask = connected_mask_for_face(Face::East);
                    let mut east_x = max_x;
                    let mut east_y0 = min_y;
                    let east_top_n = top_ne;
                    let east_top_s = top_se;
                    if is_fluid_box {
                        if reg.fluid(east_neigh) {
                            east_y0 =
                                east_y0.max(fluid_top_from_level(reg.fluid_level(east_neigh)));
                        } else if east_neigh != 0 {
                            let inset = if reg.solid(east_neigh) && !reg.opaque(east_neigh) {
                                side_plane_inset_transparent_solid
                            } else {
                                side_plane_inset
                            };
                            east_x -= inset;
                        }
                    }
                    if east_y0 < east_top_n.max(east_top_s) - 0.0001 {
                        b.quad_with_ctm(
                            [
                                [east_x, east_y0, max_z],
                                [east_x, east_y0, min_z],
                                [east_x, east_top_n, min_z],
                                [east_x, east_top_s, max_z],
                            ],
                            [1.0, 0.0, 0.0],
                            uvq_tiled(uv_span(size_m[2]), uv_span(size_m[1]), true),
                            if connected {
                                let uv = reg
                                    .connected_mask4_uv(id, mask)
                                    .unwrap_or_else(|| reg.uv(id, Face::East));
                                [uv.u0, uv.v0, uv.u1, uv.v1]
                            } else {
                                [u_east.u0, u_east.v0, u_east.u1, u_east.v1]
                            },
                            face_ctm(mask),
                        );
                    }
                }
                let west_neigh = face_neighbor(Face::West, x, y, z);
                if (!requires_face_visibility || face_visible(id, west_neigh))
                    && !(connected
                        && connected_neighbor_occludes_face(
                            id,
                            size_m,
                            offset_m,
                            Face::West,
                            x,
                            y,
                            z,
                        ))
                    && !same_cell_connected_occludes_face(
                        id,
                        size_m,
                        offset_m,
                        same_cell_other,
                        Face::West,
                    )
                {
                    let mask = connected_mask_for_face(Face::West);
                    let mut west_x = min_x;
                    let mut west_y0 = min_y;
                    let west_top_n = top_nw;
                    let west_top_s = top_sw;
                    if is_fluid_box {
                        if reg.fluid(west_neigh) {
                            west_y0 =
                                west_y0.max(fluid_top_from_level(reg.fluid_level(west_neigh)));
                        } else if west_neigh != 0 {
                            let inset = if reg.solid(west_neigh) && !reg.opaque(west_neigh) {
                                side_plane_inset_transparent_solid
                            } else {
                                side_plane_inset
                            };
                            west_x += inset;
                        }
                    }
                    if west_y0 < west_top_n.max(west_top_s) - 0.0001 {
                        b.quad_with_ctm(
                            [
                                [west_x, west_y0, min_z],
                                [west_x, west_y0, max_z],
                                [west_x, west_top_s, max_z],
                                [west_x, west_top_n, min_z],
                            ],
                            [-1.0, 0.0, 0.0],
                            uvq_tiled(uv_span(size_m[2]), uv_span(size_m[1]), true),
                            if connected {
                                let uv = reg
                                    .connected_mask4_uv(id, mask)
                                    .unwrap_or_else(|| reg.uv(id, Face::West));
                                [uv.u0, uv.v0, uv.u1, uv.v1]
                            } else {
                                [u_west.u0, u_west.v0, u_west.u1, u_west.v1]
                            },
                            face_ctm(mask),
                        );
                    }
                }
                let south_neigh = face_neighbor(Face::South, x, y, z);
                if (!requires_face_visibility || face_visible(id, south_neigh))
                    && !(connected
                        && connected_neighbor_occludes_face(
                            id,
                            size_m,
                            offset_m,
                            Face::South,
                            x,
                            y,
                            z,
                        ))
                    && !same_cell_connected_occludes_face(
                        id,
                        size_m,
                        offset_m,
                        same_cell_other,
                        Face::South,
                    )
                {
                    let mask = connected_mask_for_face(Face::South);
                    let mut south_z = max_z;
                    let mut south_y0 = min_y;
                    let south_top_w = top_sw;
                    let south_top_e = top_se;
                    if is_fluid_box {
                        if reg.fluid(south_neigh) {
                            south_y0 =
                                south_y0.max(fluid_top_from_level(reg.fluid_level(south_neigh)));
                        } else if south_neigh != 0 {
                            let inset = if reg.solid(south_neigh) && !reg.opaque(south_neigh) {
                                side_plane_inset_transparent_solid
                            } else {
                                side_plane_inset
                            };
                            south_z -= inset;
                        }
                    }
                    if south_y0 < south_top_w.max(south_top_e) - 0.0001 {
                        b.quad_with_ctm(
                            [
                                [min_x, south_y0, south_z],
                                [max_x, south_y0, south_z],
                                [max_x, south_top_e, south_z],
                                [min_x, south_top_w, south_z],
                            ],
                            [0.0, 0.0, 1.0],
                            uvq_tiled(uv_span(size_m[0]), uv_span(size_m[1]), true),
                            if connected {
                                let uv = reg
                                    .connected_mask4_uv(id, mask)
                                    .unwrap_or_else(|| reg.uv(id, Face::South));
                                [uv.u0, uv.v0, uv.u1, uv.v1]
                            } else {
                                [u_south.u0, u_south.v0, u_south.u1, u_south.v1]
                            },
                            face_ctm(mask),
                        );
                    }
                }
                let north_neigh = face_neighbor(Face::North, x, y, z);
                if (!requires_face_visibility || face_visible(id, north_neigh))
                    && !(connected
                        && connected_neighbor_occludes_face(
                            id,
                            size_m,
                            offset_m,
                            Face::North,
                            x,
                            y,
                            z,
                        ))
                    && !same_cell_connected_occludes_face(
                        id,
                        size_m,
                        offset_m,
                        same_cell_other,
                        Face::North,
                    )
                {
                    let mask = connected_mask_for_face(Face::North);
                    let mut north_z = min_z;
                    let mut north_y0 = min_y;
                    let north_top_e = top_ne;
                    let north_top_w = top_nw;
                    if is_fluid_box {
                        if reg.fluid(north_neigh) {
                            north_y0 =
                                north_y0.max(fluid_top_from_level(reg.fluid_level(north_neigh)));
                        } else if north_neigh != 0 {
                            let inset = if reg.solid(north_neigh) && !reg.opaque(north_neigh) {
                                side_plane_inset_transparent_solid
                            } else {
                                side_plane_inset
                            };
                            north_z += inset;
                        }
                    }
                    if north_y0 < north_top_e.max(north_top_w) - 0.0001 {
                        b.quad_with_ctm(
                            [
                                [max_x, north_y0, north_z],
                                [min_x, north_y0, north_z],
                                [min_x, north_top_w, north_z],
                                [max_x, north_top_e, north_z],
                            ],
                            [0.0, 0.0, -1.0],
                            uvq_tiled(uv_span(size_m[0]), uv_span(size_m[1]), true),
                            if connected {
                                let uv = reg
                                    .connected_mask4_uv(id, mask)
                                    .unwrap_or_else(|| reg.uv(id, Face::North));
                                [uv.u0, uv.v0, uv.u1, uv.v1]
                            } else {
                                [u_north.u0, u_north.v0, u_north.u1, u_north.v1]
                            },
                            face_ctm(mask),
                        );
                    }
                }
            }
        }
    }

    // Secondary stacked slab pass (same voxel, second occupant).
    for y in y0..y1 {
        for z in 0..CZ {
            for x in 0..CX {
                let id = chunk.get_stacked(x, y, z);
                if !reg.mesh_visible(id) {
                    continue;
                }
                if reg.fluid(id) {
                    // Fluids are rendered from the primary occupant pass.
                    // Skipping here prevents duplicate transparent geometry.
                    continue;
                }
                let Some((size_m, offset_m)) = reg.custom_mesh_box(id) else {
                    continue;
                };

                let u_top = reg.uv(id, Face::Top);
                let u_bottom = reg.uv(id, Face::Bottom);
                let u_east = reg.uv(id, Face::East);
                let u_west = reg.uv(id, Face::West);
                let u_south = reg.uv(id, Face::South);
                let u_north = reg.uv(id, Face::North);
                let b = by_block.entry(id).or_insert_with(MeshBuild::new);

                let half_x = (size_m[0] * s * 0.5).max(0.0005);
                let half_y = (size_m[1] * s * 0.5).max(0.0005);
                let half_z = (size_m[2] * s * 0.5).max(0.0005);
                let cx = (x as f32 + 0.5 + offset_m[0]) * s;
                let cy = (y as f32 + 0.5 + offset_m[1]) * s;
                let cz = (z as f32 + 0.5 + offset_m[2]) * s;
                let min_x = cx - half_x;
                let max_x = cx + half_x;
                let min_y = cy - half_y;
                let max_y = cy + half_y;
                let min_z = cz - half_z;
                let max_z = cz + half_z;
                let connected = reg.has_connected_mask4(id);
                let framed_slab = connected
                    && reg.connected_edge_clip_uv(id) > 0.0
                    && (size_m[0] < 0.999 || size_m[1] < 0.999 || size_m[2] < 0.999);
                let uv_span =
                    |dim: f32| -> f32 { if framed_slab && dim < 0.999 { 1.0 } else { dim } };
                let same_cell_other = chunk.get(x, y, z);
                let connected_mask_for_face = |face: Face| -> u8 {
                    let neighbor_mask =
                        connected_neighbor_edge_mask(id, size_m, offset_m, face, x, y, z);
                    let same_cell_mask =
                        same_cell_connected_edge_mask(id, size_m, offset_m, same_cell_other, face);
                    neighbor_mask | same_cell_mask
                };

                if (!connected || face_visible(id, face_neighbor(Face::Top, x, y, z)))
                    && !(connected
                        && connected_neighbor_occludes_face(
                            id,
                            size_m,
                            offset_m,
                            Face::Top,
                            x,
                            y,
                            z,
                        ))
                    && !same_cell_connected_occludes_face(
                        id,
                        size_m,
                        offset_m,
                        same_cell_other,
                        Face::Top,
                    )
                {
                    let mask = connected_mask_for_face(Face::Top);
                    b.quad_with_ctm(
                        [
                            [min_x, max_y, max_z],
                            [max_x, max_y, max_z],
                            [max_x, max_y, min_z],
                            [min_x, max_y, min_z],
                        ],
                        [0.0, 1.0, 0.0],
                        uvq_tiled(uv_span(size_m[0]), uv_span(size_m[2]), false),
                        if connected {
                            let uv = reg
                                .connected_mask4_uv(id, mask)
                                .unwrap_or_else(|| reg.uv(id, Face::Top));
                            [uv.u0, uv.v0, uv.u1, uv.v1]
                        } else {
                            [u_top.u0, u_top.v0, u_top.u1, u_top.v1]
                        },
                        if connected {
                            [mask as f32, reg.connected_edge_clip_uv(id)]
                        } else {
                            [-1.0, 0.0]
                        },
                    );
                }
                if (!connected || face_visible(id, face_neighbor(Face::Bottom, x, y, z)))
                    && !(connected
                        && connected_neighbor_occludes_face(
                            id,
                            size_m,
                            offset_m,
                            Face::Bottom,
                            x,
                            y,
                            z,
                        ))
                    && !same_cell_connected_occludes_face(
                        id,
                        size_m,
                        offset_m,
                        same_cell_other,
                        Face::Bottom,
                    )
                {
                    let mask = connected_mask_for_face(Face::Bottom);
                    b.quad_with_ctm(
                        [
                            [min_x, min_y, min_z],
                            [max_x, min_y, min_z],
                            [max_x, min_y, max_z],
                            [min_x, min_y, max_z],
                        ],
                        [0.0, -1.0, 0.0],
                        uvq_tiled(uv_span(size_m[0]), uv_span(size_m[2]), false),
                        if connected {
                            let uv = reg
                                .connected_mask4_uv(id, mask)
                                .unwrap_or_else(|| reg.uv(id, Face::Bottom));
                            [uv.u0, uv.v0, uv.u1, uv.v1]
                        } else {
                            [u_bottom.u0, u_bottom.v0, u_bottom.u1, u_bottom.v1]
                        },
                        if connected {
                            [mask as f32, reg.connected_edge_clip_uv(id)]
                        } else {
                            [-1.0, 0.0]
                        },
                    );
                }
                if (!connected || face_visible(id, face_neighbor(Face::East, x, y, z)))
                    && !(connected
                        && connected_neighbor_occludes_face(
                            id,
                            size_m,
                            offset_m,
                            Face::East,
                            x,
                            y,
                            z,
                        ))
                    && !same_cell_connected_occludes_face(
                        id,
                        size_m,
                        offset_m,
                        same_cell_other,
                        Face::East,
                    )
                {
                    let mask = connected_mask_for_face(Face::East);
                    b.quad_with_ctm(
                        [
                            [max_x, min_y, max_z],
                            [max_x, min_y, min_z],
                            [max_x, max_y, min_z],
                            [max_x, max_y, max_z],
                        ],
                        [1.0, 0.0, 0.0],
                        uvq_tiled(uv_span(size_m[2]), uv_span(size_m[1]), true),
                        if connected {
                            let uv = reg
                                .connected_mask4_uv(id, mask)
                                .unwrap_or_else(|| reg.uv(id, Face::East));
                            [uv.u0, uv.v0, uv.u1, uv.v1]
                        } else {
                            [u_east.u0, u_east.v0, u_east.u1, u_east.v1]
                        },
                        if connected {
                            [mask as f32, reg.connected_edge_clip_uv(id)]
                        } else {
                            [-1.0, 0.0]
                        },
                    );
                }
                if (!connected || face_visible(id, face_neighbor(Face::West, x, y, z)))
                    && !(connected
                        && connected_neighbor_occludes_face(
                            id,
                            size_m,
                            offset_m,
                            Face::West,
                            x,
                            y,
                            z,
                        ))
                    && !same_cell_connected_occludes_face(
                        id,
                        size_m,
                        offset_m,
                        same_cell_other,
                        Face::West,
                    )
                {
                    let mask = connected_mask_for_face(Face::West);
                    b.quad_with_ctm(
                        [
                            [min_x, min_y, min_z],
                            [min_x, min_y, max_z],
                            [min_x, max_y, max_z],
                            [min_x, max_y, min_z],
                        ],
                        [-1.0, 0.0, 0.0],
                        uvq_tiled(uv_span(size_m[2]), uv_span(size_m[1]), true),
                        if connected {
                            let uv = reg
                                .connected_mask4_uv(id, mask)
                                .unwrap_or_else(|| reg.uv(id, Face::West));
                            [uv.u0, uv.v0, uv.u1, uv.v1]
                        } else {
                            [u_west.u0, u_west.v0, u_west.u1, u_west.v1]
                        },
                        if connected {
                            [mask as f32, reg.connected_edge_clip_uv(id)]
                        } else {
                            [-1.0, 0.0]
                        },
                    );
                }
                if (!connected || face_visible(id, face_neighbor(Face::South, x, y, z)))
                    && !(connected
                        && connected_neighbor_occludes_face(
                            id,
                            size_m,
                            offset_m,
                            Face::South,
                            x,
                            y,
                            z,
                        ))
                    && !same_cell_connected_occludes_face(
                        id,
                        size_m,
                        offset_m,
                        same_cell_other,
                        Face::South,
                    )
                {
                    let mask = connected_mask_for_face(Face::South);
                    b.quad_with_ctm(
                        [
                            [min_x, min_y, max_z],
                            [max_x, min_y, max_z],
                            [max_x, max_y, max_z],
                            [min_x, max_y, max_z],
                        ],
                        [0.0, 0.0, 1.0],
                        uvq_tiled(uv_span(size_m[0]), uv_span(size_m[1]), true),
                        if connected {
                            let uv = reg
                                .connected_mask4_uv(id, mask)
                                .unwrap_or_else(|| reg.uv(id, Face::South));
                            [uv.u0, uv.v0, uv.u1, uv.v1]
                        } else {
                            [u_south.u0, u_south.v0, u_south.u1, u_south.v1]
                        },
                        if connected {
                            [mask as f32, reg.connected_edge_clip_uv(id)]
                        } else {
                            [-1.0, 0.0]
                        },
                    );
                }
                if (!connected || face_visible(id, face_neighbor(Face::North, x, y, z)))
                    && !(connected
                        && connected_neighbor_occludes_face(
                            id,
                            size_m,
                            offset_m,
                            Face::North,
                            x,
                            y,
                            z,
                        ))
                    && !same_cell_connected_occludes_face(
                        id,
                        size_m,
                        offset_m,
                        same_cell_other,
                        Face::North,
                    )
                {
                    let mask = connected_mask_for_face(Face::North);
                    b.quad_with_ctm(
                        [
                            [max_x, min_y, min_z],
                            [min_x, min_y, min_z],
                            [min_x, max_y, min_z],
                            [max_x, max_y, min_z],
                        ],
                        [0.0, 0.0, -1.0],
                        uvq_tiled(uv_span(size_m[0]), uv_span(size_m[1]), true),
                        if connected {
                            let uv = reg
                                .connected_mask4_uv(id, mask)
                                .unwrap_or_else(|| reg.uv(id, Face::North));
                            [uv.u0, uv.v0, uv.u1, uv.v1]
                        } else {
                            [u_north.u0, u_north.v0, u_north.u1, u_north.v1]
                        },
                        if connected {
                            [mask as f32, reg.connected_edge_clip_uv(id)]
                        } else {
                            [-1.0, 0.0]
                        },
                    );
                }
            }
        }
    }

    // Prop pass: crossed planes (Minecraft/Hytale style plants).
    for y in y0..y1 {
        for z in 0..CZ {
            for x in 0..CX {
                let id = chunk.get(x, y, z);
                if !reg.mesh_visible(id) {
                    continue;
                }
                let Some(prop) = reg.prop(id) else {
                    continue;
                };
                if !prop.is_crossed_planes() {
                    continue;
                }

                let u = reg.uv(id, Face::North);
                let tile_rect = [u.u0, u.v0, u.u1, u.v1];
                let b = by_block.entry(id).or_insert_with(MeshBuild::new);

                let cx = (x as f32 + 0.5) * s;
                let cy0 = y as f32 * s;
                let cy1 = cy0 + prop.height_m * s;
                let cz = (z as f32 + 0.5) * s;
                let half_w = 0.5 * prop.width_m * s;
                let plane_count = prop.plane_count.max(2) as usize;
                let uv = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
                let seed = ((y as u32) << 16) ^ (id as u32).wrapping_mul(1_315_423_911);
                let h0 = col_rand_u32(x as i32, z as i32, seed);
                let h1 = col_rand_u32(z as i32, x as i32, seed ^ 0xA511_E9B3);
                let base_angle = (h0 as f32 / u32::MAX as f32) * std::f32::consts::PI;
                let lean_angle = (h1 as f32 / u32::MAX as f32) * std::f32::consts::TAU;
                let lean_len = (prop.height_m * s) * prop.tilt_deg.to_radians().tan();
                let lean = Vec2::new(lean_angle.cos(), lean_angle.sin()) * lean_len;

                for i in 0..plane_count {
                    let angle =
                        base_angle + (i as f32) * std::f32::consts::PI / (plane_count as f32);
                    let dir = Vec2::new(angle.cos(), angle.sin());

                    let p0 = [cx - dir.x * half_w, cy0, cz - dir.y * half_w];
                    let p1 = [cx + dir.x * half_w, cy0, cz + dir.y * half_w];
                    let p2 = [
                        cx + dir.x * half_w + lean.x,
                        cy1,
                        cz + dir.y * half_w + lean.y,
                    ];
                    let p3 = [
                        cx - dir.x * half_w + lean.x,
                        cy1,
                        cz - dir.y * half_w + lean.y,
                    ];

                    let p0v = Vec3::from(p0);
                    let p1v = Vec3::from(p1);
                    let p3v = Vec3::from(p3);
                    let fallback_normal = Vec3::new(dir.y, 0.0, -dir.x);
                    let mut normal = (p3v - p0v).cross(p1v - p0v);
                    if normal.length_squared() > 1e-6 {
                        normal = normal.normalize();
                    } else {
                        normal = fallback_normal;
                    }

                    b.quad([p0, p1, p2, p3], normal.to_array(), uv, tile_rect);
                    b.quad([p1, p0, p3, p2], (-normal).to_array(), uv, tile_rect);
                }
            }
        }
    }

    // Secondary stacked prop pass (water-logged plants etc.).
    for y in y0..y1 {
        for z in 0..CZ {
            for x in 0..CX {
                let id = chunk.get_stacked(x, y, z);
                if !reg.mesh_visible(id) {
                    continue;
                }
                let Some(prop) = reg.prop(id) else {
                    continue;
                };
                if !prop.is_crossed_planes() {
                    continue;
                }

                let u = reg.uv(id, Face::North);
                let tile_rect = [u.u0, u.v0, u.u1, u.v1];
                let b = by_block.entry(id).or_insert_with(MeshBuild::new);

                let cx = (x as f32 + 0.5) * s;
                let cy0 = y as f32 * s;
                let cy1 = cy0 + prop.height_m * s;
                let cz = (z as f32 + 0.5) * s;
                let half_w = 0.5 * prop.width_m * s;
                let plane_count = prop.plane_count.max(2) as usize;
                let uv = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
                let seed = ((y as u32) << 16) ^ (id as u32).wrapping_mul(1_315_423_911);
                let h0 = col_rand_u32(x as i32, z as i32, seed);
                let h1 = col_rand_u32(z as i32, x as i32, seed ^ 0xA511_E9B3);
                let base_angle = (h0 as f32 / u32::MAX as f32) * std::f32::consts::PI;
                let lean_angle = (h1 as f32 / u32::MAX as f32) * std::f32::consts::TAU;
                let lean_len = (prop.height_m * s) * prop.tilt_deg.to_radians().tan();
                let lean = Vec2::new(lean_angle.cos(), lean_angle.sin()) * lean_len;

                for i in 0..plane_count {
                    let angle =
                        base_angle + (i as f32) * std::f32::consts::PI / (plane_count as f32);
                    let dir = Vec2::new(angle.cos(), angle.sin());

                    let p0 = [cx - dir.x * half_w, cy0, cz - dir.y * half_w];
                    let p1 = [cx + dir.x * half_w, cy0, cz + dir.y * half_w];
                    let p2 = [
                        cx + dir.x * half_w + lean.x,
                        cy1,
                        cz + dir.y * half_w + lean.y,
                    ];
                    let p3 = [
                        cx - dir.x * half_w + lean.x,
                        cy1,
                        cz - dir.y * half_w + lean.y,
                    ];

                    let p0v = Vec3::from(p0);
                    let p1v = Vec3::from(p1);
                    let p3v = Vec3::from(p3);
                    let fallback_normal = Vec3::new(dir.y, 0.0, -dir.x);
                    let mut normal = (p3v - p0v).cross(p1v - p0v);
                    if normal.length_squared() > 1e-6 {
                        normal = normal.normalize();
                    } else {
                        normal = fallback_normal;
                    }

                    b.quad([p0, p1, p2, p3], normal.to_array(), uv, tile_rect);
                    b.quad([p1, p0, p3, p2], (-normal).to_array(), uv, tile_rect);
                }
            }
        }
    }

    by_block.into_iter().map(|(k, b)| (k, b)).collect()
}

/// Saves chunk sync for the `generator::chunk::chunk_utils` module.
#[allow(dead_code)]
pub fn save_chunk_sync(
    ws: &WorldSave,
    cache: &mut RegionCache,
    coord: IVec2,
    ch: &ChunkData,
) -> std::io::Result<()> {
    let _guard = world_save_io_guard();
    let blocks = encode_chunk(ch);

    let old = cache.read_chunk(ws, coord).ok().flatten();
    let merged = container_upsert(old.as_deref(), TAG_BLK1, &blocks);

    cache.write_chunk_replace(ws, coord, &merged)
}

/// Saves chunk at root sync for the `generator::chunk::chunk_utils` module.
pub fn save_chunk_at_root_sync(
    ws_root: PathBuf,
    coord: IVec2,
    ch: &ChunkData,
) -> std::io::Result<()> {
    let _guard = world_save_io_guard();
    let blocks = encode_chunk(ch);
    let rc = chunk_to_region(coord);
    let path = ws_root
        .join("region")
        .join(format!("r.{}.{}.region", rc.x, rc.y));
    let mut rf = RegionFile::open(&path)?;
    let old = rf.read_chunk(coord).ok().flatten();
    let merged = container_upsert(old.as_deref(), TAG_BLK1, &blocks);
    let idx = region_slot_index(coord);
    rf.write_slot_replace(idx, &merged)
}

/// Loads or gen chunk async for the `generator::chunk::chunk_utils` module.
pub async fn load_or_gen_chunk_async(
    ws_root: PathBuf,
    coord: IVec2,
    reg: &BlockRegistry,    // ⟵ NEW: pass the registry
    biomes: &BiomeRegistry, // ⟵ NEW: pass the biome registry
    trees: &TreeRegistry,
    cfg: WorldGenConfig, // we only need cfg.seed right now
) -> ChunkData {
    load_or_gen_chunk_async_with_origin(ws_root, coord, reg, biomes, trees, cfg)
        .await
        .0
}

/// Loads chunk from region if present and decodable.
pub fn load_chunk_at_root_sync(ws_root: &Path, coord: IVec2) -> Option<ChunkData> {
    let (r_coord, _) = chunk_to_region_slot(coord);
    let path = ws_root
        .join("region")
        .join(format!("r.{}.{}.region", r_coord.x, r_coord.y));
    let _guard = world_save_io_guard();
    let Ok(mut rf) = RegionFile::open(&path) else {
        return None;
    };
    let Ok(Some(buf)) = rf.read_chunk(coord) else {
        return None;
    };

    let data = if slot_is_container(&buf) {
        container_find(&buf, TAG_BLK1).map(|b| b.to_vec())
    } else {
        Some(buf)
    }?;

    decode_chunk(&data).ok()
}

/// Loads chunk from region or generates it, returning whether generation was needed.
pub async fn load_or_gen_chunk_async_with_origin(
    ws_root: PathBuf,
    coord: IVec2,
    reg: &BlockRegistry,
    biomes: &BiomeRegistry,
    trees: &TreeRegistry,
    cfg: WorldGenConfig,
) -> (ChunkData, bool) {
    if let Some(chunk) = load_chunk_at_root_sync(ws_root.as_path(), coord) {
        return (chunk, false);
    }

    // Fallback: generate fresh chunk via biome-based generator
    // Note: new generator expects (coord, &BlockRegistry, seed, &BiomeRegistry, &TreeRegistry)
    (
        generate_chunk_async_biome(coord, reg, cfg.seed, biomes, trees).await,
        true,
    )
}

/// Runs the `snapshot_borders` routine for snapshot borders in the `generator::chunk::chunk_utils` module.
pub fn snapshot_borders(
    chunk_map: &ChunkMap,
    coord: IVec2,
    y0: usize,
    y1: usize,
) -> BorderSnapshot {
    let mut snap = BorderSnapshot {
        y0,
        y1,
        east: None,
        west: None,
        south: None,
        north: None,
        east_stacked: None,
        west_stacked: None,
        south_stacked: None,
        north_stacked: None,
    };

    let take_xz = |c: &ChunkData, x: usize, z: usize, y: usize| -> BlockId { c.get(x, y, z) };
    let take_xz_stacked =
        |c: &ChunkData, x: usize, z: usize, y: usize| -> BlockId { c.get_stacked(x, y, z) };

    if let Some(n) = chunk_map.chunks.get(&IVec2::new(coord.x + 1, coord.y)) {
        let mut v = Vec::with_capacity((y1 - y0) * CZ);
        let mut vs = Vec::with_capacity((y1 - y0) * CZ);
        for y in y0..y1 {
            for z in 0..CZ {
                v.push(take_xz(n, 0, z, y));
                vs.push(take_xz_stacked(n, 0, z, y));
            }
        }
        snap.east = Some(v);
        snap.east_stacked = Some(vs);
    }
    if let Some(n) = chunk_map.chunks.get(&IVec2::new(coord.x - 1, coord.y)) {
        let mut v = Vec::with_capacity((y1 - y0) * CZ);
        let mut vs = Vec::with_capacity((y1 - y0) * CZ);
        for y in y0..y1 {
            for z in 0..CZ {
                v.push(take_xz(n, CX - 1, z, y));
                vs.push(take_xz_stacked(n, CX - 1, z, y));
            }
        }
        snap.west = Some(v);
        snap.west_stacked = Some(vs);
    }
    if let Some(n) = chunk_map.chunks.get(&IVec2::new(coord.x, coord.y + 1)) {
        let mut v = Vec::with_capacity((y1 - y0) * CX);
        let mut vs = Vec::with_capacity((y1 - y0) * CX);
        for y in y0..y1 {
            for x in 0..CX {
                v.push(take_xz(n, x, 0, y));
                vs.push(take_xz_stacked(n, x, 0, y));
            }
        }
        snap.south = Some(v);
        snap.south_stacked = Some(vs);
    }
    if let Some(n) = chunk_map.chunks.get(&IVec2::new(coord.x, coord.y - 1)) {
        let mut v = Vec::with_capacity((y1 - y0) * CX);
        let mut vs = Vec::with_capacity((y1 - y0) * CX);
        for y in y0..y1 {
            for x in 0..CX {
                v.push(take_xz(n, x, CZ - 1, y));
                vs.push(take_xz_stacked(n, x, CZ - 1, y));
            }
        }
        snap.north = Some(v);
        snap.north_stacked = Some(vs);
    }
    snap
}

/// Runs the `area_ready` routine for area ready in the `generator::chunk::chunk_utils` module.
pub fn area_ready(
    center: IVec2,
    radius: i32,
    chunk_map: &ChunkMap,
    pending_gen: &PendingGen,
    pending_mesh: &PendingMesh,
    backlog: &MeshBacklog,
) -> bool {
    let pending_mesh_chunks: HashSet<IVec2> =
        pending_mesh.0.keys().map(|(coord, _)| *coord).collect();
    let backlog_chunks: HashSet<IVec2> = backlog.0.iter().map(|(coord, _)| *coord).collect();

    for dz in -radius..=radius {
        for dx in -radius..=radius {
            let c = IVec2::new(center.x + dx, center.y + dz);
            if !chunk_map.chunks.contains_key(&c) {
                return false;
            }
            if pending_gen.0.contains_key(&c) {
                return false;
            }
            if pending_mesh_chunks.contains(&c) {
                return false;
            }
            if backlog_chunks.contains(&c) {
                return false;
            }
        }
    }
    true
}

/// Lightweight check for multiplayer: only requires chunks to be present in the map.
/// Meshing continues asynchronously in-game.
pub fn area_chunks_in_map(center: IVec2, radius: i32, chunk_map: &ChunkMap) -> bool {
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            let c = IVec2::new(center.x + dx, center.y + dz);
            if !chunk_map.chunks.contains_key(&c) {
                return false;
            }
        }
    }
    true
}

/// Runs the `despawn_mesh_set` routine for despawn mesh set in the `generator::chunk::chunk_utils` module.
pub fn despawn_mesh_set(
    keys: impl IntoIterator<Item = (IVec2, u8, BlockId)>,
    mesh_index: &mut ChunkMeshIndex,
    commands: &mut Commands,
    q_mesh: &Query<&Mesh3d>,
    meshes: &mut Assets<Mesh>,
) {
    for key in keys {
        if let Some(ent) = mesh_index.map.remove(&key) {
            if let Ok(Mesh3d(handle)) = q_mesh.get(ent) {
                meshes.remove(handle.id());
            }
            safe_despawn_entity(commands, ent);
        }
    }
}

/// Runs the `safe_despawn_entity` routine for safe despawn entity in the `generator::chunk::chunk_utils` module.
pub fn safe_despawn_entity(commands: &mut Commands, ent: Entity) {
    commands.queue(move |world: &mut World| {
        if world.get_entity(ent).is_ok() {
            let _ = world.despawn(ent);
        }
    });
}

/// Checks whether spawn mesh in the `generator::chunk::chunk_utils` module.
pub fn can_spawn_mesh(pending_mesh: &PendingMesh) -> bool {
    pending_mesh.0.len() < MAX_INFLIGHT_MESH
}
/// Checks whether spawn gen in the `generator::chunk::chunk_utils` module.
pub fn can_spawn_gen(pending_gen: &PendingGen) -> bool {
    pending_gen.0.len() < MAX_INFLIGHT_GEN
}

/// Runs the `backlog_contains` routine for backlog contains in the `generator::chunk::chunk_utils` module.
pub fn backlog_contains(backlog: &MeshBacklog, key: (IVec2, usize)) -> bool {
    backlog.0.iter().any(|&k| k == key)
}

/// Runs the `enqueue_mesh` routine for enqueue mesh in the `generator::chunk::chunk_utils` module.
pub fn enqueue_mesh(backlog: &mut MeshBacklog, pending: &PendingMesh, key: (IVec2, usize)) {
    if pending.0.contains_key(&key) {
        return;
    }
    if backlog_contains(backlog, key) {
        return;
    }
    backlog.0.push_back(key);
}

/// Encodes chunk for the `generator::chunk::chunk_utils` module.
pub fn encode_chunk(ch: &ChunkData) -> Vec<u8> {
    let ser =
        wincode::serialize(&(ch.blocks.clone(), ch.stacked_blocks.clone())).expect("encode blocks");
    compress_prepend_size(&ser)
}

/// Decodes chunk for the `generator::chunk::chunk_utils` module.
pub fn decode_chunk(buf: &[u8]) -> std::io::Result<ChunkData> {
    let de = decompress_size_prepended(buf)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let (blocks, stacked_blocks): (Vec<BlockId>, Vec<BlockId>) =
        match wincode::deserialize::<(Vec<BlockId>, Vec<BlockId>)>(&de) {
            Ok(v2) => v2,
            Err(_) => {
                let blocks: Vec<BlockId> = wincode::deserialize(&de).map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
                })?;
                (blocks, vec![0; CX * CY * CZ])
            }
        };

    if blocks.len() != CX * CY * CZ {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "block array size mismatch",
        ));
    }
    if stacked_blocks.len() != CX * CY * CZ {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "stacked block array size mismatch",
        ));
    }

    let mut c = ChunkData::new();
    c.blocks.copy_from_slice(&blocks);
    c.stacked_blocks.copy_from_slice(&stacked_blocks);
    Ok(c)
}

/// Runs the `leap` routine for leap in the `generator::chunk::chunk_utils` module.
#[inline]
pub fn leap(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Runs the `map01` routine for map01 in the `generator::chunk::chunk_utils` module.
#[inline]
pub fn map01(x: f32) -> f32 {
    x * 0.5 + 0.5
}

/// Runs the `chunk_to_region_slot` routine for chunk to region slot in the `generator::chunk::chunk_utils` module.
#[inline]
pub fn chunk_to_region_slot(c: IVec2) -> (IVec2, usize) {
    let rx = div_floor(c.x, REGION_SIZE);
    let rz = div_floor(c.y, REGION_SIZE);
    let lx = mod_floor(c.x, REGION_SIZE) as usize;
    let lz = mod_floor(c.y, REGION_SIZE) as usize;
    let idx = lz * (REGION_SIZE as usize) + lx;
    (IVec2::new(rx, rz), idx)
}

/// Runs the `col_rand_u32` routine for col rand u32 in the `generator::chunk::chunk_utils` module.
#[inline]
pub(crate) fn col_rand_u32(x: i32, z: i32, seed: u32) -> u32 {
    let mut n = (x as u32).wrapping_mul(374761393) ^ (z as u32).wrapping_mul(668265263) ^ seed;
    n ^= n >> 13;
    n = n.wrapping_mul(1274126177);
    n ^ (n >> 16)
}

/// Runs the `div_floor` routine for div floor in the `generator::chunk::chunk_utils` module.
#[inline]
fn div_floor(a: i32, b: i32) -> i32 {
    (a as f32 / b as f32).floor() as i32
}
/// Runs the `mod_floor` routine for mod floor in the `generator::chunk::chunk_utils` module.
#[inline]
fn mod_floor(a: i32, b: i32) -> i32 {
    a - div_floor(a, b) * b
}

/// Checks whether waiting in the `generator::chunk::chunk_utils` module.
#[inline]
pub fn is_waiting(state: &State<AppState>) -> bool {
    matches!(state.get(), AppState::Loading(LoadingStates::BaseGen))
}

/// Runs the `neighbors_ready` routine for neighbors ready in the `generator::chunk::chunk_utils` module.
#[inline]
pub(crate) fn neighbors_ready(chunk_map: &ChunkMap, c: IVec2) -> bool {
    neighbors4_iter(c).all(|nc| chunk_map.chunks.contains_key(&nc))
}

/// Runs the `neighbors4_iter` routine for neighbors4 iter in the `generator::chunk::chunk_utils` module.
#[inline]
pub(crate) fn neighbors4_iter(c: IVec2) -> impl Iterator<Item = IVec2> {
    DIR4.into_iter().map(move |d| c + d)
}
