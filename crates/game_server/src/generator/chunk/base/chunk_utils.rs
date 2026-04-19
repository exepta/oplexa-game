use crate::generator::chunk::chunk_gen::generate_chunk_async_biome;
use crate::generator::chunk::trees::registry::TreeRegistry;
use bevy::math::IVec2;
use lz4_flex::{compress_prepend_size, decompress_size_prepended};
use oplexa_core::world::biome::registry::BiomeRegistry;
use oplexa_core::world::block::{BlockId, BlockRegistry};
use oplexa_core::world::chunk::ChunkData;
use oplexa_core::world::chunk_dimension::{CX, CY, CZ, SEC_COUNT};
use oplexa_core::world::save::{
    RegionCache, RegionFile, TAG_BLK1, WorldSave, chunk_to_region, container_find,
    container_upsert, region_slot_index, slot_is_container, world_save_io_guard,
};
use oplexa_shared::config::WorldGenConfig;
use std::path::{Path, PathBuf};

#[inline]
pub fn map01(v: f32) -> f32 {
    (v + 1.0) * 0.5
}

#[allow(dead_code)]
pub fn save_chunk_sync(
    ws: &WorldSave,
    cache: &mut RegionCache,
    coord: IVec2,
    ch: &ChunkData,
) -> std::io::Result<()> {
    let _guard = world_save_io_guard();
    let blocks = encode_chunk(ch);
    let old = cache.read_chunk(ws, coord).ok().flatten();
    let merged = container_upsert(old.as_deref(), TAG_BLK1, &blocks);
    cache.write_chunk_replace(ws, coord, &merged)
}

pub fn save_chunk_at_root_sync(
    ws_root: PathBuf,
    coord: IVec2,
    ch: &ChunkData,
) -> std::io::Result<()> {
    let _guard = world_save_io_guard();
    let blocks = encode_chunk(ch);
    let rc = chunk_to_region(coord);
    let path = ws_root
        .join("region")
        .join(format!("r.{}.{}.region", rc.x, rc.y));
    let mut rf = RegionFile::open(&path)?;
    let old = rf.read_chunk(coord).ok().flatten();
    let merged = container_upsert(old.as_deref(), TAG_BLK1, &blocks);
    let idx = region_slot_index(coord);
    rf.write_slot_replace(idx, &merged)
}

pub async fn load_or_gen_chunk_async(
    ws_root: PathBuf,
    coord: IVec2,
    reg: &BlockRegistry,
    biomes: &BiomeRegistry,
    trees: &TreeRegistry,
    cfg: WorldGenConfig,
) -> ChunkData {
    load_or_gen_chunk_async_with_origin(ws_root, coord, reg, biomes, trees, cfg)
        .await
        .0
}

pub fn load_chunk_at_root_sync(ws_root: &Path, coord: IVec2) -> Option<ChunkData> {
    let r_coord = chunk_to_region(coord);
    let path = ws_root
        .join("region")
        .join(format!("r.{}.{}.region", r_coord.x, r_coord.y));
    let _guard = world_save_io_guard();
    let Ok(mut rf) = RegionFile::open(&path) else {
        return None;
    };
    let Ok(Some(buf)) = rf.read_chunk(coord) else {
        return None;
    };

    let data = if slot_is_container(&buf) {
        container_find(&buf, TAG_BLK1).map(|b| b.to_vec())
    } else {
        Some(buf)
    }?;

    decode_chunk(&data).ok()
}

pub async fn load_or_gen_chunk_async_with_origin(
    ws_root: PathBuf,
    coord: IVec2,
    reg: &BlockRegistry,
    biomes: &BiomeRegistry,
    trees: &TreeRegistry,
    cfg: WorldGenConfig,
) -> (ChunkData, bool) {
    if let Some(chunk) = load_chunk_at_root_sync(ws_root.as_path(), coord) {
        return (chunk, false);
    }

    (
        generate_chunk_async_biome(coord, reg, cfg.seed, biomes, trees).await,
        true,
    )
}

pub fn encode_chunk(ch: &ChunkData) -> Vec<u8> {
    let raw =
        wincode::serialize(&(ch.blocks.clone(), ch.stacked_blocks.clone())).expect("encode blocks");
    compress_prepend_size(&raw)
}

pub fn decode_chunk(buf: &[u8]) -> std::io::Result<ChunkData> {
    let de = decompress_size_prepended(buf).map_err(std::io::Error::other)?;
    let (blocks, stacked_blocks) = match wincode::deserialize::<(Vec<BlockId>, Vec<BlockId>)>(&de) {
        Ok(tuple) => tuple,
        Err(_) => {
            let blocks: Vec<BlockId> = wincode::deserialize(&de).map_err(|error| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string())
            })?;
            (blocks, vec![0; CX * CY * CZ])
        }
    };
    if blocks.len() != CX * CY * CZ || stacked_blocks.len() != CX * CY * CZ {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "invalid chunk buffer sizes: blocks={}, stacked={}, expected={}",
                blocks.len(),
                stacked_blocks.len(),
                CX * CY * CZ
            ),
        ));
    }

    let mut chunk = ChunkData::new();
    chunk.blocks = blocks;
    chunk.stacked_blocks = stacked_blocks;
    chunk.dirty_mask = u32::MAX >> (32 - SEC_COUNT);
    Ok(chunk)
}
