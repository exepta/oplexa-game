const HUD_CHEST_PREVIEW_COMPACT_SLOTS: usize = 5;
const HUD_CHEST_PREVIEW_EXPANDED_COLUMNS: u16 = 8;

/// Runs the `show_hud_hotbar_ui` routine for show hud hotbar ui in the `graphic::components::hud` module.
fn show_hud_hotbar_ui(mut root: Query<&mut Visibility, With<HudRoot>>) {
    if let Ok(mut visible) = root.single_mut() {
        *visible = Visibility::Inherited;
    }
}

/// Runs the `hide_hud_hotbar_ui` routine for hide hud hotbar ui in the `graphic::components::hud` module.
fn hide_hud_hotbar_ui(mut root: Query<&mut Visibility, With<HudRoot>>) {
    if let Ok(mut visible) = root.single_mut() {
        *visible = Visibility::Hidden;
    }
}

/// Runs the `cycle_hotbar_with_scroll` routine for cycle hotbar with scroll in the `graphic::components::hud` module.
fn cycle_hotbar_with_scroll(
    mut wheel_reader: MessageReader<MouseWheel>,
    ui_interaction: Res<UiInteractionState>,
    inventory: Res<PlayerInventory>,
    item_registry: Res<ItemRegistry>,
    active_structure_recipe: Res<ActiveStructureRecipeState>,
    mut hotbar_state: ResMut<HotbarSelectionState>,
) {
    if ui_interaction.chat_open {
        for _ in wheel_reader.read() {}
        return;
    }

    if active_structure_recipe.selected_recipe_name.is_some()
        && is_hammer_selected_for_hotbar(&inventory, &hotbar_state, &item_registry)
    {
        for _ in wheel_reader.read() {}
        return;
    }

    let mut total_steps = 0_i32;

    for wheel in wheel_reader.read() {
        let raw = match wheel.unit {
            MouseScrollUnit::Line => wheel.y,
            MouseScrollUnit::Pixel => wheel.y / 24.0,
        };
        if raw.abs() < f32::EPSILON {
            continue;
        }

        let discrete = raw.round() as i32;
        if discrete != 0 {
            total_steps += discrete;
        } else {
            total_steps += raw.signum() as i32;
        }
    }

    if total_steps == 0 {
        return;
    }

    let steps = total_steps.unsigned_abs() as usize;
    for _ in 0..steps {
        if total_steps > 0 {
            hotbar_state.selected_index =
                (hotbar_state.selected_index + HOTBAR_SLOTS - 1) % HOTBAR_SLOTS;
        } else {
            hotbar_state.selected_index = (hotbar_state.selected_index + 1) % HOTBAR_SLOTS;
        }
    }
}

fn is_hammer_selected_for_hotbar(
    inventory: &PlayerInventory,
    hotbar_state: &HotbarSelectionState,
    item_registry: &ItemRegistry,
) -> bool {
    let Some(slot) = inventory.slots.get(hotbar_state.selected_index) else {
        return false;
    };
    if slot.is_empty() {
        return false;
    }
    let Some(item) = item_registry.def_opt(slot.item_id) else {
        return false;
    };
    item.localized_name == "oplexa:hammer" || item.key == "hammer"
}

/// Runs the `select_hotbar_with_number_keys` routine for select hotbar with number keys in the `graphic::components::hud` module.
fn select_hotbar_with_number_keys(
    keyboard: Res<ButtonInput<KeyCode>>,
    global_config: Res<GlobalConfig>,
    ui_interaction: Res<UiInteractionState>,
    mut hotbar_state: ResMut<HotbarSelectionState>,
) {
    if ui_interaction.blocks_game_input() {
        return;
    }

    for slot_index in 0..HOTBAR_SLOTS {
        let key_name = match slot_index {
            0 => global_config.input.hotbar_slot_1.as_str(),
            1 => global_config.input.hotbar_slot_2.as_str(),
            2 => global_config.input.hotbar_slot_3.as_str(),
            3 => global_config.input.hotbar_slot_4.as_str(),
            4 => global_config.input.hotbar_slot_5.as_str(),
            5 => global_config.input.hotbar_slot_6.as_str(),
            6 => global_config.input.hotbar_slot_7.as_str(),
            _ => global_config.input.hotbar_slot_8.as_str(),
        };
        let fallback = match slot_index {
            0 => KeyCode::Digit1,
            1 => KeyCode::Digit2,
            2 => KeyCode::Digit3,
            3 => KeyCode::Digit4,
            4 => KeyCode::Digit5,
            5 => KeyCode::Digit6,
            6 => KeyCode::Digit7,
            _ => KeyCode::Digit8,
        };
        let key = convert(key_name).unwrap_or(fallback);

        if keyboard.just_pressed(key) {
            hotbar_state.selected_index = slot_index;
            return;
        }
    }
}

