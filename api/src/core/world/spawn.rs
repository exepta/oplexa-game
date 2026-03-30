use crate::core::config::WorldGenConfig;
use crate::core::world::biome::registry::BiomeRegistry;
use crate::core::world::block::BlockRegistry;
use crate::core::world::chunk::{ChunkData, SEA_LEVEL};
use crate::core::world::chunk_dimension::{CY, local_y_to_world, world_to_chunk_xz};
use crate::generator::chunk::chunk_utils::{load_or_gen_chunk_async, save_chunk_at_root_sync};
use bevy::prelude::IVec2;
use bevy::tasks::futures_lite::future;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

const SPAWN_GENERATION_RADIUS: i32 = 2;
const SPAWN_SEARCH_RADIUS_BLOCKS: i32 = 32;

pub fn ensure_world_spawn_generated(world_root: &Path, world_seed: i32) -> [f32; 3] {
    let block_registry = BlockRegistry::load_headless("assets/blocks");
    let biome_registry = BiomeRegistry::load_from_folder("assets/biomes");
    let generated_chunks =
        generate_spawn_chunks(world_root, world_seed, &block_registry, &biome_registry);

    let spawn_file = world_root.join("spawn.txt");
    read_spawn_translation(&spawn_file).unwrap_or_else(|| {
        let spawn = derive_spawn_translation(&generated_chunks, &block_registry);
        if let Err(error) = write_spawn_translation(&spawn_file, spawn) {
            panic!("Failed to write spawn file {:?}: {}", spawn_file, error);
        }
        spawn
    })
}

fn generate_spawn_chunks(
    world_root: &Path,
    world_seed: i32,
    block_registry: &BlockRegistry,
    biome_registry: &BiomeRegistry,
) -> HashMap<IVec2, ChunkData> {
    let mut chunks = HashMap::new();
    let config = WorldGenConfig { seed: world_seed };

    for z in -SPAWN_GENERATION_RADIUS..=SPAWN_GENERATION_RADIUS {
        for x in -SPAWN_GENERATION_RADIUS..=SPAWN_GENERATION_RADIUS {
            let coord = IVec2::new(x, z);
            let chunk = future::block_on(load_or_gen_chunk_async(
                world_root.to_path_buf(),
                coord,
                block_registry,
                biome_registry,
                config.clone(),
            ));
            save_chunk_at_root_sync(world_root.to_path_buf(), coord, &chunk)
                .expect("Failed to persist generated spawn chunk");
            chunks.insert(coord, chunk);
        }
    }

    chunks
}

fn derive_spawn_translation(
    chunks: &HashMap<IVec2, ChunkData>,
    block_registry: &BlockRegistry,
) -> [f32; 3] {
    let mut best: Option<(bool, i32, i32, i32, i32)> = None;

    for wz in -SPAWN_SEARCH_RADIUS_BLOCKS..=SPAWN_SEARCH_RADIUS_BLOCKS {
        for wx in -SPAWN_SEARCH_RADIUS_BLOCKS..=SPAWN_SEARCH_RADIUS_BLOCKS {
            let (chunk_coord, local) = world_to_chunk_xz(wx, wz);
            let Some(chunk) = chunks.get(&chunk_coord) else {
                continue;
            };
            let lx = local.x as usize;
            let lz = local.y as usize;

            for ly in (0..CY).rev() {
                let block_id = chunk.get(lx, ly, lz);
                if block_id == 0 || block_registry.is_fluid(block_id) {
                    continue;
                }

                let world_y = local_y_to_world(ly);
                let dry_land = world_y >= SEA_LEVEL;
                let dist2 = wx * wx + wz * wz;

                let replace = match best {
                    None => true,
                    Some((best_dry, best_dist2, best_y, _, _)) => {
                        (dry_land && !best_dry)
                            || (dry_land == best_dry
                                && (dist2 < best_dist2
                                    || (dist2 == best_dist2 && world_y > best_y)))
                    }
                };

                if replace {
                    best = Some((dry_land, dist2, world_y, wx, wz));
                }
                break;
            }
        }
    }

    if let Some((_, _, world_y, wx, wz)) = best {
        [wx as f32 + 0.5, world_y as f32 + 2.0, wz as f32 + 0.5]
    } else {
        [0.5, 180.0, 0.5]
    }
}

fn read_spawn_translation(path: &Path) -> Option<[f32; 3]> {
    let text = fs::read_to_string(path).ok()?;
    let mut parts = text.split_whitespace();
    let x = parts.next()?.parse::<f32>().ok()?;
    let y = parts.next()?.parse::<f32>().ok()?;
    let z = parts.next()?.parse::<f32>().ok()?;
    Some([x, y, z])
}

fn write_spawn_translation(path: &Path, spawn_translation: [f32; 3]) -> std::io::Result<()> {
    fs::write(
        path,
        format!(
            "{} {} {}",
            spawn_translation[0], spawn_translation[1], spawn_translation[2]
        ),
    )
}
