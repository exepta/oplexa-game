pub mod biome;
pub mod block;
pub mod chunk;
pub mod chunk_dimension;
pub mod fluid;
pub mod prop;
pub mod save;
pub mod spawn;

use crate::core::entities::player::block_selection::BlockHit;
use crate::core::events::chunk_events::SubChunkNeedRemeshEvent;
use crate::core::world::block::*;
use crate::core::world::chunk::*;
use crate::core::world::chunk_dimension::*;
use bevy::prelude::*;

/// Represents world mut access used by the `core::world` module.
pub struct WorldMutAccess<'a> {
    pub chunk: &'a mut ChunkData,
    pub lx: usize,
    pub ly: usize,
    pub lz: usize,
    pub sub: usize,
}
impl<'a> WorldMutAccess<'a> {
    /// Returns the requested data for the `core::world` module.
    #[inline]
    pub fn get(&self) -> BlockId {
        self.chunk.get(self.lx, self.ly, self.lz)
    }
    /// Sets the requested data for the `core::world` module.
    #[inline]
    pub fn set(&mut self, id: BlockId) {
        self.chunk.set(self.lx, self.ly, self.lz, id);
        self.chunk.mark_dirty_local_y(self.sub);
    }
    /// Returns stacked block data for the `core::world` module.
    #[inline]
    pub fn get_stacked(&self) -> BlockId {
        self.chunk.get_stacked(self.lx, self.ly, self.lz)
    }
    /// Sets stacked block data for the `core::world` module.
    #[inline]
    pub fn set_stacked(&mut self, id: BlockId) {
        self.chunk.set_stacked(self.lx, self.ly, self.lz, id);
        self.chunk.mark_dirty_local_y(self.sub);
    }
}

