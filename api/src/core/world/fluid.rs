use crate::core::world::chunk_dimension::*;
use bevy::prelude::*;
use std::collections::HashMap;

pub const WATER_FLOW_CAP: usize = 6;
pub const WATER_FLOW_MAX_INFLIGHT: usize = 16;
pub const WATER_FLOW_BUDGET_PER_FRAME: usize = 12;

/// Represents fluid map used by the `core::world::fluid` module.
#[derive(Resource, Default)]
pub struct FluidMap(pub HashMap<IVec2, FluidChunk>);

/// Represents water mesh index used by the `core::world::fluid` module.
#[derive(Resource, Default)]
pub struct WaterMeshIndex(pub HashMap<(IVec2, u8), Entity>);

/// Represents fluid chunk used by the `core::world::fluid` module.
#[derive(Clone)]
pub struct FluidChunk {
    pub sea_level: i32,
    pub bits: Vec<u64>,
}

impl FluidChunk {
    /// Runs the `sub_has_any` routine for sub has any in the `core::world::fluid` module.
    pub fn sub_has_any(&self, sub: usize) -> bool {
        let y0 = sub * SEC_H;
        let y1 = (y0 + SEC_H).min(CY);
        for y in y0..y1 {
            for z in 0..CZ {
                for x in 0..CX {
                    if self.get(x, y, z) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Runs the `bit_len` routine for bit len in the `core::world::fluid` module.
    #[inline]
    fn bit_len() -> usize {
        CX * CY * CZ
    }
    /// Runs the `idx` routine for idx in the `core::world::fluid` module.
    #[inline]
    fn idx(x: usize, y: usize, z: usize) -> usize {
        (y * CZ + z) * CX + x
    }
    /// Creates a new instance for the `core::world::fluid` module.
    pub fn new(sea_level: i32) -> Self {
        let n = (Self::bit_len() + 63) / 64;
        Self {
            sea_level,
            bits: vec![0u64; n],
        }
    }
    /// Returns the requested data for the `core::world::fluid` module.
    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> bool {
        let i = Self::idx(x, y, z);
        (self.bits[i >> 6] >> (i & 63)) & 1 == 1
    }
    /// Sets the requested data for the `core::world::fluid` module.
    #[inline]
    pub fn set(&mut self, x: usize, y: usize, z: usize, on: bool) {
        let i = Self::idx(x, y, z);
        let w = &mut self.bits[i >> 6];
        let m = 1u64 << (i & 63);
        if on {
            *w |= m;
        } else {
            *w &= !m;
        }
    }
    /// Runs the `fill_column` routine for fill column in the `core::world::fluid` module.
    #[inline]
    pub fn fill_column(&mut self, x: usize, z: usize, y0: i32, y1: i32) {
        let lo = y0.max(Y_MIN);
        let hi = y1.min(Y_MIN + CY as i32 - 1);
        if lo > hi {
            return;
        }

        for wy in lo..=hi {
            let ly = (wy - Y_MIN) as usize; // 0..CY-1
            self.set(x, ly, z, true);
        }
    }
}

/// Represents seed used by the `core::world::fluid` module.
#[derive(Clone, Copy, Debug)]
pub struct Seed {
    pub c: IVec2,
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

/// Represents flow job used by the `core::world::fluid` module.
#[derive(Clone)]
pub struct FlowJob {
    pub seeds: Vec<Seed>,
    pub sea_level: i32,
    pub cap: usize,
}

/// Represents flow result used by the `core::world::fluid` module.
#[derive(Default)]
pub struct FlowResult {
    pub filled: Vec<Seed>,
    pub spill: Vec<Seed>,
    pub more: Vec<Seed>,
}

/// Represents solid snapshot used by the `core::world::fluid` module.
#[derive(Clone)]
pub struct SolidSnapshot {
    pub center: IVec2,
    pub bits: HashMap<IVec2, Vec<u8>>,
}
