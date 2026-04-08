use crate::core::world::biome::Biome;
use crate::core::world::biome::func::*;
use crate::core::world::biome::registry::BiomeRegistry;
use crate::core::world::block::{BlockId, BlockRegistry};
use crate::core::world::chunk::{ChunkData, SEA_LEVEL};
use crate::core::world::chunk_dimension::{CX, CY, CZ, Y_MAX, Y_MIN};
use crate::generator::chunk::cave_utils::{CaveParams, worm_edits_for_chunk};
use crate::generator::chunk::chunk_utils::map01;
use crate::generator::chunk::river_utils::RiverSystem;
use crate::generator::chunk::trees::registry::TreeRegistry;
use crate::generator::chunk::trees::tree_gen::populate_trees_in_chunk;
use crate::generator::chunk::vegetation::prop_gen::populate_vegetation_props_in_chunk;
use bevy::prelude::*;
use fastnoise_lite::*;
/* ========================= Generator =================================== */

/// Generates chunk async biome for the `generator::chunk::chunk_gen` module.
pub(crate) async fn generate_chunk_async_biome(
    coord: IVec2,
    reg: &BlockRegistry,
    cfg_seed: i32,
    biomes: &BiomeRegistry,
    trees: &TreeRegistry,
) -> ChunkData {
    let fallback_label = choose_biome_label_smoothed(biomes, coord, cfg_seed);

    // Border/bedrock ID used at world bottom (Y_MIN)
    let id_border = reg.id_or_air("border_block");
    let id_air = reg.id_or_air("air_block");
    let id_water = reg.id_or_air("water_block");
    let id_sand = reg.id_or_air("sand_block");
    let id_dirt = reg.id_or_air("dirt_block");
    let id_gravel = reg.id_or_air("gravel_block");

    // Per-chunk noises
    let seafloor_n = make_seafloor_noise(cfg_seed, OCEAN_FREQ);
    let plains_n = make_plains_noise(cfg_seed, PLAINS_FREQ);
    let coast_n = make_coast_noise(cfg_seed ^ SALT_COAST, COAST_NOISE_FREQ);
    let coast_d = make_coast_noise(cfg_seed ^ SALT_COAST2, COAST_DETAIL_FREQ);
    let sub_edge_n = make_coast_noise(cfg_seed ^ SALT_SUB_EDGE, SUB_EDGE_NOISE_FREQ);
    let river_mode_n = make_coast_noise(cfg_seed ^ 0x5256_4D44, 0.0016);
    let macro_uplift_n = make_coast_noise(cfg_seed ^ 0x4D41_4352, 0.0014);
    let macro_ridge_n = make_coast_noise(cfg_seed ^ 0x5249_4447, 0.0039);
    let seafloor_mat_n = make_coast_noise(cfg_seed ^ 0x53EA_F001, 0.0048);
    let seafloor_mat_d = make_coast_noise(cfg_seed ^ 0x53EA_F002, 0.0185);
    let underground_blob_n = make_coast_noise(cfg_seed ^ 0x554E_4447, 0.0100);
    let underground_mat_n = make_coast_noise(cfg_seed ^ 0x554E_444D, 0.0280);

    let pick_seed: u32 = (cfg_seed as u32) ^ 0x0CE4_11CE;
    let mut chunk = ChunkData::new();

    // --- Rivers: init once per chunk -------------------------------------
    // Deterministic river system, fed by world seed.
    let river = RiverSystem::new(cfg_seed);

    // Chunk world origin in block coordinates
    let chunk_origin_wx = coord.x * CX as i32;
    let chunk_origin_wz = coord.y * CZ as i32;

    // Use a stable probe width to detect whether a river is entering from neighbors.
    // This is only used for "continuation" when a biome has rivers=true but chance==0.0.
    let probe_width = 12;
    let neighbor_has_river = river.neighbor_continuation_for_chunk(
        chunk_origin_wx,
        chunk_origin_wz,
        CX,
        CZ,
        probe_width,
    );

    // Small helper: compute total land height (base and optional mountains) for one land site
    // and tell which biome should provide surface materials for this column if that site dominates.
    /// Runs the `site_total_height` routine for site total height in the `generator::chunk::chunk_gen` module.
    #[inline]
    fn site_total_height<'a>(
        plains_n: &FastNoiseLite,
        macro_uplift_n: &FastNoiseLite,
        macro_ridge_n: &FastNoiseLite,
        sub_edge_n: &FastNoiseLite,
        biomes: &'a BiomeRegistry,
        // site definition
        site_biome: &'a Biome,
        site_pos: Vec2,
        site_r: f32,
        // world position
        wxf: f32,
        wzf: f32,
        p_chunks: Vec2,
        // mixing with other sites
        w_site: f32,
        w_sum: f32,
        // world seed
        cfg_seed: i32,
    ) -> (f32, &'a Biome, i32) {
        // --- base plains height from this site's own settings
        let base_off = site_biome.settings.height_offset;
        let base_amp = site_biome.settings.land_amp.unwrap_or(PLAINS_AMP);
        let base_freq = site_biome.settings.land_freq.unwrap_or(PLAINS_FREQ);
        let freq_scale = (base_freq / PLAINS_FREQ).clamp(0.2, 4.0);
        let mut h = sample_plains_height(
            plains_n, wxf, wzf, SEA_LEVEL, base_off, base_amp, freq_scale,
        );

        // Macro relief produces broader highlands/ridges so terrain is less uniform.
        let mut uplift = sample_land_uplift(macro_uplift_n, macro_ridge_n, wxf, wzf, base_amp);
        if site_biome.name.eq_ignore_ascii_case("desert") {
            // Keep most desert flat; mountain share comes from dedicated desert sub-biomes.
            uplift *= 0.35;
        }
        h += uplift;

        // default materials/soil
        let mut mat_biome = site_biome;
        let mut soil_cap_local = 3;

        // --- optional mountains from this site (if it has a matching sub-biome active here)
        // convert the per-site "distance score" for this column
        let s_site = p_chunks.distance(site_pos) / site_r.max(1.0);

        if site_biome.stand_alone && s_site.is_finite() && s_site < SUB_COAST_LIMIT {
            if let Some((sub_b, s_sub)) = pick_relief_sub_biome_in_host(
                biomes, site_biome, site_pos, site_r, p_chunks, cfg_seed,
            ) {
                // core weight with a little edge noise
                let edge_jit =
                    (map01(sub_edge_n.get_noise_2d(wxf, wzf)) - 0.5) * 2.0 * SUB_EDGE_NOISE_AMP;
                let mut core = sub_core_factor(s_sub + edge_jit);

                // sub-biome adjacency guard
                let adj =
                    adjacency_support_factor(biomes, p_chunks, cfg_seed, site_biome, &sub_b.name);
                core *= adj;

                // fade mountains as this site loses influence against others
                let site_influence = (w_site / w_sum).clamp(0.0, 1.0);
                core *= site_influence;

                if core > 0.0
                    && (sub_b.settings.mount_amp.is_some() || sub_b.settings.mount_freq.is_some())
                {
                    let amp = sub_b.settings.mount_amp.unwrap_or(32.0);
                    let freq = sub_b.settings.mount_freq.unwrap_or(0.015);
                    let scale = (freq / MNT_BASE_FREQ).max(0.01);

                    let ux = wxf * scale;
                    let uz = wzf * scale;

                    // mountain delta from plain noise
                    let delta_c = sample_plains_mountain_delta(plains_n, ux, uz, amp);

                    // 8-neighborhood sampling for smoothing & slope limiting
                    let step_w = scale.max(0.001);
                    let d_e = sample_plains_mountain_delta(plains_n, ux + step_w, uz, amp);
                    let d_w = sample_plains_mountain_delta(plains_n, ux - step_w, uz, amp);
                    let d_n = sample_plains_mountain_delta(plains_n, ux, uz + step_w, amp);
                    let d_s = sample_plains_mountain_delta(plains_n, ux, uz - step_w, amp);
                    let d_ne =
                        sample_plains_mountain_delta(plains_n, ux + step_w, uz + step_w, amp);
                    let d_nw =
                        sample_plains_mountain_delta(plains_n, ux - step_w, uz + step_w, amp);
                    let d_se =
                        sample_plains_mountain_delta(plains_n, ux + step_w, uz - step_w, amp);
                    let d_sw =
                        sample_plains_mountain_delta(plains_n, ux - step_w, uz - step_w, amp);

                    let neigh_avg8 = (d_e + d_w + d_n + d_s + d_ne + d_nw + d_se + d_sw) * 0.125;
                    let neigh_max8 = d_e
                        .max(d_w)
                        .max(d_n.max(d_s))
                        .max(d_ne.max(d_nw).max(d_se.max(d_sw)));
                    let delta_smooth = (delta_c * 2.0 + neigh_avg8) / 3.0;

                    let slope_allow = MNT_WORLD_SLOPE * (0.5 + 0.5 * core);
                    let delta_clamped = delta_smooth
                        .min(neigh_avg8 + slope_allow)
                        .min(neigh_max8 + slope_allow * 0.65);

                    let edge_fade =
                        smoothstep(MNT_DETAIL_EDGE_FADE_START, MNT_DETAIL_EDGE_FADE_END, core);
                    let delta = delta_clamped * edge_fade;

                    // Fade dome uplift near sub-biome edge to avoid hard mountain "walls"
                    // against surrounding plains.
                    let dome_fade = smoothstep(0.14, 0.82, core);
                    let dome = (amp * MNT_DOME_GAIN) * core.powf(MNT_DOME_EXP) * dome_fade;
                    h += dome + core * delta;

                    // surface materials taken from the sub-biome when the site dominates locally
                    if site_influence > 0.5 {
                        mat_biome = sub_b;
                    }

                    // allow caller to use thicker soil on steeper spots
                    soil_cap_local = slope_to_soil_cap(
                        core,
                        delta_c,
                        [d_e, d_w, d_n, d_s, d_ne, d_nw, d_se, d_sw],
                    );
                }
            }
        }

        (h, mat_biome, soil_cap_local)
    }

    for lx in 0..CX {
        for lz in 0..CZ {
            let wx = coord.x * CX as i32 + lx as i32;
            let wz = coord.y * CZ as i32 + lz as i32;
            let wxf = wx as f32;
            let wzf = wz as f32;

            // Column position in "chunk units" with sub-chunk precision
            let px = coord.x as f32 + (lx as f32 + 0.5) / (CX as f32);
            let pz = coord.y as f32 + (lz as f32 + 0.5) / (CZ as f32);
            let p_chunks = Vec2::new(px, pz);

            // Nearest land and ocean sites (host = best land)
            let (_, best_ocean) = best_land_and_ocean_sites(biomes, p_chunks, cfg_seed);

            // Second-best distinct land site (neighbor)
            let (land0, pos0, r0, s0, land1_opt, pos1, r1, s1) =
                best_two_land_sites(biomes, p_chunks, cfg_seed, fallback_label);

            // Inverse-square weights (robust 0-division guard)
            let w0 = land_weight_from_score(s0);
            let w1 = land_weight_from_score(s1);
            let w_sum = (w0 + w1).max(1e-6);

            // --- total height for site 0 (host)
            let (h0_total, mats0, soil0) = site_total_height(
                &plains_n,
                &macro_uplift_n,
                &macro_ridge_n,
                &sub_edge_n,
                biomes,
                land0,
                pos0,
                r0,
                wxf,
                wzf,
                p_chunks,
                w0,
                w_sum,
                cfg_seed,
            );

            // --- total height for site 1 (neighbor) – if none, reuse site 0 so blend is identity
            let (h1_total, mats1, soil1) = if let Some(land1) = land1_opt {
                site_total_height(
                    &plains_n,
                    &macro_uplift_n,
                    &macro_ridge_n,
                    &sub_edge_n,
                    biomes,
                    land1,
                    pos1,
                    r1,
                    wxf,
                    wzf,
                    p_chunks,
                    w1,
                    w_sum,
                    cfg_seed,
                )
            } else {
                (h0_total, land0, soil0)
            };

            // Final land height:
            // 1) Use softer weights for height blending (less abrupt site handover).
            // 2) If both sites strongly disagree in height, add boundary smoothing near 50/50.
            let w0_h = w0.powf(0.68);
            let w1_h = w1.powf(0.68);
            let w_sum_h = (w0_h + w1_h).max(1e-6);
            let t01 = (w1_h / w_sum_h).clamp(0.0, 1.0);
            let t_soft = smoothstep(0.08, 0.92, t01);
            let h_blend = lerp(h0_total, h1_total, t_soft);

            let h_diff = (h0_total - h1_total).abs();
            let boundary = (1.0 - (t01 - 0.5).abs() * 2.0).clamp(0.0, 1.0);
            let diff_smooth = smoothstep(16.0, 112.0, h_diff);
            let boundary_smooth = boundary * boundary;
            let soften = (0.42 * diff_smooth * boundary_smooth).clamp(0.0, 0.42);
            let h_mean = (h0_total + h1_total) * 0.5;
            let h_land = lerp(h_blend, h_mean, soften);

            // Choose materials from whichever land site dominates locally
            let land_biome_for_materials = if w0 >= w1 { mats0 } else { mats1 };

            // Ocean biome/materials (fallback to any ocean or current land materials)
            let ocean_biome = if let Some((b, _p, _r, _s)) = best_ocean {
                b
            } else {
                any_ocean_biome(biomes).unwrap_or(land_biome_for_materials)
            };

            // Ocean height from ocean biome settings
            let h_ocean = {
                let amp = ocean_biome.settings.seafloor_amp.unwrap_or(OCEAN_AMP);
                let off = ocean_biome.settings.height_offset;
                let freq = ocean_biome.settings.seafloor_freq.unwrap_or(OCEAN_FREQ);
                let freq_scale = (freq / OCEAN_FREQ).clamp(0.2, 4.0);
                sample_ocean_height(&seafloor_n, wxf, wzf, SEA_LEVEL, off, amp, freq_scale)
            };

            // Coast mask: only fade to ocean if outside *all* land regions
            let s_for_coast = s0.min(s1);
            let coast_offset =
                (map01(coast_n.get_noise_2d(wxf, wzf)) - 0.5) * 2.0 * COAST_NOISE_AMP_SCORE;
            let t_ocean = smoothstep(
                1.0 - COAST_INSET_SCORE + coast_offset,
                1.0 + COAST_BAND_SCORE + coast_offset,
                s_for_coast,
            );
            let t_land = 1.0 - t_ocean;

            // Final height with optional sea floor clamp near open ocean
            let mut h_f =
                lerp(h_land, h_ocean, t_ocean).clamp((Y_MIN + 1) as f32, (SEA_LEVEL + 170) as f32);
            if t_ocean > 0.55 {
                h_f = h_f.min((SEA_LEVEL - 1) as f32);
            }
            let mut river_tunnel_weight = 0.0f32;
            let mut river_tunnel_center_y = SEA_LEVEL - 6;
            let mut river_tunnel_half = 0i32;
            let mut river_tunnel_roof = 0i32;

            {
                // 1) Local river permission from the two land sites (soft biome fade).
                let allow0 = if land0.generation.rivers.enabled() {
                    w0
                } else {
                    0.0
                };
                let allow1 = if let Some(land1) = land1_opt {
                    if land1.generation.rivers.enabled() {
                        w1
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };
                let perm_here = ((allow0 + allow1) / w_sum).clamp(0.0, 1.0);

                let generate = &land_biome_for_materials.generation;

                /* ==================== RIVER PATCH (uses potential_gated_smoothed) ====================
                 * English:
                 * - We compute a smoothed, chance-gated river potential via RiverSystem::potential_gated_smoothed.
                 * - We still respect cross-site permission (perm_here) by modulating the depth.
                 * - We keep neighbor continuation: if chance==0 but the river is coming from a neighbor,
                 *   we allow a gentle tail so ends never stop abruptly at the biome boundary.
                 */
                if generate.rivers.enabled() && perm_here > 0.02 {
                    let allow_sum = (allow0 + allow1).max(1e-6);
                    let chance0 = if allow0 > 0.0 {
                        land0.generation.river_chance
                    } else {
                        0.0
                    };
                    let chance1 = if allow1 > 0.0 {
                        land1_opt
                            .map(|b| b.generation.river_chance)
                            .unwrap_or(chance0)
                    } else {
                        0.0
                    };
                    let river_chance_here =
                        ((chance0 * allow0 + chance1 * allow1) / allow_sum).clamp(0.0, 1.0);

                    let size0 = if allow0 > 0.0 {
                        land0.generation.river_size_between
                    } else {
                        generate.river_size_between
                    };
                    let size1 = if allow1 > 0.0 {
                        land1_opt
                            .map(|b| b.generation.river_size_between)
                            .unwrap_or(size0)
                    } else {
                        size0
                    };
                    let lo_blend = ((size0.0 as f32 * allow0 + size1.0 as f32 * allow1) / allow_sum)
                        .round() as i32;
                    let hi_blend = ((size0.1 as f32 * allow0 + size1.1 as f32 * allow1) / allow_sum)
                        .round() as i32;
                    let river_size_between = if lo_blend <= hi_blend {
                        (lo_blend, hi_blend)
                    } else {
                        (hi_blend, lo_blend)
                    };

                    let tunnel_pref0 = if allow0 > 0.0 && land0.generation.rivers.tunnel {
                        allow0
                    } else {
                        0.0
                    };
                    let tunnel_pref1 = if allow1 > 0.0
                        && land1_opt
                            .map(|b| b.generation.rivers.tunnel)
                            .unwrap_or(false)
                    {
                        allow1
                    } else {
                        0.0
                    };
                    let stop_pref0 = if allow0 > 0.0 && land0.generation.rivers.stop {
                        allow0
                    } else {
                        0.0
                    };
                    let stop_pref1 = if allow1 > 0.0
                        && land1_opt.map(|b| b.generation.rivers.stop).unwrap_or(false)
                    {
                        allow1
                    } else {
                        0.0
                    };
                    let tunnel_mode_pref = tunnel_pref0 + tunnel_pref1;
                    let stop_mode_pref = stop_pref0 + stop_pref1;
                    let mode_pref_sum = (tunnel_mode_pref + stop_mode_pref).max(1e-6);
                    let tunnel_mode_ratio = (tunnel_mode_pref / mode_pref_sum).clamp(0.0, 1.0);

                    // Stable width per coarse tile
                    let width_blocks = river.tile_width_blocks(wx, wz, river_size_between);

                    // Base smoothed potential from the river field (already handles tile gating and longitudinal fade)
                    let mut p_smoothed = river.potential_gated_smoothed(
                        wxf,
                        wzf,
                        width_blocks,
                        river_chance_here, // blended local chance threshold
                        true,              // rivers_enabled at this biome
                    );

                    // If this biome's chance is 0.0, only allow continuation from neighbors:
                    if river_chance_here <= 0.0001 {
                        if neighbor_has_river {
                            // soften a bit so the tail looks natural
                            p_smoothed *= 0.90;
                        } else {
                            p_smoothed = 0.0;
                        }
                    }

                    // Modulate by cross-site permission -> soft fade across biome blends
                    let width_fac = (width_blocks as f32 / 16.0).clamp(0.75, 2.0);
                    let p_eff = p_smoothed * perm_here.powf(1.15 + 0.60 * width_fac);

                    if p_eff > 0.0 {
                        let h_before = h_f;
                        let rel_h = (h_before - SEA_LEVEL as f32).max(0.0);
                        let selector = map01(river_mode_n.get_noise_2d(wxf, wzf));
                        let tunnel_pick = smoothstep(0.34, 0.66, selector);
                        let (tunnel_w, stop_w) = if tunnel_mode_pref <= 0.0 {
                            (0.0, 1.0)
                        } else if stop_mode_pref <= 0.0 {
                            (1.0, 0.0)
                        } else {
                            // Blend smooth random field with biome preference to avoid sharp mode seams.
                            let t = (0.58 * tunnel_pick + 0.42 * tunnel_mode_ratio).clamp(0.0, 1.0);
                            (t, 1.0 - t)
                        };

                        // STOP profile: gradually fades out before high mountains, with runout blend.
                        let h_stop = {
                            let stop_fac = 1.0 - smoothstep(18.0, 92.0, rel_h);
                            let stop_bias = stop_fac * (0.65 + 0.35 * stop_fac);
                            let p_mode = p_eff * stop_bias;
                            let carved = if p_mode <= 0.0005 {
                                h_before
                            } else {
                                river.carve_height(h_before, p_mode, width_blocks, SEA_LEVEL as f32)
                            };
                            let runout = smoothstep(24.0, 104.0, rel_h);
                            lerp(carved, h_before, runout * 0.55)
                        };

                        // TUNNEL profile: keep mountain silhouette by limiting visible surface cut.
                        let h_tunnel = {
                            let desired =
                                river.carve_height(h_before, p_eff, width_blocks, SEA_LEVEL as f32);
                            let max_cut = lerp(12.0, 1.6, smoothstep(22.0, 96.0, rel_h));
                            let cut = (h_before - desired).max(0.0).min(max_cut);
                            let carved = h_before - cut;
                            let high_soft = smoothstep(96.0, 138.0, rel_h);
                            lerp(carved, h_before, high_soft * 0.30)
                        };

                        let mut h_carved = (h_stop * stop_w + h_tunnel * tunnel_w)
                            .clamp((Y_MIN + 1) as f32, (SEA_LEVEL + 170) as f32);

                        // Small safety against sudden large cuts if p_smoothed dropped quickly:
                        // reduce the final cut by ~20% near the boundary (keeps lips rounded).
                        if p_smoothed < 0.25 {
                            h_carved = h_carved + (h_before - h_carved) * 0.20;
                        }

                        h_f = h_carved;

                        let tunnel_core = smoothstep(0.48, 0.92, p_eff);
                        let tunnel_strength = tunnel_w * tunnel_core;
                        if tunnel_strength > 0.28 && rel_h >= 26.0 {
                            let y_jit =
                                (map01(coast_d.get_noise_2d(wxf * 0.55 + 17.0, wzf * 0.55 - 11.0))
                                    - 0.5)
                                    * 3.0;
                            let depth_bias = smoothstep(26.0, 150.0, rel_h);
                            river_tunnel_center_y =
                                (SEA_LEVEL as f32 - 6.0 - depth_bias * 9.0 + y_jit).round() as i32;
                            river_tunnel_half = (1.5 + 2.7 * tunnel_strength).round() as i32;
                            river_tunnel_roof =
                                (9.0 + rel_h * 0.10).round().clamp(9.0, 24.0) as i32;
                            river_tunnel_weight = tunnel_strength;
                        }
                    }
                }
                /* ================== /RIVER PATCH END ================== */
            }

            let h_final = h_f.round() as i32;

            // Dominant materials from land vs. ocean
            let dom_biome = if t_land >= 0.5 {
                land_biome_for_materials
            } else {
                ocean_biome
            };

            // Resolve block ids for surface/strata
            let top_name = pick(&dom_biome.surface.top, wx, wz, pick_seed ^ 0x11);
            let bottom_name = pick(&dom_biome.surface.bottom, wx, wz, pick_seed ^ 0x22);
            let sea_floor_name = pick_clustered_noise(
                &ocean_biome.surface.sea_floor,
                &seafloor_mat_n,
                &seafloor_mat_d,
                wxf,
                wzf,
            );
            let upper_zero_name = pick(&dom_biome.surface.upper_zero, wx, wz, pick_seed ^ 0x44);

            let id_top = reg.id_or_air(top_name);
            let id_bottom = reg.id_or_air(bottom_name);
            let id_sea_floor = reg.id_or_air(sea_floor_name);
            let id_upper_zero = reg.id_or_air(upper_zero_name);

            // NEW: resolve under/upper-zero for land and ocean biomes
            let under_zero_name = pick(&dom_biome.surface.under_zero, wx, wz, pick_seed ^ 0x55);
            let id_under_zero = reg.id_or_air(under_zero_name);

            let ocean_upper_zero_name =
                pick(&ocean_biome.surface.upper_zero, wx, wz, pick_seed ^ 0x66);
            let ocean_under_zero_name =
                pick(&ocean_biome.surface.under_zero, wx, wz, pick_seed ^ 0x77);
            let id_ocean_upper_zero = reg.id_or_air(ocean_upper_zero_name);
            let id_ocean_under_zero = reg.id_or_air(ocean_under_zero_name);
            let id_deep_stone = reg.id_opt("deep_stone_block").unwrap_or(0);

            // Beach cap width with detail noise
            let bw_noise = map01(coast_d.get_noise_2d(wxf, wzf));
            let beach_cap = BEACH_MIN + ((BEACH_MAX - BEACH_MIN) as f32 * bw_noise).round() as i32;

            // Adaptive soil from the dominating land site
            let soil_cap = if w0 >= w1 { soil0 } else { soil1 };
            let deep_mix = 0.72 * map01(underground_blob_n.get_noise_2d(wxf, wzf))
                + 0.28 * map01(underground_mat_n.get_noise_2d(wxf, wzf));
            let deep_sel =
                map01(underground_mat_n.get_noise_2d(wxf * 1.7 + 91.3, wzf * 1.7 - 47.2));
            let id_deep_mix = if deep_sel > 0.53 { id_gravel } else { id_dirt };
            let beach_mix = 0.82 * map01(seafloor_mat_n.get_noise_2d(wxf * 0.85, wzf * 0.85))
                + 0.18 * map01(seafloor_mat_d.get_noise_2d(wxf * 0.85, wzf * 0.85));
            let snowy_shore = land_biome_for_materials
                .name
                .eq_ignore_ascii_case("snow_plains")
                || land_biome_for_materials
                    .name
                    .eq_ignore_ascii_case("snowy_mountains");
            let gravel_threshold = if snowy_shore { 0.30 } else { 0.87 };
            let id_beach = if beach_mix > gravel_threshold {
                id_gravel
            } else {
                id_sand
            };

            // --- COLUMN WRITE LOOP (unchanged) ---------------------------------
            // Split at y=0: upper_zero for wy >= 0, under_zero for wy < 0.
            // Ensure the seabed (when underwater) is at least 3 blocks thick.
            for ly in 0..CY {
                let wy = Y_MIN + ly as i32;
                if wy > h_final {
                    break;
                }

                let carve_river_tunnel = river_tunnel_weight > 0.0
                    && river_tunnel_half > 0
                    && wy <= h_final - river_tunnel_roof
                    && (wy - river_tunnel_center_y).abs() <= river_tunnel_half;
                if carve_river_tunnel {
                    // Tunnel river: water in lower half, air in upper half -> cave-like passage, no gorge.
                    let id = if wy <= river_tunnel_center_y - 1 {
                        id_water
                    } else {
                        id_air
                    };
                    if id != 0 {
                        chunk.set(lx, ly, lz, id);
                    }
                    continue;
                }

                let underwater = h_final < SEA_LEVEL;

                let mut id: BlockId = if underwater {
                    // OCEAN: seabed is at h_final; enforce ≥3 blocks of sea_floor.
                    let depth_from_seabed = (h_final - wy).max(0);
                    if depth_from_seabed <= 2 {
                        // Top 3 layers of seabed
                        let shallow_coast = (SEA_LEVEL - h_final).clamp(0, 999) <= 4;
                        if shallow_coast {
                            id_beach
                        } else {
                            id_sea_floor
                        }
                    } else {
                        // Below seabed: use ocean biome's zero-split
                        let depth_wave =
                            ((wy as f32) * 0.11 + deep_sel * std::f32::consts::TAU).sin() * 0.08;
                        let use_deep_mix = depth_from_seabed > 5
                            && wy <= 30
                            && wy >= -96
                            && deep_mix + depth_wave > 0.70;
                        if use_deep_mix {
                            id_deep_mix
                        } else if wy >= 0 {
                            id_ocean_upper_zero
                        } else {
                            id_ocean_under_zero
                        }
                    }
                } else {
                    // LAND: normal surface/soil then zero-split core below soil.
                    let near_coast =
                        t_ocean > 0.10 && t_ocean < 0.90 && (h_final - SEA_LEVEL).abs() <= 5;

                    if wy == h_final {
                        if near_coast { id_beach } else { id_top }
                    } else if wy >= h_final - if near_coast { beach_cap } else { soil_cap } {
                        if near_coast { id_beach } else { id_bottom }
                    } else {
                        // Core below the soil horizon -> split at world zero
                        let depth_from_surface = (h_final - wy).max(0);
                        let depth_wave =
                            ((wy as f32) * 0.11 + deep_sel * std::f32::consts::TAU).sin() * 0.08;
                        let use_deep_mix = depth_from_surface >= 5
                            && wy <= 42
                            && wy >= -96
                            && deep_mix + depth_wave > 0.70;

                        if use_deep_mix {
                            id_deep_mix
                        } else if wy >= 0 {
                            id_upper_zero
                        } else {
                            id_under_zero
                        }
                    }
                };

                // Enforce deep_stone only below Y -20.
                if id_deep_stone != 0 && id == id_deep_stone && wy > -20 {
                    id = if underwater {
                        if id_ocean_under_zero != 0 && id_ocean_under_zero != id_deep_stone {
                            id_ocean_under_zero
                        } else if id_sea_floor != 0 && id_sea_floor != id_deep_stone {
                            id_sea_floor
                        } else if id_sand != 0 {
                            id_sand
                        } else {
                            id_gravel
                        }
                    } else if id_under_zero != 0 && id_under_zero != id_deep_stone {
                        id_under_zero
                    } else if id_bottom != 0 && id_bottom != id_deep_stone {
                        id_bottom
                    } else if id_upper_zero != 0 && id_upper_zero != id_deep_stone {
                        id_upper_zero
                    } else if id_dirt != 0 {
                        id_dirt
                    } else {
                        id_gravel
                    };
                }

                if id != 0 {
                    chunk.set(lx, ly, lz, id);
                }
            }

            // Always enforce bedrock/border at the world bottom (ly == 0 -> Y_MIN)
            chunk.set(lx, 0, lz, id_border);
        }
    }

    carve_caves_into_chunk(&mut chunk, coord, cfg_seed, id_air, id_water, id_border);
    populate_trees_in_chunk(&mut chunk, coord, reg, biomes, trees, cfg_seed);
    populate_vegetation_props_in_chunk(&mut chunk, coord, reg, biomes, cfg_seed);
    chunk
}

