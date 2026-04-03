use bevy::prelude::*;

/// Defines the possible loading phase variants in the `core::states::world_gen` module.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LoadingPhase {
    #[default]
    BaseGen,
    WaterGen,
    CaveGen,
    Done,
}

/// Represents phase detail used by the `core::states::world_gen` module.
#[derive(Clone, Copy, Default, Debug)]
pub struct PhaseDetail {
    pub gen_done: usize,
    pub gen_total: usize,
    pub mesh_done: usize,
    pub mesh_total: usize,
    pub pct: f32,
}

/// Represents loading progress used by the `core::states::world_gen` module.
#[derive(Resource, Clone, Default, Debug)]
pub struct LoadingProgress {
    pub phase: LoadingPhase,
    pub base: PhaseDetail,
    pub water: PhaseDetail,
    pub cave: PhaseDetail,
    pub overall_pct: f32,
}

/// Represents loading target used by the `core::states::world_gen` module.
#[derive(Resource, Clone, Copy, Debug)]
pub struct LoadingTarget {
    pub center: IVec2,
    pub radius: i32,
}
