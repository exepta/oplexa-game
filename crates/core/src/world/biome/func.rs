use crate::core::world::biome::registry::BiomeRegistry;
use crate::core::world::biome::{Biome, BiomeSize};
use crate::core::world::chunk_dimension::{CX, Y_MIN};
use bevy::prelude::*;

/* ======================================================================= */
/* == Constants: Base Noise / Scores ===================================== */
/* ======================================================================= */

/// Ocean base noise frequency.
pub const OCEAN_FREQ: f32 = 0.012;
/// Ocean base noise amplitude.
pub const OCEAN_AMP: f32 = 12.0;
/// Plains base noise frequency.
pub const PLAINS_FREQ: f32 = 0.008;
/// Plains base noise amplitude.
pub const PLAINS_AMP: f32 = 22.0;

/* ======================================================================= */
/* == Constants: Field / Coast Parameters ================================= */
/* ======================================================================= */

/// Site grid base cell size in chunks.
pub const BASE_CELL_CHUNKS: i32 = 8;
/// Search radius in site cells for nearest-site queries.
pub const SEARCH_RADIUS_CELLS: i32 = 3;
/// Jitter fraction for site center offset within a cell.
pub const JITTER_FRAC: f32 = 0.35;

/// Max land site score threshold to accept land dominance.
pub const LAND_SCORE_MAX: f32 = 1.02;
/// Minimum fraction for size lower bound.
pub const SIZE_MIN_FRAC: f32 = 0.75;

/// Ocean area limits in chunk^2 units.
pub const OCEAN_MIN_AREA: f32 = 4000.0;
pub const OCEAN_MAX_AREA: f32 = 30000.0;
/// Weight multiplier so oceans appear frequently among standalone sites.
pub const OCEAN_WEIGHT_MULTI: f64 = 3.5;

/// Label smoothing radius (in chunks) and iteration count.
pub const SMOOTH_RADIUS_CH: i32 = 1;
pub const SMOOTH_ITERS: usize = 1;

/// Controls coastline transition band around land/ocean junctions.
pub const COAST_INSET_SCORE: f32 = 0.12;
pub const COAST_BAND_SCORE: f32 = 0.35;
pub const COAST_NOISE_FREQ: f32 = 0.03;
pub const COAST_NOISE_AMP_SCORE: f32 = 0.10;
pub const COAST_DETAIL_FREQ: f32 = 0.12;
/// Min/max beach widths in blocks.
pub const BEACH_MIN: i32 = 3;
pub const BEACH_MAX: i32 = 8;

/* ======================================================================= */
/* == Constants: Sub-biome Control ======================================== */
/* ======================================================================= */

/// Max normalized distance to host center where a sub-biome may appear.
pub const SUB_COAST_LIMIT: f32 = 1.15;
/// Clamp range for sub-biome rarity-driven presence.
pub const SUB_PRESENT_MIN: f32 = 0.05;
pub const SUB_PRESENT_MAX: f32 = 0.70;

/// Core zone influence (smoothstep window).
pub const SUB_CORE_START: f32 = 0.28;
pub const SUB_CORE_END: f32 = 1.05;
/// Lower threshold for ocean sub-biome switches so ocean subs appear reliably.
pub const OCEAN_SUB_CORE_SWITCH: f32 = 0.14;
/// Default smooth fade band after `min_land_distance_blocks` for ocean subs.
pub const OCEAN_SUB_MIN_LAND_BLEND_DEFAULT_BLOCKS: f32 = 128.0;
/// Edge noise parameters for sub-biome shapes.
pub const SUB_EDGE_NOISE_FREQ: f32 = 0.02;
pub const SUB_EDGE_NOISE_AMP: f32 = 0.06;

/* ======================================================================= */
/* == Constants: Mountain Shaping ========================================= */
/* ======================================================================= */

pub const MNT_BASE_FREQ: f32 = 0.02;
pub const MNT_DOME_GAIN: f32 = 0.55;
pub const MNT_DOME_EXP: f32 = 1.55;
pub const MNT_DETAIL_EDGE_FADE_START: f32 = 0.10;
pub const MNT_DETAIL_EDGE_FADE_END: f32 = 0.40;
pub const MNT_WORLD_SLOPE: f32 = 0.90;

/// Sub-biome cross-guard when neighbor is foreign (used by adjacency).
pub const FOREIGN_GUARD_START: f32 = 0.95;
pub const FOREIGN_GUARD_END: f32 = 1.20;

/* ======================================================================= */
/* == Constants: Salts / Hashing ========================================== */
/* ======================================================================= */

pub const SALT_PICK_BIOME: u32 = 0xB10E_55ED;
pub const SALT_PICK_SIZE: u32 = 0x51AE_0001 ^ 0x0000_1234;
pub const SALT_JITTER_X: u32 = 0xA11E_D00F;
pub const SALT_JITTER_Z: u32 = 0xC0FF_EE00;
pub const SALT_COAST: i32 = 0x00C0_4751;
pub const SALT_COAST2: i32 = 0xB34C_0001u32 as i32;
pub const SALT_SUB_SITES: u32 = 0x5AB5_1735;
pub const SALT_SUB_EDGE: i32 = 0x53AB_CAFEi32;

