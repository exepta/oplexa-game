use naia_shared::Message;

#[derive(Message)]
pub struct ServerDropSpawn {
    pub drop_id: u64,
    pub location: [i32; 3],
    pub block_id: u16,
    pub has_motion: bool,
    pub spawn_translation: [f32; 3],
    pub initial_velocity: [f32; 3],
}

impl ServerDropSpawn {
    pub fn new(
        drop_id: u64,
        location: [i32; 3],
        block_id: u16,
        has_motion: bool,
        spawn_translation: [f32; 3],
        initial_velocity: [f32; 3],
    ) -> Self {
        Self {
            drop_id,
            location,
            block_id,
            has_motion,
            spawn_translation,
            initial_velocity,
        }
    }
}

#[derive(Message)]
pub struct ClientDropPickup {
    pub drop_id: u64,
}

impl ClientDropPickup {
    pub fn new(drop_id: u64) -> Self {
        Self { drop_id }
    }
}

#[derive(Message)]
pub struct ClientDropItem {
    pub location: [i32; 3],
    pub block_id: u16,
    pub amount: u16,
    pub spawn_translation: [f32; 3],
    pub initial_velocity: [f32; 3],
}

impl ClientDropItem {
    pub fn new(
        location: [i32; 3],
        block_id: u16,
        amount: u16,
        spawn_translation: [f32; 3],
        initial_velocity: [f32; 3],
    ) -> Self {
        Self {
            location,
            block_id,
            amount,
            spawn_translation,
            initial_velocity,
        }
    }
}

#[derive(Message)]
pub struct ServerDropPicked {
    pub drop_id: u64,
    pub player_id: u64,
    pub block_id: u16,
}

impl ServerDropPicked {
    pub fn new(drop_id: u64, player_id: u64, block_id: u16) -> Self {
        Self {
            drop_id,
            player_id,
            block_id,
        }
    }
}
