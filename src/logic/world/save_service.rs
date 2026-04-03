use crate::core::events::chunk_events::SubChunkNeedRemeshEvent;
use crate::core::multiplayer::MultiplayerConnectionState;
use crate::core::states::states::{AppState, InGameStates};
use crate::core::world::chunk::ChunkMap;
use crate::core::world::save::*;
use crate::generator::chunk::chunk_utils::save_chunk_sync;
use bevy::prelude::*;
use std::collections::{HashMap, VecDeque};

/// Represents world save service used by the `logic::world::save_service` module.
pub struct WorldSaveService;

/// Represents save queue used by the `logic::world::save_service` module.
#[derive(Resource, Default)]
struct SaveQueue(VecDeque<IVec2>);

/// Represents save debounce used by the `logic::world::save_service` module.
#[derive(Resource, Default)]
struct SaveDebounce(HashMap<IVec2, Timer>);

const SAVE_DEBOUNCE_MS: u64 = 250;

impl Plugin for WorldSaveService {
    /// Builds this component for the `logic::world::save_service` module.
    fn build(&self, app: &mut App) {
        app.init_resource::<RegionCache>()
            .init_resource::<SaveDebounce>()
            .init_resource::<SaveQueue>();
        app.add_systems(OnEnter(AppState::Preload), setup_world_save);
        app.add_systems(
            Update,
            (enqueue_save_on_dirty, tick_save_debounce, drain_save_queue)
                .run_if(in_state(AppState::InGame(InGameStates::Game))),
        )
        .add_systems(
            OnExit(AppState::InGame(InGameStates::Game)),
            cleanup_world_save_runtime,
        );
    }
}

/// Runs the `setup_world_save` routine for setup world save in the `logic::world::save_service` module.
fn setup_world_save(mut commands: Commands) {
    let world_root = default_saves_root().join("world");
    commands.insert_resource(WorldSave { root: world_root });
}

/// Runs the `enqueue_save_on_dirty` routine for enqueue save on dirty in the `logic::world::save_service` module.
fn enqueue_save_on_dirty(
    mut ev_dirty: MessageReader<SubChunkNeedRemeshEvent>,
    mut deb: ResMut<SaveDebounce>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
) {
    if !multiplayer_connection.uses_local_save_data() {
        for _ in ev_dirty.read() {}
        return;
    }

    for e in ev_dirty.read().copied() {
        deb.0
            .entry(e.coord)
            .and_modify(|t| {
                t.reset();
            })
            .or_insert_with(|| {
                let mut t = Timer::from_seconds(SAVE_DEBOUNCE_MS as f32 / 1000.0, TimerMode::Once);
                t.reset();
                t
            });
    }
}

/// Runs the `tick_save_debounce` routine for tick save debounce in the `logic::world::save_service` module.
fn tick_save_debounce(
    time: Res<Time>,
    mut deb: ResMut<SaveDebounce>,
    mut queue: ResMut<SaveQueue>,
) {
    let mut to_queue = Vec::new();

    for (coord, timer) in deb.0.iter_mut() {
        timer.tick(time.delta());
        if timer.is_finished() {
            to_queue.push(*coord);
        }
    }
    for coord in to_queue {
        deb.0.remove(&coord);
        if !queue.0.iter().any(|&c| c == coord) {
            queue.0.push_back(coord);
        }
    }
}

/// Runs the `drain_save_queue` routine for drain save queue in the `logic::world::save_service` module.
fn drain_save_queue(
    ws: Res<WorldSave>,
    mut cache: ResMut<RegionCache>,
    chunk_map: Res<ChunkMap>,
    mut queue: ResMut<SaveQueue>,
    multiplayer_connection: Res<MultiplayerConnectionState>,
) {
    if !multiplayer_connection.uses_local_save_data() {
        queue.0.clear();
        return;
    }

    while let Some(coord) = queue.0.pop_front() {
        if let Some(chunk) = chunk_map.chunks.get(&coord) {
            let _ = save_chunk_sync(&ws, &mut cache, coord, chunk);
        }
    }
}

/// Runs the `cleanup_world_save_runtime` routine for cleanup world save runtime in the `logic::world::save_service` module.
fn cleanup_world_save_runtime(
    mut cache: ResMut<RegionCache>,
    mut debounce: ResMut<SaveDebounce>,
    mut queue: ResMut<SaveQueue>,
) {
    cache.0.clear();
    debounce.0.clear();
    queue.0.clear();
}
