use crate::models::{HostedDrop, HostedPlayer};
use api::{
    core::{
        config::WorldGenConfig,
        world::{
            biome::registry::BiomeRegistry,
            block::{BlockId, BlockRegistry},
            chunk::{ChunkData, SEA_LEVEL},
            chunk_dimension::{CX, CY, CZ, Y_MAX, Y_MIN, world_to_chunk_xz, world_y_to_local},
        },
    },
    generator::chunk::{
        cave_utils::{CaveParams, worm_edits_for_chunk},
        chunk_utils::{encode_chunk, load_or_gen_chunk_async, save_chunk_at_root_sync},
        trees::registry::TreeRegistry,
    },
};
use bevy::ecs::entity::Entity;
use bevy::math::IVec2;
use bevy::prelude::Resource;
use bevy::tasks::{AsyncComputeTaskPool, Task, TaskPool};
use futures_lite::future;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

const STREAM_CHUNK_CACHE_LIMIT: usize = 512;

/// Represents server runtime config used by the `state` module.
#[derive(Resource)]
pub struct ServerRuntimeConfig {
    pub server_name: String,
    pub motd: String,
    pub max_players: usize,
    pub client_timeout: u64,
    pub world_name: String,
    pub world_seed: i32,
    pub spawn_translation: [f32; 3],
    pub chunk_stream_sends_per_tick_base: usize,
    pub chunk_stream_sends_per_tick_per_client: usize,
    pub chunk_stream_sends_per_tick_max: usize,
    pub chunk_stream_inflight_per_client: usize,
    pub chunk_flight_timeout_ms: u64,
    pub chunk_stream_gen_max_inflight: usize,
    pub max_stream_radius: i32,
    pub locate_search_radius: i32,
    pub dead_entity_check_interval_secs: u64,
}

/// Represents server state used by the `state` module.
#[derive(Resource)]
pub struct ServerState {
    pub world_root: PathBuf,
    pub block_registry: Arc<BlockRegistry>,
    pub biome_registry: Arc<BiomeRegistry>,
    pub tree_registry: Arc<TreeRegistry>,
    pub world_gen_config: WorldGenConfig,
    pub block_overrides: HashMap<[i32; 3], u16>,
    pub streamed_chunk_cache: HashMap<IVec2, Vec<u8>>,
    pub streamed_chunk_cache_order: VecDeque<IVec2>,
    pub pending_stream_chunk_tasks: HashMap<IVec2, Task<(IVec2, Vec<u8>)>>,
    pub pending_stream_chunk_queue: VecDeque<IVec2>,
    pub pending_stream_chunk_queued: HashSet<IVec2>,
    pub pending_stream_chunk_waiters: HashMap<IVec2, HashSet<Entity>>,
    pub pending_chunk_sends: VecDeque<(Entity, IVec2)>,
    /// Per-client timestamps of recently sent chunks; used to limit in-flight reliable data.
    pub chunk_send_window: HashMap<Entity, VecDeque<Instant>>,
    pub next_player_id: u64,
    pub next_drop_id: u64,
    /// Connection entities waiting for their Auth message
    pub pending_auth: HashMap<Entity, String>,
    pub players: HashMap<Entity, HostedPlayer>,
    pub drops: HashMap<u64, HostedDrop>,
}

