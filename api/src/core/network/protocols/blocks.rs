use serde::{Deserialize, Serialize};

/// Represents client block break used by the `core::network::protocols::blocks` module.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientBlockBreak {
    pub location: [i32; 3],
    pub drop_block_id: u16,
    pub drop_id: u64,
}

impl ClientBlockBreak {
    /// Creates a new instance for the `core::network::protocols::blocks` module.
    pub fn new(location: [i32; 3], drop_block_id: u16, drop_id: u64) -> Self {
        Self {
            location,
            drop_block_id,
            drop_id,
        }
    }
}

/// Represents client block place used by the `core::network::protocols::blocks` module.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientBlockPlace {
    pub location: [i32; 3],
    pub block_id: u16,
    pub stacked_block_id: u16,
}

impl ClientBlockPlace {
    /// Creates a new instance for the `core::network::protocols::blocks` module.
    pub fn new(location: [i32; 3], block_id: u16, stacked_block_id: u16) -> Self {
        Self {
            location,
            block_id,
            stacked_block_id,
        }
    }
}

/// Represents server block break used by the `core::network::protocols::blocks` module.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerBlockBreak {
    pub player_id: u64,
    pub location: [i32; 3],
}

impl ServerBlockBreak {
    /// Creates a new instance for the `core::network::protocols::blocks` module.
    pub fn new(player_id: u64, location: [i32; 3]) -> Self {
        Self {
            player_id,
            location,
        }
    }
}

/// Represents server block place used by the `core::network::protocols::blocks` module.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerBlockPlace {
    pub player_id: u64,
    pub location: [i32; 3],
    pub block_id: u16,
    pub stacked_block_id: u16,
}

impl ServerBlockPlace {
    /// Creates a new instance for the `core::network::protocols::blocks` module.
    pub fn new(player_id: u64, location: [i32; 3], block_id: u16, stacked_block_id: u16) -> Self {
        Self {
            player_id,
            location,
            block_id,
            stacked_block_id,
        }
    }
}