/// Runs the `drop_selected_hotbar_item` routine for drop selected hotbar item in the `graphic::components::hud` module.
fn drop_selected_hotbar_item(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    global_config: Res<GlobalConfig>,
    game_mode: Res<GameModeState>,
    ui_interaction: Res<UiInteractionState>,
    hotbar_state: Res<HotbarSelectionState>,
    multiplayer_connection: Option<Res<MultiplayerConnectionState>>,
    mut inventory: ResMut<PlayerInventory>,
    player_q: Query<(&Transform, Option<&FpsController>), With<Player>>,
    mut drop_requests: MessageWriter<DropItemRequest>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    block_registry: Res<BlockRegistry>,
    item_registry: Res<ItemRegistry>,
) {
    if ui_interaction.blocks_game_input() {
        return;
    }

    if matches!(game_mode.0, GameMode::Spectator) {
        return;
    }

    let drop_key = convert(global_config.input.drop_item.as_str()).unwrap_or(KeyCode::KeyQ);
    if !keyboard.just_pressed(drop_key) {
        return;
    }

    let slot_index = hotbar_state.selected_index;
    if slot_index >= HOTBAR_SLOTS || slot_index >= inventory.slots.len() {
        return;
    }

    let slot = &mut inventory.slots[slot_index];
    if slot.is_empty() {
        return;
    }

    let Ok((player_tf, player_ctrl)) = player_q.single() else {
        return;
    };
    let player_forward = player_drop_forward_vector(player_tf, player_ctrl);

    let dropped_item_id = slot.item_id;
    if slot.count <= 1 {
        *slot = Default::default();
    } else {
        slot.count -= 1;
    }

    if multiplayer_connection
        .as_ref()
        .is_some_and(|state| state.connected)
    {
        let (spawn_center, initial_velocity) =
            player_drop_spawn_motion(player_tf.translation, player_forward);
        let world_loc = player_drop_world_location(player_tf.translation, player_forward);
        drop_requests.write(DropItemRequest::new(
            dropped_item_id,
            1,
            world_loc.to_array(),
            spawn_center.to_array(),
            initial_velocity.to_array(),
        ));
    } else {
        spawn_player_dropped_item_stack(
            &mut commands,
            &mut meshes,
            &block_registry,
            &item_registry,
            dropped_item_id,
            1,
            player_tf.translation,
            player_forward,
            time.elapsed_secs(),
        );
    }
}

#[inline]
fn player_drop_forward_vector(player_tf: &Transform, player_ctrl: Option<&FpsController>) -> Vec3 {
    if let Some(ctrl) = player_ctrl {
        let look_forward =
            Quat::from_rotation_y(ctrl.yaw) * Quat::from_rotation_x(ctrl.pitch) * Vec3::NEG_Z;
        if look_forward.length_squared() > 0.000_001 {
            return look_forward.normalize();
        }
    }
    player_tf.forward().as_vec3()
}

/// Synchronizes hotbar selected block for the `graphic::components::hud` module.
fn sync_hotbar_selected_block(
    hotbar_state: Res<HotbarSelectionState>,
    inventory: Res<PlayerInventory>,
    item_registry: Res<ItemRegistry>,
    registry: Res<BlockRegistry>,
    language: Res<ClientLanguageState>,
    mut selected: ResMut<SelectedBlock>,
) {
    let Some(slot) = inventory.slots.get(hotbar_state.selected_index).copied() else {
        selected.id = 0;
        selected.name = language.as_ref().localize_name_key("KEY_AIR");
        return;
    };

    if slot.is_empty() {
        selected.id = 0;
        selected.name = language.as_ref().localize_name_key("KEY_AIR");
        return;
    }

    let Some(block_id) = item_registry.block_for_item(slot.item_id) else {
        selected.id = 0;
        selected.name = language.as_ref().localize_name_key("KEY_AIR");
        return;
    };

    selected.id = block_id;
    selected.name = localize_block_name_for_id(language.as_ref(), &registry, block_id);
}

