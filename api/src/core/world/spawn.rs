use crate::core::config::WorldGenConfig;
use crate::core::world::biome::registry::BiomeRegistry;
use crate::core::world::block::BlockRegistry;
use crate::core::world::chunk::{ChunkData, SEA_LEVEL};
use crate::core::world::chunk_dimension::{
    CY, Y_MAX, Y_MIN, local_y_to_world, world_to_chunk_xz, world_y_to_local,
};
use crate::generator::chunk::chunk_utils::{load_or_gen_chunk_async, save_chunk_at_root_sync};
use crate::generator::chunk::trees::registry::TreeRegistry;
use bevy::log::info;
use bevy::prelude::IVec2;
use bevy::tasks::futures_lite::future;
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

const SPAWN_GENERATION_RADIUS: i32 = 2;
const SPAWN_SEARCH_RADIUS_BLOCKS: i32 = 32;

/// Runs the `ensure_world_spawn_generated` routine for ensure world spawn generated in the `core::world::spawn` module.
pub fn ensure_world_spawn_generated(world_root: &Path, world_seed: i32) -> [f32; 3] {
    info!(
        "Preparing world spawn area at {:?} (seed={})",
        world_root, world_seed
    );
    let block_registry = BlockRegistry::load_headless("assets/blocks");
    let biome_registry = BiomeRegistry::load_from_folder("assets/biomes");
    let tree_registry = TreeRegistry::load_from_folder("assets/data/trees");
    let generated_chunks = generate_spawn_chunks(
        world_root,
        world_seed,
        &block_registry,
        &biome_registry,
        &tree_registry,
    );

    let spawn = derive_spawn_translation(&generated_chunks, &block_registry, world_seed);
    info!(
        "Derived world spawn: [{:.2}, {:.2}, {:.2}]",
        spawn[0], spawn[1], spawn[2]
    );
    spawn
}

/// Generates spawn chunks for the `core::world::spawn` module.
fn generate_spawn_chunks(
    world_root: &Path,
    world_seed: i32,
    block_registry: &BlockRegistry,
    biome_registry: &BiomeRegistry,
    tree_registry: &TreeRegistry,
) -> HashMap<IVec2, ChunkData> {
    let mut chunks = HashMap::new();
    let config = WorldGenConfig { seed: world_seed };
    let diameter = (SPAWN_GENERATION_RADIUS * 2 + 1) as usize;
    let total_chunks = diameter * diameter;
    let mut completed = 0usize;
    let started_at = Instant::now();
    let mut last_progress_log = Instant::now();

    info!(
        "World build started: generating {} spawn chunks (radius={})",
        total_chunks, SPAWN_GENERATION_RADIUS
    );

    for z in -SPAWN_GENERATION_RADIUS..=SPAWN_GENERATION_RADIUS {
        for x in -SPAWN_GENERATION_RADIUS..=SPAWN_GENERATION_RADIUS {
            let coord = IVec2::new(x, z);
            let chunk = future::block_on(load_or_gen_chunk_async(
                world_root.to_path_buf(),
                coord,
                block_registry,
                biome_registry,
                tree_registry,
                config.clone(),
            ));
            save_chunk_at_root_sync(world_root.to_path_buf(), coord, &chunk)
                .expect("Failed to persist generated spawn chunk");
            chunks.insert(coord, chunk);

            completed += 1;
            let now = Instant::now();
            if completed == total_chunks
                || now.duration_since(last_progress_log) >= Duration::from_millis(500)
            {
                let percent = (completed as f32 / total_chunks as f32) * 100.0;
                info!(
                    "World build progress: {:.1}% ({}/{})",
                    percent, completed, total_chunks
                );
                last_progress_log = now;
            }
        }
    }

    info!(
        "World build finished in {:.2}s",
        started_at.elapsed().as_secs_f32()
    );

    chunks
}

