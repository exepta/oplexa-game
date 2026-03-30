use naia_shared::Message;

#[derive(Message)]
pub struct ClientChunkInterest {
    pub center: [i32; 2],
    pub radius: i32,
}

impl ClientChunkInterest {
    pub fn new(center: [i32; 2], radius: i32) -> Self {
        Self { center, radius }
    }
}

#[derive(Message)]
pub struct ServerChunkData {
    pub coord: [i32; 2],
    pub blocks: Vec<u8>,
}

impl ServerChunkData {
    pub fn new(coord: [i32; 2], blocks: Vec<u8>) -> Self {
        Self { coord, blocks }
    }
}
