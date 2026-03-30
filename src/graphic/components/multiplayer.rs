fn enter_multiplayer_screen(
    mut commands: Commands,
    ui_entities: Res<UiEntities>,
    mut ui_state: ResMut<MultiplayerUiState>,
    mut probe_runtime: NonSendMut<ServerProbeRuntime>,
    item_entities: Query<Entity, With<MultiplayerListItem>>,
    mut form_inputs: Query<(&CssID, &mut InputField, &mut InputValue)>,
    mut roots: ParamSet<(
        Query<&mut Visibility, With<MultiplayerRoot>>,
        Query<&mut Visibility, With<MainMenuRoot>>,
    )>,
) {
    if let Ok(mut visible) = roots.p1().single_mut() {
        *visible = Visibility::Hidden;
    }
    if let Ok(mut visible) = roots.p0().single_mut() {
        *visible = Visibility::Inherited;
    }

    ui_state.saved_servers = load_saved_servers();
    ui_state.form_dialog = None;
    ui_state.pending_delete_key = None;
    ui_state.joining_key = None;
    rebuild_display_servers(&mut ui_state, 0.0);
    rebuild_multiplayer_cards(
        &mut commands,
        ui_entities.multiplayer_server_list,
        &mut ui_state,
        &item_entities,
    );
    clear_server_form_inputs(&mut form_inputs);
    probe_runtime.configure();
}

