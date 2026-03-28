use std::time::{Duration, Instant};

pub const PLAYER_STALE_TIMEOUT: Duration = Duration::from_secs(8);

pub struct HostedPlayer {
    pub player_id: u64,
    pub username: String,
    pub translation: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub last_seen: Instant,
}

pub struct HostedDrop {
    pub drop_id: u64,
    pub location: [i32; 3],
    pub block_id: u16,
    pub has_motion: bool,
    pub spawn_translation: [f32; 3],
    pub initial_velocity: [f32; 3],
}