/* ======================================================================= */
/* == Public API: Dominant Biome / Labels ================================= */
/* ======================================================================= */

/// Return the dominant biome at a world position measured in chunks.
/// Blends between two nearest land sites, then applies coast/ocean override.
pub fn dominant_biome_at_p_chunks(
    biomes: &BiomeRegistry,
    world_seed: i32,
    p_chunks: Vec2,
) -> &Biome {
    let fallback_label = biomes
        .by_name
        .get(&biomes.ordered_names[0])
        .expect("BiomeRegistry empty?");

    let (land0, pos0, r0, s0, land1_opt, pos1, r1, s1) =
        best_two_land_sites(biomes, p_chunks, world_seed, fallback_label);

    let w0 = land_weight_from_score(s0);
    let w1 = land_weight_from_score(s1);
    let w_sum = (w0 + w1).max(1e-6);

    /// Runs the `mats_for_site` routine for mats for site in the `core::world::biome::func` module.
    #[inline]
    fn mats_for_site<'a>(
        biomes: &'a BiomeRegistry,
        site_biome: &'a Biome,
        site_pos: Vec2,
        site_r: f32,
        p_chunks: Vec2,
        w_site: f32,
        w_sum: f32,
        world_seed: i32,
    ) -> &'a Biome {
        let s_site = p_chunks.distance(site_pos) / site_r.max(1.0);
        let mut mat_biome = site_biome;

        // Allow sub-biomes only inside host's influence band.
        if site_biome.stand_alone && s_site.is_finite() && s_site < SUB_COAST_LIMIT {
            if let Some((sub_b, s_sub)) =
                pick_sub_biome_in_host(biomes, site_biome, site_pos, site_r, p_chunks, world_seed)
            {
                let mut core = sub_core_factor(s_sub);

                // Neighbor support (guard against foreign land sites).
                let adj =
                    adjacency_support_factor(biomes, p_chunks, world_seed, site_biome, &sub_b.name);
                core *= adj;

                // Influence from site weight vs. combined weights.
                let site_influence = (w_site / w_sum).clamp(0.0, 1.0);
                core *= site_influence;

                // Prevent flat subs (e.g. forest) from taking over labels where
                // a relief sub (e.g. mountains) is currently dominant for terrain.
                let sub_is_relief =
                    sub_b.settings.mount_amp.is_some() || sub_b.settings.mount_freq.is_some();
                let blocked_by_relief = if sub_is_relief {
                    false
                } else if let Some((relief_sub, s_relief)) = pick_relief_sub_biome_in_host(
                    biomes, site_biome, site_pos, site_r, p_chunks, world_seed,
                ) {
                    let mut relief_core = sub_core_factor(s_relief);
                    let relief_adj = adjacency_support_factor(
                        biomes,
                        p_chunks,
                        world_seed,
                        site_biome,
                        &relief_sub.name,
                    );
                    relief_core *= relief_adj;
                    relief_core *= site_influence;
                    relief_core > 0.20
                } else {
                    false
                };

                // Switch biome label/rules when sub influence is meaningfully dominant.
                if core > 0.22 && site_influence > 0.5 && !blocked_by_relief {
                    mat_biome = sub_b;
                }
            }
        }
        mat_biome
    }

    let mats0 = mats_for_site(biomes, land0, pos0, r0, p_chunks, w0, w_sum, world_seed);
    let mats1 = if let Some(land1) = land1_opt {
        mats_for_site(biomes, land1, pos1, r1, p_chunks, w1, w_sum, world_seed)
    } else {
        mats0
    };

    let land_mats = if w0 >= w1 { mats0 } else { mats1 };

    // Coast/ocean override based on the better of two site scores.
    let s_for_coast = s0.min(s1);
    let t_ocean = smoothstep(1.0 - COAST_INSET_SCORE, 1.0 + COAST_BAND_SCORE, s_for_coast);
    let ocean = if let Some((b, ocean_pos, ocean_r, ocean_s)) =
        best_land_and_ocean_sites(biomes, p_chunks, world_seed).1
    {
        let mut ocean_mats = b;
        if b.stand_alone && ocean_s.is_finite() && ocean_s < SUB_COAST_LIMIT {
            if let Some((sub_b, s_sub)) =
                pick_sub_biome_in_host(biomes, b, ocean_pos, ocean_r, p_chunks, world_seed)
            {
                let core = sub_core_factor(s_sub);
                if core > OCEAN_SUB_CORE_SWITCH {
                    ocean_mats = sub_b;
                }
            }
        }
        ocean_mats
    } else {
        any_ocean_biome(biomes).unwrap_or(land_mats)
    };

    if (1.0 - t_ocean) >= 0.5 {
        land_mats
    } else {
        ocean
    }
}

