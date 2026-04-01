fn enter_single_player_screen(
    time: Res<Time>,
    mut commands: Commands,
    ui_entities: Res<UiEntities>,
    mut ui_state: ResMut<SinglePlayerUiState>,
    world_gen_config: Res<WorldGenConfig>,
    item_entities: Query<Entity, With<SinglePlayerListItem>>,
    mut create_inputs: Query<(&CssID, &mut InputField, &mut InputValue)>,
    children_q: Query<&Children>,
    names_q: Query<&Name>,
    mut roots: ParamSet<(
        Query<&mut Visibility, With<SinglePlayerRoot>>,
        Query<&mut Visibility, With<CreateWorldRoot>>,
        Query<&mut Visibility, With<MainMenuRoot>>,
    )>,
) {
    if let Ok(mut visible) = roots.p2().single_mut() {
        *visible = Visibility::Hidden;
    }

    ui_state.page = SinglePlayerPage::List;
    ui_state.pending_delete_index = None;
    ui_state.last_card_click = Some((usize::MAX, time.elapsed_secs_f64()));
    ui_state.closing_for_world_load = false;

    refresh_single_player_content(&mut ui_state, world_gen_config.seed);
    rebuild_single_player_cards(
        &mut commands,
        ui_entities.single_player_world_list,
        &ui_state.worlds,
        &item_entities,
        &children_q,
        &names_q,
    );
    clear_create_world_inputs(&mut create_inputs);

    if let Ok(mut visible) = roots.p0().single_mut() {
        *visible = Visibility::Inherited;
    }
    if let Ok(mut visible) = roots.p1().single_mut() {
        *visible = Visibility::Hidden;
    }
}

