use crate::core::world::biome::BiomeVegetationSpawn;
use crate::core::world::biome::func::{col_rand_f32, dominant_biome_at_p_chunks};
use crate::core::world::biome::registry::BiomeRegistry;
use crate::core::world::block::{BlockEnvironment, BlockId, BlockRegistry};
use crate::core::world::chunk::{ChunkData, SEA_LEVEL};
use crate::core::world::chunk_dimension::{CX, CY, CZ, Y_MIN};
use bevy::prelude::*;

const SALT_VEGETATION_SPAWN: u32 = 0x4C2B_8111;
const SALT_VEGETATION_PICK: u32 = 0x4C2B_8222;

/// Populates a generated chunk with weighted biome vegetation props.
pub fn populate_vegetation_props_in_chunk(
    chunk: &mut ChunkData,
    coord: IVec2,
    reg: &BlockRegistry,
    biomes: &BiomeRegistry,
    world_seed: i32,
) {
    let seed = world_seed as u32;

    for lx in 0..CX {
        for lz in 0..CZ {
            let wx = coord.x * CX as i32 + lx as i32;
            let wz = coord.y * CZ as i32 + lz as i32;

            let px = coord.x as f32 + (lx as f32 + 0.5) / CX as f32;
            let pz = coord.y as f32 + (lz as f32 + 0.5) / CZ as f32;
            let biome = dominant_biome_at_p_chunks(biomes, world_seed, Vec2::new(px, pz));
            let vegetation = &biome.generation.vegetation;
            if vegetation.items.is_empty() {
                continue;
            }

            let spawn_chance = vegetation.density.clamp(0.0, 0.95);
            if spawn_chance <= 0.0 {
                continue;
            }
            if col_rand_f32(wx, wz, seed ^ SALT_VEGETATION_SPAWN) >= spawn_chance {
                continue;
            }

            let Some(prop_id) = pick_weighted_prop(wx, wz, seed, &vegetation.items, reg) else {
                continue;
            };

            let Some(ground_ly) = find_spawn_ground_local_y(chunk, reg, prop_id, lx, lz) else {
                continue;
            };
            let ground_world_y = Y_MIN + ground_ly as i32;
            let place_ly = ground_ly + 1;
            let place_world_y = Y_MIN + place_ly as i32;
            let allows_water_env = reg
                .allowed_environments(prop_id)
                .contains(&BlockEnvironment::Water);
            if !allows_water_env && ground_world_y < SEA_LEVEL - 1 {
                continue;
            }
            if !prop_environment_allows_spawn(chunk, reg, prop_id, lx, place_ly, lz, place_world_y)
            {
                continue;
            }

            let place_id = chunk.get(lx, place_ly, lz);
            if reg.is_water_logged(prop_id) && place_id != 0 && reg.is_fluid(place_id) {
                chunk.set_stacked(lx, place_ly, lz, prop_id);
            } else {
                chunk.set(lx, place_ly, lz, prop_id);
            }
        }
    }
}

fn find_spawn_ground_local_y(
    chunk: &ChunkData,
    reg: &BlockRegistry,
    prop_id: BlockId,
    lx: usize,
    lz: usize,
) -> Option<usize> {
    if CY < 2 {
        return None;
    }

    for ground_ly in (0..(CY - 1)).rev() {
        let ground_id = chunk.get(lx, ground_ly, lz);
        if !is_valid_ground_for_prop(reg, prop_id, ground_id) {
            continue;
        }

        let place_ly = ground_ly + 1;
        let place_id = chunk.get(lx, place_ly, lz);
        if place_id != 0 && !(reg.is_water_logged(prop_id) && reg.is_fluid(place_id)) {
            continue;
        }

        return Some(ground_ly);
    }

    None
}

fn pick_weighted_prop(
    wx: i32,
    wz: i32,
    seed: u32,
    items: &[BiomeVegetationSpawn],
    reg: &BlockRegistry,
) -> Option<BlockId> {
    let mut total_weight = 0.0f32;
    for item in items {
        if item.weight <= 0.0 || item.block.trim().is_empty() {
            continue;
        }
        let id = reg.id_or_air(item.block.as_str());
        if id != 0 && reg.is_prop(id) {
            total_weight += item.weight;
        }
    }
    if total_weight <= 0.0 {
        return None;
    }

    let mut pick = col_rand_f32(wx, wz, seed ^ SALT_VEGETATION_PICK) * total_weight;
    for item in items {
        if item.weight <= 0.0 || item.block.trim().is_empty() {
            continue;
        }
        let id = reg.id_or_air(item.block.as_str());
        if id == 0 || !reg.is_prop(id) {
            continue;
        }
        pick -= item.weight;
        if pick <= 0.0 {
            return Some(id);
        }
    }

    for item in items {
        if item.weight <= 0.0 || item.block.trim().is_empty() {
            continue;
        }
        let id = reg.id_or_air(item.block.as_str());
        if id != 0 && reg.is_prop(id) {
            return Some(id);
        }
    }
    None
}

#[inline]
fn is_valid_ground_for_prop(reg: &BlockRegistry, prop_id: BlockId, ground_id: BlockId) -> bool {
    ground_id != 0
        && !reg.is_fluid(ground_id)
        && !reg.stats(ground_id).foliage
        && !reg.is_prop(ground_id)
        && reg.stats(ground_id).solid
        && reg.prop_allows_ground(prop_id, ground_id)
}

#[inline]
fn prop_environment_allows_spawn(
    chunk: &ChunkData,
    reg: &BlockRegistry,
    prop_id: BlockId,
    lx: usize,
    place_ly: usize,
    lz: usize,
    place_world_y: i32,
) -> bool {
    let allowed = reg.allowed_environments(prop_id);
    if allowed.is_empty() {
        return true;
    }

    let place_id = chunk.get(lx, place_ly, lz);
    allowed.iter().any(|env| match env {
        BlockEnvironment::Water => place_id != 0 && reg.is_fluid(place_id),
        BlockEnvironment::Overworld => place_world_y > 50,
        BlockEnvironment::Cave => place_world_y <= 10,
    })
}