/// Runs the `best_two_land_sites` routine for best two land sites in the `core::world::biome::func` module.
pub fn best_two_land_sites<'a>(
    biomes: &'a BiomeRegistry,
    p_chunks: Vec2,
    world_seed: i32,
    fallback_label: &'a Biome,
) -> (&'a Biome, Vec2, f32, f32, Option<&'a Biome>, Vec2, f32, f32) {
    let (best_land, _best_ocean) = best_land_and_ocean_sites(biomes, p_chunks, world_seed);

    // Host
    let (land0, pos0, r0, s0) = if let Some((b, pos, r, s)) = best_land {
        (b, pos, r, s)
    } else {
        (fallback_label, Vec2::ZERO, 1.0, f32::INFINITY)
    };

    let (land1_opt, pos1, r1, s1) = if let Some((b2, p2, rr2, ss2)) =
        best_second_land_site(biomes, p_chunks, world_seed, pos0)
    {
        (Some(b2), p2, rr2, ss2)
    } else {
        (None, Vec2::ZERO, 1.0, f32::INFINITY)
    };

    (land0, pos0, r0, s0, land1_opt, pos1, r1, s1)
}

/// Choose a biome label with local smoothing to avoid speckle along borders.
pub fn choose_biome_label_smoothed<'a>(
    biomes: &'a BiomeRegistry,
    coord: IVec2,
    seed: i32,
) -> &'a Biome {
    if SMOOTH_RADIUS_CH <= 0 || SMOOTH_ITERS == 0 {
        return choose_biome_label_thresholded(biomes, coord, seed);
    }
    let mut label = choose_biome_label_thresholded(biomes, coord, seed);
    for _ in 0..SMOOTH_ITERS {
        let mut counts: Vec<(&'a Biome, u32)> = Vec::new();
        for dz in -SMOOTH_RADIUS_CH..=SMOOTH_RADIUS_CH {
            for dx in -SMOOTH_RADIUS_CH..=SMOOTH_RADIUS_CH {
                let b = choose_biome_label_thresholded(
                    biomes,
                    IVec2::new(coord.x + dx, coord.y + dz),
                    seed,
                );
                if let Some((_, c)) = counts.iter_mut().find(|(bi, _)| std::ptr::eq(*bi, b)) {
                    *c += 1;
                } else {
                    counts.push((b, 1));
                }
            }
        }
        counts.sort_by(|(a, ca), (b, cb)| {
            cb.cmp(ca).then_with(|| {
                b.rarity
                    .partial_cmp(&a.rarity)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        });
        if let Some((b, _)) = counts.first() {
            label = *b;
        }
    }
    label
}

/// Choose a biome label at a chunk coordinate using land-score thresholding.
pub fn choose_biome_label_thresholded(biomes: &BiomeRegistry, coord: IVec2, seed: i32) -> &Biome {
    let px = coord.x as f32 + 0.5;
    let pz = coord.y as f32 + 0.5;
    let (best_land, best_ocean) = best_land_and_ocean_sites(biomes, Vec2::new(px, pz), seed);

    if let Some((b, _pos, _r, s)) = best_land
        && s <= LAND_SCORE_MAX
    {
        return b;
    }
    if let Some((b, _pos, _r, _s)) = best_ocean {
        return b;
    }
    biomes.by_name.get(&biomes.ordered_names[0]).unwrap()
}

/// Locates the nearest chunk whose dominant biome matches `localized_name`.
///
/// Searches ring-by-ring around `origin_chunk` up to `max_radius_chunks`.
pub fn locate_biome_chunk_by_localized_name(
    biomes: &BiomeRegistry,
    world_seed: i32,
    origin_chunk: IVec2,
    localized_name: &str,
    max_radius_chunks: i32,
) -> Option<IVec2> {
    if biomes.is_empty() {
        return None;
    }

    let target = localized_name.trim();
    if target.is_empty() {
        return None;
    }

    let max_radius_chunks = max_radius_chunks.max(0);

    // Use dominant biome directly for locate queries to keep command latency stable.
    // (The smoothed label pass is significantly more expensive over large radii.)
    let matches = |coord: IVec2| {
        let p = Vec2::new(coord.x as f32 + 0.5, coord.y as f32 + 0.5);
        dominant_biome_at_p_chunks(biomes, world_seed, p)
            .localized_name
            .eq_ignore_ascii_case(target)
    };

    for radius in 0..=max_radius_chunks {
        if radius == 0 {
            if matches(origin_chunk) {
                return Some(origin_chunk);
            }
            continue;
        }

        let min_x = origin_chunk.x - radius;
        let max_x = origin_chunk.x + radius;
        let min_z = origin_chunk.y - radius;
        let max_z = origin_chunk.y + radius;

        for x in min_x..=max_x {
            if matches(IVec2::new(x, min_z)) {
                return Some(IVec2::new(x, min_z));
            }
            if matches(IVec2::new(x, max_z)) {
                return Some(IVec2::new(x, max_z));
            }
        }

        if max_z - min_z <= 1 {
            continue;
        }

        for z in (min_z + 1)..=(max_z - 1) {
            if matches(IVec2::new(min_x, z)) {
                return Some(IVec2::new(min_x, z));
            }
            if matches(IVec2::new(max_x, z)) {
                return Some(IVec2::new(max_x, z));
            }
        }
    }

    None
}

/* ======================================================================= */
/* == Site Queries / Sizes ================================================ */
/* ======================================================================= */

type SiteScore<'a> = (&'a Biome, Vec2, f32, f32);

#[inline]
fn set_if_better_site<'a>(best: &mut Option<SiteScore<'a>>, candidate: SiteScore<'a>) {
    if best.is_none_or(|(_, _, _, best_score)| candidate.3 < best_score) {
        *best = Some(candidate);
    }
}

