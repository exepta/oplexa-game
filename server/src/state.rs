use crate::models::{HostedDrop, HostedPlayer};
use api::{
    core::{
        config::WorldGenConfig,
        entities::player::inventory::{
            InventorySlot, PLAYER_INVENTORY_SLOTS, PLAYER_INVENTORY_STACK_MAX, PlayerInventory,
        },
        inventory::{items::ItemRegistry, recipe::load_building_structure_recipe_registry},
        world::{
            biome::registry::BiomeRegistry,
            block::{BlockId, BlockRegistry},
            chunk::{ChunkData, SEA_LEVEL},
            chunk_dimension::{CX, CY, CZ, Y_MAX, Y_MIN, world_to_chunk_xz, world_y_to_local},
        },
    },
    generator::chunk::{
        chunk_utils::{encode_chunk, load_or_gen_chunk_async_with_origin, save_chunk_at_root_sync},
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
const STREAM_GEN_SPAWN_BUDGET_BASE_PER_TICK: usize = 11;
const STREAM_GEN_SPAWN_BUDGET_PER_CLIENT: usize = 5;
const STREAM_GEN_MAX_BURST_PER_TICK: usize = 44;
const STREAM_GEN_MIN_INFLIGHT_WHEN_CONNECTED: usize = 22;
const PLAYER_SAVE_FILE_NAME: &str = "save.data";
const LEGACY_PLAYER_SAVE_PREFIX: &str = "save-";
const LEGACY_PLAYER_SAVE_SUFFIX: &str = ".data";
const BLOCK_OVERRIDES_FILE_NAME: &str = "blocks.bin";
const LEGACY_BLOCK_OVERRIDES_FILE_NAME: &str = "blocks.txt";
const BLOCK_OVERRIDES_MAGIC: [u8; 4] = *b"BOV1";
const PLAYER_SAVE_MAGIC: [u8; 4] = *b"PINV";
const PLAYER_SAVE_VERSION_LEGACY: u8 = 1;
const PLAYER_SAVE_VERSION_POSITION: u8 = 2;
const PLAYER_SAVE_VERSION: u8 = 3;
const PLAYER_SAVE_FLAG_HAS_POSITION: u8 = 0x01;
const PLAYER_SAVE_FLAG_HAS_YAW_PITCH: u8 = 0x02;

/// Player save payload persisted in `world/data/<uuid>/save.data`.
#[derive(Clone)]
pub struct PlayerPersistedData {
    pub translation: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub inventory_slots: [InventorySlot; PLAYER_INVENTORY_SLOTS],
}

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
    pub item_registry: Arc<ItemRegistry>,
    pub biome_registry: Arc<BiomeRegistry>,
    pub tree_registry: Arc<TreeRegistry>,
    pub world_gen_config: WorldGenConfig,
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
        let mut block_registry = BlockRegistry::load_headless("assets/blocks");
        let item_registry = ItemRegistry::load_headless("assets/items", &block_registry);
        let mut structure_recipe_registry =
            load_building_structure_recipe_registry("assets/recipes/structures", &item_registry);
        for recipe in &mut structure_recipe_registry.recipes {
            let Some(registration) = recipe.model_meta.block_registration.as_mut() else {
                continue;
            };
            let block_id = block_registry.ensure_runtime_block_headless(
                registration.localized_name.as_str(),
                registration.name.as_str(),
                recipe.model_meta.stats.clone(),
            );
            for rotation_quarters in 1..4u8 {
                let localized_name =
                    format!("{}_r{}", registration.localized_name, rotation_quarters);
                let name_key = format!("{}_R{}", registration.name, rotation_quarters);
                let _ = block_registry.ensure_runtime_block_headless(
                    localized_name.as_str(),
                    name_key.as_str(),
                    recipe.model_meta.stats.clone(),
                );
            }
            registration.block_id = Some(block_id);
        }

        let block_registry = Arc::new(block_registry);
        let item_registry = Arc::new(item_registry);
        let biome_registry = Arc::new(BiomeRegistry::load_from_folder("assets/biomes"));
        let tree_registry = Arc::new(TreeRegistry::load_from_folder("assets/data/trees"));
        let world_gen_config = WorldGenConfig { seed: world_seed };
        AsyncComputeTaskPool::get_or_init(TaskPool::default);
        let mut state = Self {
            world_root,
            block_registry,
            item_registry,
            biome_registry,
            tree_registry,
            world_gen_config,
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
        };
        state.migrate_legacy_block_overrides_to_regions();
        state
    }

    /// Persists one world-space block into the chunk region data.
    pub fn set_block_persisted(&mut self, location: [i32; 3], block_id: u16) -> bool {
        self.set_block_persisted_with_stacked(location, block_id, 0)
    }

    /// Persists one world-space block + optional stacked block into the chunk region data.
    pub fn set_block_persisted_with_stacked(
        &mut self,
        location: [i32; 3],
        block_id: u16,
        stacked_block_id: u16,
    ) -> bool {
        let world_y = location[1];
        if !(Y_MIN..=Y_MAX).contains(&world_y) {
            return false;
        }

        let (coord, local) = world_to_chunk_xz(location[0], location[2]);
        let ly = world_y_to_local(world_y);
        let edits = [(
            local.x as usize,
            ly,
            local.y as usize,
            block_id,
            stacked_block_id,
        )];

        let refill_ocean_water = block_id == 0;
        if let Err(error) = self.persist_chunk_edits(coord, &edits, refill_ocean_water) {
            log::warn!(
                "Failed to persist block edit at {:?} (id={}, stacked={}): {}",
                location,
                block_id,
                stacked_block_id,
                error
            );
            return false;
        }

        self.invalidate_streamed_chunk(coord);
        true
    }

    fn persist_chunk_edits(
        &self,
        coord: IVec2,
        edits: &[(usize, usize, usize, u16, u16)],
        refill_ocean_water: bool,
    ) -> std::io::Result<()> {
        let water_id = self.block_registry.id_opt("water_block").unwrap_or(0);

        let (mut chunk, _generated) = future::block_on(load_or_gen_chunk_async_with_origin(
            self.world_root.clone(),
            coord,
            &self.block_registry,
            &self.biome_registry,
            &self.tree_registry,
            self.world_gen_config.clone(),
        ));

        for (lx, ly, lz, block_id, stacked_block_id) in edits {
            chunk.set(*lx, *ly, *lz, *block_id);
            chunk.set_stacked(*lx, *ly, *lz, *stacked_block_id);
        }
        if refill_ocean_water && water_id != 0 {
            flood_ocean_connected_water(&mut chunk, SEA_LEVEL, water_id);
        }
        chunk.mark_all_dirty();
        save_chunk_at_root_sync(self.world_root.clone(), coord, &chunk)
    }

    fn migrate_legacy_block_overrides_to_regions(&mut self) {
        let binary_path = self.world_root.join(BLOCK_OVERRIDES_FILE_NAME);
        let legacy_path = self.world_root.join(LEGACY_BLOCK_OVERRIDES_FILE_NAME);
        let overrides = read_block_overrides_binary(binary_path.as_path())
            .unwrap_or_else(|| read_legacy_block_overrides_text(legacy_path.as_path()));
        if overrides.is_empty() {
            return;
        }

        let mut edits_by_chunk: HashMap<IVec2, Vec<(usize, usize, usize, u16, u16)>> =
            HashMap::new();
        for (location, block_id) in overrides {
            let world_y = location[1];
            if !(Y_MIN..=Y_MAX).contains(&world_y) {
                continue;
            }

            let (coord, local) = world_to_chunk_xz(location[0], location[2]);
            edits_by_chunk.entry(coord).or_default().push((
                local.x as usize,
                world_y_to_local(world_y),
                local.y as usize,
                block_id,
                0,
            ));
        }

        let mut failed = 0usize;
        for (coord, edits) in edits_by_chunk {
            if let Err(error) = self.persist_chunk_edits(coord, edits.as_slice(), false) {
                failed += 1;
                log::warn!(
                    "Failed migrating legacy block overrides for chunk {:?}: {}",
                    coord,
                    error
                );
            }
        }

        if failed == 0 {
            let _ = fs::remove_file(binary_path.as_path());
            let _ = fs::remove_file(legacy_path.as_path());
            log::info!("Migrated legacy block overrides into region files.");
        } else {
            log::warn!(
                "Legacy block override migration incomplete ({} chunk write failure(s)); keeping source files.",
                failed
            );
        }
    }

    /// Loads persisted player data for the `state` module.
    pub fn load_player_data(&self, client_uuid: &str) -> Option<PlayerPersistedData> {
        let path = self.player_save_path(client_uuid);
        if let Some(data) = read_player_data_from_file(path.as_path()) {
            return Some(data);
        }

        // Migration path from legacy `<world>/data/save-<uuid>.data` format.
        let legacy_path = self.legacy_player_save_path(client_uuid);
        let data = read_player_data_from_file(legacy_path.as_path())?;
        self.persist_player_data(
            client_uuid,
            data.translation,
            data.yaw,
            data.pitch,
            &data.inventory_slots,
        );
        let _ = fs::remove_file(legacy_path);
        Some(data)
    }

    /// Persists player data for the `state` module.
    pub fn persist_player_data(
        &self,
        client_uuid: &str,
        translation: [f32; 3],
        yaw: f32,
        pitch: f32,
        inventory_slots: &[InventorySlot; PLAYER_INVENTORY_SLOTS],
    ) {
        let path = self.player_save_path(client_uuid);
        let Some(parent) = path.parent() else {
            return;
        };

        if let Err(error) = fs::create_dir_all(parent) {
            log::warn!(
                "Failed to prepare player save folder for uuid='{}': {}",
                client_uuid,
                error
            );
            return;
        }

        let tmp_path = parent.join("save.data.tmp");
        let payload = encode_player_blob(translation, yaw, pitch, inventory_slots);
        if let Err(error) = fs::write(&tmp_path, payload) {
            log::warn!(
                "Failed to write temporary player save for uuid='{}': {}",
                client_uuid,
                error
            );
            return;
        }

        if let Err(error) = fs::rename(&tmp_path, &path) {
            log::warn!(
                "Failed to persist player save for uuid='{}': {}",
                client_uuid,
                error
            );
            return;
        }

        // Best-effort cleanup from legacy one-file naming.
        let _ = fs::remove_file(self.legacy_player_save_path(client_uuid));
    }

    /// Returns player save path for the `state` module.
    fn player_save_path(&self, client_uuid: &str) -> PathBuf {
        self.world_root
            .join("data")
            .join(sanitize_player_path_segment(client_uuid))
            .join(PLAYER_SAVE_FILE_NAME)
    }

    /// Returns legacy player save path for the `state` module.
    fn legacy_player_save_path(&self, client_uuid: &str) -> PathBuf {
        self.world_root.join("data").join(format!(
            "{LEGACY_PLAYER_SAVE_PREFIX}{}{LEGACY_PLAYER_SAVE_SUFFIX}",
            sanitize_player_path_segment(client_uuid)
        ))
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
    pub fn pump_stream_chunk_tasks(
        &mut self,
        config: &ServerRuntimeConfig,
        connected_clients: usize,
    ) {
        if connected_clients == 0 {
            return;
        }

        let configured_max_inflight = config.chunk_stream_gen_max_inflight.max(1);
        let dynamic_inflight_target = config
            .chunk_stream_inflight_per_client
            .max(1)
            .saturating_mul(connected_clients)
            .saturating_add(STREAM_GEN_MIN_INFLIGHT_WHEN_CONNECTED)
            .clamp(
                STREAM_GEN_MIN_INFLIGHT_WHEN_CONNECTED,
                configured_max_inflight,
            );
        let max_inflight = configured_max_inflight.min(dynamic_inflight_target).max(1);
        if self.pending_stream_chunk_tasks.len() >= max_inflight {
            return;
        }

        let mut spawn_budget = STREAM_GEN_SPAWN_BUDGET_BASE_PER_TICK
            .saturating_add(
                STREAM_GEN_SPAWN_BUDGET_PER_CLIENT.saturating_mul(connected_clients.max(1)),
            )
            .min(STREAM_GEN_MAX_BURST_PER_TICK);
        spawn_budget =
            spawn_budget.min(max_inflight.saturating_sub(self.pending_stream_chunk_tasks.len()));
        if spawn_budget == 0 {
            return;
        }

        let world_gen_config = self.world_gen_config.clone();
        let pool = AsyncComputeTaskPool::get();
        let mut spawned = 0usize;
        let mut scanned = 0usize;
        let scan_cap = self.pending_stream_chunk_queue.len().max(spawn_budget);

        while self.pending_stream_chunk_tasks.len() < max_inflight
            && spawned < spawn_budget
            && scanned < scan_cap
        {
            scanned += 1;
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
            let cfg = world_gen_config.clone();

            let task = pool.spawn(async move {
                let (chunk, _generated) = load_or_gen_chunk_async_with_origin(
                    world_root.clone(),
                    coord,
                    &block_registry,
                    &biome_registry,
                    &tree_registry,
                    cfg,
                )
                .await;

                (coord, encode_chunk(&chunk))
            });

            self.pending_stream_chunk_tasks.insert(coord, task);
            spawned += 1;
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

/// Runs the `flood_ocean_connected_water` routine for flood ocean connected water in the `state` module.
fn flood_ocean_connected_water(chunk: &mut ChunkData, sea_level: i32, water_id: BlockId) -> bool {
    if water_id == 0 {
        return false;
    }

    let sea_level = sea_level.clamp(Y_MIN, Y_MAX);
    let sea_ly = world_y_to_local(sea_level);
    let mut queue: VecDeque<(usize, usize, usize)> = VecDeque::new();
    let mut seen = vec![false; CX * CY * CZ];
    let mut changed = false;

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
                try_push_ocean_seed(
                    chunk,
                    &mut seen,
                    water_id,
                    x,
                    ly,
                    z,
                    &mut queue,
                    &mut changed,
                );
            }
        }
    }

    while let Some((x, y, z)) = queue.pop_front() {
        if y + 1 <= sea_ly {
            try_push_ocean_seed(
                chunk,
                &mut seen,
                water_id,
                x,
                y + 1,
                z,
                &mut queue,
                &mut changed,
            );
        }
        if y > 0 {
            try_push_ocean_seed(
                chunk,
                &mut seen,
                water_id,
                x,
                y - 1,
                z,
                &mut queue,
                &mut changed,
            );
        }
        if x + 1 < CX {
            try_push_ocean_seed(
                chunk,
                &mut seen,
                water_id,
                x + 1,
                y,
                z,
                &mut queue,
                &mut changed,
            );
        }
        if x > 0 {
            try_push_ocean_seed(
                chunk,
                &mut seen,
                water_id,
                x - 1,
                y,
                z,
                &mut queue,
                &mut changed,
            );
        }
        if z + 1 < CZ {
            try_push_ocean_seed(
                chunk,
                &mut seen,
                water_id,
                x,
                y,
                z + 1,
                &mut queue,
                &mut changed,
            );
        }
        if z > 0 {
            try_push_ocean_seed(
                chunk,
                &mut seen,
                water_id,
                x,
                y,
                z - 1,
                &mut queue,
                &mut changed,
            );
        }
    }

    changed
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
    changed: &mut bool,
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
        *changed = true;
    }
    queue.push_back((x, y, z));
}