/// Runs the `derive_spawn_translation` routine for derive spawn translation in the `core::world::spawn` module.
fn derive_spawn_translation(
    chunks: &HashMap<IVec2, ChunkData>,
    block_registry: &BlockRegistry,
    world_seed: i32,
) -> [f32; 3] {
    let (anchor_x, anchor_z) = spawn_anchor_from_seed(world_seed);
    let mut best: Option<SpawnCandidate> = None;

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
                if block_id == 0
                    || block_registry.is_fluid(block_id)
                    || !block_registry.is_solid_for_collision(block_id)
                    || block_registry
                        .name_opt(block_id)
                        .is_some_and(|name| name.contains("leaves"))
                {
                    continue;
                }

                let world_y = local_y_to_world(ly);
                let dry_land = world_y >= SEA_LEVEL;
                let clearance = column_has_spawn_clearance(chunks, block_registry, wx, world_y, wz);
                let dx = wx - anchor_x;
                let dz = wz - anchor_z;
                let dist2 = dx * dx + dz * dz;
                let candidate = SpawnCandidate {
                    tier: spawn_candidate_tier(clearance, dry_land),
                    dist2,
                    world_y,
                    wx,
                    wz,
                };

                if should_replace_spawn_candidate(best, candidate) {
                    best = Some(candidate);
                }
                break;
            }
        }
    }

    if let Some(SpawnCandidate {
        world_y, wx, wz, ..
    }) = best
    {
        [wx as f32 + 0.5, world_y as f32 + 2.0, wz as f32 + 0.5]
    } else {
        [0.5, 180.0, 0.5]
    }
}

#[inline]
pub fn spawn_anchor_from_seed(seed: i32) -> (i32, i32) {
    let mut value = seed as u32 ^ 0xA53C_4F1D;
    value = value.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    let x =
        (value as i32).rem_euclid(SPAWN_SEARCH_RADIUS_BLOCKS * 2 + 1) - SPAWN_SEARCH_RADIUS_BLOCKS;
    value = value.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    let z =
        (value as i32).rem_euclid(SPAWN_SEARCH_RADIUS_BLOCKS * 2 + 1) - SPAWN_SEARCH_RADIUS_BLOCKS;
    (x, z)
}

#[derive(Clone, Copy)]
struct SpawnCandidate {
    tier: u8,
    dist2: i32,
    world_y: i32,
    wx: i32,
    wz: i32,
}

#[inline]
fn should_replace_spawn_candidate(best: Option<SpawnCandidate>, next: SpawnCandidate) -> bool {
    match best {
        None => true,
        Some(current) => {
            next.tier > current.tier
                || (next.tier == current.tier
                    && (next.dist2 < current.dist2
                        || (next.dist2 == current.dist2 && next.world_y > current.world_y)))
        }
    }
}

#[inline]
fn spawn_candidate_tier(clearance: bool, dry_land: bool) -> u8 {
    match (clearance, dry_land) {
        (true, true) => 3,
        (true, false) => 2,
        (false, true) => 1,
        (false, false) => 0,
    }
}

#[inline]
fn column_has_spawn_clearance(
    chunks: &HashMap<IVec2, ChunkData>,
    block_registry: &BlockRegistry,
    wx: i32,
    ground_y: i32,
    wz: i32,
) -> bool {
    if ground_y + 2 > Y_MAX {
        return false;
    }

    let Some(head1) = world_block_id(chunks, wx, ground_y + 1, wz) else {
        return false;
    };
    let Some(head2) = world_block_id(chunks, wx, ground_y + 2, wz) else {
        return false;
    };

    block_registry.is_air(head1) && block_registry.is_air(head2)
}

#[inline]
fn world_block_id(chunks: &HashMap<IVec2, ChunkData>, wx: i32, wy: i32, wz: i32) -> Option<u16> {
    if !(Y_MIN..=Y_MAX).contains(&wy) {
        return None;
    }

    let (coord, local) = world_to_chunk_xz(wx, wz);
    let chunk = chunks.get(&coord)?;
    let ly = world_y_to_local(wy);
    Some(chunk.get(local.x as usize, ly, local.y as usize))
}
