use bevy_math::IVec2;
use std::collections::HashSet;
use std::time::Instant;

pub struct HostedPlayer {
    pub player_id: u64,
    pub username: String,
    pub translation: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub last_seen: Instant,
    pub streamed_chunks: HashSet<IVec2>,
}

pub struct HostedDrop {
    pub drop_id: u64,
    pub location: [i32; 3],
    pub block_id: u16,
    pub has_motion: bool,
    pub spawn_translation: [f32; 3],
    pub initial_velocity: [f32; 3],
}