#[inline]
fn site_search_grid_origin(p_chunks: Vec2) -> (i32, i32) {
    (
        (p_chunks.x.floor() as i32).div_euclid(BASE_CELL_CHUNKS),
        (p_chunks.y.floor() as i32).div_euclid(BASE_CELL_CHUNKS),
    )
}

#[inline]
fn for_each_site_in_search_radius<'a>(
    biomes: &'a BiomeRegistry,
    p_chunks: Vec2,
    world_seed: i32,
    mut visitor: impl FnMut(Vec2, &'a Biome, f32),
) {
    let (grid_x, grid_z) = site_search_grid_origin(p_chunks);
    for cell_offset_z in -SEARCH_RADIUS_CELLS..=SEARCH_RADIUS_CELLS {
        for cell_offset_x in -SEARCH_RADIUS_CELLS..=SEARCH_RADIUS_CELLS {
            let cell_x = grid_x + cell_offset_x;
            let cell_z = grid_z + cell_offset_z;
            let (site_pos, site_biome, site_radius) =
                site_properties_for_cell(biomes, cell_x, cell_z, world_seed);
            visitor(site_pos, site_biome, site_radius);
        }
    }
}

#[inline]
fn second_best_site_with_kind<'a>(
    biomes: &'a BiomeRegistry,
    p_chunks: Vec2,
    world_seed: i32,
    host_site_pos: Vec2,
    want_ocean_site: bool,
) -> Option<SiteScore<'a>> {
    let mut best: Option<SiteScore<'a>> = None;
    for_each_site_in_search_radius(
        biomes,
        p_chunks,
        world_seed,
        |site_pos, site_biome, site_radius| {
            if is_ocean_biome(site_biome) != want_ocean_site {
                return;
            }
            // Filter the exact same site (same center) as host.
            if (site_pos - host_site_pos).length_squared() < 1e-6 {
                return;
            }
            let score = p_chunks.distance(site_pos) / site_radius.max(1.0);
            set_if_better_site(&mut best, (site_biome, site_pos, site_radius, score));
        },
    );

    best
}

/// Find best land and ocean sites around `p_chunks` within a search window.
pub fn best_land_and_ocean_sites<'a>(
    biomes: &'a BiomeRegistry,
    p_chunks: Vec2,
    world_seed: i32,
) -> (Option<SiteScore<'a>>, Option<SiteScore<'a>>) {
    let mut best_land: Option<SiteScore<'a>> = None;
    let mut best_ocean: Option<SiteScore<'a>> = None;

    for_each_site_in_search_radius(
        biomes,
        p_chunks,
        world_seed,
        |site_pos, site_biome, site_radius| {
            let score = p_chunks.distance(site_pos) / site_radius.max(1.0);
            if is_ocean_biome(site_biome) {
                set_if_better_site(&mut best_ocean, (site_biome, site_pos, site_radius, score));
            } else {
                set_if_better_site(&mut best_land, (site_biome, site_pos, site_radius, score));
            }
        },
    );

    (best_land, best_ocean)
}

/// Find the second-best *land* site distinct from `host_site_pos`.
pub fn best_second_land_site<'a>(
    biomes: &'a BiomeRegistry,
    p_chunks: Vec2,
    world_seed: i32,
    host_site_pos: Vec2,
) -> Option<SiteScore<'a>> {
    second_best_site_with_kind(biomes, p_chunks, world_seed, host_site_pos, false)
}

/// Find the second-best *ocean* site distinct from `host_site_pos`.
pub fn best_second_ocean_site<'a>(
    biomes: &'a BiomeRegistry,
    p_chunks: Vec2,
    world_seed: i32,
    host_site_pos: Vec2,
) -> Option<SiteScore<'a>> {
    second_best_site_with_kind(biomes, p_chunks, world_seed, host_site_pos, true)
}