fn set_multiplayer_interaction(
    mut ui_interaction: ResMut<UiInteractionState>,
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    ui_interaction.menu_open = true;
    if let Ok(mut cursor) = cursor_q.single_mut() {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
}

fn handle_multiplayer_back_navigation(
    keyboard: Res<ButtonInput<KeyCode>>,
    global_config: Res<GlobalConfig>,
    mut ui_state: ResMut<MultiplayerUiState>,
    mut disconnect_writer: MessageWriter<DisconnectFromServerRequest>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let close_key = convert(global_config.input.ui_close_back.as_str()).unwrap_or(KeyCode::Escape);
    if !keyboard.just_pressed(close_key) {
        return;
    }

    if ui_state.joining_key.is_some() {
        ui_state.joining_key = None;
        disconnect_writer.write(DisconnectFromServerRequest);
        return;
    }

    if ui_state.form_dialog.is_some() {
        ui_state.form_dialog = None;
        return;
    }

    if ui_state.pending_delete_key.is_some() {
        ui_state.pending_delete_key = None;
        return;
    }

    next_state.set(AppState::Screen(BeforeUiState::Menu));
}

#[allow(clippy::too_many_arguments)]
fn handle_multiplayer_actions(
    time: Res<Time>,
    mut commands: Commands,
    ui_entities: Res<UiEntities>,
    mut ui_state: ResMut<MultiplayerUiState>,
    mut widgets: Query<(&CssID, &mut UIWidgetState), With<Button>>,
    item_entities: Query<Entity, With<MultiplayerListItem>>,
    mut form_inputs: Query<(&CssID, &mut InputField, &mut InputValue)>,
    mut connect_writer: MessageWriter<ConnectToServerRequest>,
    mut probe_runtime: NonSendMut<ServerProbeRuntime>,
) {
    let actions = collect_multiplayer_actions(&mut widgets);
    if actions.is_empty() {
        return;
    }

    let now = time.elapsed_secs_f64();
    for action in actions {
        match action {
            MultiplayerAction::SelectServer(index) => {
                if let Some(server) = ui_state.display_servers.get(index) {
                    ui_state.selected_key = Some(server.key.clone());
                    ui_state.pending_delete_key = None;
                }
            }
            MultiplayerAction::JoinServer => {
                let selected = ui_state.selected_server().cloned();
                let Some(server) = selected else {
                    continue;
                };

                ui_state.joining_key = Some(server.key.clone());
                connect_writer.write(ConnectToServerRequest {
                    session_url: server.session_url.clone(),
                    server_name: server.server_name.clone(),
                });
            }
            MultiplayerAction::RefreshServers => {
                request_multiplayer_server_probe(&ui_state.saved_servers, &mut probe_runtime, now);
            }
            MultiplayerAction::OpenAddServer => {
                ui_state.form_dialog = Some(ServerFormDialogState {
                    mode: ServerFormMode::Add,
                    editing_saved_index: None,
                });
                let selected = ui_state.selected_server().cloned();
                if let Some(server) = selected {
                    populate_server_form_inputs(
                        &mut form_inputs,
                        server.server_name.as_str(),
                        server.host.as_str(),
                        server.port,
                    );
                } else {
                    clear_server_form_inputs(&mut form_inputs);
                }
            }
            MultiplayerAction::OpenEditServer => {
                let selected = ui_state.selected_server().cloned();
                let Some(server) = selected else {
                    continue;
                };

                let mode = if server.saved_index.is_some() {
                    ServerFormMode::Edit
                } else {
                    ServerFormMode::Add
                };

                ui_state.form_dialog = Some(ServerFormDialogState {
                    mode,
                    editing_saved_index: server.saved_index,
                });
                populate_server_form_inputs(
                    &mut form_inputs,
                    server.server_name.as_str(),
                    server.host.as_str(),
                    server.port,
                );
            }
            MultiplayerAction::OpenDeleteServer => {
                let key = ui_state.selected_server().map(|server| server.key.clone());
                if let Some(key) = key {
                    ui_state.pending_delete_key = Some(key);
                }
            }
            MultiplayerAction::ConfirmDelete => {
                let Some(key) = ui_state.pending_delete_key.take() else {
                    continue;
                };

                if let Some(index) = ui_state
                    .saved_servers
                    .iter()
                    .position(|server| server.key() == key)
                {
                    ui_state.saved_servers.remove(index);
                    save_saved_servers(&ui_state.saved_servers);
                } else {
                    ui_state.dismissed_server_keys.insert(key.clone());
                    ui_state.probed_servers.remove(&key);
                }

                if ui_state.selected_key.as_ref() == Some(&key) {
                    ui_state.selected_key = None;
                }

                if rebuild_display_servers(&mut ui_state, now) {
                    rebuild_multiplayer_cards(
                        &mut commands,
                        ui_entities.multiplayer_server_list,
                        &mut ui_state,
                        &item_entities,
                    );
                }
            }
            MultiplayerAction::AbortDelete => {
                ui_state.pending_delete_key = None;
            }
            MultiplayerAction::SubmitAdd | MultiplayerAction::SubmitEdit => {
                let Some((server_name, host, port)) = read_server_form_inputs(&mut form_inputs)
                else {
                    continue;
                };

                let form_state = ui_state.form_dialog.clone();
                let Some(form_state) = form_state else {
                    continue;
                };

                match form_state.mode {
                    ServerFormMode::Add => {
                        let key = server_key(host.as_str(), port);
                        if let Some(existing) = ui_state
                            .saved_servers
                            .iter_mut()
                            .find(|server| server.key() == key)
                        {
                            existing.server_name = server_name.clone();
                            existing.host = host.clone();
                            existing.port = port;
                        } else {
                            ui_state.saved_servers.push(SavedServerEntry {
                                server_name: server_name.clone(),
                                host: host.clone(),
                                port,
                            });
                        }
                        ui_state.selected_key = Some(key);
                    }
                    ServerFormMode::Edit => {
                        if let Some(index) = form_state.editing_saved_index
                            && let Some(entry) = ui_state.saved_servers.get_mut(index)
                        {
                            entry.server_name = server_name.clone();
                            entry.host = host.clone();
                            entry.port = port;
                            ui_state.selected_key = Some(entry.key());
                        }
                    }
                }

                ui_state
                    .dismissed_server_keys
                    .remove(&server_key(host.as_str(), port));
                ui_state.form_dialog = None;
                save_saved_servers(&ui_state.saved_servers);
                if rebuild_display_servers(&mut ui_state, now) {
                    rebuild_multiplayer_cards(
                        &mut commands,
                        ui_entities.multiplayer_server_list,
                        &mut ui_state,
                        &item_entities,
                    );
                }
            }
            MultiplayerAction::AbortForm => {
                ui_state.form_dialog = None;
            }
        }
    }
}

fn poll_multiplayer_servers(
    time: Res<Time>,
    mut commands: Commands,
    ui_entities: Res<UiEntities>,
    mut ui_state: ResMut<MultiplayerUiState>,
    mut probe_runtime: NonSendMut<ServerProbeRuntime>,
    item_entities: Query<Entity, With<MultiplayerListItem>>,
) {
    if probe_runtime.client.is_none() {
        return;
    }

    let now = time.elapsed_secs_f64();
    probe_runtime.probe_timer.tick(time.delta());
    if probe_runtime.probe_timer.just_finished() {
        request_multiplayer_server_probe(&ui_state.saved_servers, &mut probe_runtime, now);
    }

    let Some(client) = probe_runtime.client.as_ref() else {
        return;
    };
    let Ok(found_servers) = client.poll() else {
        return;
    };

    let mut structure_changed = false;
    for server in found_servers {
        let response_key = session_url_to_key(server.session_url.as_str());
        let observed_key = server.observed_addr.as_ref().and_then(|host| {
            parse_session_url(server.session_url.as_str())
                .map(|(_, port)| server_key(host.as_str(), port))
        });
        let matched_saved_key = response_key
            .as_ref()
            .filter(|key| {
                probe_runtime
                    .pending_direct_probes
                    .contains_key(key.as_str())
            })
            .cloned()
            .or_else(|| {
                observed_key
                    .as_ref()
                    .filter(|key| {
                        probe_runtime
                            .pending_direct_probes
                            .contains_key(key.as_str())
                    })
                    .cloned()
            });
        let ping_ms = matched_saved_key
            .as_ref()
            .and_then(|key| probe_runtime.pending_direct_probes.get(key))
            .map(|sent_at| ((now - *sent_at).max(0.0) * 1000.0).round() as u32)
            .or_else(|| {
                probe_runtime
                    .last_broadcast_sent_at
                    .map(|sent_at| ((now - sent_at).max(0.0) * 1000.0).round() as u32)
            });

        structure_changed |=
            update_probed_server(&mut ui_state, server, matched_saved_key, ping_ms, now);
    }

    if structure_changed && rebuild_display_servers(&mut ui_state, now) {
        rebuild_multiplayer_cards(
            &mut commands,
            ui_entities.multiplayer_server_list,
            &mut ui_state,
            &item_entities,
        );
    } else {
        let _ = rebuild_display_servers(&mut ui_state, now);
    }
}

fn rebuild_multiplayer_cards(
    commands: &mut Commands,
    list_entity: Entity,
    ui_state: &mut MultiplayerUiState,
    existing_items: &Query<Entity, With<MultiplayerListItem>>,
) {
    for entity in existing_items.iter() {
        commands.entity(entity).despawn();
    }
    commands.entity(list_entity).with_children(|list| {
        if ui_state.display_servers.is_empty() {
            list.spawn((
                Paragraph {
                    text: "No servers found.".to_string(),
                    ..default()
                },
                UiTextTone::Darker,
                MultiplayerListItem,
            ));
            return;
        }

        for (index, server) in ui_state.display_servers.iter().enumerate() {
            let ping = server
                .ping_ms
                .map(|value| format!("{value} ms"))
                .unwrap_or_else(|| "-".to_string());

            list.spawn((
                Button {
                    text: format!(
                        "Server Name: {}\nMOTD: {}\nServer IP: {}\nServer Port: {}\nPing: {}",
                        server.server_name, server.motd, server.host, server.port, ping
                    ),
                    ..default()
                },
                CssID(format!("{MULTIPLAYER_CARD_PREFIX}{index}")),
                UiButtonKind::Card,
                UiButtonTone::Normal,
                MultiplayerListItem,
            ));
        }
    });

    ui_state.rendered_keys = ui_state
        .display_servers
        .iter()
        .map(|server| server.key.clone())
        .collect();
}

fn sync_multiplayer_dialogs(
    ui_state: Res<MultiplayerUiState>,
    connection_state: Res<MultiplayerConnectionState>,
    mut visibilities: ParamSet<(
        Query<&mut Visibility, With<MultiplayerFormDialog>>,
        Query<&mut Visibility, With<MultiplayerFormAddButton>>,
        Query<&mut Visibility, With<MultiplayerFormEditButton>>,
        Query<&mut Visibility, With<MultiplayerDeleteDialog>>,
        Query<&mut Visibility, With<MultiplayerConnectDialog>>,
    )>,
    mut form_title: Query<(&CssID, &mut Paragraph)>,
) {
    if let Ok(mut visible) = visibilities.p0().single_mut() {
        *visible = if ui_state.form_dialog.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    if let Some(dialog_state) = ui_state.form_dialog.as_ref() {
        for (css_id, mut paragraph) in &mut form_title {
            if css_id.0 != MULTIPLAYER_FORM_TITLE_ID {
                continue;
            }
            paragraph.text = match dialog_state.mode {
                ServerFormMode::Add => "Add Server".to_string(),
                ServerFormMode::Edit => "Edit Server".to_string(),
            };
        }
    }

    if let Ok(mut visible) = visibilities.p1().single_mut() {
        *visible = if ui_state
            .form_dialog
            .as_ref()
            .is_some_and(|state| state.mode == ServerFormMode::Add)
        {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    if let Ok(mut visible) = visibilities.p2().single_mut() {
        *visible = if ui_state
            .form_dialog
            .as_ref()
            .is_some_and(|state| state.mode == ServerFormMode::Edit)
        {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    if let Ok(mut visible) = visibilities.p3().single_mut() {
        *visible = if ui_state.pending_delete_key.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    if let Ok(mut visible) = visibilities.p4().single_mut() {
        *visible = if ui_state.joining_key.is_some()
            || connection_state.phase == MultiplayerConnectionPhase::Connecting
        {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

fn sync_multiplayer_card_text(
    ui_state: Res<MultiplayerUiState>,
    connection_state: Res<MultiplayerConnectionState>,
    mut card_buttons: Query<(&CssID, &mut Button)>,
    mut delete_text: Query<(&mut Paragraph, Option<&MultiplayerDeleteText>)>,
) {
    for (css_id, mut button) in &mut card_buttons {
        let Some(index) = parse_card_index(css_id.0.as_str(), MULTIPLAYER_CARD_PREFIX) else {
            continue;
        };
        let Some(server) = ui_state.display_servers.get(index) else {
            continue;
        };
        let connected = is_active_connected_server(server, &connection_state);
        let motd = if connected && !server.online {
            "Connected (Discovery unavailable)".to_string()
        } else {
            server.motd.clone()
        };
        let ping = match server.ping_ms {
            Some(ping) if server.online => format!("{ping} ms"),
            _ if connected => "connected".to_string(),
            _ => "-".to_string(),
        };
        button.text = format!(
            "Server Name: {}\nMOTD: {}\nServer IP: {}\nServer Port: {}\nPing: {}",
            server.server_name, motd, server.host, server.port, ping
        );
    }

    let name = ui_state
        .pending_delete_key
        .as_ref()
        .and_then(|key| {
            ui_state
                .display_servers
                .iter()
                .find(|server| &server.key == key)
        })
        .map(|server| server.server_name.as_str())
        .unwrap_or_default();

    for (mut paragraph, marker) in &mut delete_text {
        if marker.is_none() {
            continue;
        }
        paragraph.text = format!("Delete server `{name}`?");
    }
}

fn sync_multiplayer_card_style(
    ui_state: Res<MultiplayerUiState>,
    connection_state: Res<MultiplayerConnectionState>,
    mut cards: Query<(&CssID, &mut BorderColor, &mut BackgroundColor), With<Button>>,
) {
    for (css_id, mut border, mut background) in &mut cards {
        let Some(index) = parse_card_index(css_id.0.as_str(), MULTIPLAYER_CARD_PREFIX) else {
            continue;
        };
        let Some(server) = ui_state.display_servers.get(index) else {
            continue;
        };
        let connected = is_active_connected_server(server, &connection_state);

        let border_color = if server.online || connected {
            color_accent()
        } else {
            color_background_hover()
        };

        border.top = border_color;
        border.right = border_color;
        border.bottom = border_color;
        border.left = border_color;

        background.0 = if ui_state.selected_key.as_ref() == Some(&server.key) {
            color_background_hover()
        } else {
            color_background()
        };
    }
}

fn exit_multiplayer_screen(
    mut ui_interaction: ResMut<UiInteractionState>,
    mut ui_state: ResMut<MultiplayerUiState>,
    mut root: Query<&mut Visibility, With<MultiplayerRoot>>,
) {
    if let Ok(mut visible) = root.single_mut() {
        *visible = Visibility::Hidden;
    }
    ui_interaction.menu_open = false;
    ui_state.form_dialog = None;
    ui_state.pending_delete_key = None;
    ui_state.joining_key = None;
}

fn collect_multiplayer_actions(
    widgets: &mut Query<(&CssID, &mut UIWidgetState), With<Button>>,
) -> Vec<MultiplayerAction> {
    let mut actions = Vec::new();

    for (css_id, mut state) in widgets.iter_mut() {
        if let Some(index) = parse_card_index(css_id.0.as_str(), MULTIPLAYER_CARD_PREFIX) {
            if state.checked || state.focused {
                state.checked = false;
                state.focused = false;
                actions.push(MultiplayerAction::SelectServer(index));
            }
            continue;
        }

        if !state.checked {
            continue;
        }

        state.checked = false;
        if let Some(action) = parse_multiplayer_action(css_id.0.as_str()) {
            actions.push(action);
        }
    }

    actions
}

fn parse_multiplayer_action(id: &str) -> Option<MultiplayerAction> {
    match id {
        MULTIPLAYER_JOIN_ID => Some(MultiplayerAction::JoinServer),
        MULTIPLAYER_REFRESH_ID => Some(MultiplayerAction::RefreshServers),
        MULTIPLAYER_ADD_ID => Some(MultiplayerAction::OpenAddServer),
        MULTIPLAYER_EDIT_ID => Some(MultiplayerAction::OpenEditServer),
        MULTIPLAYER_DELETE_ID => Some(MultiplayerAction::OpenDeleteServer),
        MULTIPLAYER_DELETE_CONFIRM_ID => Some(MultiplayerAction::ConfirmDelete),
        MULTIPLAYER_DELETE_ABORT_ID => Some(MultiplayerAction::AbortDelete),
        MULTIPLAYER_FORM_ADD_ID => Some(MultiplayerAction::SubmitAdd),
        MULTIPLAYER_FORM_EDIT_ID => Some(MultiplayerAction::SubmitEdit),
        MULTIPLAYER_FORM_ABORT_ID => Some(MultiplayerAction::AbortForm),
        _ => parse_card_index(id, MULTIPLAYER_CARD_PREFIX).map(MultiplayerAction::SelectServer),
    }
}

fn parse_card_index(id: &str, prefix: &str) -> Option<usize> {
    id.strip_prefix(prefix)?.parse::<usize>().ok()
}

fn rebuild_display_servers(ui_state: &mut MultiplayerUiState, now: f64) -> bool {
    let mut display_servers = ui_state
        .saved_servers
        .iter()
        .enumerate()
        .map(|(index, server)| DisplayServerEntry {
            key: server.key(),
            saved_index: Some(index),
            server_name: server.server_name.clone(),
            host: server.host.clone(),
            port: server.port,
            motd: offline_status_message(server.port),
            ping_ms: None,
            online: false,
            session_url: server.session_url(),
        })
        .collect::<Vec<_>>();

    for status in ui_state.probed_servers.values() {
        let response_key = session_url_to_key(status.session_url.as_str());
        let observed_key = status.observed_host.as_ref().and_then(|host| {
            parse_session_url(status.session_url.as_str()).map(|(_, port)| server_key(host, port))
        });

        let target_key = status
            .matched_saved_key
            .clone()
            .or_else(|| {
                response_key
                    .as_ref()
                    .filter(|key| display_servers.iter().any(|server| &server.key == *key))
                    .cloned()
            })
            .or_else(|| {
                observed_key
                    .as_ref()
                    .filter(|key| display_servers.iter().any(|server| &server.key == *key))
                    .cloned()
            })
            .or(response_key.clone())
            .or(observed_key.clone());

        let Some(target_key) = target_key else {
            continue;
        };

        if ui_state.dismissed_server_keys.contains(&target_key)
            && !display_servers
                .iter()
                .any(|server| server.key == target_key)
        {
            continue;
        }

        let online = (now - status.last_seen_at) <= SERVER_STALE_AFTER_SECS;

        if let Some(existing) = display_servers
            .iter_mut()
            .find(|server| server.key == target_key)
        {
            existing.server_name = status.server_name.clone();
            existing.motd = if online {
                status.motd.clone()
            } else {
                offline_status_message(existing.port)
            };
            existing.ping_ms = if online { status.ping_ms } else { None };
            existing.online = online;
            existing.session_url = status.session_url.clone();
            if let Some((host, port)) = parse_session_url(status.session_url.as_str()) {
                if existing.saved_index.is_none() {
                    existing.host = status.observed_host.clone().unwrap_or(host);
                }
                existing.port = port;
            }
            continue;
        }

        if let Some((host, port)) = parse_session_url(status.session_url.as_str()) {
            display_servers.push(DisplayServerEntry {
                key: target_key,
                saved_index: None,
                server_name: status.server_name.clone(),
                host: status.observed_host.clone().unwrap_or(host),
                port,
                motd: if online {
                    status.motd.clone()
                } else {
                    offline_status_message(port)
                },
                ping_ms: if online { status.ping_ms } else { None },
                online,
                session_url: status.session_url.clone(),
            });
        }
    }

    display_servers.sort_by(|left, right| {
        left.saved_index
            .is_none()
            .cmp(&right.saved_index.is_none())
            .then_with(|| left.key.cmp(&right.key))
    });

    if ui_state
        .selected_key
        .as_ref()
        .is_some_and(|key| !display_servers.iter().any(|server| &server.key == key))
    {
        ui_state.selected_key = None;
    }

    let new_keys = display_servers
        .iter()
        .map(|server| server.key.clone())
        .collect::<Vec<_>>();
    let structure_changed = new_keys != ui_state.rendered_keys;
    ui_state.display_servers = display_servers;
    structure_changed
}

fn update_probed_server(
    ui_state: &mut MultiplayerUiState,
    server: LanServerInfo,
    matched_saved_key: Option<String>,
    ping_ms: Option<u32>,
    now: f64,
) -> bool {
    let Some(storage_key) = session_url_to_key(server.session_url.as_str()).or_else(|| {
        server.observed_addr.as_ref().and_then(|host| {
            parse_session_url(server.session_url.as_str()).map(|(_, port)| server_key(host, port))
        })
    }) else {
        return false;
    };

    let is_new = !ui_state.probed_servers.contains_key(&storage_key);
    ui_state.probed_servers.insert(
        storage_key,
        ProbedServerStatus {
            session_url: server.session_url,
            observed_host: server.observed_addr,
            matched_saved_key,
            server_name: server.server_name,
            motd: server.motd,
            ping_ms,
            last_seen_at: now,
        },
    );
    is_new
}

fn load_saved_servers() -> Vec<SavedServerEntry> {
    let path = PathBuf::from(MULTIPLAYER_SERVER_FILE);
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };

    toml::from_str::<SavedServerConfig>(&contents)
        .map(|config| config.servers)
        .unwrap_or_default()
}

fn save_saved_servers(servers: &[SavedServerEntry]) {
    let config = SavedServerConfig {
        servers: servers.to_vec(),
    };
    let Ok(text) = toml::to_string_pretty(&config) else {
        warn!("Failed to serialize multiplayer server list.");
        return;
    };

    let path = PathBuf::from(MULTIPLAYER_SERVER_FILE);
    if let Some(parent) = path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        warn!("Failed to create multiplayer config directory: {}", error);
        return;
    }

    if let Err(error) = fs::write(&path, text) {
        warn!(
            "Failed to write multiplayer server list {:?}: {}",
            path, error
        );
    }
}

fn read_server_form_inputs(
    form_inputs: &mut Query<(&CssID, &mut InputField, &mut InputValue)>,
) -> Option<(String, String, u16)> {
    let mut name_text = String::new();
    let mut address_text = String::new();

    for (css_id, field, _) in form_inputs.iter_mut() {
        if css_id.0 == MULTIPLAYER_FORM_NAME_INPUT_ID {
            name_text = field.text.clone();
            continue;
        }
        if css_id.0 == MULTIPLAYER_FORM_ADDRESS_INPUT_ID {
            address_text = field.text.clone();
        }
    }

    let server_name = name_text.trim().to_string();
    if server_name.is_empty() {
        warn!("Add Server: server name is required.");
        return None;
    }

    let Some((host, port)) = parse_server_address(address_text.as_str()) else {
        return None;
    };

    let normalized_address = display_server_address(host.as_str(), port);
    for (css_id, mut field, mut input_value) in form_inputs.iter_mut() {
        if css_id.0 != MULTIPLAYER_FORM_ADDRESS_INPUT_ID {
            continue;
        }
        field.text = normalized_address.clone();
        field.cursor_position = field.text.len();
        input_value.0 = field.text.clone();
    }

    Some((server_name, host, port))
}

fn populate_server_form_inputs(
    form_inputs: &mut Query<(&CssID, &mut InputField, &mut InputValue)>,
    server_name: &str,
    host: &str,
    port: u16,
) {
    for (css_id, mut field, mut input_value) in form_inputs.iter_mut() {
        if css_id.0 == MULTIPLAYER_FORM_NAME_INPUT_ID {
            field.text = server_name.to_string();
            field.cursor_position = field.text.len();
            input_value.0 = field.text.clone();
            continue;
        }

        if css_id.0 == MULTIPLAYER_FORM_ADDRESS_INPUT_ID {
            field.text = display_server_address(host, port);
            field.cursor_position = field.text.len();
            input_value.0 = field.text.clone();
        }
    }
}

fn clear_server_form_inputs(form_inputs: &mut Query<(&CssID, &mut InputField, &mut InputValue)>) {
    for (css_id, mut field, mut input_value) in form_inputs.iter_mut() {
        if css_id.0 != MULTIPLAYER_FORM_NAME_INPUT_ID
            && css_id.0 != MULTIPLAYER_FORM_ADDRESS_INPUT_ID
        {
            continue;
        }
        field.text.clear();
        field.cursor_position = 0;
        input_value.0.clear();
    }
}

fn parse_server_address(input: &str) -> Option<(String, u16)> {
    let mut value = input.trim().trim_matches('/').to_string();
    if value.is_empty() {
        warn!("Add Server: server IP is required.");
        return None;
    }

    if let Some(stripped) = value.strip_prefix("http://") {
        value = stripped.to_string();
    }
    if let Some(stripped) = value.strip_prefix("https://") {
        value = stripped.to_string();
    }

    if let Some((host, port_text)) = value.rsplit_once(':')
        && port_text.chars().all(|ch| ch.is_ascii_digit())
    {
        let port = match port_text.parse::<u16>() {
            Ok(port) => port,
            Err(_) => {
                warn!("Add Server: invalid port '{}'.", port_text);
                return None;
            }
        };
        let host = host.trim().trim_end_matches('/').to_string();
        if host.is_empty() {
            warn!("Add Server: invalid server IP.");
            return None;
        }
        return Some((host, port));
    }

    Some((value.trim_end_matches('/').to_string(), DEFAULT_SERVER_PORT))
}

fn display_server_address(host: &str, port: u16) -> String {
    format!("http://{host}:{port}")
}

fn resolve_probe_addrs(host: &str, port: u16) -> Vec<SocketAddr> {
    format!("{host}:{port}")
        .to_socket_addrs()
        .map(|iter| iter.collect())
        .unwrap_or_default()
}

fn request_multiplayer_server_probe(
    saved_servers: &[SavedServerEntry],
    probe_runtime: &mut ServerProbeRuntime,
    now: f64,
) {
    let Some(client) = probe_runtime.client.as_ref() else {
        return;
    };

    if let Err(error) = client.broadcast_query() {
        warn!("LAN discovery broadcast failed: {}", error);
    } else {
        probe_runtime.last_broadcast_sent_at = Some(now);
    }

    for server in saved_servers {
        for addr in resolve_probe_addrs(server.host.as_str(), discovery_port_for(server.port)) {
            if let Err(error) = client.query_addr(addr) {
                warn!("Probe for {} failed: {}", server.key(), error);
                continue;
            }
            probe_runtime
                .pending_direct_probes
                .insert(server.key(), now);
        }
    }
}

fn discovery_port_for(game_port: u16) -> u16 {
    game_port.saturating_add(1)
}

fn server_key(host: &str, port: u16) -> String {
    format!("{}:{}", host.trim().to_ascii_lowercase(), port)
}

fn session_url_to_key(session_url: &str) -> Option<String> {
    parse_session_url(session_url).map(|(host, port)| server_key(host.as_str(), port))
}

fn parse_session_url(session_url: &str) -> Option<(String, u16)> {
    let trimmed = session_url.trim();
    let without_scheme = trimmed
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(trimmed);
    let host_port = without_scheme.split('/').next()?.trim();
    let (host, port_text) = host_port.rsplit_once(':')?;
    let port = port_text.parse::<u16>().ok()?;
    Some((host.to_string(), port))
}

fn offline_status_message(game_port: u16) -> String {
    format!(
        "No discovery response on UDP {}.",
        discovery_port_for(game_port)
    )
}

fn is_active_connected_server(
    server: &DisplayServerEntry,
    connection_state: &MultiplayerConnectionState,
) -> bool {
    if !connection_state.connected {
        return false;
    }

    let Some(active_url) = connection_state.active_session_url.as_ref() else {
        return false;
    };

    let expected_key = server_key(server.host.as_str(), server.port);
    let active_key = session_url_to_key(active_url.as_str());
    active_key
        .as_ref()
        .is_some_and(|key| key == &server.key || key == &expected_key)
}
