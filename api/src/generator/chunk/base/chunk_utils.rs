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
use std::path::PathBuf;

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

    let (east, west, south, north, snap_y0, _snap_y1) = if let Some(b) = borders {
        debug_assert_eq!(b.y0, y0, "BorderSnapshot.y0 != sub y0");
        debug_assert_eq!(b.y1, y1, "BorderSnapshot.y1 != sub y1");
        (b.east, b.west, b.south, b.north, b.y0, b.y1)
    } else {
        (None, None, None, None, y0, y1)
    };

    let sample_opt =
        |opt: &Option<Vec<BlockId>>, y: usize, i: usize, stride: usize| -> Option<BlockId> {
            opt.as_ref().map(|v| {
                let iy = y - snap_y0;
                v[iy * stride + i]
            })
        };

    let east_at_opt = |y: usize, z: usize| sample_opt(&east, y, z, CZ);
    let west_at_opt = |y: usize, z: usize| sample_opt(&west, y, z, CZ);
    let south_at_opt = |y: usize, x: usize| sample_opt(&south, y, x, CX);
    let north_at_opt = |y: usize, x: usize| sample_opt(&north, y, x, CX);

    let get = |x: isize, y: isize, z: isize| -> BlockId {
        if x < 0 || y < 0 || z < 0 || x >= CX as isize || y >= CY as isize || z >= CZ as isize {
            0
        } else {
            chunk.blocks[((y as usize) * CZ + (z as usize)) * CX + (x as usize)]
        }
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

        // Treat neighboring foliage blocks as connected canopy: don't render inner faces.
        if reg.foliage(self_id) && reg.foliage(neigh_id) {
            return false;
        }

        let self_fluid = reg.fluid(self_id);
        let neigh_fluid = reg.fluid(neigh_id);

        if self_fluid && neigh_fluid {
            return false;
        }

        !reg.opaque(neigh_id)
    };
    let is_cube_voxel = |id: BlockId| id != 0 && !reg.is_crossed_prop(id);

    // +Y (Top): greedy in XZ plane for each Y slice.
    let mut top_mask = vec![0u16; CX * CZ];
    for y in y0..y1 {
        top_mask.fill(0);
        for z in 0..CZ {
            for x in 0..CX {
                let id = chunk.get(x, y, z);
                if !is_cube_voxel(id) {
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
                if !is_cube_voxel(id) {
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
                if !is_cube_voxel(id) {
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
                if !is_cube_voxel(id) {
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
                if !is_cube_voxel(id) {
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
                if !is_cube_voxel(id) {
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

    // Prop pass: crossed planes (Minecraft/Hytale style plants).
    for y in y0..y1 {
        for z in 0..CZ {
            for x in 0..CX {
                let id = chunk.get(x, y, z);
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

                for i in 0..plane_count {
                    let angle = (i as f32) * std::f32::consts::PI / (plane_count as f32);
                    let dir = Vec2::new(angle.cos(), angle.sin());
                    let nx = dir.y;
                    let nz = -dir.x;

                    let p0 = [cx - dir.x * half_w, cy0, cz - dir.y * half_w];
                    let p1 = [cx + dir.x * half_w, cy0, cz + dir.y * half_w];
                    let p2 = [cx + dir.x * half_w, cy1, cz + dir.y * half_w];
                    let p3 = [cx - dir.x * half_w, cy1, cz - dir.y * half_w];

                    b.quad([p0, p1, p2, p3], [nx, 0.0, nz], uv, tile_rect);
                    b.quad([p1, p0, p3, p2], [-nx, 0.0, -nz], uv, tile_rect);
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
    // Try to load from a region file first
    let (r_coord, _) = chunk_to_region_slot(coord);
    let path = ws_root
        .join("region")
        .join(format!("r.{}.{}.region", r_coord.x, r_coord.y));
    {
        let _guard = world_save_io_guard();
        if let Ok(mut rf) = RegionFile::open(&path) {
            if let Ok(Some(buf)) = rf.read_chunk(coord) {
                // Detect legacy container-wrapped blobs
                let data = if slot_is_container(&buf) {
                    container_find(&buf, TAG_BLK1).map(|b| b.to_vec())
                } else {
                    Some(buf)
                };
                if let Some(b) = data {
                    if let Ok(c) = decode_chunk(&b) {
                        return c;
                    }
                }
            }
        }
    }

    // Fallback: generate fresh chunk via biome-based generator
    // Note: new generator expects (coord, &BlockRegistry, seed, &BiomeRegistry, &TreeRegistry)
    generate_chunk_async_biome(coord, reg, cfg.seed, biomes, trees).await
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
    };

    let take_xz = |c: &ChunkData, x: usize, z: usize, y: usize| -> BlockId { c.get(x, y, z) };

    if let Some(n) = chunk_map.chunks.get(&IVec2::new(coord.x + 1, coord.y)) {
        let mut v = Vec::with_capacity((y1 - y0) * CZ);
        for y in y0..y1 {
            for z in 0..CZ {
                v.push(take_xz(n, 0, z, y));
            }
        }
        snap.east = Some(v);
    }
    if let Some(n) = chunk_map.chunks.get(&IVec2::new(coord.x - 1, coord.y)) {
        let mut v = Vec::with_capacity((y1 - y0) * CZ);
        for y in y0..y1 {
            for z in 0..CZ {
                v.push(take_xz(n, CX - 1, z, y));
            }
        }
        snap.west = Some(v);
    }
    if let Some(n) = chunk_map.chunks.get(&IVec2::new(coord.x, coord.y + 1)) {
        let mut v = Vec::with_capacity((y1 - y0) * CX);
        for y in y0..y1 {
            for x in 0..CX {
                v.push(take_xz(n, x, 0, y));
            }
        }
        snap.south = Some(v);
    }
    if let Some(n) = chunk_map.chunks.get(&IVec2::new(coord.x, coord.y - 1)) {
        let mut v = Vec::with_capacity((y1 - y0) * CX);
        for y in y0..y1 {
            for x in 0..CX {
                v.push(take_xz(n, x, CZ - 1, y));
            }
        }
        snap.north = Some(v);
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
    let ser = wincode::serialize(&ch.blocks).expect("encode blocks");
    compress_prepend_size(&ser)
}

/// Decodes chunk for the `generator::chunk::chunk_utils` module.
pub fn decode_chunk(buf: &[u8]) -> std::io::Result<ChunkData> {
    let de = decompress_size_prepended(buf)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let blocks: Vec<BlockId> = wincode::deserialize(&de)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

    if blocks.len() != CX * CY * CZ {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "block array size mismatch",
        ));
    }

    let mut c = ChunkData::new();
    c.blocks.copy_from_slice(&blocks);
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