/// Compute site position, chosen biome and radius for a cell.
pub fn site_properties_for_cell(
    biomes: &BiomeRegistry,
    cell_x: i32,
    cell_z: i32,
    world_seed: i32,
) -> (Vec2, &Biome, f32) {
    let cell_w = BASE_CELL_CHUNKS as f32;

    // Jitter site center inside the cell (deterministic).
    let jx = (rand01(cell_x, cell_z, (world_seed as u32) ^ SALT_JITTER_X) - 0.5)
        * 2.0
        * JITTER_FRAC
        * cell_w;
    let jz = (rand01(cell_x, cell_z, (world_seed as u32) ^ SALT_JITTER_Z) - 0.5)
        * 2.0
        * JITTER_FRAC
        * cell_w;

    let center_x = (cell_x as f32 + 0.5) * cell_w + jx;
    let center_z = (cell_z as f32 + 0.5) * cell_w + jz;
    let pos = Vec2::new(center_x, center_z);

    // Pick a standalone biome or ocean for this site.
    let r = rand01(cell_x, cell_z, (world_seed as u32) ^ SALT_PICK_BIOME) as f64;
    let biome = rarity_pick_site(biomes, r).expect("No biomes registered");

    // Draw site area uniformly from configured size range.
    let (area_min, area_max) = size_to_area_bounds(if biome.sizes.is_empty() {
        &BiomeSize::Medium
    } else {
        &biome.sizes[(rand_u32(cell_x, cell_z, (world_seed as u32) ^ SALT_PICK_SIZE) as usize)
            % biome.sizes.len()]
    });

    let t = rand01(
        cell_x,
        cell_z,
        (world_seed as u32).wrapping_add(0xFACE_FEED),
    );
    let target_area_chunks = area_min + t * (area_max - area_min);

    // Convert area to radius, add small jitter, and enforce a minimum.
    let mut radius_chunks = (target_area_chunks / std::f32::consts::PI).sqrt();
    let jitter = 0.95
        + 0.10
            * rand01(
                cell_x,
                cell_z,
                (world_seed as u32).wrapping_add(0xDEAD_BEEF),
            );
    radius_chunks *= jitter;
    let min_r = (area_min / std::f32::consts::PI).sqrt();
    radius_chunks = radius_chunks.max(min_r * 0.98);

    (pos, biome, radius_chunks.max(1.0))
}

/// Probability-weighted site pick among standalone biomes and oceans.
pub fn rarity_pick_site(biomes: &BiomeRegistry, r01: f64) -> Option<&Biome> {
    if biomes.ordered_names.is_empty() {
        return None;
    }

    let mut total = 0.0f64;
    let mut eff: Vec<f64> = Vec::with_capacity(biomes.ordered_names.len());

    for (i, &w) in biomes.weights.iter().enumerate() {
        let name = &biomes.ordered_names[i];
        let base = (w as f64).max(0.0);
        let multi = if let Some(b) = biomes.by_name.get(name) {
            if is_ocean_biome(b) {
                OCEAN_WEIGHT_MULTI
            } else if b.stand_alone {
                1.0
            } else {
                0.0
            }
        } else {
            0.0
        };
        let v = base * multi;
        eff.push(v);
        total += v;
    }

    if total <= 0.0 {
        let name = &biomes.ordered_names[0];
        return biomes.by_name.get(name);
    }

    let target = r01.min(0.999_999_999).max(0.0) * total;
    let mut acc = 0.0;
    for (i, v) in eff.iter().enumerate() {
        acc += *v;
        if acc > target {
            let name = &biomes.ordered_names[i];
            return biomes.by_name.get(name);
        }
    }
    let last = biomes.ordered_names.last().unwrap();
    biomes.by_name.get(last)
}

/// Map a `BiomeSize` to (min,max) area bounds in chunk^2 units.
pub fn size_to_area_bounds(size: &BiomeSize) -> (f32, f32) {
    match size {
        BiomeSize::VeryTiny => (SIZE_MIN_FRAC * 4.0, 4.0),
        BiomeSize::Tiny => (SIZE_MIN_FRAC * 20.0, 20.0),
        BiomeSize::Small => (SIZE_MIN_FRAC * 56.0, 56.0),
        BiomeSize::Medium => (SIZE_MIN_FRAC * 96.0, 96.0),
        BiomeSize::Large => (SIZE_MIN_FRAC * 196.0, 196.0),
        BiomeSize::Huge => (SIZE_MIN_FRAC * 392.0, 392.0),
        BiomeSize::Giant => (SIZE_MIN_FRAC * 560.0, 560.0),
        BiomeSize::Ocean => (OCEAN_MIN_AREA, OCEAN_MAX_AREA),
    }
}

/* ======================================================================= */
/* == Sub-biomes: Picking / Adjacency ===================================== */
/* ======================================================================= */

#[derive(Clone, Copy)]
struct SubBiomeHostQuery<'a>(&'a BiomeRegistry, &'a Biome, Vec2, f32, Vec2, i32);