/// Runs the `world_access_mut` routine for world access mut in the `core::world` module.
pub fn world_access_mut(chunk_map: &'_ mut ChunkMap, wp: IVec3) -> Option<WorldMutAccess<'_>> {
    if wp.y < Y_MIN || wp.y > Y_MAX {
        return None;
    }
    let (cc, local) = world_to_chunk_xz(wp.x, wp.z);
    let chunk = chunk_map.chunks.get_mut(&cc)?;
    let lx = local.x as usize;
    let lz = local.y as usize;
    let ly = world_y_to_local(wp.y);
    if lx >= CX || lz >= CZ || ly >= CY {
        return None;
    }
    let sub = ly / SEC_H;
    Some(WorldMutAccess {
        chunk,
        lx,
        ly,
        lz,
        sub,
    })
}

/// Runs the `mark_dirty_block_and_neighbors` routine for mark dirty block and neighbors in the `core::world` module.
pub fn mark_dirty_block_and_neighbors(
    chunk_map: &mut ChunkMap,
    wp: IVec3,
    ev: &mut MessageWriter<SubChunkNeedRemeshEvent>,
) {
    const OFFS: [IVec3; 7] = [
        IVec3::new(0, 0, 0),
        IVec3::new(1, 0, 0),
        IVec3::new(-1, 0, 0),
        IVec3::new(0, 1, 0),
        IVec3::new(0, -1, 0),
        IVec3::new(0, 0, 1),
        IVec3::new(0, 0, -1),
    ];

    for o in OFFS {
        let p = wp + o;
        if p.y < Y_MIN || p.y > Y_MAX {
            continue;
        }

        let (coord, local_xz) = world_to_chunk_xz(p.x, p.z);
        let ly = world_y_to_local(p.y);
        let sub = ly / SEC_H;

        let mut mark = |c: IVec2, s: usize| {
            if s < SEC_COUNT {
                if let Some(ch) = chunk_map.chunks.get_mut(&c) {
                    ch.mark_dirty_local_y(s);
                    ev.write(SubChunkNeedRemeshEvent { coord: c, sub: s });
                }
            }
        };

        mark(coord, sub);
        if ly % SEC_H == 0 && sub > 0 {
            mark(coord, sub - 1);
        }
        if ly % SEC_H == SEC_H - 1 && sub + 1 < SEC_COUNT {
            mark(coord, sub + 1);
        }

        let _ = local_xz;
    }
}

/// Runs the `ray_cast_voxels` routine for ray cast voxels in the `core::world` module.
pub fn ray_cast_voxels(
    origin: Vec3,
    dir_in: Vec3,
    max_dist: f32,
    chunk_map: &ChunkMap,
    registry: &BlockRegistry,
) -> Option<BlockHit> {
    let dir = dir_in.normalize_or_zero();
    if dir == Vec3::ZERO {
        return None;
    }

    let mut x = origin.x.floor() as i32;
    let mut y = origin.y.floor() as i32;
    let mut z = origin.z.floor() as i32;

    let step_x = if dir.x > 0.0 {
        1
    } else if dir.x < 0.0 {
        -1
    } else {
        0
    };
    let step_y = if dir.y > 0.0 {
        1
    } else if dir.y < 0.0 {
        -1
    } else {
        0
    };
    let step_z = if dir.z > 0.0 {
        1
    } else if dir.z < 0.0 {
        -1
    } else {
        0
    };

    let next_boundary = |p: f32, step: i32| -> f32 {
        if step > 0 {
            p.floor() + 1.0
        } else {
            p.ceil() - 1.0
        }
    };

    let mut t_max_x = if step_x != 0 {
        (next_boundary(origin.x, step_x) - origin.x) / dir.x
    } else {
        f32::INFINITY
    };
    let mut t_max_y = if step_y != 0 {
        (next_boundary(origin.y, step_y) - origin.y) / dir.y
    } else {
        f32::INFINITY
    };
    let mut t_max_z = if step_z != 0 {
        (next_boundary(origin.z, step_z) - origin.z) / dir.z
    } else {
        f32::INFINITY
    };

    let t_delta_x = if step_x != 0 {
        1.0 / dir.x.abs()
    } else {
        f32::INFINITY
    };
    let t_delta_y = if step_y != 0 {
        1.0 / dir.y.abs()
    } else {
        f32::INFINITY
    };
    let t_delta_z = if step_z != 0 {
        1.0 / dir.z.abs()
    } else {
        f32::INFINITY
    };

    let mut t: f32;

    for _ in 0..512 {
        let block_pos = IVec3::new(x, y, z);
        let mut best_hit: Option<(f32, Face, BlockId, bool)> = None;

        let id = get_block_world(chunk_map, block_pos);
        if id != 0
            && let Some((t_hit, face)) =
                ray_hits_block_collider_face(origin, dir, max_dist, block_pos, id, registry)
        {
            best_hit = Some((t_hit, face, id, false));
        }

        let stacked_id = get_stacked_block_world(chunk_map, block_pos);
        if stacked_id != 0
            && let Some((t_hit, face)) =
                ray_hits_block_collider_face(origin, dir, max_dist, block_pos, stacked_id, registry)
        {
            match best_hit {
                Some((best_t, _, _, _)) if best_t + 1e-5 < t_hit => {}
                // On equal-distance ties, prefer stacked occupant so slab+slab cells
                // remain targetable via their secondary collider.
                Some((best_t, _, _, _)) if (best_t - t_hit).abs() <= 1e-5 => {
                    best_hit = Some((t_hit, face, stacked_id, true));
                }
                _ => best_hit = Some((t_hit, face, stacked_id, true)),
            }
        }

        if let Some((t_hit, face, hit_id, is_stacked)) = best_hit {
            let hit_point = origin + dir * t_hit;
            let hit_local = (hit_point - block_pos.as_vec3()).clamp(Vec3::ZERO, Vec3::ONE);
            return Some(BlockHit {
                block_pos,
                block_id: hit_id,
                is_stacked,
                face,
                hit_local,
                place_pos: block_pos + face_offset(face),
            });
        }

        if t_max_x < t_max_y && t_max_x < t_max_z {
            x += step_x;
            t = t_max_x;
            t_max_x += t_delta_x;
        } else if t_max_y < t_max_z {
            y += step_y;
            t = t_max_y;
            t_max_y += t_delta_y;
        } else {
            z += step_z;
            t = t_max_z;
            t_max_z += t_delta_z;
        }

        if t > max_dist {
            break;
        }
        if y < Y_MIN || y > Y_MAX {
            break;
        }
    }

    None
}

fn ray_hits_block_collider_face(
    origin: Vec3,
    dir: Vec3,
    max_dist: f32,
    block_pos: IVec3,
    id: BlockId,
    registry: &BlockRegistry,
) -> Option<(f32, Face)> {
    let (size, offset) = registry.selection_box(id)?;

    let center = Vec3::new(
        block_pos.x as f32 + 0.5 + offset[0],
        block_pos.y as f32 + 0.5 + offset[1],
        block_pos.z as f32 + 0.5 + offset[2],
    );
    let half = Vec3::new(size[0], size[1], size[2]) * 0.5;
    ray_intersect_aabb_face(origin, dir, center - half, center + half, max_dist)
}

fn ray_intersect_aabb_face(
    origin: Vec3,
    dir: Vec3,
    min: Vec3,
    max: Vec3,
    max_dist: f32,
) -> Option<(f32, Face)> {
    const EPS: f32 = 1e-6;

    let mut t_enter = f32::NEG_INFINITY;
    let mut t_exit = f32::INFINITY;
    let mut enter_axis: Option<usize> = None;
    let mut exit_axis: Option<usize> = None;

    for axis in 0..3 {
        let (o, d, min_axis, max_axis) = match axis {
            0 => (origin.x, dir.x, min.x, max.x),
            1 => (origin.y, dir.y, min.y, max.y),
            _ => (origin.z, dir.z, min.z, max.z),
        };

        if d.abs() < EPS {
            if o < min_axis || o > max_axis {
                return None;
            }
            continue;
        }

        let t1 = (min_axis - o) / d;
        let t2 = (max_axis - o) / d;
        let near = t1.min(t2);
        let far = t1.max(t2);

        if near > t_enter {
            t_enter = near;
            enter_axis = Some(axis);
        }
        if far < t_exit {
            t_exit = far;
            exit_axis = Some(axis);
        }

        if t_exit < t_enter {
            return None;
        }
    }

    if t_exit < 0.0 {
        return None;
    }

    if t_enter >= 0.0 {
        if t_enter > max_dist {
            return None;
        }
        let axis = enter_axis?;
        Some((t_enter, entry_face_for_axis(axis, dir)))
    } else {
        let axis = exit_axis?;
        if t_exit > max_dist {
            return None;
        }
        Some((t_exit, exit_face_for_axis(axis, dir)))
    }
}

#[inline]
fn entry_face_for_axis(axis: usize, dir: Vec3) -> Face {
    match axis {
        0 => {
            if dir.x >= 0.0 {
                Face::West
            } else {
                Face::East
            }
        }
        1 => {
            if dir.y >= 0.0 {
                Face::Bottom
            } else {
                Face::Top
            }
        }
        _ => {
            if dir.z >= 0.0 {
                Face::North
            } else {
                Face::South
            }
        }
    }
}

#[inline]
fn exit_face_for_axis(axis: usize, dir: Vec3) -> Face {
    match axis {
        0 => {
            if dir.x >= 0.0 {
                Face::East
            } else {
                Face::West
            }
        }
        1 => {
            if dir.y >= 0.0 {
                Face::Top
            } else {
                Face::Bottom
            }
        }
        _ => {
            if dir.z >= 0.0 {
                Face::South
            } else {
                Face::North
            }
        }
    }
}