impl ServerState {
    /// Creates a new instance for the `state` module.
    pub fn new(world_root: PathBuf, world_seed: i32) -> Self {
        let block_registry = Arc::new(BlockRegistry::load_headless("assets/blocks"));
        let biome_registry = Arc::new(BiomeRegistry::load_from_folder("assets/biomes"));
        let tree_registry = Arc::new(TreeRegistry::load_from_folder("assets/data/trees"));
        let world_gen_config = WorldGenConfig { seed: world_seed };
        let block_overrides = load_block_overrides(world_root.join("blocks.txt").as_path());
        AsyncComputeTaskPool::get_or_init(TaskPool::default);
        Self {
            world_root,
            block_registry,
            biome_registry,
            tree_registry,
            world_gen_config,
            block_overrides,
            streamed_chunk_cache: HashMap::new(),
            streamed_chunk_cache_order: VecDeque::new(),
            pending_stream_chunk_tasks: HashMap::new(),
            pending_stream_chunk_queue: VecDeque::new(),
            pending_stream_chunk_queued: HashSet::new(),
            pending_stream_chunk_waiters: HashMap::new(),
            pending_chunk_sends: VecDeque::new(),
            chunk_send_window: HashMap::new(),
            next_player_id: 1,
            next_drop_id: 1,
            pending_auth: HashMap::new(),
            players: HashMap::new(),
            drops: HashMap::new(),
        }
    }

    /// Persists block overrides for the `state` module.
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

    /// Runs the `queue_chunk_for_stream` routine for queue chunk for stream in the `state` module.
    pub fn queue_chunk_for_stream(&mut self, entity: Entity, coord: IVec2) {
        if self.streamed_chunk_cache.contains_key(&coord) {
            self.pending_chunk_sends.push_back((entity, coord));
            return;
        }

        self.pending_stream_chunk_waiters
            .entry(coord)
            .or_default()
            .insert(entity);

        if !self.pending_stream_chunk_tasks.contains_key(&coord)
            && self.pending_stream_chunk_queued.insert(coord)
        {
            self.pending_stream_chunk_queue.push_back(coord);
        }
    }

    /// Spawns new stream chunk generation tasks up to the configured in-flight limit.
    pub fn pump_stream_chunk_tasks(&mut self, config: &ServerRuntimeConfig) {
        let max_inflight = config.chunk_stream_gen_max_inflight.max(1);
        if self.pending_stream_chunk_tasks.len() >= max_inflight {
            return;
        }

        let border_id = self.block_registry.id_opt("border_block").unwrap_or(0);
        let water_id = self.block_registry.id_opt("water_block").unwrap_or(0);
        let cave_seed = self.world_gen_config.seed;
        let world_gen_config = self.world_gen_config.clone();
        let pool = AsyncComputeTaskPool::get();

        while self.pending_stream_chunk_tasks.len() < max_inflight {
            let Some(coord) = self.pending_stream_chunk_queue.pop_front() else {
                break;
            };
            self.pending_stream_chunk_queued.remove(&coord);

            if self.streamed_chunk_cache.contains_key(&coord) {
                if let Some(waiters) = self.pending_stream_chunk_waiters.get(&coord) {
                    let waiters = waiters.iter().copied().collect::<Vec<_>>();
                    for entity in waiters {
                        self.pending_chunk_sends.push_back((entity, coord));
                    }
                }
                continue;
            }

            if self.pending_stream_chunk_tasks.contains_key(&coord) {
                continue;
            }

            let has_waiters = self
                .pending_stream_chunk_waiters
                .get(&coord)
                .is_some_and(|waiters| !waiters.is_empty());
            if !has_waiters {
                continue;
            }

            let world_root = self.world_root.clone();
            let block_registry = Arc::clone(&self.block_registry);
            let biome_registry = Arc::clone(&self.biome_registry);
            let tree_registry = Arc::clone(&self.tree_registry);
            let chunk_overrides = collect_chunk_overrides_snapshot(&self.block_overrides, coord);
            let cfg = world_gen_config.clone();

            let task = pool.spawn(async move {
                let mut chunk = load_or_gen_chunk_async(
                    world_root.clone(),
                    coord,
                    &block_registry,
                    &biome_registry,
                    &tree_registry,
                    cfg,
                )
                .await;

                apply_server_caves(&mut chunk, coord, cave_seed, border_id, water_id);
                if water_id != 0 {
                    flood_ocean_connected_water(&mut chunk, SEA_LEVEL, water_id);
                }
                apply_chunk_overrides_snapshot(&chunk_overrides, &mut chunk);
                chunk.mark_all_dirty();

                if let Err(error) = save_chunk_at_root_sync(world_root, coord, &chunk) {
                    log::warn!("Failed to persist streamed chunk {:?}: {}", coord, error);
                }

                (coord, encode_chunk(&chunk))
            });

            self.pending_stream_chunk_tasks.insert(coord, task);
        }
    }