fn pick_sub_biome_in_host_internal<'a>(
    query: SubBiomeHostQuery<'a>,
    require_relief_sub: bool,
) -> Option<(&'a Biome, f32)> {
    let SubBiomeHostQuery(biomes, host, host_pos, host_r, p, world_seed) = query;
    let subs = host.subs.as_ref()?;
    if subs.is_empty() {
        return None;
    }
    let host_is_ocean = is_ocean_biome(host);
    let (nearest_land_edge_blocks, second_land_edge_blocks) = if host_is_ocean {
        if let Some((_, land_pos, land_r, _)) = best_land_and_ocean_sites(biomes, p, world_seed).0 {
            let dist_chunks = p.distance(land_pos);
            let edge_dist_chunks = (dist_chunks - land_r).max(0.0);
            let first = edge_dist_chunks * (CX as f32);

            let second = if let Some((_, land2_pos, land2_r, _)) =
                best_second_land_site(biomes, p, world_seed, land_pos)
            {
                let dist2_chunks = p.distance(land2_pos);
                let edge2_dist_chunks = (dist2_chunks - land2_r).max(0.0);
                edge2_dist_chunks * (CX as f32)
            } else {
                f32::INFINITY
            };

            (first, second)
        } else {
            (f32::INFINITY, f32::INFINITY)
        }
    } else {
        (f32::INFINITY, f32::INFINITY)
    };

    let mut best: Option<(&Biome, f32)> = None;

    for (si, sub_raw) in subs.iter().enumerate() {
        let sub = match get_biome_case_insensitive(biomes, sub_raw) {
            Some(b) => b,
            None => continue,
        };
        if require_relief_sub
            && sub.settings.mount_amp.is_none()
            && sub.settings.mount_freq.is_none()
        {
            continue;
        }
        let ocean_land_distance_gate = if host_is_ocean {
            if let Some(min_land_dist_blocks) = sub.settings.sub_min_land_distance_blocks {
                let min_land_dist_blocks = min_land_dist_blocks.max(0.0);
                if !min_land_dist_blocks.is_finite() || min_land_dist_blocks <= 0.0 {
                    1.0
                } else {
                    let fade_blocks = sub
                        .settings
                        .sub_min_land_distance_blend_blocks
                        .unwrap_or(OCEAN_SUB_MIN_LAND_BLEND_DEFAULT_BLOCKS);
                    let fade_blocks = if fade_blocks.is_finite() {
                        fade_blocks.max(1.0)
                    } else {
                        OCEAN_SUB_MIN_LAND_BLEND_DEFAULT_BLOCKS
                    };
                    let mut gate = smoothstep(
                        min_land_dist_blocks,
                        min_land_dist_blocks + fade_blocks,
                        nearest_land_edge_blocks,
                    );
                    if second_land_edge_blocks.is_finite() {
                        // Narrow sea between two land biomes should not immediately become deep ocean.
                        let corridor_width_blocks =
                            nearest_land_edge_blocks + second_land_edge_blocks;
                        let corridor_gate = smoothstep(
                            min_land_dist_blocks * 2.0,
                            min_land_dist_blocks * 2.0 + fade_blocks,
                            corridor_width_blocks,
                        );
                        gate *= corridor_gate;
                    }
                    gate
                }
            } else {
                1.0
            }
        } else {
            1.0
        };
        if ocean_land_distance_gate <= 0.0 {
            continue;
        }

        // Determine a sub-area from its size list (default Small).
        let (area_min, area_max) = if sub.sizes.is_empty() {
            size_to_area_bounds(&BiomeSize::Small)
        } else {
            let idx = (rand_u32(si as i32, world_seed, SALT_PICK_SIZE) as usize) % sub.sizes.len();
            size_to_area_bounds(&sub.sizes[idx])
        };

        // Number of sub-sites scales softly with rarity.
        let rr = sub.rarity.clamp(SUB_PRESENT_MIN, SUB_PRESENT_MAX);
        let n_sites = (1.0 + rr * 5.0).round() as i32;

        for k in 0..n_sites.max(1) {
            let s_seed = (world_seed as u32)
                ^ SALT_SUB_SITES
                ^ hash32_str(&host.name)
                ^ hash32_str(&sub.name)
                ^ (k as u32);

            // Draw site area in [min, max].
            let t_r = rand01(host_pos.x as i32 + k, host_pos.y as i32 - k, s_seed ^ 0xA1);
            let area_site = area_min + t_r * (area_max - area_min);
            let mut r_site = (area_site / std::f32::consts::PI).sqrt();
            r_site = if host_is_ocean {
                // Ocean subs should be broad fields, not tiny pockets, otherwise
                // seafloor profiles change too quickly and create hard walls.
                let ocean_min_r = (host_r * 0.35).max(18.0);
                let ocean_max_r = (host_r * 0.92).max(ocean_min_r + 1.0);
                r_site.clamp(ocean_min_r, ocean_max_r)
            } else {
                r_site.min(host_r * 0.75).max(4.0)
            };

            // Bias placement toward mid/outer ring to reduce overlap with host center.
            let u = rand01(
                host_pos.x as i32 - 13 * k,
                host_pos.y as i32 + 19 * k,
                s_seed ^ 0xB7,
            );
            let d = if host_is_ocean {
                // Ocean subs (e.g. deep ocean) should prefer inner ocean areas,
                // not only outer/coast rings.
                let max_d = ((host_r - r_site).max(1.0) * 0.55).max(1.0);
                u.powf(2.0) * max_d
            } else {
                let d_edge_bias = u.powf(0.25);
                let max_d = (host_r - r_site).max(1.0);
                let min_d = (0.55 * host_r).min(max_d);
                min_d + (max_d - min_d) * d_edge_bias
            };

            // Random angle with slight rotation per site index.
            let ang = (rand01(
                host_pos.x as i32 + 23 * k,
                host_pos.y as i32 - 29 * k,
                s_seed ^ 0xC7,
            ) * std::f32::consts::TAU)
                + 0.17 * (k as f32);
            let sub_pos = host_pos + Vec2::new(ang.cos(), ang.sin()) * d;

            // Score = normalized distance to the sub site.
            let s_raw = p.distance(sub_pos) / r_site.max(1.0);
            let s = if host_is_ocean && ocean_land_distance_gate < 1.0 {
                // Near the land-distance threshold, softly down-weight ocean subs
                // instead of creating a binary ocean/deep-ocean wall.
                (s_raw + (1.0 - ocean_land_distance_gate) * 1.25).min(4.0)
            } else {
                s_raw
            };
            if best.is_none_or(|(_, sb)| s < sb) {
                best = Some((sub, s));
            }
        }
    }

    best
}

