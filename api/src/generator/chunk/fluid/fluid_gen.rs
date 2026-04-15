use crate::core::world::block::{BlockId, BlockRegistry};
use crate::core::world::chunk::{ChunkData, SEA_LEVEL};
use crate::core::world::chunk_dimension::{CX, CY, CZ, Y_MIN};
use std::collections::VecDeque;

const BASIN_MAX_NARROW_RUN: u16 = 14;
const BASIN_NARROW_HORIZONTAL_OPEN_MAX: u8 = 2;

#[inline]
fn sea_local_y() -> usize {
    (SEA_LEVEL - Y_MIN).clamp(0, CY as i32 - 1) as usize
}

#[inline]
fn water_worldgen_id(reg: &BlockRegistry) -> BlockId {
    // Prefer inert full-height variant so worldgen water does not become a runtime flow source.
    reg.id_opt("water_block_flow_10")
        .or_else(|| reg.id_opt("water_flow_10"))
        .or_else(|| reg.id_opt("water_block_flow_9"))
        .or_else(|| reg.id_opt("water_flow_9"))
        .unwrap_or_else(|| reg.id_or_air("water_block"))
}

#[inline]
fn is_fillable_for_worldgen(id: BlockId, reg: &BlockRegistry) -> bool {
    id == 0 || reg.is_overridable(id)
}

#[inline]
fn is_open_for_basin_path(
    chunk: &ChunkData,
    x: usize,
    y: usize,
    z: usize,
    reg: &BlockRegistry,
    water_id: BlockId,
) -> bool {
    let id = chunk.get(x, y, z);
    id == water_id || is_fillable_for_worldgen(id, reg)
}

#[inline]
fn basin_open_counts(
    chunk: &ChunkData,
    x: usize,
    y: usize,
    z: usize,
    reg: &BlockRegistry,
    water_id: BlockId,
) -> (u8, u8) {
    let mut horizontal = 0u8;
    let mut vertical = 0u8;
    if x + 1 < CX && is_open_for_basin_path(chunk, x + 1, y, z, reg, water_id) {
        horizontal += 1;
    }
    if x > 0 && is_open_for_basin_path(chunk, x - 1, y, z, reg, water_id) {
        horizontal += 1;
    }
    if z + 1 < CZ && is_open_for_basin_path(chunk, x, y, z + 1, reg, water_id) {
        horizontal += 1;
    }
    if z > 0 && is_open_for_basin_path(chunk, x, y, z - 1, reg, water_id) {
        horizontal += 1;
    }
    if y + 1 < CY && is_open_for_basin_path(chunk, x, y + 1, z, reg, water_id) {
        vertical += 1;
    }
    if y > 1 && is_open_for_basin_path(chunk, x, y - 1, z, reg, water_id) {
        vertical += 1;
    }
    (horizontal, vertical)
}

#[inline]
fn basin_cap_jitter(x: usize, y: usize, z: usize) -> u16 {
    // Small deterministic jitter to avoid perfectly flat stop-fronts in caves.
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1)
        ^ (y as u32).wrapping_mul(0x85EB_CA77)
        ^ (z as u32).wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    (h % 5) as u16
}

