use crate::models::{HostedDrop, HostedPlayer};
use api::{
    core::{
        config::WorldGenConfig,
        world::{
            biome::registry::BiomeRegistry,
            block::BlockRegistry,
            chunk::ChunkData,
            chunk_dimension::{Y_MAX, Y_MIN, world_to_chunk_xz, world_y_to_local},
        },
    },
    generator::chunk::chunk_utils::{encode_chunk, load_or_gen_chunk_async},
};
use bevy::ecs::entity::Entity;
use bevy::math::IVec2;
use bevy::prelude::Resource;
use bevy::tasks::{AsyncComputeTaskPool, Task, TaskPool};
use futures_lite::future;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

const STREAM_CHUNK_CACHE_LIMIT: usize = 512;

#[derive(Resource)]
pub struct ServerRuntimeConfig {
    pub server_name: String,
    pub motd: String,
    pub max_players: usize,
    pub client_timeout: u64,
    pub world_name: String,
    pub world_seed: i32,
    pub spawn_translation: [f32; 3],
}

#[derive(Resource)]
pub struct ServerState {
    pub world_root: PathBuf,
    pub block_registry: BlockRegistry,
    pub biome_registry: BiomeRegistry,
    pub world_gen_config: WorldGenConfig,
    pub block_overrides: HashMap<[i32; 3], u16>,
    pub streamed_chunk_cache: HashMap<IVec2, Vec<u8>>,
    pub streamed_chunk_cache_order: VecDeque<IVec2>,
    pub pending_stream_chunk_tasks: HashMap<IVec2, Task<(IVec2, ChunkData)>>,
    pub pending_stream_chunk_waiters: HashMap<IVec2, HashSet<Entity>>,
    pub pending_chunk_sends: VecDeque<(Entity, IVec2)>,
    pub next_player_id: u64,
    pub next_drop_id: u64,
    /// Connection entities waiting for their Auth message
    pub pending_auth: HashMap<Entity, String>,
    pub players: HashMap<Entity, HostedPlayer>,
    pub drops: HashMap<u64, HostedDrop>,
}

impl ServerState {
    pub fn new(world_root: PathBuf, world_seed: i32) -> Self {
        let block_registry = BlockRegistry::load_headless("assets/blocks");
        let biome_registry = BiomeRegistry::load_from_folder("assets/biomes");
        let world_gen_config = WorldGenConfig { seed: world_seed };
        let block_overrides = load_block_overrides(world_root.join("blocks.txt").as_path());
        AsyncComputeTaskPool::get_or_init(TaskPool::default);
        Self {
            world_root,
            block_registry,
            biome_registry,
            world_gen_config,
            block_overrides,
            streamed_chunk_cache: HashMap::new(),
            streamed_chunk_cache_order: VecDeque::new(),
            pending_stream_chunk_tasks: HashMap::new(),
            pending_stream_chunk_waiters: HashMap::new(),
            pending_chunk_sends: VecDeque::new(),
            next_player_id: 1,
            next_drop_id: 1,
            pending_auth: HashMap::new(),
            players: HashMap::new(),
            drops: HashMap::new(),
        }
    }

    pub fn persist_block_overrides(&self) {
        let path = self.world_root.join("blocks.txt");
        let mut lines = self
            .block_overrides
            .iter()
            .map(|(location, block_id)| {
                format!(
                    "{} {} {} {}",
                    location[0], location[1], location[2], block_id
                )
            })
            .collect::<Vec<_>>();
        lines.sort();

        if let Err(error) = fs::write(&path, lines.join("\n")) {
            log::warn!("Failed to persist block overrides {:?}: {}", path, error);
        }
    }

    pub fn queue_chunk_for_stream(&mut self, entity: Entity, coord: IVec2) {
        if self.streamed_chunk_cache.contains_key(&coord) {
            self.pending_chunk_sends.push_back((entity, coord));
            return;
        }

        self.pending_stream_chunk_waiters
            .entry(coord)
            .or_default()
            .insert(entity);

        if self.pending_stream_chunk_tasks.contains_key(&coord) {
            return;
        }

        let world_root = self.world_root.clone();
        let block_registry = self.block_registry.clone();
        let biome_registry = self.biome_registry.clone();
        let world_gen_config = self.world_gen_config.clone();
        let pool = AsyncComputeTaskPool::get();
        let task = pool.spawn(async move {
            let chunk = load_or_gen_chunk_async(
                world_root,
                coord,
                &block_registry,
                &biome_registry,
                world_gen_config,
            )
            .await;
            (coord, chunk)
        });

        self.pending_stream_chunk_tasks.insert(coord, task);
    }

    pub fn collect_ready_stream_chunks(&mut self) {
        let mut finished = Vec::new();
        let mut ready_chunks = Vec::new();

        for (coord, task) in &mut self.pending_stream_chunk_tasks {
            if let Some((ready_coord, mut chunk)) = future::block_on(future::poll_once(task)) {
                apply_block_overrides(&self.block_overrides, ready_coord, &mut chunk);
                chunk.mark_all_dirty();
                ready_chunks.push((ready_coord, encode_chunk(&chunk)));
                finished.push(*coord);
            }
        }

        for coord in finished {
            self.pending_stream_chunk_tasks.remove(&coord);
        }

        for (ready_coord, encoded) in ready_chunks {
            self.store_stream_chunk(ready_coord, encoded);

            if let Some(waiters) = self.pending_stream_chunk_waiters.remove(&ready_coord) {
                for entity in waiters {
                    self.pending_chunk_sends.push_back((entity, ready_coord));
                }
            }
        }
    }

    pub fn invalidate_streamed_chunk(&mut self, coord: IVec2) {
        self.streamed_chunk_cache.remove(&coord);
        self.pending_stream_chunk_tasks.remove(&coord);
        self.pending_stream_chunk_waiters.remove(&coord);
    }

    fn store_stream_chunk(&mut self, coord: IVec2, encoded: Vec<u8>) {
        self.streamed_chunk_cache.insert(coord, encoded);
        self.streamed_chunk_cache_order
            .retain(|cached| *cached != coord);
        self.streamed_chunk_cache_order.push_back(coord);

        while self.streamed_chunk_cache.len() > STREAM_CHUNK_CACHE_LIMIT {
            let Some(oldest) = self.streamed_chunk_cache_order.pop_front() else {
                break;
            };

            self.streamed_chunk_cache.remove(&oldest);
        }
    }
}

fn apply_block_overrides(overrides: &HashMap<[i32; 3], u16>, coord: IVec2, chunk: &mut ChunkData) {
    for (location, block_id) in overrides {
        let world_y = location[1];
        if !(Y_MIN..=Y_MAX).contains(&world_y) {
            continue;
        }

        let (override_coord, local) = world_to_chunk_xz(location[0], location[2]);
        if override_coord != coord {
            continue;
        }

        let lx = local.x as usize;
        let lz = local.y as usize;
        let ly = world_y_to_local(world_y);
        chunk.set(lx, ly, lz, *block_id);
    }
}

fn load_block_overrides(path: &Path) -> HashMap<[i32; 3], u16> {
    let Ok(contents) = fs::read_to_string(path) else {
        return HashMap::new();
    };

    contents
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let x = parts.next()?.parse::<i32>().ok()?;
            let y = parts.next()?.parse::<i32>().ok()?;
            let z = parts.next()?.parse::<i32>().ok()?;
            let block_id = parts.next()?.parse::<u16>().ok()?;
            Some(([x, y, z], block_id))
        })
        .collect()
}