/// Synchronizes hud hotbar ui for the `graphic::components::hud` module.
fn sync_hud_hotbar_ui(
    hotbar_state: Res<HotbarSelectionState>,
    hotbar_tooltip: Res<HotbarSelectionTooltipState>,
    inventory: Res<PlayerInventory>,
    item_registry: Res<ItemRegistry>,
    block_registry: Res<BlockRegistry>,
    asset_server: Res<AssetServer>,
    mut image_cache: ResMut<ImageCache>,
    mut images: ResMut<Assets<Image>>,
    mut buttons: Query<(&CssID, &mut Button, &mut BorderColor, &mut BackgroundColor)>,
    mut badges: Query<
        (&CssID, &mut Paragraph, &mut Visibility),
        Without<HotbarSelectionTooltipText>,
    >,
    mut tooltip_text: Query<(&mut Paragraph, &mut Visibility), With<HotbarSelectionTooltipText>>,
) {
    for (css_id, mut button, mut border, mut background) in &mut buttons {
        if let Some(slot_number) = css_id.0.strip_prefix(HUD_SLOT_PREFIX) {
            let Ok(slot_index) = slot_number.parse::<usize>() else {
                continue;
            };
            let Some(slot) = inventory.slots.get(slot_index.saturating_sub(1)) else {
                continue;
            };

            if !button.text.is_empty() {
                button.text.clear();
            }
            let next_icon = if slot.is_empty() {
                None
            } else {
                resolve_item_icon_path(
                    &item_registry,
                    &block_registry,
                    &asset_server,
                    &mut image_cache,
                    &mut images,
                    slot.item_id,
                )
            };
            if button.icon_path != next_icon {
                button.icon_path = next_icon;
            }

            let selected = slot_index.saturating_sub(1) == hotbar_state.selected_index;
            let border_color = if selected {
                color_accent()
            } else {
                color_background_hover()
            };
            border.top = border_color;
            border.right = border_color;
            border.bottom = border_color;
            border.left = border_color;
            background.0 = if selected {
                color_background_hover()
            } else {
                color_background()
            };
        }
    }

    for (css_id, mut paragraph, mut visibility) in &mut badges {
        let Some(slot_number) = css_id.0.strip_prefix(HUD_SLOT_BADGE_PREFIX) else {
            continue;
        };
        let Ok(slot_index) = slot_number.parse::<usize>() else {
            continue;
        };
        let Some(slot) = inventory.slots.get(slot_index.saturating_sub(1)) else {
            continue;
        };

        if slot.is_empty() {
            if !paragraph.text.is_empty() {
                paragraph.text.clear();
            }
            *visibility = Visibility::Hidden;
            continue;
        }

        let count_text = slot.count.to_string();
        if paragraph.text != count_text {
            paragraph.text = count_text;
        }
        *visibility = Visibility::Inherited;
    }

    if let Ok((mut text, mut visibility)) = tooltip_text.single_mut() {
        if hotbar_tooltip.visible {
            if text.text != hotbar_tooltip.text {
                text.text = hotbar_tooltip.text.clone();
            }
            *visibility = Visibility::Inherited;
        } else {
            if !text.text.is_empty() {
                text.text.clear();
            }
            *visibility = Visibility::Hidden;
        }
    }
}

/// Synchronizes looked block hud card for the `graphic::components::hud` module.
#[derive(SystemParam)]
struct LookedBlockHudQueries<'w, 's> {
    card_visibility_q: Query<
        'w,
        's,
        &'static mut Visibility,
        (With<HudLookedBlockCard>, Without<HudLookedBlockProgress>),
    >,
    icon_q: Query<
        'w,
        's,
        (&'static mut ImageNode, &'static mut Node),
        (
            With<HudLookedBlockIcon>,
            Without<HudLookedBlockChestPreviewIcon>,
            Without<HudLookedBlockChestPreviewRow>,
        ),
    >,
    display_name_q: Query<
        'w,
        's,
        &'static mut Paragraph,
        (
            With<HudLookedBlockDisplayName>,
            Without<HudLookedBlockLocalizedName>,
            Without<HudLookedBlockLevel>,
        ),
    >,
    localized_name_q: Query<
        'w,
        's,
        &'static mut Paragraph,
        (
            With<HudLookedBlockLocalizedName>,
            Without<HudLookedBlockDisplayName>,
            Without<HudLookedBlockLevel>,
        ),
    >,
    level_q: Query<
        'w,
        's,
        (&'static mut Paragraph, &'static mut TextColor),
        (
            With<HudLookedBlockLevel>,
            Without<HudLookedBlockDisplayName>,
            Without<HudLookedBlockLocalizedName>,
        ),
    >,
    progress_q: Query<
        'w,
        's,
        (&'static mut ProgressBar, &'static mut Visibility),
        (With<HudLookedBlockProgress>, Without<HudLookedBlockCard>),
    >,
    chest_count_q: Query<
        'w,
        's,
        (&'static mut Paragraph, &'static mut Visibility),
        (
            With<HudLookedBlockChestCount>,
            Without<HudLookedBlockCard>,
            Without<HudLookedBlockProgress>,
            Without<HudLookedBlockDisplayName>,
            Without<HudLookedBlockLocalizedName>,
            Without<HudLookedBlockLevel>,
            Without<HudLookedBlockChestPreviewBadge>,
        ),
    >,
    chest_preview_row_q: Query<
        'w,
        's,
        (&'static mut Visibility, &'static mut Node),
        (
            With<HudLookedBlockChestPreviewRow>,
            Without<HudLookedBlockCard>,
            Without<HudLookedBlockProgress>,
            Without<HudLookedBlockChestCount>,
            Without<HudLookedBlockChestPreviewSlot>,
            Without<HudLookedBlockChestPreviewBadge>,
            Without<HudLookedBlockChestPreviewMore>,
            Without<HudLookedBlockIcon>,
        ),
    >,
    chest_slot_q: Query<
        'w,
        's,
        (
            &'static HudLookedBlockChestPreviewSlot,
            &'static mut Visibility,
            &'static mut Node,
        ),
        (
            Without<HudLookedBlockCard>,
            Without<HudLookedBlockProgress>,
            Without<HudLookedBlockChestCount>,
            Without<HudLookedBlockChestPreviewRow>,
            Without<HudLookedBlockChestPreviewBadge>,
            Without<HudLookedBlockChestPreviewMore>,
            Without<HudLookedBlockChestPreviewIcon>,
            Without<HudLookedBlockIcon>,
        ),
    >,
    chest_icon_q: Query<
        'w,
        's,
        (
            &'static HudLookedBlockChestPreviewIcon,
            &'static mut ImageNode,
            &'static mut Node,
        ),
        (
            With<HudLookedBlockChestPreviewIcon>,
            Without<HudLookedBlockIcon>,
            Without<HudLookedBlockChestPreviewSlot>,
            Without<HudLookedBlockChestPreviewBadge>,
            Without<HudLookedBlockChestPreviewMore>,
            Without<HudLookedBlockChestPreviewRow>,
        ),
    >,
    chest_badge_q: Query<
        'w,
        's,
        (
            &'static HudLookedBlockChestPreviewBadge,
            &'static mut Paragraph,
            &'static mut Visibility,
        ),
        (
            Without<HudLookedBlockCard>,
            Without<HudLookedBlockProgress>,
            Without<HudLookedBlockDisplayName>,
            Without<HudLookedBlockLocalizedName>,
            Without<HudLookedBlockLevel>,
            Without<HudLookedBlockChestCount>,
            Without<HudLookedBlockChestPreviewRow>,
            Without<HudLookedBlockChestPreviewSlot>,
            Without<HudLookedBlockChestPreviewMore>,
        ),
    >,
    chest_more_q: Query<
        'w,
        's,
        (&'static mut Visibility, &'static mut Node),
        (
            With<HudLookedBlockChestPreviewMore>,
            Without<HudLookedBlockCard>,
            Without<HudLookedBlockProgress>,
            Without<HudLookedBlockChestCount>,
            Without<HudLookedBlockChestPreviewRow>,
            Without<HudLookedBlockChestPreviewSlot>,
            Without<HudLookedBlockChestPreviewBadge>,
            Without<HudLookedBlockChestPreviewIcon>,
            Without<HudLookedBlockIcon>,
        ),
    >,
}

