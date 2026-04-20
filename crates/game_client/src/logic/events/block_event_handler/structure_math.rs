/// Structure transform and rotation utility helpers.
pub(crate) fn structure_model_translation(
    recipe: &BuildingStructureRecipe,
    place_origin: IVec3,
    placement_rotation_quarters: u8,
    model_rotation_quarters: u8,
) -> Vec3 {
    match recipe.model_meta.model_anchor {
        BuildingModelAnchor::Center => {
            // Center anchor follows occupied recipe space (player preview / placement logic).
            let recipe_space =
                rotated_structure_space(recipe.space, placement_rotation_quarters).as_vec3();
            Vec3::new(
                (place_origin.x as f32 + recipe_space.x * 0.5) * VOXEL_SIZE,
                (place_origin.y as f32 + recipe_space.y * 0.5) * VOXEL_SIZE,
                (place_origin.z as f32 + recipe_space.z * 0.5) * VOXEL_SIZE,
            )
        }
        BuildingModelAnchor::MinCorner => {
            let offset = rotated_model_corner_offset(recipe.space, model_rotation_quarters);
            Vec3::new(
                (place_origin.x as f32 + offset.x) * VOXEL_SIZE,
                (place_origin.y as f32) * VOXEL_SIZE,
                (place_origin.z as f32 + offset.z) * VOXEL_SIZE,
            )
        }
    }
}

#[inline]
pub(crate) fn rotated_model_corner_offset(space: UVec3, rotation_quarters: u8) -> Vec3 {
    match rotation_quarters % 4 {
        0 => Vec3::ZERO,
        1 => Vec3::new(space.z as f32, 0.0, 0.0),
        2 => Vec3::new(space.x as f32, 0.0, space.z as f32),
        _ => Vec3::new(0.0, 0.0, space.x as f32),
    }
}

#[inline]
pub(crate) fn normalize_rotation_quarters(raw: i32) -> u8 {
    raw.rem_euclid(4) as u8
}

#[inline]
pub(crate) fn normalize_rotation_steps(raw: i32) -> u8 {
    raw.rem_euclid(8) as u8
}

#[inline]
pub(crate) fn rotation_steps_to_placement_quarters(rotation_steps: u8) -> u8 {
    normalize_rotation_quarters((rotation_steps as i32) / 2)
}

#[inline]
pub(crate) fn rotated_structure_space(space: UVec3, rotation_quarters: u8) -> UVec3 {
    if rotation_quarters.is_multiple_of(2) {
        space
    } else {
        UVec3::new(space.z, space.y, space.x)
    }
}

#[inline]
pub(crate) fn rotated_structure_offset(
    local_x: i32,
    local_z: i32,
    size_x: i32,
    size_z: i32,
    rotation_quarters: u8,
) -> (i32, i32) {
    match rotation_quarters % 4 {
        0 => (local_x, local_z),
        1 => (local_z, size_x - 1 - local_x),
        2 => (size_x - 1 - local_x, size_z - 1 - local_z),
        _ => (size_z - 1 - local_z, local_x),
    }
}

#[inline]
fn rand_f32() -> f32 {
    rand::random::<f32>()
}

#[inline]
fn rand_range(min: f32, max: f32) -> f32 {
    min + (max - min) * rand_f32()
}

#[inline]
fn rand_i32(min: i32, max: i32) -> i32 {
    if min >= max {
        return min;
    }
    let span = (max - min + 1) as u32;
    min + (rand::random::<u32>() % span) as i32
}

#[inline]
fn random_unit_vector3() -> Vec3 {
    let mut v = Vec3::new(
        rand_range(-1.0, 1.0),
        rand_range(-1.0, 1.0),
        rand_range(-1.0, 1.0),
    );
    if v.length_squared() <= 1e-6 {
        v = Vec3::X;
    }
    v.normalize()
}