macro_rules! define_sub_biome_picker {
    ($fn_name:ident, $require_relief:expr, $doc:literal) => {
        #[doc = $doc]
        pub fn $fn_name<'a>(
            biomes: &'a BiomeRegistry,
            host: &'a Biome,
            host_pos: Vec2,
            host_r: f32,
            p: Vec2,
            world_seed: i32,
        ) -> Option<(&'a Biome, f32)> {
            pick_sub_biome_in_host_internal(
                SubBiomeHostQuery(biomes, host, host_pos, host_r, p, world_seed),
                $require_relief,
            )
        }
    };
}

define_sub_biome_picker!(
    pick_sub_biome_in_host,
    false,
    "Pick a sub-biome inside a host land biome near `p`. Returns `(sub, s)` where `s` is normalized distance to sub center (`p.distance/r_site`)."
);
define_sub_biome_picker!(
    pick_relief_sub_biome_in_host,
    true,
    "Pick the nearest sub-biome that can contribute mountain relief (`mount_*` set)."
);

/// Down-weight sub-biome dominance if the nearest *different* land site
/// would likely not support the sub-biome; returns [0..1] multiplier.
pub fn adjacency_support_factor(
    biomes: &BiomeRegistry,
    p_chunks: Vec2,
    world_seed: i32,
    host_biome: &Biome,
    sub_name: &str,
) -> f32 {
    let mut best: Option<(f32, bool)> = None;
    for_each_site_in_search_radius(
        biomes,
        p_chunks,
        world_seed,
        |site_pos, site_biome, site_radius| {
            if is_ocean_biome(site_biome) {
                return;
            }
            if std::ptr::eq(site_biome, host_biome) {
                return;
            }
            if site_biome.name.eq_ignore_ascii_case(&host_biome.name) {
                return;
            }
            let distance_to_site = p_chunks.distance(site_pos);
            let score = distance_to_site / site_radius.max(1.0);
            let neighbor_supports = supports_sub(site_biome, sub_name);
            if best.is_none_or(|(s, _)| score < s) {
                best = Some((score, neighbor_supports));
            }
        },
    );

    if let Some((s, ok)) = best {
        if ok {
            1.0
        } else {
            smoothstep(FOREIGN_GUARD_START, FOREIGN_GUARD_END, s)
        }
    } else {
        1.0
    }
}

/* ======================================================================= */
/* == Terrain Helpers (caps, slopes) ====================================== */
/* ======================================================================= */

/// Estimate soil cap thickness in blocks from local slope/roughness context.
/// `core`: influence (0..1), `delta_c`: center height delta, `n8`: neighbor deltas.
pub fn slope_to_soil_cap(core: f32, delta_c: f32, n8: [f32; 8]) -> i32 {
    if core <= 0.0 {
        return 3;
    } // plains/default

    // Max absolute difference to center (proxy for slope).
    let mut max_diff = 0.0f32;
    let mut sum = 0.0f32;
    for v in n8 {
        let d = (v - delta_c).abs();
        if d > max_diff {
            max_diff = d;
        }
        sum += v;
    }
    let mean = sum * 0.125;
    let mean_diff = (mean - delta_c).abs();

    // Roughness combines sharp contrast and average tilt; scaled by core.
    let rough = (max_diff * 0.85 + mean_diff * 0.40) * core;

    // Convert to extra soil thickness in blocks.
    let extra = (rough * 2.6).round() as i32; // tune factor to taste
    (3 + extra).clamp(3, 12)
}

/* ======================================================================= */
/* == Random / Hash Utilities ============================================= */
/* ======================================================================= */

/// Column RNG: 32-bit hash on (x,z,seed) with good avalanche.
#[inline]
pub fn col_rand_u32(x: i32, z: i32, seed: u32) -> u32 {
    // Mix (x, z, seed) into 64-bit, then avalanche (Murmur/xxHash-like).
    let mut h = (x as u64).wrapping_mul(0x517C_C1B7_2722_0A95);
    h ^= (z as u64).wrapping_mul(0x2545_F491_4F6C_DD1D);
    h ^= (seed as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);

    // Finalizers (from Murmur3 64-bit variant).
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    h = h.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    h ^= h >> 33;

    (h & 0xFFFF_FFFF) as u32
}

/// Same hash but mapped to `[0,1)` float. Handy for thresholds/roulette.
#[inline]
pub fn col_rand_f32(x: i32, z: i32, seed: u32) -> f32 {
    let u = col_rand_u32(x, z, seed) as f64;
    (u / ((u32::MAX as f64) + 1.0)) as f32
}