#[allow(clippy::too_many_arguments)]
fn sync_hud_looked_block_card(
    keyboard: Res<ButtonInput<KeyCode>>,
    global_config: Res<GlobalConfig>,
    selection: Res<SelectionState>,
    mining_state: Res<MiningState>,
    hotbar_state: Res<HotbarSelectionState>,
    inventory: Res<PlayerInventory>,
    block_registry: Res<BlockRegistry>,
    item_registry: Res<ItemRegistry>,
    language: Res<ClientLanguageState>,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    mut image_cache: ResMut<ImageCache>,
    mut images: ResMut<Assets<Image>>,
    q_structures: Query<&crate::logic::events::block_event_handler::PlacedStructureMetadata>,
    structure_runtime: Option<Res<crate::logic::events::block_event_handler::StructureRuntimeState>>,
    hud_q: LookedBlockHudQueries,
) {
    let LookedBlockHudQueries {
        mut card_visibility_q,
        mut icon_q,
        mut display_name_q,
        mut localized_name_q,
        mut level_q,
        mut progress_q,
        mut chest_count_q,
        mut chest_preview_row_q,
        mut chest_slot_q,
        mut chest_icon_q,
        mut chest_badge_q,
        mut chest_more_q,
    } = hud_q;

    struct LookedHudData {
        display_name: String,
        localized_name: String,
        level: u8,
        icon_block_id: Option<u16>,
        block_progress_target: Option<(IVec3, u16)>,
        world_pos: Option<IVec3>,
        is_chest: bool,
    }

    let looked = if let Some(hit) = selection.hit.filter(|hit| hit.block_id != 0) {
        let Some(block) = block_registry.def_opt(hit.block_id) else {
            if let Ok(mut visibility) = card_visibility_q.single_mut() {
                *visibility = Visibility::Hidden;
            }
            if let Ok((mut progress_bar, mut visibility)) = progress_q.single_mut() {
                progress_bar.value = 0.0;
                *visibility = Visibility::Hidden;
            }
            reset_hud_chest_preview(
                &mut chest_count_q,
                &mut chest_preview_row_q,
                &mut chest_slot_q,
                &mut chest_icon_q,
                &mut chest_badge_q,
                &mut chest_more_q,
            );
            return;
        };
        LookedHudData {
            display_name: localize_block_name_for_id(language.as_ref(), &block_registry, hit.block_id),
            localized_name: block.localized_name.clone(),
            level: block.stats.level,
            icon_block_id: Some(hit.block_id),
            block_progress_target: Some((hit.block_pos, hit.block_id)),
            world_pos: Some(hit.block_pos),
            is_chest: is_chest_block_id(hit.block_id, &block_registry),
        }
    } else if let Some(structure_hit) = selection.structure_hit {
        let Ok(meta) = q_structures.get(structure_hit.entity) else {
            if let Ok(mut visibility) = card_visibility_q.single_mut() {
                *visibility = Visibility::Hidden;
            }
            if let Ok((mut progress_bar, mut visibility)) = progress_q.single_mut() {
                progress_bar.value = 0.0;
                *visibility = Visibility::Hidden;
            }
            reset_hud_chest_preview(
                &mut chest_count_q,
                &mut chest_preview_row_q,
                &mut chest_slot_q,
                &mut chest_icon_q,
                &mut chest_badge_q,
                &mut chest_more_q,
            );
            return;
        };

        let registration = meta.registration.as_ref();
        let block_id = registration.and_then(|entry| entry.block_id).filter(|id| *id != 0);
        let display_name = if let Some(id) = block_id {
            localize_block_name_for_id(language.as_ref(), &block_registry, id)
        } else if let Some(registration) = registration {
            language.localize_name_key(registration.name.as_str())
        } else {
            meta.recipe_name.clone()
        };
        let localized_name = if let Some(id) = block_id {
            block_registry
                .def_opt(id)
                .map(|block| block.localized_name.clone())
                .unwrap_or_else(|| meta.recipe_name.clone())
        } else if let Some(registration) = registration {
            registration.name.clone()
        } else {
            meta.recipe_name.clone()
        };

        LookedHudData {
            display_name,
            localized_name,
            level: meta.stats.level,
            icon_block_id: block_id,
            block_progress_target: None,
            world_pos: Some(meta.place_origin),
            is_chest: is_chest_structure_meta(meta),
        }
    } else {
        if let Ok(mut visibility) = card_visibility_q.single_mut() {
            *visibility = Visibility::Hidden;
        }
        if let Ok((mut progress_bar, mut visibility)) = progress_q.single_mut() {
            progress_bar.value = 0.0;
            *visibility = Visibility::Hidden;
        }
        reset_hud_chest_preview(
            &mut chest_count_q,
            &mut chest_preview_row_q,
            &mut chest_slot_q,
            &mut chest_icon_q,
            &mut chest_badge_q,
            &mut chest_more_q,
        );
        return;
    };

    if let Ok(mut visibility) = card_visibility_q.single_mut() {
        *visibility = Visibility::Inherited;
    }

    if let Ok(mut display_name) = display_name_q.single_mut() {
        if display_name.text != looked.display_name {
            display_name.text = looked.display_name.clone();
        }
    }

    if let Ok(mut paragraph) = localized_name_q.single_mut() {
        let next_localized_name = format!(
            "{}: {}",
            language.localize_name_key("KEY_UI_LOCALIZED_NAME_LABEL"),
            looked.localized_name
        );
        if paragraph.text != next_localized_name {
            paragraph.text = next_localized_name;
        }
    }

    if let Ok((mut paragraph, mut text_color)) = level_q.single_mut() {
        let next_level = format!(
            "{}: {}",
            language.localize_name_key("KEY_UI_MINING_LEVEL_LABEL"),
            looked.level
        );
        if paragraph.text != next_level {
            paragraph.text = next_level;
        }

        let held_tool = inventory
            .slots
            .get(hotbar_state.selected_index)
            .filter(|slot| !slot.is_empty())
            .and_then(|slot| item_registry.def_opt(slot.item_id))
            .and_then(|item| infer_tool_from_item_key(item.key.as_str()));
        let meets_requirement = looked
            .icon_block_id
            .and_then(|id| block_requirement_for_id(id, &block_registry))
            .map(|requirement| requirement.is_met_by(held_tool))
            .unwrap_or(true);
        text_color.0 = if meets_requirement {
            Color::srgb_u8(0x77, 0xd8, 0x82)
        } else {
            Color::srgb_u8(0xf2, 0x66, 0x66)
        };
    }

    if let Ok((mut icon, mut icon_node)) = icon_q.single_mut() {
        let mut next_icon = looked
            .icon_block_id
            .and_then(|block_id| item_registry.item_for_block(block_id))
            .and_then(|item_id| {
                resolve_item_icon_path(
                    &item_registry,
                    &block_registry,
                    &asset_server,
                    &mut image_cache,
                    &mut images,
                    item_id,
                )
            });
        if next_icon.is_none() {
            let fallback = "textures/items/missing.png".to_string();
            ensure_item_icon_nearest_sampler(&asset_server, &image_cache, &mut images, fallback.as_str());
            next_icon = Some(fallback);
        }
        let is_block_icon = next_icon
            .as_deref()
            .and_then(parse_block_icon_cache_key)
            .is_some();
        let target_size = if is_block_icon { 48.0 } else { 34.0 };
        icon_node.width = Val::Px(target_size);
        icon_node.height = Val::Px(target_size);
        icon_node.min_width = Val::Px(target_size);
        icon_node.min_height = Val::Px(target_size);
        icon_node.max_width = Val::Px(target_size);
        icon_node.max_height = Val::Px(target_size);

        let next_icon_handle = next_icon
            .as_ref()
            .map(|path| {
                image_cache
                    .map
                    .get(path.as_str())
                    .cloned()
                    .unwrap_or_else(|| asset_server.load(path.clone()))
            })
            .unwrap_or_default();
        if icon.image != next_icon_handle {
            icon.image = next_icon_handle;
        }
    }

    if let Ok((mut progress_bar, mut visibility)) = progress_q.single_mut() {
        let progress_percent = looked.block_progress_target.and_then(|(loc, id)| {
            mining_state.target.and_then(|target| {
                if target.loc == loc && target.id == id {
                    Some(mining_progress(time.elapsed_secs(), &target) * 100.0)
                } else {
                    None
                }
            })
        });

        if let Some(percent) = progress_percent {
            progress_bar.value = percent.clamp(0.0, 100.0);
            *visibility = Visibility::Inherited;
        } else {
            progress_bar.value = 0.0;
            *visibility = Visibility::Hidden;
        }
    }

    if !looked.is_chest {
        reset_hud_chest_preview(
            &mut chest_count_q,
            &mut chest_preview_row_q,
            &mut chest_slot_q,
            &mut chest_icon_q,
            &mut chest_badge_q,
            &mut chest_more_q,
        );
        return;
    }

    let preview = looked
        .world_pos
        .and_then(|world_pos| build_chest_hud_preview(world_pos, structure_runtime.as_deref(), &item_registry))
        .unwrap_or_default();
    let sneak_key = convert(global_config.input.movement_sneak.as_str()).unwrap_or(KeyCode::ShiftLeft);
    let show_expanded = keyboard.pressed(sneak_key);
    let compact_items: Vec<InventorySlot> = preview
        .slots
        .iter()
        .copied()
        .filter(|slot| !slot.is_empty())
        .take(HUD_CHEST_PREVIEW_COMPACT_SLOTS)
        .collect();
    let compact_has_more = preview.non_empty_count > HUD_CHEST_PREVIEW_COMPACT_SLOTS;

    if let Ok((mut paragraph, mut visibility)) = chest_count_q.single_mut() {
        paragraph.text = format!(
            "{}: {}",
            language.localize_name_key("KEY_UI_ITEMS"),
            preview.total_count
        );
        *visibility = Visibility::Inherited;
    }
    if let Ok((mut visibility, mut node)) = chest_preview_row_q.single_mut() {
        if show_expanded {
            node.display = Display::Grid;
            node.grid_template_columns =
                RepeatedGridTrack::fr(HUD_CHEST_PREVIEW_EXPANDED_COLUMNS, 1.0);
            node.grid_auto_rows = vec![GridTrack::px(40.0)];
            node.row_gap = Val::Px(6.0);
            node.column_gap = Val::Px(6.0);
            *visibility = Visibility::Inherited;
        } else {
            node.display = Display::Flex;
            node.grid_template_columns.clear();
            node.grid_auto_rows.clear();
            node.row_gap = Val::Px(0.0);
            node.column_gap = Val::Px(6.0);
            *visibility = if compact_items.is_empty() && !compact_has_more {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
        }
    }

    for (slot, mut visibility, mut node) in &mut chest_slot_q {
        let should_show = if show_expanded {
            slot.index < CHEST_INVENTORY_SLOTS
        } else {
            compact_items.get(slot.index).is_some()
        };
        *visibility = if should_show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        node.display = if should_show {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (icon, mut img, mut node) in &mut chest_icon_q {
        let source_slot = if show_expanded {
            preview.slots.get(icon.index).copied().unwrap_or_default()
        } else {
            compact_items.get(icon.index).copied().unwrap_or_default()
        };
        let next_handle = (!source_slot.is_empty())
            .then(|| {
                resolve_item_icon_path(
                    &item_registry,
                    &block_registry,
                    &asset_server,
                    &mut image_cache,
                    &mut images,
                    source_slot.item_id,
                )
            })
            .flatten()
            .map(|path| {
                image_cache
                    .map
                    .get(path.as_str())
                    .cloned()
                    .unwrap_or_else(|| asset_server.load(path))
            })
            .unwrap_or_default();
        if img.image != next_handle {
            img.image = next_handle;
        }
        node.display = if source_slot.is_empty() {
            Display::None
        } else {
            Display::Flex
        };
    }
    for (badge, mut paragraph, mut visibility) in &mut chest_badge_q {
        let slot = if show_expanded {
            preview.slots.get(badge.index).copied().unwrap_or_default()
        } else {
            compact_items.get(badge.index).copied().unwrap_or_default()
        };
        if !slot.is_empty() {
            paragraph.text = slot.count.to_string();
            *visibility = Visibility::Inherited;
        } else {
            if !paragraph.text.is_empty() {
                paragraph.text.clear();
            }
            *visibility = Visibility::Hidden;
        }
    }
    if let Ok((mut visibility, mut node)) = chest_more_q.single_mut() {
        let show_more = !show_expanded && compact_has_more;
        *visibility = if show_more {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        node.display = if show_more {
            Display::Flex
        } else {
            Display::None
        };
    }
}

#[derive(Clone, Debug, Default)]
struct ChestHudPreview {
    total_count: u32,
    slots: [InventorySlot; CHEST_INVENTORY_SLOTS],
    non_empty_count: usize,
}

fn build_chest_hud_preview(
    world_pos: IVec3,
    runtime: Option<&crate::logic::events::block_event_handler::StructureRuntimeState>,
    item_registry: &ItemRegistry,
) -> Option<ChestHudPreview> {
    let runtime = runtime?;
    let (coord, _) = world_to_chunk_xz(world_pos.x, world_pos.z);
    let entry = runtime.records_by_chunk.get(&coord).and_then(|entries| {
        entries
            .iter()
            .find(|entry| entry.place_origin == [world_pos.x, world_pos.y, world_pos.z])
    })?;

    let mut ordered_slots = entry.inventory_slots.clone();
    ordered_slots.sort_by_key(|slot| slot.slot);

    let mut preview = ChestHudPreview::default();
    for saved in ordered_slots {
        let slot_index = saved.slot as usize;
        if slot_index >= CHEST_INVENTORY_SLOTS || saved.count == 0 {
            continue;
        }
        let item_name = saved.item.trim();
        if item_name.is_empty() {
            continue;
        }
        let Some(item_id) = item_registry.id_opt(item_name) else {
            continue;
        };
        preview.slots[slot_index] = InventorySlot {
            item_id,
            count: saved.count.max(1),
        };
    }
    preview.total_count = preview.slots.iter().map(|slot| slot.count as u32).sum();
    preview.non_empty_count = preview.slots.iter().filter(|slot| !slot.is_empty()).count();
    Some(preview)
}

fn is_chest_structure_meta(
    meta: &crate::logic::events::block_event_handler::PlacedStructureMetadata,
) -> bool {
    if meta.recipe_name.eq_ignore_ascii_case("chest") {
        return true;
    }
    meta.registration.as_ref().is_some_and(|registration| {
        registration
            .localized_name
            .eq_ignore_ascii_case("chest_block")
    })
}

fn is_chest_block_id(block_id: u16, registry: &BlockRegistry) -> bool {
    if block_id == 0 {
        return false;
    }
    registry.def_opt(block_id).is_some_and(|def| {
        let localized = def.localized_name.to_ascii_lowercase();
        let key = def.name.to_ascii_uppercase();
        localized == "chest_block"
            || localized.starts_with("chest_block_r")
            || key == "KEY_CHEST_BLOCK"
            || key.starts_with("KEY_CHEST_BLOCK_R")
    })
}

fn reset_hud_chest_preview(
    count_q: &mut Query<
        (&mut Paragraph, &mut Visibility),
        (
            With<HudLookedBlockChestCount>,
            Without<HudLookedBlockCard>,
            Without<HudLookedBlockProgress>,
            Without<HudLookedBlockDisplayName>,
            Without<HudLookedBlockLocalizedName>,
            Without<HudLookedBlockLevel>,
            Without<HudLookedBlockChestPreviewBadge>,
        ),
    >,
    row_q: &mut Query<
        (&mut Visibility, &mut Node),
        (
            With<HudLookedBlockChestPreviewRow>,
            Without<HudLookedBlockCard>,
            Without<HudLookedBlockProgress>,
            Without<HudLookedBlockChestCount>,
            Without<HudLookedBlockChestPreviewSlot>,
            Without<HudLookedBlockChestPreviewBadge>,
            Without<HudLookedBlockChestPreviewMore>,
            Without<HudLookedBlockIcon>,
        ),
    >,
    slot_q: &mut Query<
        (
            &HudLookedBlockChestPreviewSlot,
            &mut Visibility,
            &mut Node,
        ),
        (
            Without<HudLookedBlockCard>,
            Without<HudLookedBlockProgress>,
            Without<HudLookedBlockChestCount>,
            Without<HudLookedBlockChestPreviewRow>,
            Without<HudLookedBlockChestPreviewBadge>,
            Without<HudLookedBlockChestPreviewMore>,
            Without<HudLookedBlockChestPreviewIcon>,
            Without<HudLookedBlockIcon>,
        ),
    >,
    icon_q: &mut Query<
        (&HudLookedBlockChestPreviewIcon, &mut ImageNode, &mut Node),
        (
            With<HudLookedBlockChestPreviewIcon>,
            Without<HudLookedBlockIcon>,
            Without<HudLookedBlockChestPreviewSlot>,
            Without<HudLookedBlockChestPreviewBadge>,
            Without<HudLookedBlockChestPreviewMore>,
            Without<HudLookedBlockChestPreviewRow>,
        ),
    >,
    badge_q: &mut Query<
        (
            &HudLookedBlockChestPreviewBadge,
            &mut Paragraph,
            &mut Visibility,
        ),
        (
            Without<HudLookedBlockCard>,
            Without<HudLookedBlockProgress>,
            Without<HudLookedBlockDisplayName>,
            Without<HudLookedBlockLocalizedName>,
            Without<HudLookedBlockLevel>,
            Without<HudLookedBlockChestCount>,
            Without<HudLookedBlockChestPreviewRow>,
            Without<HudLookedBlockChestPreviewSlot>,
            Without<HudLookedBlockChestPreviewMore>,
        ),
    >,
    more_q: &mut Query<
        (&mut Visibility, &mut Node),
        (
            With<HudLookedBlockChestPreviewMore>,
            Without<HudLookedBlockCard>,
            Without<HudLookedBlockProgress>,
            Without<HudLookedBlockChestCount>,
            Without<HudLookedBlockChestPreviewRow>,
            Without<HudLookedBlockChestPreviewSlot>,
            Without<HudLookedBlockChestPreviewBadge>,
            Without<HudLookedBlockChestPreviewIcon>,
            Without<HudLookedBlockIcon>,
        ),
    >,
) {
    if let Ok((mut paragraph, mut visibility)) = count_q.single_mut() {
        if !paragraph.text.is_empty() {
            paragraph.text.clear();
        }
        *visibility = Visibility::Hidden;
    }
    if let Ok((mut visibility, mut node)) = row_q.single_mut() {
        node.display = Display::Flex;
        node.grid_template_columns.clear();
        node.grid_auto_rows.clear();
        node.row_gap = Val::Px(0.0);
        node.column_gap = Val::Px(6.0);
        *visibility = Visibility::Hidden;
    }
    for (_, mut visibility, mut node) in slot_q.iter_mut() {
        *visibility = Visibility::Hidden;
        node.display = Display::None;
    }
    for (_, mut img, mut node) in icon_q.iter_mut() {
        if img.image != Handle::default() {
            img.image = Handle::default();
        }
        node.display = Display::None;
    }
    for (_, mut paragraph, mut visibility) in badge_q.iter_mut() {
        if !paragraph.text.is_empty() {
            paragraph.text.clear();
        }
        *visibility = Visibility::Hidden;
    }
    if let Ok((mut visibility, mut node)) = more_q.single_mut() {
        *visibility = Visibility::Hidden;
        node.display = Display::None;
    }
}

/// Tracks selected hotbar item tooltip state for the `graphic::components::hud` module.
fn track_hotbar_selection_tooltip(
    time: Res<Time>,
    hotbar_state: Res<HotbarSelectionState>,
    inventory: Res<PlayerInventory>,
    item_registry: Res<ItemRegistry>,
    language: Res<ClientLanguageState>,
    mut tooltip: ResMut<HotbarSelectionTooltipState>,
) {
    tooltip.timer.tick(time.delta());
    if tooltip.timer.is_finished() {
        tooltip.visible = false;
    }

    if hotbar_state.selected_index == tooltip.last_selected_index {
        return;
    }

    tooltip.last_selected_index = hotbar_state.selected_index;
    let selected_item_name = inventory
        .slots
        .get(hotbar_state.selected_index)
        .filter(|slot| !slot.is_empty())
        .and_then(|slot| item_registry.def_opt(slot.item_id))
        .map(|item| localize_item_name(language.as_ref(), item));

    if let Some(name) = selected_item_name {
        tooltip.timer.reset();
        tooltip.visible = true;
        tooltip.text = name;
    } else {
        tooltip.visible = false;
        tooltip.text.clear();
    }
}

/// Runs the `resolve_item_icon_path` routine for resolve item icon path in the `graphic::components::hud` module.
fn resolve_item_icon_path(
    registry: &ItemRegistry,
    block_registry: &BlockRegistry,
    asset_server: &AssetServer,
    image_cache: &mut ImageCache,
    images: &mut Assets<Image>,
    item_id: u16,
) -> Option<String> {
    let path = registry.icon_path(asset_server, item_id)?;
    ensure_block_icon_cached(
        block_registry,
        asset_server,
        image_cache,
        images,
        path.as_str(),
    );
    ensure_item_icon_nearest_sampler(asset_server, image_cache, images, path.as_str());
    Some(path)
}

/// Ensures a virtual block-icon path has a populated in-memory image cache entry.
fn ensure_block_icon_cached(
    block_registry: &BlockRegistry,
    asset_server: &AssetServer,
    image_cache: &mut ImageCache,
    images: &mut Assets<Image>,
    path: &str,
) {
    let Some(block_id) = parse_block_icon_cache_key(path) else {
        return;
    };
    if image_cache.map.contains_key(path) {
        return;
    }
    let Some(image) = build_block_item_icon_image(block_registry, asset_server, block_id) else {
        return;
    };
    let handle = images.add(image);
    apply_nearest_sampler_to_image(images, handle.id());
    image_cache.map.insert(path.to_string(), handle);
}

/// Ensures item icons from `textures/items/*` are sampled with nearest filtering.
fn ensure_item_icon_nearest_sampler(
    asset_server: &AssetServer,
    image_cache: &ImageCache,
    images: &mut Assets<Image>,
    path: &str,
) {
    if parse_block_icon_cache_key(path).is_some() {
        if let Some(handle) = image_cache.map.get(path) {
            apply_nearest_sampler_to_image(images, handle.id());
        }
        return;
    }

    if !path.starts_with("textures/items/") {
        return;
    }

    let handle: Handle<Image> = asset_server.load(path.to_string());
    apply_nearest_sampler_to_image(images, handle.id());
}

#[inline]
fn apply_nearest_sampler_to_image(images: &mut Assets<Image>, image_id: AssetId<Image>) {
    let Some(image) = images.get_mut(image_id) else {
        return;
    };

    image.sampler = bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
        address_mode_u: bevy::image::ImageAddressMode::ClampToEdge,
        address_mode_v: bevy::image::ImageAddressMode::ClampToEdge,
        address_mode_w: bevy::image::ImageAddressMode::ClampToEdge,
        mag_filter: bevy::image::ImageFilterMode::Nearest,
        min_filter: bevy::image::ImageFilterMode::Nearest,
        mipmap_filter: bevy::image::ImageFilterMode::Nearest,
        anisotropy_clamp: 1,
        ..default()
    });
}