#[inline]
fn default_cave_params(seed: i32) -> CaveParams {
    CaveParams {
        seed,
        y_top: 52,
        y_bottom: -110,
        worms_per_region: 1.65,
        region_chunks: 3,
        base_radius: 4.1,
        radius_var: 3.8,
        step_len: 1.45,
        worm_len_steps: 400,
        room_event_chance: 0.16,
        room_radius_min: 6.5,
        room_radius_max: 12.5,
        caverns_per_region: 0.72,
        cavern_room_count_min: 6,
        cavern_room_count_max: 13,
        cavern_room_radius_xz_min: 14.0,
        cavern_room_radius_xz_max: 38.0,
        cavern_room_radius_y_min: 8.5,
        cavern_room_radius_y_max: 23.0,
        cavern_connector_radius: 11.5,
        cavern_y_top: -10,
        cavern_y_bottom: -100,
        mega_caverns_per_region: 0.11,
        mega_room_count_min: 1,
        mega_room_count_max: 4,
        mega_room_radius_xz_min: 44.0,
        mega_room_radius_xz_max: 156.0,
        mega_room_radius_y_min: 20.0,
        mega_room_radius_y_max: 52.0,
        mega_connector_radius: 10.0,
        mega_y_top: -30,
        mega_y_bottom: -105,
        entrance_chance: 0.62,
        entrance_len_steps: 48,
        entrance_radius_scale: 0.55,
        entrance_min_radius: 2.8,
        entrance_trigger_band: 12.0,
    }
}