/// Applies settled water worldgen to one chunk.
///
/// This is intentionally independent from runtime fluid simulation:
/// it seeds static water bodies during chunk generation and avoids
/// tick-based propagation for worldgen water.
pub fn apply_settled_water_worldgen(chunk: &mut ChunkData, reg: &BlockRegistry) {
    let water_id = water_worldgen_id(reg);
    if water_id == 0 {
        return;
    }
    let source_water_id = reg.id_or_air("water_block");

    // Normalize any generator-written source water (e.g. cave tunnel helper)
    // to the settled worldgen water id.
    if source_water_id != 0 && source_water_id != water_id {
        for y in 0..CY {
            for z in 0..CZ {
                for x in 0..CX {
                    if chunk.get(x, y, z) == source_water_id {
                        chunk.set(x, y, z, water_id);
                    }
                }
            }
        }
    }

    let sea_ly = sea_local_y();

    // Pass 1: fill vertical water columns up to sea level.
    for x in 0..CX {
        for z in 0..CZ {
            let Some(top) = chunk.column_top_local_y(x, z) else {
                for y in 1..=sea_ly {
                    if is_fillable_for_worldgen(chunk.get(x, y, z), reg) {
                        chunk.set(x, y, z, water_id);
                    }
                }
                continue;
            };

            if top >= sea_ly {
                continue;
            }

            let start = (top + 1).max(1);
            for y in start..=sea_ly {
                let id = chunk.get(x, y, z);
                if is_fillable_for_worldgen(id, reg) {
                    chunk.set(x, y, z, water_id);
                } else if !reg.is_fluid(id) {
                    break;
                }
            }
        }
    }

    // Pass 2: flood-fill only caves that are connected to already-watered cells.
    // This keeps enclosed caves dry and avoids runtime flow spikes.
    let mut visited = vec![false; CX * CY * CZ];
    let mut queue: VecDeque<(usize, usize, usize, u16)> = VecDeque::new();
    let idx = |x: usize, y: usize, z: usize| (y * CZ + z) * CX + x;

    let mut try_seed =
        |x: usize, y: usize, z: usize, queue: &mut VecDeque<(usize, usize, usize, u16)>| {
            if y > sea_ly {
                return;
            }
            if chunk.get(x, y, z) != water_id {
                return;
            }
            let i = idx(x, y, z);
            if visited[i] {
                return;
            }
            visited[i] = true;
            queue.push_back((x, y, z, 0));
        };

    // Water surface and chunk edges are valid ingress points for cave flooding.
    for x in 0..CX {
        for z in 0..CZ {
            try_seed(x, sea_ly, z, &mut queue);
        }
    }
    for y in 0..=sea_ly {
        for z in 0..CZ {
            try_seed(0, y, z, &mut queue);
            try_seed(CX - 1, y, z, &mut queue);
        }
        for x in 0..CX {
            try_seed(x, y, 0, &mut queue);
            try_seed(x, y, CZ - 1, &mut queue);
        }
    }

    while let Some((x, y, z, narrow_run)) = queue.pop_front() {
        let neighbors = [
            (x.wrapping_add(1), y, z, x + 1 < CX),
            (x.wrapping_sub(1), y, z, x > 0),
            (x, y, z.wrapping_add(1), z + 1 < CZ),
            (x, y, z.wrapping_sub(1), z > 0),
            (x, y.wrapping_add(1), z, y < sea_ly),
            (x, y.wrapping_sub(1), z, y > 1),
        ];

        for (nx, ny, nz, in_bounds) in neighbors {
            if !in_bounds {
                continue;
            }
            let ni = idx(nx, ny, nz);
            if visited[ni] {
                continue;
            }
            let nid = chunk.get(nx, ny, nz);
            let is_open = nid == water_id || is_fillable_for_worldgen(nid, reg);
            if !is_open {
                visited[ni] = true;
                continue;
            }

            let (horizontal_open, vertical_open) =
                basin_open_counts(chunk, nx, ny, nz, reg, water_id);
            let is_narrow_tunnel = ny < sea_ly
                && horizontal_open <= BASIN_NARROW_HORIZONTAL_OPEN_MAX
                && vertical_open == 0;
            let next_narrow_run = if is_narrow_tunnel {
                narrow_run.saturating_add(1)
            } else {
                0
            };
            let cap = BASIN_MAX_NARROW_RUN + basin_cap_jitter(nx, ny, nz);
            if next_narrow_run > cap {
                // Basin sealing: prevent very long narrow cave channels from fully flooding.
                continue;
            }

            visited[ni] = true;
            if nid == water_id {
                queue.push_back((nx, ny, nz, next_narrow_run));
                continue;
            }
            chunk.set(nx, ny, nz, water_id);
            queue.push_back((nx, ny, nz, next_narrow_run));
        }
    }
}
