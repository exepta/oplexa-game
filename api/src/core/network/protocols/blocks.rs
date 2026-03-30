use naia_shared::Message;

#[derive(Message)]
pub struct ClientBlockBreak {
    pub location: [i32; 3],
    pub drop_block_id: u16,
    pub drop_id: u64,
}

impl ClientBlockBreak {
    pub fn new(location: [i32; 3], drop_block_id: u16, drop_id: u64) -> Self {
        Self {
            location,
            drop_block_id,
            drop_id,
        }
    }
}

#[derive(Message)]
pub struct ClientBlockPlace {
    pub location: [i32; 3],
    pub block_id: u16,
}

impl ClientBlockPlace {
    pub fn new(location: [i32; 3], block_id: u16) -> Self {
        Self { location, block_id }
    }
}

#[derive(Message)]
pub struct ServerBlockBreak {
    pub player_id: u64,
    pub location: [i32; 3],
}

impl ServerBlockBreak {
    pub fn new(player_id: u64, location: [i32; 3]) -> Self {
        Self {
            player_id,
            location,
        }
    }
}

#[derive(Message)]
pub struct ServerBlockPlace {
    pub player_id: u64,
    pub location: [i32; 3],
    pub block_id: u16,
}

impl ServerBlockPlace {
    pub fn new(player_id: u64, location: [i32; 3], block_id: u16) -> Self {
        Self {
            player_id,
            location,
            block_id,
        }
    }
}