#[inline]
fn carve_caves_into_chunk(
    chunk: &mut ChunkData,
    coord: IVec2,
    seed: i32,
    id_air: BlockId,
    id_water: BlockId,
    id_border: BlockId,
) {
    let params = default_cave_params(seed);
    let edits = worm_edits_for_chunk(
        &params,
        coord,
        IVec2::new(CX as i32, CZ as i32),
        Y_MIN,
        Y_MAX,
    );
    for (lx, ly, lz) in edits {
        let x = lx as usize;
        let y = ly as usize;
        let z = lz as usize;
        let cur = chunk.get(x, y, z);
        if cur != 0 && cur != id_water && cur != id_border {
            chunk.set(x, y, z, id_air);
        }
    }
}

/* ============================= Noises ======================================= */

/// Creates seafloor noise for the `generator::chunk::chunk_gen` module.
fn make_seafloor_noise(seed: i32, freq: f32) -> FastNoiseLite {
    let mut n = FastNoiseLite::with_seed(seed);
    n.set_noise_type(Some(NoiseType::OpenSimplex2));
    n.set_frequency(Some(freq));
    n.set_fractal_type(Some(FractalType::FBm));
    n.set_fractal_octaves(Some(3));
    n.set_fractal_gain(Some(0.5));
    n.set_fractal_lacunarity(Some(2.0));
    n
}