fn set_single_player_interaction(
    ui_state: Res<SinglePlayerUiState>,
    mut ui_interaction: ResMut<UiInteractionState>,
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if ui_state.closing_for_world_load {
        ui_interaction.menu_open = false;
        return;
    }

    ui_interaction.menu_open = true;
    if let Ok(mut cursor) = cursor_q.single_mut() {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
}

fn sync_single_player_visibility(
    ui_state: Res<SinglePlayerUiState>,
    mut roots: ParamSet<(
        Query<&mut Visibility, With<SinglePlayerRoot>>,
        Query<&mut Visibility, With<CreateWorldRoot>>,
        Query<&mut Visibility, With<SinglePlayerDeleteDialog>>,
    )>,
) {
    if ui_state.closing_for_world_load {
        if let Ok(mut visible) = roots.p0().single_mut() {
            *visible = Visibility::Hidden;
        }
        if let Ok(mut visible) = roots.p1().single_mut() {
            *visible = Visibility::Hidden;
        }
        return;
    }

    if let Ok(mut visible) = roots.p0().single_mut() {
        *visible = if ui_state.page == SinglePlayerPage::List {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    if let Ok(mut visible) = roots.p1().single_mut() {
        *visible = if ui_state.page == SinglePlayerPage::CreateWorld {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    if let Ok(mut visible) = roots.p2().single_mut() {
        *visible = if ui_state.page == SinglePlayerPage::List
            && ui_state.pending_delete_index.is_some()
        {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

fn sync_single_player_delete_dialog(
    ui_state: Res<SinglePlayerUiState>,
    mut paragraphs: Query<(&mut Paragraph, Option<&SinglePlayerDeleteText>)>,
) {
    let name = ui_state
        .pending_delete_index
        .and_then(|index| ui_state.worlds.get(index))
        .map(|world| world.folder_name.as_str())
        .unwrap_or_default();

    for (mut paragraph, marker) in &mut paragraphs {
        if marker.is_none() {
            continue;
        }
        paragraph.text = format!("Delete world `{name}`?");
    }
}

fn sync_single_player_card_style(
    ui_state: Res<SinglePlayerUiState>,
    mut cards: Query<(&CssID, &mut BorderColor, &mut BackgroundColor), With<Button>>,
) {
    if ui_state.page != SinglePlayerPage::List {
        return;
    }

    for (css_id, mut border, mut background) in &mut cards {
        let Some(index) = parse_world_card_index(css_id.0.as_str()) else {
            continue;
        };

        if ui_state.selected_index == Some(index) {
            background.0 = color_background_hover();
            border.top = color_accent();
            border.right = color_accent();
            border.bottom = color_accent();
            border.left = color_accent();
        }
    }
}

fn handle_single_player_back_navigation(
    keyboard: Res<ButtonInput<KeyCode>>,
    global_config: Res<GlobalConfig>,
    mut ui_state: ResMut<SinglePlayerUiState>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let close_key = convert(global_config.input.ui_close_back.as_str()).unwrap_or(KeyCode::Escape);
    if !keyboard.just_pressed(close_key) {
        return;
    }

    if ui_state.pending_delete_index.is_some() {
        ui_state.pending_delete_index = None;
        return;
    }

    match ui_state.page {
        SinglePlayerPage::CreateWorld => {
            ui_state.page = SinglePlayerPage::List;
        }
        SinglePlayerPage::List => {
            next_state.set(AppState::Screen(BeforeUiState::Menu));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_single_player_actions(
    time: Res<Time>,
    mut commands: Commands,
    ui_entities: Res<UiEntities>,
    mut ui_state: ResMut<SinglePlayerUiState>,
    mut ui_interaction: ResMut<UiInteractionState>,
    mut world_gen_config: ResMut<WorldGenConfig>,
    mut widgets: Query<(&CssID, &mut UIWidgetState), With<Button>>,
    item_entities: Query<Entity, With<SinglePlayerListItem>>,
    mut create_inputs: Query<(&CssID, &mut InputField, &mut InputValue)>,
    children_q: Query<&Children>,
    names_q: Query<&Name>,
    mut next_state: ResMut<NextState<AppState>>,
    mut region_cache: ResMut<RegionCache>,
    mut chunk_map: ResMut<ChunkMap>,
    mut fluid_map: ResMut<FluidMap>,
    mut water_mesh_index: ResMut<WaterMeshIndex>,
) {
    let actions = collect_single_player_actions(&mut widgets);
    if actions.is_empty() {
        return;
    }

    for action in actions {
        match action {
            SinglePlayerAction::SelectWorld(index) => {
                if ui_state.page != SinglePlayerPage::List || index >= ui_state.worlds.len() {
                    continue;
                }

                let now = time.elapsed_secs_f64();
                let double_click = ui_state
                    .last_card_click
                    .is_some_and(|(last_idx, last_time)| {
                        last_idx == index && (now - last_time) <= DOUBLE_CLICK_WINDOW_SECS
                    });

                ui_state.selected_index = Some(index);
                ui_state.pending_delete_index = None;
                ui_state.last_card_click = Some((index, now));

                if double_click && let Some(entry) = ui_state.worlds.get(index).cloned() {
                    ui_state.closing_for_world_load = true;
                    ui_interaction.menu_open = false;
                    load_world_and_start(
                        &entry,
                        &mut world_gen_config,
                        &mut commands,
                        &mut next_state,
                        &mut region_cache,
                        &mut chunk_map,
                        &mut fluid_map,
                        &mut water_mesh_index,
                    );
                    return;
                }
            }
            SinglePlayerAction::OpenCreateWorld => {
                if ui_state.page != SinglePlayerPage::List {
                    continue;
                }
                ui_state.page = SinglePlayerPage::CreateWorld;
                ui_state.pending_delete_index = None;
                clear_create_world_inputs(&mut create_inputs);
            }
            SinglePlayerAction::PlayWorld => {
                if ui_state.page != SinglePlayerPage::List {
                    continue;
                }
                let entry = ui_state
                    .selected_index
                    .and_then(|index| ui_state.worlds.get(index))
                    .cloned();
                if let Some(entry) = entry {
                    ui_state.closing_for_world_load = true;
                    ui_interaction.menu_open = false;
                    load_world_and_start(
                        &entry,
                        &mut world_gen_config,
                        &mut commands,
                        &mut next_state,
                        &mut region_cache,
                        &mut chunk_map,
                        &mut fluid_map,
                        &mut water_mesh_index,
                    );
                    return;
                }
            }
            SinglePlayerAction::DeleteWorld => {
                if ui_state.page != SinglePlayerPage::List {
                    continue;
                }
                if let Some(index) = ui_state
                    .selected_index
                    .filter(|&idx| idx < ui_state.worlds.len())
                {
                    ui_state.pending_delete_index = Some(index);
                }
            }
            SinglePlayerAction::ConfirmDelete => {
                if ui_state.page != SinglePlayerPage::List {
                    continue;
                }
                let Some(index) = ui_state.pending_delete_index.take() else {
                    continue;
                };
                let Some(entry) = ui_state.worlds.get(index).cloned() else {
                    continue;
                };

                match fs::remove_dir_all(&entry.path) {
                    Ok(_) => info!("Deleted world '{}'", entry.folder_name),
                    Err(error) => {
                        warn!("Failed to delete world '{}': {}", entry.folder_name, error)
                    }
                }

                ui_state.selected_index = None;
                ui_state.last_card_click = None;
                refresh_single_player_content(&mut ui_state, world_gen_config.seed);
                rebuild_single_player_cards(
                    &mut commands,
                    ui_entities.single_player_world_list,
                    &ui_state.worlds,
                    &item_entities,
                    &children_q,
                    &names_q,
                );
            }
            SinglePlayerAction::CancelDelete => {
                ui_state.pending_delete_index = None;
            }
            SinglePlayerAction::CreateWorldSubmit => {
                if ui_state.page != SinglePlayerPage::CreateWorld {
                    continue;
                }

                let Some((folder_name, seed_override)) =
                    read_create_world_inputs(&mut create_inputs)
                else {
                    continue;
                };

                let Some(entry) = create_world_with_name(
                    folder_name.as_str(),
                    seed_override,
                    world_gen_config.seed,
                ) else {
                    continue;
                };

                ui_state.closing_for_world_load = true;
                ui_interaction.menu_open = false;
                load_world_and_start(
                    &entry,
                    &mut world_gen_config,
                    &mut commands,
                    &mut next_state,
                    &mut region_cache,
                    &mut chunk_map,
                    &mut fluid_map,
                    &mut water_mesh_index,
                );
                return;
            }
            SinglePlayerAction::CreateWorldAbort => {
                if ui_state.page != SinglePlayerPage::CreateWorld {
                    continue;
                }
                ui_state.page = SinglePlayerPage::List;
            }
        }
    }
}

fn exit_single_player_screen(
    mut commands: Commands,
    mut ui_state: ResMut<SinglePlayerUiState>,
    mut ui_interaction: ResMut<UiInteractionState>,
    item_entities: Query<Entity, With<SinglePlayerListItem>>,
    mut roots: ParamSet<(
        Query<&mut Visibility, With<SinglePlayerRoot>>,
        Query<&mut Visibility, With<CreateWorldRoot>>,
    )>,
) {
    if let Ok(mut visible) = roots.p0().single_mut() {
        *visible = Visibility::Hidden;
    }
    if let Ok(mut visible) = roots.p1().single_mut() {
        *visible = Visibility::Hidden;
    }

    for entity in item_entities.iter() {
        commands.entity(entity).despawn();
    }

    ui_interaction.menu_open = false;
    ui_state.page = SinglePlayerPage::List;
    ui_state.pending_delete_index = None;
    ui_state.last_card_click = None;
    ui_state.closing_for_world_load = false;
}

fn refresh_single_player_content(ui_state: &mut SinglePlayerUiState, default_seed: i32) {
    let selected_name = ui_state
        .selected_index
        .and_then(|index| ui_state.worlds.get(index))
        .map(|world| world.folder_name.clone());

    ui_state.worlds = list_saved_worlds(default_seed);
    ui_state.selected_index = selected_name.and_then(|name| {
        ui_state
            .worlds
            .iter()
            .position(|world| world.folder_name == name)
    });
    ui_state.pending_delete_index = ui_state
        .pending_delete_index
        .filter(|&index| index < ui_state.worlds.len());
}

/// Finds the `Div-ScrollContent-*` child of a Div wrapper entity, if it exists.
/// Cards must be inserted there directly so the scroll container can measure them.
fn find_scroll_content_child(
    parent: Entity,
    children_q: &Query<&Children>,
    names_q: &Query<&Name>,
) -> Option<Entity> {
    if let Ok(children) = children_q.get(parent) {
        for child in children.iter() {
            if let Ok(name) = names_q.get(child) {
                if name.as_str().starts_with("Div-ScrollContent-") {
                    return Some(child);
                }
            }
        }
    }
    None
}

fn rebuild_single_player_cards(
    commands: &mut Commands,
    list_entity: Entity,
    worlds: &[SavedWorldEntry],
    existing_items: &Query<Entity, With<SinglePlayerListItem>>,
    children_q: &Query<&Children>,
    names_q: &Query<&Name>,
) {
    let target = find_scroll_content_child(list_entity, children_q, names_q)
        .unwrap_or(list_entity);

    for entity in existing_items.iter() {
        commands.entity(entity).despawn();
    }
    commands.entity(target).with_children(|list| {
        if worlds.is_empty() {
            list.spawn((
                Paragraph {
                    text: "No worlds found in saves/".to_string(),
                    ..default()
                },
                UiTextTone::Darker,
                SinglePlayerListItem,
            ));
            return;
        }

        for (index, world) in worlds.iter().enumerate() {
            list.spawn((
                Button {
                    text: format!("WeltName: {}\nSeed: {}", world.folder_name, world.seed),
                    ..default()
                },
                CssID(format!("{SINGLE_PLAYER_WORLD_CARD_PREFIX}{index}")),
                UiButtonKind::Card,
                UiButtonTone::Normal,
                SinglePlayerListItem,
            ));
        }
    });
}

fn collect_single_player_actions(
    widgets: &mut Query<(&CssID, &mut UIWidgetState), With<Button>>,
) -> Vec<SinglePlayerAction> {
    let mut actions = Vec::new();

    for (css_id, mut state) in widgets.iter_mut() {
        if let Some(index) = parse_world_card_index(css_id.0.as_str()) {
            if state.checked || state.focused {
                state.checked = false;
                state.focused = false;
                actions.push(SinglePlayerAction::SelectWorld(index));
            }
            continue;
        }

        if !state.checked {
            continue;
        }
        state.checked = false;

        if let Some(action) = parse_single_player_action(css_id.0.as_str()) {
            actions.push(action);
        }
    }

    actions
}

fn parse_single_player_action(id: &str) -> Option<SinglePlayerAction> {
    if id == SINGLE_PLAYER_CREATE_WORLD_ID {
        return Some(SinglePlayerAction::OpenCreateWorld);
    }
    if id == SINGLE_PLAYER_PLAY_WORLD_ID {
        return Some(SinglePlayerAction::PlayWorld);
    }
    if id == SINGLE_PLAYER_DELETE_WORLD_ID {
        return Some(SinglePlayerAction::DeleteWorld);
    }
    if id == SINGLE_PLAYER_DELETE_CONFIRM_ID {
        return Some(SinglePlayerAction::ConfirmDelete);
    }
    if id == SINGLE_PLAYER_DELETE_CANCEL_ID {
        return Some(SinglePlayerAction::CancelDelete);
    }
    if id == CREATE_WORLD_CREATE_ID {
        return Some(SinglePlayerAction::CreateWorldSubmit);
    }
    if id == CREATE_WORLD_ABORT_ID {
        return Some(SinglePlayerAction::CreateWorldAbort);
    }
    parse_world_card_index(id).map(SinglePlayerAction::SelectWorld)
}

fn parse_world_card_index(id: &str) -> Option<usize> {
    id.strip_prefix(SINGLE_PLAYER_WORLD_CARD_PREFIX)?
        .parse::<usize>()
        .ok()
}

fn read_create_world_inputs(
    create_inputs: &mut Query<(&CssID, &mut InputField, &mut InputValue)>,
) -> Option<(String, Option<i32>)> {
    let mut name_text = String::new();
    let mut seed_text = String::new();

    for (css_id, field, _) in create_inputs.iter_mut() {
        if css_id.0 == CREATE_WORLD_NAME_INPUT_ID {
            name_text = field.text.clone();
            continue;
        }
        if css_id.0 == CREATE_WORLD_SEED_INPUT_ID {
            seed_text = field.text.clone();
        }
    }

    let name = name_text.trim().to_string();
    if name.is_empty() {
        warn!("Create World: world name is required.");
        return None;
    }

    let seed_trimmed = seed_text.trim();
    let seed = if seed_trimmed.is_empty() {
        None
    } else {
        match seed_trimmed.parse::<i32>() {
            Ok(value) => Some(value),
            Err(_) => {
                warn!("Create World: seed must be a valid number.");
                return None;
            }
        }
    };

    Some((name, seed))
}

fn clear_create_world_inputs(
    create_inputs: &mut Query<(&CssID, &mut InputField, &mut InputValue)>,
) {
    for (css_id, mut field, mut input_value) in create_inputs.iter_mut() {
        if css_id.0 != CREATE_WORLD_NAME_INPUT_ID && css_id.0 != CREATE_WORLD_SEED_INPUT_ID {
            continue;
        }
        field.text.clear();
        field.cursor_position = 0;
        input_value.0.clear();
    }
}

fn list_saved_worlds(default_seed: i32) -> Vec<SavedWorldEntry> {
    let root = saves_root();
    if let Err(error) = fs::create_dir_all(&root) {
        warn!("Failed to create saves directory {:?}: {}", root, error);
        return Vec::new();
    }

    let mut worlds = Vec::new();
    let Ok(entries) = fs::read_dir(&root) else {
        return worlds;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let folder_name = entry.file_name().to_string_lossy().to_string();
        let seed = read_world_seed(&path, default_seed);
        worlds.push(SavedWorldEntry {
            folder_name,
            seed,
            path,
        });
    }

    worlds.sort_by(|a, b| a.folder_name.cmp(&b.folder_name));
    worlds
}

fn read_world_seed(world_path: &Path, default_seed: i32) -> i32 {
    let meta_path = world_path.join(WORLD_META_FILE);
    let Ok(text) = fs::read_to_string(meta_path) else {
        return default_seed;
    };
    serde_json::from_str::<WorldMeta>(&text)
        .map(|meta| meta.seed)
        .unwrap_or(default_seed)
}

fn create_world_with_name(
    raw_name: &str,
    seed_override: Option<i32>,
    default_seed: i32,
) -> Option<SavedWorldEntry> {
    let normalized = normalize_world_name(raw_name);
    if normalized.is_empty() {
        warn!("Create World: invalid world name.");
        return None;
    }

    let root = saves_root();
    if let Err(error) = fs::create_dir_all(&root) {
        warn!("Failed to create saves directory {:?}: {}", root, error);
        return None;
    }

    let world_path = unique_world_path(&root, normalized.as_str());
    let folder_name = world_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())?;

    let seed =
        seed_override.unwrap_or_else(|| generate_seed(default_seed, folder_name.len() as u64));
    if let Err(error) = fs::create_dir_all(world_path.join("region")) {
        warn!("Failed to create world folder {:?}: {}", world_path, error);
        return None;
    }
    if let Err(error) = write_world_meta(&world_path, seed) {
        warn!("Failed to write world meta for {:?}: {}", world_path, error);
    }

    Some(SavedWorldEntry {
        folder_name,
        seed,
        path: world_path,
    })
}

fn normalize_world_name(raw_name: &str) -> String {
    raw_name
        .trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect::<String>()
}

fn unique_world_path(root: &Path, base_name: &str) -> PathBuf {
    let candidate = root.join(base_name);
    if !candidate.exists() {
        return candidate;
    }

    for i in 2..10_000 {
        let with_suffix = root.join(format!("{base_name}-{i}"));
        if !with_suffix.exists() {
            return with_suffix;
        }
    }

    root.join(format!("{base_name}-{}", generate_seed(1, 0xA11CE_u64)))
}

fn generate_seed(default_seed: i32, salt: u64) -> i32 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let mixed = nanos ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15);

    let mut seed = (mixed as i32).wrapping_abs();
    if seed == 0 {
        seed = default_seed.max(1);
    }
    seed
}

fn write_world_meta(world_path: &Path, seed: i32) -> Result<(), std::io::Error> {
    let meta = WorldMeta { seed };
    let text = serde_json::to_string_pretty(&meta)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    fs::write(world_path.join(WORLD_META_FILE), text)
}

#[allow(clippy::too_many_arguments)]
fn load_world_and_start(
    world: &SavedWorldEntry,
    world_gen_config: &mut WorldGenConfig,
    commands: &mut Commands,
    next_state: &mut NextState<AppState>,
    region_cache: &mut RegionCache,
    chunk_map: &mut ChunkMap,
    fluid_map: &mut FluidMap,
    water_mesh_index: &mut WaterMeshIndex,
) {
    if let Err(error) = fs::create_dir_all(world.path.join("region")) {
        warn!(
            "Failed to prepare world '{}' at {:?}: {}",
            world.folder_name, world.path, error
        );
        return;
    }

    if let Err(error) = write_world_meta(&world.path, world.seed) {
        warn!(
            "Failed to store world metadata for '{}': {}",
            world.folder_name, error
        );
    }

    world_gen_config.seed = world.seed;
    commands.insert_resource(WorldSave::new(world.path.clone()));
    region_cache.0.clear();
    chunk_map.chunks.clear();
    fluid_map.0.clear();
    water_mesh_index.0.clear();
    next_state.set(AppState::Loading(LoadingStates::BaseGen));
}

fn saves_root() -> PathBuf {
    default_saves_root()
}
