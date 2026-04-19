const SPAWN_SEARCH_RADIUS_BLOCKS: i32 = 32;

#[inline]
pub fn spawn_anchor_from_seed(seed: i32) -> (i32, i32) {
    let mut value = seed as u32 ^ 0xA53C_4F1D;
    value = value.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    let x =
        (value as i32).rem_euclid(SPAWN_SEARCH_RADIUS_BLOCKS * 2 + 1) - SPAWN_SEARCH_RADIUS_BLOCKS;
    value = value.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    let z =
        (value as i32).rem_euclid(SPAWN_SEARCH_RADIUS_BLOCKS * 2 + 1) - SPAWN_SEARCH_RADIUS_BLOCKS;
    (x, z)
}