/// Creates plains noise for the `generator::chunk::chunk_gen` module.
fn make_plains_noise(seed: i32, freq: f32) -> FastNoiseLite {
    const SEED_SALT_PLAINS: i32 = 0x504C_4149;
    let mut n = FastNoiseLite::with_seed(seed ^ SEED_SALT_PLAINS);
    n.set_noise_type(Some(NoiseType::OpenSimplex2));
    n.set_frequency(Some(freq));
    n.set_fractal_type(Some(FractalType::FBm));
    n.set_fractal_octaves(Some(4));
    n.set_fractal_gain(Some(0.5));
    n.set_fractal_lacunarity(Some(2.0));
    n
}

/// Creates coast noise for the `generator::chunk::chunk_gen` module.
fn make_coast_noise(seed: i32, freq: f32) -> FastNoiseLite {
    let mut n = FastNoiseLite::with_seed(seed);
    n.set_noise_type(Some(NoiseType::OpenSimplex2));
    n.set_frequency(Some(freq));
    n.set_fractal_type(Some(FractalType::FBm));
    n.set_fractal_octaves(Some(3));
    n.set_fractal_gain(Some(0.5));
    n.set_fractal_lacunarity(Some(2.0));
    n
}

/* ============================= Height Samplers =============================== */

/// Runs the `sample_ocean_height` routine for sample ocean height in the `generator::chunk::chunk_gen` module.
#[inline]
fn sample_ocean_height(
    n: &FastNoiseLite,
    wxf: f32,
    wzf: f32,
    sea_level: i32,
    height_offset: f32,
    seafloor_amp: f32,
    seafloor_freq_scale: f32,
) -> f32 {
    // Slightly slower noise to make a sea floor undulate more gently
    let s = (0.9 * seafloor_freq_scale).clamp(0.1, 4.0);
    let hn = map01(n.get_noise_2d(wxf * s, wzf * s));
    let undulation = (hn - 0.5) * seafloor_amp;
    let base = sea_level as f32 + height_offset;
    clamp_world_y(base + undulation).min((sea_level - 2) as f32)
}

