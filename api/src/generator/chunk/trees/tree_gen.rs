use crate::core::world::biome::func::{col_rand_f32, col_rand_u32, dominant_biome_at_p_chunks};
use crate::core::world::biome::registry::BiomeRegistry;
use crate::core::world::block::BlockRegistry;
use crate::core::world::chunk::{ChunkData, SEA_LEVEL};
use crate::core::world::chunk_dimension::{CX, CY, CZ, Y_MIN};
use crate::generator::chunk::trees::registry::{TreeFamily, TreeRegistry, TreeVariant};
use bevy::prelude::*;

const SALT_TREE_SPAWN: u32 = 0xA9D1_0111;
const SALT_TREE_PICK_FAMILY: u32 = 0xA9D1_0222;
const SALT_TREE_PICK_VARIANT: u32 = 0xA9D1_0333;
const SALT_TREE_TRUNK_HEIGHT: u32 = 0xA9D1_0444;
const SALT_TREE_CANOPY_RADIUS: u32 = 0xA9D1_0555;
const SALT_TREE_CANOPY_HEIGHT: u32 = 0xA9D1_0666;
const SALT_TREE_LEAF_SHAPE: u32 = 0xA9D1_0777;
const SALT_TREE_OAK_BRANCH: u32 = 0xA9D1_0888;
const TREE_LEAF_DENSITY_SCALE: f32 = 0.82;

#[derive(Clone, Copy)]
enum TreeStyle {
    Oak,
    Spruce,
    Generic,
}

/// Populates a generated chunk with trees according to biome generation settings.
pub fn populate_trees_in_chunk(
    chunk: &mut ChunkData,
    coord: IVec2,
    reg: &BlockRegistry,
    biomes: &BiomeRegistry,
    trees: &TreeRegistry,
    world_seed: i32,
) {
    if trees.by_name.is_empty() {
        return;
    }

    let seed = world_seed as u32;

    for lx in 0..CX {
        for lz in 0..CZ {
            let wx = coord.x * CX as i32 + lx as i32;
            let wz = coord.y * CZ as i32 + lz as i32;

            let px = coord.x as f32 + (lx as f32 + 0.5) / CX as f32;
            let pz = coord.y as f32 + (lz as f32 + 0.5) / CZ as f32;
            let biome = dominant_biome_at_p_chunks(biomes, world_seed, Vec2::new(px, pz));
            let rules = &biome.generation.trees;
            if rules.is_empty() {
                continue;
            }

            let mut total_density = 0.0f32;
            for rule in rules {
                if rule.density <= 0.0 {
                    continue;
                }
                if trees.get(&rule.tree_type).is_some() {
                    total_density += rule.density;
                }
            }
            if total_density <= 0.0 {
                continue;
            }

            let spawn_chance = total_density.clamp(0.0, 0.5);
            if col_rand_f32(wx, wz, seed ^ SALT_TREE_SPAWN) >= spawn_chance {
                continue;
            }

            let Some(family) = pick_family(wx, wz, seed, rules, trees, total_density) else {
                continue;
            };
            let Some(variant) = pick_variant(wx, wz, seed, family) else {
                continue;
            };

            let style = style_for_family(family);
            let _ = try_place_tree(chunk, reg, style, lx, lz, wx, wz, seed, variant);
        }
    }
}

fn pick_family<'a>(
    wx: i32,
    wz: i32,
    seed: u32,
    rules: &'a [crate::core::world::biome::BiomeTreeSpawn],
    trees: &'a TreeRegistry,
    total_density: f32,
) -> Option<&'a TreeFamily> {
    let mut pick = col_rand_f32(wx, wz, seed ^ SALT_TREE_PICK_FAMILY) * total_density.max(1e-6);
    for rule in rules {
        if rule.density <= 0.0 {
            continue;
        }
        let Some(family) = trees.get(&rule.tree_type) else {
            continue;
        };
        pick -= rule.density;
        if pick <= 0.0 {
            return Some(family);
        }
    }

    for rule in rules {
        if rule.density <= 0.0 {
            continue;
        }
        if let Some(f) = trees.get(&rule.tree_type) {
            return Some(f);
        }
    }
    None
}

