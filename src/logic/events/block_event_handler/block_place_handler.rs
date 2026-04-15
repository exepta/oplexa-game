/// Handles right-click placement for blocks and placeable structures.
fn block_place_handler(
    structure_deps: StructurePlacementDeps,
    buttons: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    sel: Res<SelectionState>,
    selected: Res<SelectedBlock>,
    registry: Res<BlockRegistry>,
    item_registry: Res<ItemRegistry>,
    game_mode: Res<GameModeState>,
    hotbar_selection: Option<Res<HotbarSelectionState>>,
    ui_state: Option<Res<UiInteractionState>>,
    rapier_context: ReadRapierContext,
    q_player_controls: Query<&FpsController, With<Player>>,
    q_structures: Query<&PlacedStructureMetadata>,
    q_structure_parents: Query<&ChildOf>,
    world_deps: PlacementWorldDeps,
) {
    let StructurePlacementDeps {
        mut commands,
        asset_server,
        structure_recipe_registry,
        mut active_structure_recipe,
        mut active_structure_placement,
        mut open_structure_menu_requests,
        mut open_workbench_menu_requests,
        mut open_chest_menu_requests,
    } = structure_deps;
    let PlacementWorldDeps {
        mut inventory,
        mut fluids,
        mut chunk_map,
        multiplayer_connection,
        ws,
        mut region_cache,
        mut structure_runtime,
        mut ev_dirty,
        mut place_ev,
    } = world_deps;

    if ui_state
        .as_ref()
        .is_some_and(|state| state.blocks_game_input())
    {
        return;
    }

    if game_mode.0.eq(&GameMode::Spectator) {
        return;
    }
    if !buttons.just_pressed(MouseButton::Right) {
        return;
    }

    let shift_held = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
    if let Some(structure_hit) = sel.structure_hit
        && let Ok(meta) = q_structures.get(structure_hit.entity)
        && structure_has_chest_ui(meta)
        && !shift_held
    {
        active_structure_recipe.selected_recipe_name = None;
        active_structure_placement.rotation_quarters = 0;
        open_chest_menu_requests.write(OpenChestInventoryMenuRequest {
            world_pos: [meta.place_origin.x, meta.place_origin.y, meta.place_origin.z],
        });
        return;
    }
    if let Some(hit) = sel.hit
        && block_has_chest_ui(get_block_world(&chunk_map, hit.block_pos), &registry)
        && !shift_held
    {
        active_structure_recipe.selected_recipe_name = None;
        active_structure_placement.rotation_quarters = 0;
        open_chest_menu_requests.write(OpenChestInventoryMenuRequest {
            world_pos: [hit.block_pos.x, hit.block_pos.y, hit.block_pos.z],
        });
        return;
    }

    if let Some(structure_hit) = sel.structure_hit
        && let Ok(meta) = q_structures.get(structure_hit.entity)
        && structure_has_workbench_ui(meta)
        && !shift_held
    {
        // Block interaction UI has priority over hammer right-click interaction.
        active_structure_recipe.selected_recipe_name = None;
        active_structure_placement.rotation_quarters = 0;
        open_workbench_menu_requests.write(OpenWorkbenchMenuRequest);
        return;
    }
    if let Some(hit) = sel.hit
        && block_has_workbench_ui(get_block_world(&chunk_map, hit.block_pos), &registry)
        && !shift_held
    {
        // Block interaction UI has priority over hammer right-click interaction.
        active_structure_recipe.selected_recipe_name = None;
        active_structure_placement.rotation_quarters = 0;
        open_workbench_menu_requests.write(OpenWorkbenchMenuRequest);
        return;
    }

    let held_item_id = selected_hotbar_item_id(&inventory, hotbar_selection.as_deref());
    let holding_hammer = held_item_id
        .and_then(|item_id| item_registry.def_opt(item_id))
        .is_some_and(|item| item.localized_name == "oplexa:hammer" || item.key == "hammer");
    if holding_hammer {
        let Some(structure_recipe_registry) = structure_recipe_registry.as_ref() else {
            return;
        };

        if let Some(active_recipe_name) = active_structure_recipe.selected_recipe_name.as_deref() {
            let Some(recipe) = structure_recipe_registry.recipe_by_name(active_recipe_name) else {
                active_structure_recipe.selected_recipe_name = None;
                active_structure_placement.rotation_quarters = 0;
                return;
            };
            let Some(hit) = sel.hit else {
                return;
            };
            let rotation_steps =
                normalize_rotation_steps(active_structure_placement.rotation_quarters) & !1;
            let rotation_quarters = rotation_steps_to_placement_quarters(rotation_steps);
            let place_origin = resolve_structure_place_origin(hit, &chunk_map, &registry);
            if !can_place_structure_recipe_at(
                place_origin,
                recipe,
                rotation_quarters,
                &chunk_map,
                &registry,
            ) {
                return;
            }

            let Some(consumed_requirements) = consume_structure_requirements_from_inventory(
                &mut inventory,
                &recipe.requirements,
                &item_registry,
            ) else {
                return;
            };
            let style_source_item_id = consumed_requirements
                .style_source_item_id
                .or_else(|| resolve_default_structure_style_item_id(recipe, &item_registry));
            let style_source_block_id = style_source_item_id
                .and_then(|item_id| item_registry.block_for_item(item_id))
                .filter(|block_id| *block_id != 0);

            if !multiplayer_connection.uses_local_save_data() {
                let Some(registered_block_id) =
                    structure_runtime_placeholder_block_id(recipe, &registry, rotation_quarters)
                else {
                    bevy::log::warn!(
                        "Structure recipe '{}' has no registered block id for rotation {}; cannot place in multiplayer.",
                        recipe.name,
                        rotation_quarters
                    );
                    return;
                };

                let (chunk_coord, local) = world_to_chunk_xz(place_origin.x, place_origin.z);
                let lx = local.x.clamp(0, (CX as i32 - 1) as u32) as usize;
                let lz = local.y.clamp(0, (CZ as i32 - 1) as u32) as usize;
                let ly = (place_origin.y - Y_MIN).clamp(0, CY as i32 - 1) as usize;
                if let Some(fc) = fluids.0.get_mut(&chunk_coord) {
                    fc.set(lx, ly, lz, false);
                }
                if let Some(mut access) = world_access_mut(&mut chunk_map, place_origin) {
                    access.set(registered_block_id);
                    access.set_stacked(style_source_block_id.unwrap_or(0));
                } else {
                    return;
                }
                mark_dirty_block_and_neighbors(&mut chunk_map, place_origin, &mut ev_dirty);

                let name = registry
                    .name_opt(registered_block_id)
                    .unwrap_or("")
                    .to_string();
                place_ev.write(BlockPlaceByPlayerEvent {
                    location: place_origin,
                    block_id: registered_block_id,
                    stacked_block_id: style_source_block_id.unwrap_or(0),
                    block_name: name,
                });

                let structure_entity = spawn_structure_model_entity(
                    &mut commands,
                    &asset_server,
                    recipe,
                    place_origin,
                    rotation_quarters,
                    rotation_steps,
                    consumed_requirements.drop_requirements.clone(),
                    style_source_item_id,
                );
                clear_props_within_structure_volume(
                    place_origin,
                    recipe,
                    rotation_quarters,
                    &mut chunk_map,
                    &registry,
                    &mut ev_dirty,
                );
                register_structure_in_runtime(
                    &mut structure_runtime,
                    structure_entity,
                    recipe,
                    place_origin,
                    rotation_quarters,
                    rotation_steps,
                    style_source_item_id,
                    consumed_requirements.drop_requirements.as_slice(),
                    &item_registry,
                    multiplayer_connection.uses_local_save_data(),
                    ws.as_deref(),
                    region_cache.as_deref_mut(),
                );

                active_structure_recipe.selected_recipe_name = None;
                active_structure_placement.rotation_quarters = 0;
                return;
            }

            let structure_entity = spawn_structure_model_entity(
                &mut commands,
                &asset_server,
                recipe,
                place_origin,
                rotation_quarters,
                rotation_steps,
                consumed_requirements.drop_requirements.clone(),
                style_source_item_id,
            );
            clear_props_within_structure_volume(
                place_origin,
                recipe,
                rotation_quarters,
                &mut chunk_map,
                &registry,
                &mut ev_dirty,
            );
            register_structure_in_runtime(
                &mut structure_runtime,
                structure_entity,
                recipe,
                place_origin,
                rotation_quarters,
                rotation_steps,
                style_source_item_id,
                consumed_requirements.drop_requirements.as_slice(),
                &item_registry,
                multiplayer_connection.uses_local_save_data(),
                ws.as_deref(),
                region_cache.as_deref_mut(),
            );
            active_structure_recipe.selected_recipe_name = None;
            active_structure_placement.rotation_quarters = 0;
            return;
        }

        active_structure_placement.rotation_quarters = 0;
        open_structure_menu_requests.write(OpenStructureBuildMenuRequest);
        return;
    }

    let id = selected.id;
    if id == 0 {
        return;
    }
    let creative_mode = matches!(game_mode.0, GameMode::Creative);
    if !creative_mode
        && !can_place_from_selected_slot(
            &inventory,
            hotbar_selection.as_deref(),
            id,
            &item_registry,
            &registry,
        )
    {
        return;
    }
    let hit = if let Some(hit) = sel.hit {
        hit
    } else if let Some(structure_hit) = sel.structure_hit {
        let Some(hit) = build_structure_surface_hit(structure_hit, &q_structures) else {
            return;
        };
        hit
    } else {
        return;
    };
    let (player_yaw, player_pitch) = q_player_controls
        .iter()
        .next()
        .map(|ctrl| (ctrl.yaw, ctrl.pitch))
        .unwrap_or((0.0, 0.0));
    let placement =
        resolve_placement_for_selected(id, hit, player_yaw, player_pitch, &chunk_map, &registry);
    let place_id = placement.block_id;
    let mut world_pos = placement.world_pos;
    let mut place_into_stacked = placement.place_into_stacked;
    let hit_primary_id = get_block_world(&chunk_map, hit.block_pos);
    if !hit.is_stacked && hit_primary_id != 0 && registry.is_overridable(hit_primary_id) {
        world_pos = hit.block_pos;
        place_into_stacked = false;
    }
    let hit_stacked_id = get_stacked_block_world(&chunk_map, hit.block_pos);
    if hit.is_stacked
        && hit_stacked_id != 0
        && hit_stacked_id == hit.block_id
        && prop_requires_water_environment(hit_stacked_id, &registry)
        && !registry.is_fluid(place_id)
    {
        // Sea-grass-like stacked props should be replaced when placing solid blocks,
        // same behavior as tall grass (except when placing water itself).
        world_pos = hit.block_pos;
        place_into_stacked = false;
    }
    let (chunk_coord, l) = world_to_chunk_xz(world_pos.x, world_pos.z);
    let lx = l.x.clamp(0, (CX as i32 - 1) as u32) as usize;
    let lz = l.y.clamp(0, (CZ as i32 - 1) as u32) as usize;
    let ly = (world_pos.y - Y_MIN).clamp(0, CY as i32 - 1) as usize;
    let Some(chunk) = chunk_map.chunks.get(&chunk_coord) else {
        return;
    };
    let existing_primary_id = chunk.get(lx, ly, lz);
    let existing_stacked_id = chunk.get_stacked(lx, ly, lz);

    if !place_into_stacked
        && registry.is_water_logged(place_id)
        && existing_primary_id != 0
        && registry.is_fluid(existing_primary_id)
        && existing_stacked_id == 0
    {
        place_into_stacked = true;
    }
    let merge_waterlogged_slab = place_into_stacked
        && slab_cell_accepts_second_slab_in_waterlogged_cell(
            existing_primary_id,
            existing_stacked_id,
            place_id,
            &registry,
        );
    let waterlog_existing_primary = !place_into_stacked
        && registry.is_fluid(place_id)
        && existing_primary_id != 0
        && !registry.is_fluid(existing_primary_id)
        && !is_horizontal_slab_variant(existing_primary_id, &registry)
        && registry.is_water_logged(existing_primary_id)
        && existing_stacked_id == 0;
    let can_place = if waterlog_existing_primary {
        true
    } else if place_into_stacked {
        if merge_waterlogged_slab {
            true
        } else if existing_primary_id == 0 || existing_stacked_id != 0 {
            false
        } else if registry.is_fluid(existing_primary_id) {
            registry.is_water_logged(place_id)
        } else {
            !registry.is_overridable(existing_primary_id)
        }
    } else {
        existing_primary_id == 0 || registry.is_overridable(existing_primary_id)
    };
    if !can_place {
        return;
    }
    if world_cell_intersects_structure(
        world_pos,
        &rapier_context,
        &q_structures,
        &q_structure_parents,
    ) {
        return;
    }

    if !block_allows_environment_at(place_id, world_pos, &chunk_map, &registry, &fluids) {
        return;
    }

    if registry.is_prop(place_id) {
        let ground_pos = world_pos + IVec3::NEG_Y;
        let ground_id = get_block_world(&chunk_map, ground_pos);
        if !registry.prop_allows_ground(place_id, ground_id) {
            return;
        }
    }

    let keep_cell_water = waterlog_existing_primary
        || (!merge_waterlogged_slab
            && place_into_stacked
            && registry.is_water_logged(place_id)
            && existing_primary_id != 0
            && registry.is_fluid(existing_primary_id));
    if let Some(fc) = fluids.0.get_mut(&chunk_coord) {
        fc.set(lx, ly, lz, keep_cell_water);
    }

    let (network_block_id, network_stacked_block_id) = if waterlog_existing_primary {
        (place_id, existing_primary_id)
    } else if place_into_stacked {
        if merge_waterlogged_slab {
            (existing_stacked_id, place_id)
        } else {
            (existing_primary_id, place_id)
        }
    } else {
        (place_id, 0)
    };

    if let Some(mut access) = world_access_mut(&mut chunk_map, world_pos) {
        if waterlog_existing_primary {
            access.set(place_id);
            access.set_stacked(existing_primary_id);
        } else if place_into_stacked {
            if merge_waterlogged_slab {
                access.set(existing_stacked_id);
                access.set_stacked(place_id);
            } else {
                access.set_stacked(place_id);
            }
        } else {
            access.set(place_id);
            access.set_stacked(0);
        }
    }

    if !creative_mode {
        let _ = consume_from_selected_slot(
            &mut inventory,
            hotbar_selection.as_deref(),
            id,
            &item_registry,
            &registry,
        );
    }

    mark_dirty_block_and_neighbors(&mut chunk_map, world_pos, &mut ev_dirty);

    let name = registry.name_opt(place_id).unwrap_or("").to_string();
    place_ev.write(BlockPlaceByPlayerEvent {
        location: world_pos,
        block_id: network_block_id,
        stacked_block_id: network_stacked_block_id,
        block_name: name,
    });

    if !place_into_stacked && !waterlog_existing_primary {
        try_spawn_runtime_structure_for_registered_block(
            &mut commands,
            &asset_server,
            structure_recipe_registry.as_deref(),
            &registry,
            &item_registry,
            world_pos,
            place_id,
            &mut structure_runtime,
            multiplayer_connection.uses_local_save_data(),
            ws.as_deref(),
            region_cache.as_deref_mut(),
        );
    }
}