/// Runs the `sample_plains_height` routine for sample plains height in the `generator::chunk::chunk_gen` module.
#[inline]
fn sample_plains_height(
    n: &FastNoiseLite,
    wxf: f32,
    wzf: f32,
    sea_level: i32,
    height_offset: f32,
    land_amp: f32,
    land_freq_scale: f32,
) -> f32 {
    // Land uses FBm around the biome's base offset
    let sf = land_freq_scale.clamp(0.2, 4.0);
    let hn = map01(n.get_noise_2d(wxf * sf, wzf * sf));
    let undulation = (hn - 0.5) * land_amp;
    let base = sea_level as f32 + height_offset;
    clamp_world_y(base + undulation)
}

#[inline]
fn sample_land_uplift(
    broad_n: &FastNoiseLite,
    ridge_n: &FastNoiseLite,
    wxf: f32,
    wzf: f32,
    land_amp: f32,
) -> f32 {
    let broad = map01(broad_n.get_noise_2d(wxf, wzf));
    let ridge = 1.0 - ridge_n.get_noise_2d(wxf, wzf).abs();
    let highland = smoothstep(0.58, 0.93, broad);
    let ridge_core = smoothstep(0.52, 0.92, ridge).powf(1.2);
    let lift_base = (land_amp * 0.18).clamp(0.25, 11.0);
    lift_base * highland + lift_base * 1.15 * highland * ridge_core
}