/// Inclusive integer range helper `[lo, hi]` using column RNG.
#[inline]
pub fn col_rand_range_u32(x: i32, z: i32, seed: u32, lo: u32, hi: u32) -> u32 {
    if lo >= hi {
        return lo;
    }
    lo + (col_rand_u32(x, z, seed) % (hi - lo + 1))
}

/// Runs the `rand_u32` routine for rand u32 in the `core::world::biome::func` module.
#[inline]
pub fn rand_u32(x: i32, z: i32, seed: u32) -> u32 {
    col_rand_u32(x, z, seed)
}

/// Runs the `rand01` routine for rand01 in the `core::world::biome::func` module.
#[inline]
pub fn rand01(x: i32, z: i32, seed: u32) -> f32 {
    let u = rand_u32(x, z, seed) as f64;
    (u / ((u32::MAX as f64) + 1.0)) as f32
}

/* ======================================================================= */
/* == Biome Predicates / Lookups ========================================== */
/* ======================================================================= */

/// `true` if biome is ocean-like by name or size.
#[inline]
pub fn is_ocean_biome(b: &Biome) -> bool {
    if b.name.eq_ignore_ascii_case("ocean") {
        return true;
    }
    b.sizes.iter().any(|s| matches!(s, BiomeSize::Ocean))
}

/// Return any ocean biome defined in the registry (if present).
pub fn any_ocean_biome(biomes: &BiomeRegistry) -> Option<&Biome> {
    biomes.by_name.values().find(|biome| is_ocean_biome(biome))
}

/// `true` if `host` supports a sub-biome named `sub_name` (case-insensitive).
#[inline]
pub fn supports_sub(host: &Biome, sub_name: &str) -> bool {
    if !host.stand_alone {
        return false;
    }
    let needle = normalized_biome_lookup_key(sub_name);
    if let Some(list) = &host.subs {
        for s in list {
            if s.eq_ignore_ascii_case(sub_name)
                || (!needle.is_empty() && normalized_biome_lookup_key(s.as_str()) == needle)
            {
                return true;
            }
        }
    }
    false
}

#[inline]
fn normalized_biome_lookup_key(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_sep = false;
    for ch in name.trim().chars() {
        let is_sep = ch == '_' || ch == '-' || ch.is_whitespace();
        if is_sep {
            if !last_sep {
                out.push('_');
                last_sep = true;
            }
            continue;
        }
        for lc in ch.to_lowercase() {
            out.push(lc);
        }
        last_sep = false;
    }
    out.trim_matches('_').to_string()
}

/// Case-insensitive lookup for dynamic sub-biome references.
#[inline]
pub fn get_biome_case_insensitive<'a>(biomes: &'a BiomeRegistry, name: &str) -> Option<&'a Biome> {
    if let Some(b) = biomes.by_name.get(name) {
        return Some(b);
    }
    let key = normalized_biome_lookup_key(name);
    for b in biomes.by_name.values() {
        if b.name.eq_ignore_ascii_case(name) {
            return Some(b);
        }
        if !key.is_empty() && normalized_biome_lookup_key(b.name.as_str()) == key {
            return Some(b);
        }
    }
    None
}

/* ======================================================================= */
/* == Math / Utility ====================================================== */
/* ======================================================================= */

/// Pick a string from list using column RNG; returns a default if list is empty.
#[inline]
pub fn pick(list: &[String], wx: i32, wz: i32, seed: u32) -> &str {
    if list.is_empty() {
        return "stone_block";
    }
    let r = col_rand_u32(wx, wz, seed);
    let idx = (r as usize) % list.len();
    &list[idx]
}

/// Runs the `lerp` routine for lerp in the `core::world::biome::func` module.
#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Smoothstep remap of `x` from `[e0, e1]` into `[0,1]` with cubic S-curve.
#[inline]
pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Clamp a world Y to be at/above `(Y_MIN+1)`.
#[inline]
pub fn clamp_world_y(y: f32) -> f32 {
    ((Y_MIN + 1) as f32).max(y)
}

/* ======================================================================= */
/* == Weights / Core Factors ============================================== */
/* ======================================================================= */

/// Convert site score `s` (0=at center) to an inverse-square weight.
#[inline]
pub fn land_weight_from_score(s: f32) -> f32 {
    // Lower scores mean closer to site center -> higher weight.
    let sc = s.max(0.001);
    1.0 / (sc * sc)
}

/// Core factor curve for sub-biomes across host-normalized radius.
#[inline]
pub fn sub_core_factor(s_norm: f32) -> f32 {
    let s = s_norm.clamp(0.0, 1.4);
    let t = ((s - SUB_CORE_END) / (SUB_CORE_START - SUB_CORE_END)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/* ======================================================================= */
/* == String Hash (deterministic seeding) ================================= */
/* ======================================================================= */

/// 32-bit FNV-1a string hash (deterministic seeding for sub-sites).
#[inline]
pub fn hash32_str(s: &str) -> u32 {
    let mut h: u32 = 0x811C_9DC5;
    for &b in s.as_bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}