fn read_block_overrides_binary(path: &Path) -> Option<HashMap<[i32; 3], u16>> {
    let bytes = fs::read(path).ok()?;
    if bytes.len() < 8 || bytes[0..4] != BLOCK_OVERRIDES_MAGIC {
        return None;
    }

    let count = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    let entry_size = 14usize;
    let expected = 8usize.saturating_add(count.saturating_mul(entry_size));
    if bytes.len() < expected {
        return None;
    }

    let mut overrides = HashMap::with_capacity(count);
    let mut offset = 8usize;
    for _ in 0..count {
        let x = i32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]);
        let y = i32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]);
        let z = i32::from_le_bytes([
            bytes[offset + 8],
            bytes[offset + 9],
            bytes[offset + 10],
            bytes[offset + 11],
        ]);
        let block_id = u16::from_le_bytes([bytes[offset + 12], bytes[offset + 13]]);
        overrides.insert([x, y, z], block_id);
        offset += entry_size;
    }
    Some(overrides)
}

fn read_legacy_block_overrides_text(path: &Path) -> HashMap<[i32; 3], u16> {
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

fn read_player_data_from_file(path: &Path) -> Option<PlayerPersistedData> {
    let bytes = fs::read(path).ok()?;
    if let Ok(decoded) = decode_player_blob(&bytes) {
        return Some(decoded);
    }

    let text = std::str::from_utf8(&bytes).ok()?;
    let (translation, yaw, pitch) = parse_legacy_player_pose_text(text)?;
    Some(PlayerPersistedData {
        translation,
        yaw,
        pitch,
        inventory_slots: [InventorySlot::default(); PLAYER_INVENTORY_SLOTS],
    })
}

fn parse_legacy_player_pose_text(text: &str) -> Option<([f32; 3], f32, f32)> {
    let mut parts = text.split_whitespace();
    let x = parts.next()?.parse::<f32>().ok()?;
    let y = parts.next()?.parse::<f32>().ok()?;
    let z = parts.next()?.parse::<f32>().ok()?;
    let yaw = parts
        .next()
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(0.0);
    let pitch = parts
        .next()
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(0.0);
    Some(([x, y, z], yaw, pitch))
}

fn encode_player_blob(
    translation: [f32; 3],
    yaw: f32,
    pitch: f32,
    inventory_slots: &[InventorySlot; PLAYER_INVENTORY_SLOTS],
) -> Vec<u8> {
    let inventory = PlayerInventory {
        slots: *inventory_slots,
    };
    encode_inventory_blob(&inventory, Some(translation), Some([yaw, pitch]))
}

fn encode_inventory_blob(
    inventory: &PlayerInventory,
    position: Option<[f32; 3]>,
    yaw_pitch: Option<[f32; 2]>,
) -> Vec<u8> {
    let has_position = position.is_some();
    let has_yaw_pitch = yaw_pitch.is_some();
    let mut out = Vec::with_capacity(8 + PLAYER_INVENTORY_SLOTS * 4 + 20);
    out.extend_from_slice(&PLAYER_SAVE_MAGIC);
    out.push(PLAYER_SAVE_VERSION);
    out.extend_from_slice(&(PLAYER_INVENTORY_SLOTS as u16).to_le_bytes());
    let mut flags = 0u8;
    if has_position {
        flags |= PLAYER_SAVE_FLAG_HAS_POSITION;
    }
    if has_yaw_pitch {
        flags |= PLAYER_SAVE_FLAG_HAS_YAW_PITCH;
    }
    out.push(flags);

    for slot in inventory.slots {
        out.extend_from_slice(&slot.item_id.to_le_bytes());
        out.extend_from_slice(&slot.count.to_le_bytes());
    }

    if let Some([x, y, z]) = position {
        out.extend_from_slice(&x.to_le_bytes());
        out.extend_from_slice(&y.to_le_bytes());
        out.extend_from_slice(&z.to_le_bytes());
    }
    if let Some([saved_yaw, saved_pitch]) = yaw_pitch {
        out.extend_from_slice(&saved_yaw.to_le_bytes());
        out.extend_from_slice(&saved_pitch.to_le_bytes());
    }

    out
}

fn decode_player_blob(blob: &[u8]) -> Result<PlayerPersistedData, &'static str> {
    if blob.len() < 7 {
        return Err("file too small");
    }

    if blob[0..4] != PLAYER_SAVE_MAGIC {
        return Err("magic mismatch");
    }

    let version = blob[4];
    if version != PLAYER_SAVE_VERSION
        && version != PLAYER_SAVE_VERSION_POSITION
        && version != PLAYER_SAVE_VERSION_LEGACY
    {
        return Err("unsupported version");
    }

    let slot_count = u16::from_le_bytes([blob[5], blob[6]]) as usize;
    let header_len = if version == PLAYER_SAVE_VERSION || version == PLAYER_SAVE_VERSION_POSITION {
        8usize
    } else {
        7usize
    };
    let expected_len = header_len + slot_count.saturating_mul(4);
    if blob.len() < expected_len {
        return Err("truncated payload");
    }

    let mut inventory = PlayerInventory::default();
    let copy_count = slot_count.min(PLAYER_INVENTORY_SLOTS);
    let mut offset = header_len;
    for index in 0..copy_count {
        let item_id = u16::from_le_bytes([blob[offset], blob[offset + 1]]);
        let count = u16::from_le_bytes([blob[offset + 2], blob[offset + 3]]);
        offset += 4;
        if item_id == 0 || count == 0 {
            inventory.slots[index] = InventorySlot::default();
            continue;
        }
        inventory.slots[index] = InventorySlot {
            item_id,
            count: count.min(PLAYER_INVENTORY_STACK_MAX),
        };
    }

    let mut translation = [0.0, 0.0, 0.0];
    let mut yaw = 0.0;
    let mut pitch = 0.0;
    if version == PLAYER_SAVE_VERSION || version == PLAYER_SAVE_VERSION_POSITION {
        let flags = blob[7];
        if (flags & PLAYER_SAVE_FLAG_HAS_POSITION) != 0 {
            if blob.len() < offset + 12 {
                return Err("truncated player position");
            }
            translation = [
                f32::from_le_bytes([
                    blob[offset],
                    blob[offset + 1],
                    blob[offset + 2],
                    blob[offset + 3],
                ]),
                f32::from_le_bytes([
                    blob[offset + 4],
                    blob[offset + 5],
                    blob[offset + 6],
                    blob[offset + 7],
                ]),
                f32::from_le_bytes([
                    blob[offset + 8],
                    blob[offset + 9],
                    blob[offset + 10],
                    blob[offset + 11],
                ]),
            ];
            offset += 12;
        }
        if (flags & PLAYER_SAVE_FLAG_HAS_YAW_PITCH) != 0 {
            if blob.len() < offset + 8 {
                return Err("truncated yaw/pitch");
            }
            yaw = f32::from_le_bytes([
                blob[offset],
                blob[offset + 1],
                blob[offset + 2],
                blob[offset + 3],
            ]);
            pitch = f32::from_le_bytes([
                blob[offset + 4],
                blob[offset + 5],
                blob[offset + 6],
                blob[offset + 7],
            ]);
        }
    }

    Ok(PlayerPersistedData {
        translation,
        yaw,
        pitch,
        inventory_slots: inventory.slots,
    })
}

/// Sanitizes path segment for player file storage in the `state` module.
fn sanitize_player_path_segment(raw: &str) -> String {
    let sanitized = raw
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();

    if sanitized.is_empty() {
        "unknown".to_string()
    } else {
        sanitized
    }
}