/* ============= Mountains (plains-based delta composer) =================== */

/// Runs the `sample_plains_mountain_delta` routine for sample plains mountain delta in the `generator::chunk::chunk_gen` module.
#[inline]
fn sample_plains_mountain_delta(n: &FastNoiseLite, ux: f32, uz: f32, amp_json: f32) -> f32 {
    // Two bands at different scales mixed towards broader shapes
    let b = map01(n.get_noise_2d(ux * 0.60, uz * 0.60));
    let d = map01(n.get_noise_2d(ux * 1.20, uz * 1.20));
    let s = 0.85 * b + 0.15 * d;

    // Emphasize peaks while keeping foothills shallow
    let peak = smoothstep(0.48, 0.86, s).powf(1.05);

    // Slight amplitude modulation to avoid uniformity
    let a_mod = 0.90 + 0.20 * map01(n.get_noise_2d(ux * 0.25, uz * 0.25));

    amp_json * a_mod * peak
}

#[inline]
fn pick_clustered_noise<'a>(
    list: &'a [String],
    macro_n: &FastNoiseLite,
    detail_n: &FastNoiseLite,
    wxf: f32,
    wzf: f32,
) -> &'a str {
    if list.is_empty() {
        return "stone_block";
    }
    if list.len() == 1 {
        return &list[0];
    }

    let broad = map01(macro_n.get_noise_2d(wxf, wzf));
    let detail = map01(detail_n.get_noise_2d(wxf, wzf));
    let v = (0.84 * broad + 0.16 * detail.powf(1.35)).clamp(0.0, 0.999_999);
    let idx = (v * list.len() as f32).floor() as usize;
    &list[idx.min(list.len() - 1)]
}