    /// Runs the `collect_ready_stream_chunks` routine for collect ready stream chunks in the `state` module.
    pub fn collect_ready_stream_chunks(&mut self) {
        let mut finished = Vec::new();
        let mut ready_chunks = Vec::new();

        for (coord, task) in &mut self.pending_stream_chunk_tasks {
            if let Some((ready_coord, encoded)) = future::block_on(future::poll_once(task)) {
                ready_chunks.push((ready_coord, encoded));
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

    /// Runs the `invalidate_streamed_chunk` routine for invalidate streamed chunk in the `state` module.
    pub fn invalidate_streamed_chunk(&mut self, coord: IVec2) {
        self.streamed_chunk_cache.remove(&coord);
        self.pending_stream_chunk_tasks.remove(&coord);
        self.pending_stream_chunk_queued.remove(&coord);
        self.pending_stream_chunk_queue
            .retain(|queued| *queued != coord);
        self.pending_stream_chunk_waiters.remove(&coord);
        self.pending_chunk_sends
            .retain(|(_, queued)| *queued != coord);
    }

    /// Stores stream chunk for the `state` module.
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

/// Applies server caves for the `state` module.
fn apply_server_caves(
    chunk: &mut ChunkData,
    coord: IVec2,
    seed: i32,
    border_id: BlockId,
    water_id: BlockId,
) {
    let params = server_cave_params(seed);
    let edits = worm_edits_for_chunk(
        &params,
        coord,
        IVec2::new(CX as i32, CZ as i32),
        Y_MIN,
        Y_MAX,
    );

    for (lx, ly, lz) in edits {
        let lx = lx as usize;
        let ly = ly as usize;
        let lz = lz as usize;
        let current = chunk.get(lx, ly, lz);

        if current != 0 && current != border_id && current != water_id {
            chunk.set(lx, ly, lz, 0);
        }
    }
}

/// Runs the `flood_ocean_connected_water` routine for flood ocean connected water in the `state` module.
fn flood_ocean_connected_water(chunk: &mut ChunkData, sea_level: i32, water_id: BlockId) {
    if water_id == 0 {
        return;
    }

    let sea_level = sea_level.clamp(Y_MIN, Y_MAX);
    let sea_ly = world_y_to_local(sea_level);
    let mut queue: VecDeque<(usize, usize, usize)> = VecDeque::new();
    let mut seen = vec![false; CX * CY * CZ];

    for z in 0..CZ {
        for x in 0..CX {
            // Topmost non-water solid in this column.
            let mut top_world = Y_MIN - 1;
            for ly in (0..CY).rev() {
                let id = chunk.get(x, ly, z);
                if id != 0 && id != water_id {
                    top_world = Y_MIN + ly as i32;
                    break;
                }
            }

            // Only ocean-open columns can seed flood water.
            if top_world >= sea_level {
                continue;
            }

            let start_world = (top_world + 1).max(Y_MIN);
            let start_ly = world_y_to_local(start_world);
            for ly in start_ly..=sea_ly {
                try_push_ocean_seed(chunk, &mut seen, water_id, x, ly, z, &mut queue);
            }
        }
    }

    while let Some((x, y, z)) = queue.pop_front() {
        if y + 1 <= sea_ly {
            try_push_ocean_seed(chunk, &mut seen, water_id, x, y + 1, z, &mut queue);
        }
        if y > 0 {
            try_push_ocean_seed(chunk, &mut seen, water_id, x, y - 1, z, &mut queue);
        }
        if x + 1 < CX {
            try_push_ocean_seed(chunk, &mut seen, water_id, x + 1, y, z, &mut queue);
        }
        if x > 0 {
            try_push_ocean_seed(chunk, &mut seen, water_id, x - 1, y, z, &mut queue);
        }
        if z + 1 < CZ {
            try_push_ocean_seed(chunk, &mut seen, water_id, x, y, z + 1, &mut queue);
        }
        if z > 0 {
            try_push_ocean_seed(chunk, &mut seen, water_id, x, y, z - 1, &mut queue);
        }
    }
}

/// Runs the `try_push_ocean_seed` routine for try push ocean seed in the `state` module.
fn try_push_ocean_seed(
    chunk: &mut ChunkData,
    seen: &mut [bool],
    water_id: BlockId,
    x: usize,
    y: usize,
    z: usize,
    queue: &mut VecDeque<(usize, usize, usize)>,
) {
    let i = (y * CZ + z) * CX + x;
    if seen[i] {
        return;
    }

    let current = chunk.get(x, y, z);
    if current != 0 && current != water_id {
        return;
    }

    seen[i] = true;
    if current == 0 {
        chunk.set(x, y, z, water_id);
    }
    queue.push_back((x, y, z));
}

/// Runs the `server_cave_params` routine for server cave params in the `state` module.
fn server_cave_params(seed: i32) -> CaveParams {
    CaveParams {
        seed,
        y_top: 52,
        y_bottom: -110,
        worms_per_region: 1.35,
        region_chunks: 3,
        base_radius: 4.2,
        radius_var: 3.0,
        step_len: 1.5,
        worm_len_steps: 360,
        room_event_chance: 0.1,
        room_radius_min: 6.0,
        room_radius_max: 10.5,
        caverns_per_region: 0.5,
        cavern_room_count_min: 6,
        cavern_room_count_max: 11,
        cavern_room_radius_xz_min: 16.0,
        cavern_room_radius_xz_max: 34.0,
        cavern_room_radius_y_min: 9.0,
        cavern_room_radius_y_max: 21.0,
        cavern_connector_radius: 12.5,
        cavern_y_top: -10,
        cavern_y_bottom: -100,
        mega_caverns_per_region: 0.075,
        mega_room_count_min: 1,
        mega_room_count_max: 3,
        mega_room_radius_xz_min: 45.0,
        mega_room_radius_xz_max: 144.0,
        mega_room_radius_y_min: 20.0,
        mega_room_radius_y_max: 46.0,
        mega_connector_radius: 8.0,
        mega_y_top: -30,
        mega_y_bottom: -105,
        entrance_chance: 0.55,
        entrance_len_steps: 40,
        entrance_radius_scale: 0.55,
        entrance_min_radius: 2.8,
        entrance_trigger_band: 12.0,
    }
}

/// Collects block overrides for one chunk into local coordinates.
fn collect_chunk_overrides_snapshot(
    overrides: &HashMap<[i32; 3], u16>,
    coord: IVec2,
) -> Vec<(usize, usize, usize, u16)> {
    let mut snapshot = Vec::new();
    for (location, block_id) in overrides {
        let world_y = location[1];
        if !(Y_MIN..=Y_MAX).contains(&world_y) {
            continue;
        }

        let (override_coord, local) = world_to_chunk_xz(location[0], location[2]);
        if override_coord != coord {
            continue;
        }

        snapshot.push((
            local.x as usize,
            world_y_to_local(world_y),
            local.y as usize,
            *block_id,
        ));
    }
    snapshot
}

/// Applies a chunk-local override snapshot to chunk data.
fn apply_chunk_overrides_snapshot(overrides: &[(usize, usize, usize, u16)], chunk: &mut ChunkData) {
    for (lx, ly, lz, block_id) in overrides {
        chunk.set(*lx, *ly, *lz, *block_id);
    }
}

/// Loads block overrides for the `state` module.
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