fn pick_variant<'a>(
    wx: i32,
    wz: i32,
    seed: u32,
    family: &'a TreeFamily,
) -> Option<&'a TreeVariant> {
    if family.variants.is_empty() {
        return None;
    }

    let total_weight: f32 = family.variants.iter().map(|v| v.weight.max(0.0)).sum();
    if total_weight <= 0.0 {
        return family.variants.first();
    }

    let mut pick = col_rand_f32(wx, wz, seed ^ SALT_TREE_PICK_VARIANT) * total_weight;
    for variant in &family.variants {
        pick -= variant.weight.max(0.0);
        if pick <= 0.0 {
            return Some(variant);
        }
    }
    family.variants.last()
}

#[inline]
fn style_for_family(family: &TreeFamily) -> TreeStyle {
    let key = family.key.as_str();
    if key.contains("spruce") || key.contains("tanne") || key.contains("fichte") {
        TreeStyle::Spruce
    } else if key.contains("oak") || key.contains("eiche") {
        TreeStyle::Oak
    } else {
        TreeStyle::Generic
    }
}

fn try_place_tree(
    chunk: &mut ChunkData,
    reg: &BlockRegistry,
    style: TreeStyle,
    lx: usize,
    lz: usize,
    wx: i32,
    wz: i32,
    seed: u32,
    variant: &TreeVariant,
) -> bool {
    let Some(ground_ly) = chunk.column_top_local_y(lx, lz) else {
        return false;
    };
    let ground_world_y = Y_MIN + ground_ly as i32;
    if ground_world_y < SEA_LEVEL - 1 {
        return false;
    }

    let ground_id = chunk.get(lx, ground_ly, lz);
    if ground_id == 0 || reg.is_fluid(ground_id) || reg.stats(ground_id).foliage {
        return false;
    }

    let trunk_h = pick_i32_inclusive(wx, wz, seed ^ SALT_TREE_TRUNK_HEIGHT, variant.trunk_height);
    let canopy_r = pick_i32_inclusive(
        wx,
        wz,
        seed ^ SALT_TREE_CANOPY_RADIUS,
        variant.canopy_radius,
    );
    let canopy_h = pick_i32_inclusive(
        wx,
        wz,
        seed ^ SALT_TREE_CANOPY_HEIGHT,
        variant.canopy_height,
    );

    let edge_margin = match style {
        TreeStyle::Oak => canopy_r + 3,
        TreeStyle::Spruce => canopy_r + 2,
        TreeStyle::Generic => canopy_r + 1,
    }
    .max(1) as usize;
    if lx < edge_margin || lz < edge_margin || lx + edge_margin >= CX || lz + edge_margin >= CZ {
        return false;
    }

    let base_ly = ground_ly + 1;
    let trunk_top = base_ly as i32 + trunk_h - 1;
    if trunk_top < 0 || trunk_top >= CY as i32 {
        return false;
    }

    let canopy_extra_top = match style {
        TreeStyle::Oak => 3,
        TreeStyle::Spruce => 2,
        TreeStyle::Generic => 1,
    };
    let canopy_floor_pad = match style {
        TreeStyle::Oak => 2,
        TreeStyle::Spruce => 1,
        TreeStyle::Generic => 0,
    };
    let canopy_top = trunk_top + canopy_extra_top;
    let canopy_bottom = trunk_top - canopy_h - canopy_floor_pad;
    if canopy_bottom < 0 || canopy_top >= CY as i32 {
        return false;
    }

    for y in base_ly as i32..=trunk_top {
        if chunk.get(lx, y as usize, lz) != 0 {
            return false;
        }
    }

    let trunk_id = reg.id_or_air(&variant.trunk_block);
    let leaves_id = reg.id_or_air(&variant.leaves_block);
    if trunk_id == 0 || leaves_id == 0 {
        return false;
    }

    for y in base_ly as i32..=trunk_top {
        chunk.set(lx, y as usize, lz, trunk_id);
    }

    match style {
        TreeStyle::Oak => place_oak_canopy(
            chunk,
            reg,
            trunk_id,
            leaves_id,
            lx,
            lz,
            wx,
            wz,
            seed,
            trunk_top,
            canopy_r,
            canopy_h,
            variant.canopy_density,
        ),
        TreeStyle::Spruce => place_spruce_canopy(
            chunk,
            reg,
            leaves_id,
            lx,
            lz,
            wx,
            wz,
            seed,
            trunk_top,
            canopy_r,
            canopy_h,
            variant.canopy_density,
        ),
        TreeStyle::Generic => place_generic_canopy(
            chunk,
            reg,
            leaves_id,
            lx,
            lz,
            wx,
            wz,
            seed,
            trunk_top,
            canopy_r,
            canopy_h,
            variant.canopy_density,
        ),
    }

    true
}

