use oplexa_core::world::save::StructureRegionEntry;
use serde::{Deserialize, Serialize};

/// Represents client chunk interest used by the `core::network::protocols::chunks` module.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientChunkInterest {
    pub center: [i32; 2],
    pub radius: i32,
}

impl ClientChunkInterest {
    /// Creates a new instance for the `core::network::protocols::chunks` module.
    pub fn new(center: [i32; 2], radius: i32) -> Self {
        Self { center, radius }
    }
}

/// Represents server chunk data used by the `core::network::protocols::chunks` module.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerChunkData {
    pub coord: [i32; 2],
    pub blocks: Vec<u8>,
    pub structures: Vec<StructureRegionEntry>,
}

impl ServerChunkData {
    /// Creates a new instance for the `core::network::protocols::chunks` module.
    pub fn new(coord: [i32; 2], blocks: Vec<u8>, structures: Vec<StructureRegionEntry>) -> Self {
        Self {
            coord,
            blocks,
            structures,
        }
    }
}
