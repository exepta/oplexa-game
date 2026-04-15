{
    // Secondary stacked slab pass (same voxel, second occupant).
    for y in y0..y1 {
        for z in 0..CZ {
            for x in 0..CX {
                let id = chunk.get_stacked(x, y, z);
                if !reg.mesh_visible(id) {
                    continue;
                }
                if reg.fluid(id) {
                    // Fluids are rendered from the primary occupant pass.
                    // Skipping here prevents duplicate transparent geometry.
                    continue;
                }
                let Some((size_m, offset_m)) = reg.custom_mesh_box(id) else {
                    continue;
                };

                let u_top = reg.uv(id, Face::Top);
                let u_bottom = reg.uv(id, Face::Bottom);
                let u_east = reg.uv(id, Face::East);
                let u_west = reg.uv(id, Face::West);
                let u_south = reg.uv(id, Face::South);
                let u_north = reg.uv(id, Face::North);
                let b = by_block.entry(id).or_insert_with(MeshBuild::new);

                let half_x = (size_m[0] * s * 0.5).max(0.0005);
                let half_y = (size_m[1] * s * 0.5).max(0.0005);
                let half_z = (size_m[2] * s * 0.5).max(0.0005);
                let cx = (x as f32 + 0.5 + offset_m[0]) * s;
                let cy = (y as f32 + 0.5 + offset_m[1]) * s;
                let cz = (z as f32 + 0.5 + offset_m[2]) * s;
                let min_x = cx - half_x;
                let max_x = cx + half_x;
                let min_y = cy - half_y;
                let max_y = cy + half_y;
                let min_z = cz - half_z;
                let max_z = cz + half_z;
                let connected = reg.has_connected_mask4(id);
                let framed_slab = connected
                    && reg.connected_edge_clip_uv(id) > 0.0
                    && (size_m[0] < 0.999 || size_m[1] < 0.999 || size_m[2] < 0.999);
                let uv_span =
                    |dim: f32| -> f32 { if framed_slab && dim < 0.999 { 1.0 } else { dim } };
                let same_cell_other = chunk.get(x, y, z);
                let connected_mask_for_face = |face: Face| -> u8 {
                    let neighbor_mask =
                        connected_neighbor_edge_mask(id, size_m, offset_m, face, x, y, z);
                    let same_cell_mask =
                        same_cell_connected_edge_mask(id, size_m, offset_m, same_cell_other, face);
                    neighbor_mask | same_cell_mask
                };

                if (!connected || face_visible(id, face_neighbor(Face::Top, x, y, z)))
                    && !(connected
                        && connected_neighbor_occludes_face(
                            id,
                            size_m,
                            offset_m,
                            Face::Top,
                            x,
                            y,
                            z,
                        ))
                    && !same_cell_connected_occludes_face(
                        id,
                        size_m,
                        offset_m,
                        same_cell_other,
                        Face::Top,
                    )
                {
                    let mask = connected_mask_for_face(Face::Top);
                    b.quad_with_ctm(
                        [
                            [min_x, max_y, max_z],
                            [max_x, max_y, max_z],
                            [max_x, max_y, min_z],
                            [min_x, max_y, min_z],
                        ],
                        [0.0, 1.0, 0.0],
                        uvq_tiled(uv_span(size_m[0]), uv_span(size_m[2]), false),
                        if connected {
                            let uv = reg
                                .connected_mask4_uv(id, mask)
                                .unwrap_or_else(|| reg.uv(id, Face::Top));
                            [uv.u0, uv.v0, uv.u1, uv.v1]
                        } else {
                            [u_top.u0, u_top.v0, u_top.u1, u_top.v1]
                        },
                        if connected {
                            [mask as f32, reg.connected_edge_clip_uv(id)]
                        } else {
                            [-1.0, 0.0]
                        },
                    );
                }
                if (!connected || face_visible(id, face_neighbor(Face::Bottom, x, y, z)))
                    && !(connected
                        && connected_neighbor_occludes_face(
                            id,
                            size_m,
                            offset_m,
                            Face::Bottom,
                            x,
                            y,
                            z,
                        ))
                    && !same_cell_connected_occludes_face(
                        id,
                        size_m,
                        offset_m,
                        same_cell_other,
                        Face::Bottom,
                    )
                {
                    let mask = connected_mask_for_face(Face::Bottom);
                    b.quad_with_ctm(
                        [
                            [min_x, min_y, min_z],
                            [max_x, min_y, min_z],
                            [max_x, min_y, max_z],
                            [min_x, min_y, max_z],
                        ],
                        [0.0, -1.0, 0.0],
                        uvq_tiled(uv_span(size_m[0]), uv_span(size_m[2]), false),
                        if connected {
                            let uv = reg
                                .connected_mask4_uv(id, mask)
                                .unwrap_or_else(|| reg.uv(id, Face::Bottom));
                            [uv.u0, uv.v0, uv.u1, uv.v1]
                        } else {
                            [u_bottom.u0, u_bottom.v0, u_bottom.u1, u_bottom.v1]
                        },
                        if connected {
                            [mask as f32, reg.connected_edge_clip_uv(id)]
                        } else {
                            [-1.0, 0.0]
                        },
                    );
                }
                if (!connected || face_visible(id, face_neighbor(Face::East, x, y, z)))
                    && !(connected
                        && connected_neighbor_occludes_face(
                            id,
                            size_m,
                            offset_m,
                            Face::East,
                            x,
                            y,
                            z,
                        ))
                    && !same_cell_connected_occludes_face(
                        id,
                        size_m,
                        offset_m,
                        same_cell_other,
                        Face::East,
                    )
                {
                    let mask = connected_mask_for_face(Face::East);
                    b.quad_with_ctm(
                        [
                            [max_x, min_y, max_z],
                            [max_x, min_y, min_z],
                            [max_x, max_y, min_z],
                            [max_x, max_y, max_z],
                        ],
                        [1.0, 0.0, 0.0],
                        uvq_tiled(uv_span(size_m[2]), uv_span(size_m[1]), true),
                        if connected {
                            let uv = reg
                                .connected_mask4_uv(id, mask)
                                .unwrap_or_else(|| reg.uv(id, Face::East));
                            [uv.u0, uv.v0, uv.u1, uv.v1]
                        } else {
                            [u_east.u0, u_east.v0, u_east.u1, u_east.v1]
                        },
                        if connected {
                            [mask as f32, reg.connected_edge_clip_uv(id)]
                        } else {
                            [-1.0, 0.0]
                        },
                    );
                }
                if (!connected || face_visible(id, face_neighbor(Face::West, x, y, z)))
                    && !(connected
                        && connected_neighbor_occludes_face(
                            id,
                            size_m,
                            offset_m,
                            Face::West,
                            x,
                            y,
                            z,
                        ))
                    && !same_cell_connected_occludes_face(
                        id,
                        size_m,
                        offset_m,
                        same_cell_other,
                        Face::West,
                    )
                {
                    let mask = connected_mask_for_face(Face::West);
                    b.quad_with_ctm(
                        [
                            [min_x, min_y, min_z],
                            [min_x, min_y, max_z],
                            [min_x, max_y, max_z],
                            [min_x, max_y, min_z],
                        ],
                        [-1.0, 0.0, 0.0],
                        uvq_tiled(uv_span(size_m[2]), uv_span(size_m[1]), true),
                        if connected {
                            let uv = reg
                                .connected_mask4_uv(id, mask)
                                .unwrap_or_else(|| reg.uv(id, Face::West));
                            [uv.u0, uv.v0, uv.u1, uv.v1]
                        } else {
                            [u_west.u0, u_west.v0, u_west.u1, u_west.v1]
                        },
                        if connected {
                            [mask as f32, reg.connected_edge_clip_uv(id)]
                        } else {
                            [-1.0, 0.0]
                        },
                    );
                }
                if (!connected || face_visible(id, face_neighbor(Face::South, x, y, z)))
                    && !(connected
                        && connected_neighbor_occludes_face(
                            id,
                            size_m,
                            offset_m,
                            Face::South,
                            x,
                            y,
                            z,
                        ))
                    && !same_cell_connected_occludes_face(
                        id,
                        size_m,
                        offset_m,
                        same_cell_other,
                        Face::South,
                    )
                {
                    let mask = connected_mask_for_face(Face::South);
                    b.quad_with_ctm(
                        [
                            [min_x, min_y, max_z],
                            [max_x, min_y, max_z],
                            [max_x, max_y, max_z],
                            [min_x, max_y, max_z],
                        ],
                        [0.0, 0.0, 1.0],
                        uvq_tiled(uv_span(size_m[0]), uv_span(size_m[1]), true),
                        if connected {
                            let uv = reg
                                .connected_mask4_uv(id, mask)
                                .unwrap_or_else(|| reg.uv(id, Face::South));
                            [uv.u0, uv.v0, uv.u1, uv.v1]
                        } else {
                            [u_south.u0, u_south.v0, u_south.u1, u_south.v1]
                        },
                        if connected {
                            [mask as f32, reg.connected_edge_clip_uv(id)]
                        } else {
                            [-1.0, 0.0]
                        },
                    );
                }
                if (!connected || face_visible(id, face_neighbor(Face::North, x, y, z)))
                    && !(connected
                        && connected_neighbor_occludes_face(
                            id,
                            size_m,
                            offset_m,
                            Face::North,
                            x,
                            y,
                            z,
                        ))
                    && !same_cell_connected_occludes_face(
                        id,
                        size_m,
                        offset_m,
                        same_cell_other,
                        Face::North,
                    )
                {
                    let mask = connected_mask_for_face(Face::North);
                    b.quad_with_ctm(
                        [
                            [max_x, min_y, min_z],
                            [min_x, min_y, min_z],
                            [min_x, max_y, min_z],
                            [max_x, max_y, min_z],
                        ],
                        [0.0, 0.0, -1.0],
                        uvq_tiled(uv_span(size_m[0]), uv_span(size_m[1]), true),
                        if connected {
                            let uv = reg
                                .connected_mask4_uv(id, mask)
                                .unwrap_or_else(|| reg.uv(id, Face::North));
                            [uv.u0, uv.v0, uv.u1, uv.v1]
                        } else {
                            [u_north.u0, u_north.v0, u_north.u1, u_north.v1]
                        },
                        if connected {
                            [mask as f32, reg.connected_edge_clip_uv(id)]
                        } else {
                            [-1.0, 0.0]
                        },
                    );
                }
            }
        }
    }

    // Prop pass: crossed planes (Minecraft/Hytale style plants).
    for y in y0..y1 {
        for z in 0..CZ {
            for x in 0..CX {
                let id = chunk.get(x, y, z);
                if !reg.mesh_visible(id) {
                    continue;
                }
                let Some(prop) = reg.prop(id) else {
                    continue;
                };
                if !prop.is_crossed_planes() {
                    continue;
                }

                let u = reg.uv(id, Face::North);
                let tile_rect = [u.u0, u.v0, u.u1, u.v1];
                let b = by_block.entry(id).or_insert_with(MeshBuild::new);

                let cx = (x as f32 + 0.5) * s;
                let cy0 = y as f32 * s;
                let cy1 = cy0 + prop.height_m * s;
                let cz = (z as f32 + 0.5) * s;
                let half_w = 0.5 * prop.width_m * s;
                let plane_count = prop.plane_count.max(2) as usize;
                let uv = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
                let seed = ((y as u32) << 16) ^ (id as u32).wrapping_mul(1_315_423_911);
                let h0 = col_rand_u32(x as i32, z as i32, seed);
                let h1 = col_rand_u32(z as i32, x as i32, seed ^ 0xA511_E9B3);
                let base_angle = (h0 as f32 / u32::MAX as f32) * std::f32::consts::PI;
                let lean_angle = (h1 as f32 / u32::MAX as f32) * std::f32::consts::TAU;
                let lean_len = (prop.height_m * s) * prop.tilt_deg.to_radians().tan();
                let lean = Vec2::new(lean_angle.cos(), lean_angle.sin()) * lean_len;

                for i in 0..plane_count {
                    let angle =
                        base_angle + (i as f32) * std::f32::consts::PI / (plane_count as f32);
                    let dir = Vec2::new(angle.cos(), angle.sin());

                    let p0 = [cx - dir.x * half_w, cy0, cz - dir.y * half_w];
                    let p1 = [cx + dir.x * half_w, cy0, cz + dir.y * half_w];
                    let p2 = [
                        cx + dir.x * half_w + lean.x,
                        cy1,
                        cz + dir.y * half_w + lean.y,
                    ];
                    let p3 = [
                        cx - dir.x * half_w + lean.x,
                        cy1,
                        cz - dir.y * half_w + lean.y,
                    ];

                    let p0v = Vec3::from(p0);
                    let p1v = Vec3::from(p1);
                    let p3v = Vec3::from(p3);
                    let fallback_normal = Vec3::new(dir.y, 0.0, -dir.x);
                    let mut normal = (p3v - p0v).cross(p1v - p0v);
                    if normal.length_squared() > 1e-6 {
                        normal = normal.normalize();
                    } else {
                        normal = fallback_normal;
                    }

                    b.quad([p0, p1, p2, p3], normal.to_array(), uv, tile_rect);
                    b.quad([p1, p0, p3, p2], (-normal).to_array(), uv, tile_rect);
                }
            }
        }
    }

    // Secondary stacked prop pass (water-logged plants etc.).
    for y in y0..y1 {
        for z in 0..CZ {
            for x in 0..CX {
                let id = chunk.get_stacked(x, y, z);
                if !reg.mesh_visible(id) {
                    continue;
                }
                let Some(prop) = reg.prop(id) else {
                    continue;
                };
                if !prop.is_crossed_planes() {
                    continue;
                }

                let u = reg.uv(id, Face::North);
                let tile_rect = [u.u0, u.v0, u.u1, u.v1];
                let b = by_block.entry(id).or_insert_with(MeshBuild::new);

                let cx = (x as f32 + 0.5) * s;
                let cy0 = y as f32 * s;
                let cy1 = cy0 + prop.height_m * s;
                let cz = (z as f32 + 0.5) * s;
                let half_w = 0.5 * prop.width_m * s;
                let plane_count = prop.plane_count.max(2) as usize;
                let uv = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
                let seed = ((y as u32) << 16) ^ (id as u32).wrapping_mul(1_315_423_911);
                let h0 = col_rand_u32(x as i32, z as i32, seed);
                let h1 = col_rand_u32(z as i32, x as i32, seed ^ 0xA511_E9B3);
                let base_angle = (h0 as f32 / u32::MAX as f32) * std::f32::consts::PI;
                let lean_angle = (h1 as f32 / u32::MAX as f32) * std::f32::consts::TAU;
                let lean_len = (prop.height_m * s) * prop.tilt_deg.to_radians().tan();
                let lean = Vec2::new(lean_angle.cos(), lean_angle.sin()) * lean_len;

                for i in 0..plane_count {
                    let angle =
                        base_angle + (i as f32) * std::f32::consts::PI / (plane_count as f32);
                    let dir = Vec2::new(angle.cos(), angle.sin());

                    let p0 = [cx - dir.x * half_w, cy0, cz - dir.y * half_w];
                    let p1 = [cx + dir.x * half_w, cy0, cz + dir.y * half_w];
                    let p2 = [
                        cx + dir.x * half_w + lean.x,
                        cy1,
                        cz + dir.y * half_w + lean.y,
                    ];
                    let p3 = [
                        cx - dir.x * half_w + lean.x,
                        cy1,
                        cz - dir.y * half_w + lean.y,
                    ];

                    let p0v = Vec3::from(p0);
                    let p1v = Vec3::from(p1);
                    let p3v = Vec3::from(p3);
                    let fallback_normal = Vec3::new(dir.y, 0.0, -dir.x);
                    let mut normal = (p3v - p0v).cross(p1v - p0v);
                    if normal.length_squared() > 1e-6 {
                        normal = normal.normalize();
                    } else {
                        normal = fallback_normal;
                    }

                    b.quad([p0, p1, p2, p3], normal.to_array(), uv, tile_rect);
                    b.quad([p1, p0, p3, p2], (-normal).to_array(), uv, tile_rect);
                }
            }
        }
    }

    by_block.into_iter().map(|(k, b)| (k, b)).collect()
}