#[allow(clippy::too_many_arguments)]
fn place_generic_canopy(
    chunk: &mut ChunkData,
    reg: &BlockRegistry,
    leaves_id: u16,
    lx: usize,
    lz: usize,
    wx: i32,
    wz: i32,
    seed: u32,
    trunk_top: i32,
    canopy_r: i32,
    canopy_h: i32,
    base_density: f32,
) {
    let x = lx as i32;
    let z = lz as i32;
    let y0 = trunk_top - canopy_h + 1;
    let y1 = trunk_top + 1;
    for y in y0..=y1 {
        let layer_from_top = (y1 - y).max(0);
        let layer_norm = if canopy_h <= 1 {
            0.0
        } else {
            layer_from_top as f32 / canopy_h as f32
        };
        let mut layer_r = canopy_r as f32 * (1.0 - layer_norm * 0.70);
        if y == y1 {
            layer_r = 1.0;
        }
        layer_r = layer_r.clamp(1.0, canopy_r.max(1) as f32);
        fill_leaf_disk(
            chunk,
            reg,
            leaves_id,
            x,
            y,
            z,
            layer_r,
            (base_density + 0.05).clamp(0.55, 0.97),
            wx,
            wz,
            seed ^ SALT_TREE_LEAF_SHAPE,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn place_oak_canopy(
    chunk: &mut ChunkData,
    reg: &BlockRegistry,
    trunk_id: u16,
    leaves_id: u16,
    lx: usize,
    lz: usize,
    wx: i32,
    wz: i32,
    seed: u32,
    trunk_top: i32,
    canopy_r: i32,
    canopy_h: i32,
    base_density: f32,
) {
    let x = lx as i32;
    let z = lz as i32;
    let crown_top = trunk_top + 1;
    let crown_mid = trunk_top - (canopy_h / 3).max(1);
    let crown_low = trunk_top - ((canopy_h * 2) / 3).max(2);

    const DIRS: [(i32, i32); 8] = [
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
        (0, -1),
        (1, -1),
    ];

    fill_leaf_blob(
        chunk,
        reg,
        leaves_id,
        x,
        crown_mid,
        z,
        canopy_r + 1,
        (canopy_h / 2).max(3),
        canopy_r + 1,
        (base_density + 0.09).clamp(0.66, 0.99),
        wx,
        wz,
        seed ^ SALT_TREE_LEAF_SHAPE,
    );

    fill_leaf_blob(
        chunk,
        reg,
        leaves_id,
        x,
        crown_low,
        z,
        canopy_r + 1,
        (canopy_h / 3).max(2),
        canopy_r + 1,
        (base_density + 0.04).clamp(0.60, 0.97),
        wx,
        wz,
        seed ^ 0x51A7_0888,
    );

    fill_leaf_blob(
        chunk,
        reg,
        leaves_id,
        x,
        crown_top + 1,
        z,
        (canopy_r - 1).max(2),
        2,
        (canopy_r - 1).max(2),
        (base_density + 0.14).clamp(0.72, 1.0),
        wx,
        wz,
        seed ^ 0x51A7_0999,
    );

    let side_blob_count = 3 + (col_rand_u32(wx, wz, seed ^ 0x51A7_1001) % 2) as i32;
    let blob_spread = (canopy_r - 1).max(2);
    let side_start = (col_rand_u32(wx, wz, seed ^ 0x51A7_1111) % DIRS.len() as u32) as usize;
    for i in 0..side_blob_count {
        let (dx, dz) = DIRS[(side_start + i as usize) % DIRS.len()];
        let dist = 1
            + (blob_spread / 3)
            + (col_rand_u32(wx + i * 7, wz - i * 11, seed ^ 0x51A7_2002) % 2) as i32;
        let off_x = dx * dist + rand_signed_offset(wx + i * 17, wz - i * 13, seed ^ 0x51A7_3003, 1);
        let off_z = dz * dist + rand_signed_offset(wx - i * 19, wz + i * 5, seed ^ 0x51A7_4004, 1);
        let off_y = rand_signed_offset(wx + i * 3, wz - i * 9, seed ^ 0x51A7_5005, 2) - 1;

        fill_leaf_blob(
            chunk,
            reg,
            leaves_id,
            x + off_x,
            crown_mid + off_y,
            z + off_z,
            canopy_r.max(2),
            (canopy_h / 3).max(2),
            canopy_r.max(2),
            (base_density + 0.05).clamp(0.62, 0.98),
            wx + off_x,
            wz + off_z,
            seed ^ 0x51A7_6006,
        );
    }

    let hanging_count = 1 + (col_rand_u32(wx, wz, seed ^ 0x51A7_6111) % 2) as i32;
    for i in 0..hanging_count {
        let (dx, dz) = DIRS[(side_start + (i as usize) * 3 + 1) % DIRS.len()];
        let dist = (canopy_r - 1).max(2);
        let hx =
            x + dx * dist + rand_signed_offset(wx + i * 29, wz - i * 31, seed ^ 0x51A7_6222, 1);
        let hz =
            z + dz * dist + rand_signed_offset(wx - i * 37, wz + i * 23, seed ^ 0x51A7_6333, 1);
        let hy = crown_low - 1 - (i % 2);
        let tuft_r = 2 + (col_rand_u32(wx + i * 41, wz - i * 43, seed ^ 0x51A7_6444) % 2) as i32;

        fill_leaf_blob(
            chunk,
            reg,
            leaves_id,
            hx,
            hy,
            hz,
            tuft_r,
            2,
            tuft_r,
            (base_density + 0.04).clamp(0.58, 0.96),
            wx + dx * dist,
            wz + dz * dist,
            seed ^ 0x51A7_6555,
        );
    }

    let branch_count = 2 + (col_rand_u32(wx, wz, seed ^ SALT_TREE_OAK_BRANCH) % 3) as i32;
    let dir_start = (col_rand_u32(wx, wz, seed ^ 0x51A7_6666) % DIRS.len() as u32) as usize;

    for i in 0..branch_count {
        let (dx, dz) = DIRS[(dir_start + (i as usize) * 2) % DIRS.len()];
        let len = 2 + (col_rand_u32(wx + i * 17, wz - i * 19, seed ^ 0x51A7_6777) % 2) as i32;
        let base_y = trunk_top - 2 - (i % 3);

        let mut end_x = x;
        let mut end_y = base_y;
        let mut end_z = z;
        for step in 1..=len {
            let bx = x + dx * step;
            let by = base_y + step / 3;
            let bz = z + dz * step;
            set_log_if_replaceable(chunk, reg, bx, by, bz, trunk_id);
            end_x = bx;
            end_y = by;
            end_z = bz;
        }

        fill_leaf_blob(
            chunk,
            reg,
            leaves_id,
            end_x,
            end_y,
            end_z,
            2 + (i % 2),
            2,
            2 + (i % 2),
            (base_density + 0.07).clamp(0.66, 0.99),
            wx + dx * len,
            wz + dz * len,
            seed ^ 0x51A7_6888,
        );
    }

    let lower_branch_count = (col_rand_u32(wx, wz, seed ^ 0x51A7_6999) % 2) as i32;
    for i in 0..lower_branch_count {
        let (dx, dz) = DIRS[(dir_start + (i as usize) * 3 + 1) % DIRS.len()];
        let len = 1 + (col_rand_u32(wx + i * 53, wz - i * 59, seed ^ 0x51A7_7000) % 2) as i32;
        let base_y = trunk_top - (canopy_h / 2).max(2) - i - 1;

        let mut end_x = x;
        let mut end_z = z;
        for step in 1..=len {
            let bx = x + dx * step;
            let bz = z + dz * step;
            set_log_if_replaceable(chunk, reg, bx, base_y, bz, trunk_id);
            end_x = bx;
            end_z = bz;
        }

        fill_leaf_blob(
            chunk,
            reg,
            leaves_id,
            end_x,
            base_y,
            end_z,
            2,
            2,
            2,
            (base_density + 0.02).clamp(0.58, 0.95),
            wx + dx * len,
            wz + dz * len,
            seed ^ 0x51A7_7111,
        );
    }

    place_leaf_if_replaceable(chunk, reg, x, crown_top + 1, z, leaves_id);
    place_leaf_if_replaceable(chunk, reg, x, crown_top + 2, z, leaves_id);
    place_leaf_if_replaceable(chunk, reg, x, crown_top + 3, z, leaves_id);
}

#[allow(clippy::too_many_arguments)]
fn place_spruce_canopy(
    chunk: &mut ChunkData,
    reg: &BlockRegistry,
    leaves_id: u16,
    lx: usize,
    lz: usize,
    wx: i32,
    wz: i32,
    seed: u32,
    trunk_top: i32,
    canopy_r: i32,
    canopy_h: i32,
    base_density: f32,
) {
    let x = lx as i32;
    let z = lz as i32;
    let branch_bottom = trunk_top - canopy_h + 1;
    let denom = (trunk_top - branch_bottom).max(1) as f32;

    for y in branch_bottom..=trunk_top {
        let t = (y - branch_bottom) as f32 / denom;
        let mut layer_r = (1.0 - t) * canopy_r as f32 + 1.15;
        if (y + (seed as i32 & 0x3)) % 2 == 0 {
            layer_r -= 0.45;
        }
        layer_r = layer_r.clamp(1.0, (canopy_r + 1) as f32);

        let density = if y == trunk_top {
            1.0
        } else {
            (base_density + 0.08).clamp(0.68, 0.98)
        };

        fill_leaf_disk(
            chunk,
            reg,
            leaves_id,
            x,
            y,
            z,
            layer_r,
            density,
            wx,
            wz,
            seed ^ SALT_TREE_LEAF_SHAPE,
        );
    }

    fill_leaf_disk(
        chunk,
        reg,
        leaves_id,
        x,
        branch_bottom - 1,
        z,
        canopy_r.max(2) as f32 + 0.25,
        (base_density - 0.10).clamp(0.48, 0.92),
        wx,
        wz,
        seed ^ 0x7331_0001,
    );

    place_leaf_if_replaceable(chunk, reg, x, trunk_top + 1, z, leaves_id);
    place_leaf_if_replaceable(chunk, reg, x, trunk_top + 2, z, leaves_id);
}

#[allow(clippy::too_many_arguments)]
fn fill_leaf_blob(
    chunk: &mut ChunkData,
    reg: &BlockRegistry,
    leaves_id: u16,
    cx: i32,
    cy: i32,
    cz: i32,
    rx: i32,
    ry: i32,
    rz: i32,
    density: f32,
    wx: i32,
    wz: i32,
    seed: u32,
) {
    let rx = rx.max(1);
    let ry = ry.max(1);
    let rz = rz.max(1);
    let density = (density * TREE_LEAF_DENSITY_SCALE).clamp(0.0, 1.0);
    let inv_rx = 1.0 / rx as f32;
    let inv_ry = 1.0 / ry as f32;
    let inv_rz = 1.0 / rz as f32;

    for y in (cy - ry)..=(cy + ry) {
        for z in (cz - rz)..=(cz + rz) {
            for x in (cx - rx)..=(cx + rx) {
                if !in_bounds_local(x, y, z) {
                    continue;
                }

                let nx = (x - cx) as f32 * inv_rx;
                let ny = (y - cy) as f32 * inv_ry;
                let nz = (z - cz) as f32 * inv_rz;
                let dist_sq = nx * nx + ny * ny + nz * nz;
                let dist = dist_sq.sqrt();
                let outer = 1.22;
                if dist > outer {
                    continue;
                }

                let edge_alpha = 1.0 - smoothstep(0.72, outer, dist);
                let center_bonus = (1.0 - dist_sq).max(0.0) * 0.16;
                let keep = (density * (0.50 + 0.50 * edge_alpha) + center_bonus).clamp(0.0, 1.0);
                let rand_keep =
                    col_rand_f32(wx + (x - cx) * 11 + y * 3, wz + (z - cz) * 7 - y * 5, seed);
                if rand_keep > keep {
                    continue;
                }

                place_leaf_if_replaceable(chunk, reg, x, y, z, leaves_id);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn fill_leaf_disk(
    chunk: &mut ChunkData,
    reg: &BlockRegistry,
    leaves_id: u16,
    cx: i32,
    y: i32,
    cz: i32,
    radius: f32,
    density: f32,
    wx: i32,
    wz: i32,
    seed: u32,
) {
    let radius = radius.max(1.0);
    let density = (density * TREE_LEAF_DENSITY_SCALE).clamp(0.0, 1.0);
    let r_outer = radius + 0.82;
    let r_inner = (radius - 0.65).max(0.25);
    let ir = r_outer.ceil() as i32;
    for dz in -ir..=ir {
        for dx in -ir..=ir {
            let x = cx + dx;
            let z = cz + dz;
            if !in_bounds_local(x, y, z) {
                continue;
            }

            let dist = ((dx * dx + dz * dz) as f32).sqrt();
            if dist > r_outer {
                continue;
            }

            let edge_alpha = 1.0 - smoothstep(r_inner, r_outer, dist);
            let center_bonus = (1.0 - (dist / r_outer).clamp(0.0, 1.0)).max(0.0) * 0.10;
            let keep = (density * (0.52 + 0.48 * edge_alpha) + center_bonus).clamp(0.0, 1.0);
            let rand_keep = col_rand_f32(wx + dx * 9 + y * 5, wz + dz * 9 - y * 3, seed);
            if rand_keep > keep {
                continue;
            }

            place_leaf_if_replaceable(chunk, reg, x, y, z, leaves_id);
        }
    }
}

#[inline]
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if (edge1 - edge0).abs() < f32::EPSILON {
        return if x < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn place_leaf_if_replaceable(
    chunk: &mut ChunkData,
    reg: &BlockRegistry,
    x: i32,
    y: i32,
    z: i32,
    leaves_id: u16,
) {
    if !in_bounds_local(x, y, z) {
        return;
    }
    let id = chunk.get(x as usize, y as usize, z as usize);
    if id == 0 || reg.stats(id).foliage {
        chunk.set(x as usize, y as usize, z as usize, leaves_id);
    }
}

#[inline]
fn set_log_if_replaceable(
    chunk: &mut ChunkData,
    reg: &BlockRegistry,
    x: i32,
    y: i32,
    z: i32,
    log_id: u16,
) {
    if !in_bounds_local(x, y, z) {
        return;
    }
    let id = chunk.get(x as usize, y as usize, z as usize);
    if id == 0 || reg.stats(id).foliage {
        chunk.set(x as usize, y as usize, z as usize, log_id);
    }
}

#[inline]
fn in_bounds_local(x: i32, y: i32, z: i32) -> bool {
    x >= 0 && x < CX as i32 && y >= 0 && y < CY as i32 && z >= 0 && z < CZ as i32
}

#[inline]
fn rand_signed_offset(wx: i32, wz: i32, seed: u32, max_abs: i32) -> i32 {
    if max_abs <= 0 {
        return 0;
    }
    let span = (max_abs * 2 + 1) as u32;
    (col_rand_u32(wx, wz, seed) % span) as i32 - max_abs
}

#[inline]
fn pick_i32_inclusive(wx: i32, wz: i32, seed: u32, range: (i32, i32)) -> i32 {
    let lo = range.0.min(range.1);
    let hi = range.0.max(range.1);
    if lo == hi {
        return lo;
    }
    let span = (hi - lo + 1).max(1) as u32;
    lo + (col_rand_u32(wx, wz, seed) % span) as i32
}
