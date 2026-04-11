use crate::core::world::block::Face;
use bevy::prelude::*;

/// Global selection state for the player/editor.
///
/// Stores the most recent block recast result (`hit`), or `None` if nothing
/// is under the cursor / in reach.
#[derive(Resource, Default)]
pub struct SelectionState {
    /// Last block intersection, if any.
    pub hit: Option<BlockHit>,
    /// Last structure/model intersection, if any.
    pub structure_hit: Option<StructureHit>,
}

/// Result of a voxel/block recast.
///
/// Semantics used throughout typical voxel editors:
/// - `block_pos`: integer world coordinates of the **solid** block that was hit.
/// - `face`: which face of `block_pos` was intersected (e.g. +X, -Y, etc.).
/// - `place_pos`: the adjacent block position on the **outside** of `block_pos`
///   along `face` — i.e., where you'd place a new block when targeting this face.
#[derive(Clone, Copy, Debug)]
pub struct BlockHit {
    /// World-space integer coordinates of the hit block.
    pub block_pos: IVec3,
    /// Block id of the actually hit occupant.
    pub block_id: u16,
    /// True when the hit came from the stacked (secondary) occupant.
    pub is_stacked: bool,
    /// The face of the hit block that was intersected.
    pub face: Face,
    /// Hit point in local voxel coordinates of `block_pos` (0..1 on each axis).
    pub hit_local: Vec3,
    /// Neighbor cell where a new block would be placed when clicking this face.
    pub place_pos: IVec3,
}

/// Result of a structure/model hit selection raycast.
#[derive(Clone, Copy, Debug)]
pub struct StructureHit {
    /// Hit structure root entity.
    pub entity: Entity,
    /// World-space hit position from rapier collider raycast.
    pub hit_world: Vec3,
    /// World-space surface normal from rapier collider raycast.
    pub hit_normal_world: Vec3,
    /// World-space center of the selection bounds.
    pub selection_center_world: Vec3,
    /// World-space size of the selection bounds.
    pub selection_size_world: Vec3,
}
