use crate::core::config::GlobalConfig;
use crate::core::events::ui_events::{ConnectToServerRequest, DisconnectFromServerRequest};
use crate::core::multiplayer::{MultiplayerConnectionPhase, MultiplayerConnectionState};
use crate::core::states::states::{AppState, BeforeUiState};
use crate::core::ui::UiInteractionState;
use crate::generator::chunk::chunk_utils::safe_despawn_entity;
use crate::utils::key_utils::convert;
use api::core::network::config::NetworkSettings;
use api::core::network::discovery::{LanDiscoveryClient, LanServerInfo};
use bevy::prelude::*;
use bevy::ui::ScrollPosition;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_extended_ui::html::HtmlSource;
use bevy_extended_ui::io::HtmlAsset;
use bevy_extended_ui::registry::UiRegistry;
use bevy_extended_ui::styles::CssID;
use bevy_extended_ui::widgets::{
    Body, Div, InputField, InputValue, Paragraph, Scrollbar, UIGenID, UIWidgetState,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::PathBuf;

const MULTIPLAYER_UI_KEY: &str = "multi-player";
const MULTIPLAYER_UI_PATH: &str = "ui/html/multiplayer.html";
const MULTIPLAYER_ROOT_ID: &str = "multi-player-root";
const MULTIPLAYER_LIST_ID: &str = "multi-player-server-list";

const MULTIPLAYER_CARD_PREFIX: &str = "multi-player-server-card-";
const MULTIPLAYER_NAME_PREFIX: &str = "multi-player-server-name-";
const MULTIPLAYER_IP_PREFIX: &str = "multi-player-server-ip-";
const MULTIPLAYER_PORT_PREFIX: &str = "multi-player-server-port-";
const MULTIPLAYER_MOTD_PREFIX: &str = "multi-player-server-motd-";
const MULTIPLAYER_PING_PREFIX: &str = "multi-player-server-ping-";

const MULTIPLAYER_JOIN_ID: &str = "multi-player-join-server";
const MULTIPLAYER_REFRESH_ID: &str = "multi-player-refresh-server-list";
const MULTIPLAYER_ADD_ID: &str = "multi-player-add-server";
const MULTIPLAYER_EDIT_ID: &str = "multi-player-edit-server";
const MULTIPLAYER_DELETE_ID: &str = "multi-player-delete-server";

const MULTIPLAYER_FORM_DIALOG_ID: &str = "multi-player-form-dialog";
const MULTIPLAYER_FORM_TITLE_ID: &str = "multi-player-form-title";
const MULTIPLAYER_FORM_NAME_INPUT_ID: &str = "multi-player-form-name-input";
const MULTIPLAYER_FORM_ADDRESS_INPUT_ID: &str = "multi-player-form-address-input";
const MULTIPLAYER_FORM_ADD_ID: &str = "multi-player-form-add";
const MULTIPLAYER_FORM_EDIT_ID: &str = "multi-player-form-edit";
const MULTIPLAYER_FORM_ABORT_ID: &str = "multi-player-form-abort";

const MULTIPLAYER_DELETE_DIALOG_ID: &str = "multi-player-delete-dialog";
const MULTIPLAYER_DELETE_TEXT_ID: &str = "multi-player-delete-text";
const MULTIPLAYER_DELETE_CONFIRM_ID: &str = "multi-player-delete-confirm";
const MULTIPLAYER_DELETE_ABORT_ID: &str = "multi-player-delete-abort";

const MULTIPLAYER_CONNECT_DIALOG_ID: &str = "multi-player-connect-dialog";

const MULTIPLAYER_SERVER_FILE: &str = "config/multiplayer_servers.toml";
const DEFAULT_SERVER_PORT: u16 = 14191;
const PROBE_INTERVAL_SECS: f32 = 3.0;
const SERVER_STALE_AFTER_SECS: f64 = 10.0;

pub struct MultiplayerUiPlugin;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct SavedServerEntry {
    server_name: String,
    host: String,
    port: u16,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct SavedServerConfig {
    #[serde(default)]
    servers: Vec<SavedServerEntry>,
}

#[derive(Clone, Debug)]
struct ProbedServerStatus {
    session_url: String,
    observed_host: Option<String>,
    matched_saved_key: Option<String>,
    server_name: String,
    motd: String,
    ping_ms: Option<u32>,
    last_seen_at: f64,
}

#[derive(Clone, Debug)]
struct DisplayServerEntry {
    key: String,
    saved_index: Option<usize>,
    server_name: String,
    host: String,
    port: u16,
    motd: String,
    ping_ms: Option<u32>,
    online: bool,
    session_url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ServerFormMode {
    Add,
    Edit,
}

#[derive(Clone, Debug)]
struct ServerFormDialogState {
    mode: ServerFormMode,
    editing_saved_index: Option<usize>,
}

#[derive(Resource, Default)]
struct MultiplayerUiState {
    saved_servers: Vec<SavedServerEntry>,
    probed_servers: HashMap<String, ProbedServerStatus>,
    dismissed_server_keys: HashSet<String>,
    display_servers: Vec<DisplayServerEntry>,
    rendered_keys: Vec<String>,
    pending_html_refresh: bool,
    selected_key: Option<String>,
    form_dialog: Option<ServerFormDialogState>,
    pending_delete_key: Option<String>,
    joining_key: Option<String>,
}

impl MultiplayerUiState {
    fn selected_server(&self) -> Option<&DisplayServerEntry> {
        let key = self.selected_key.as_ref()?;
        self.display_servers.iter().find(|entry| &entry.key == key)
    }
}

#[derive(Default)]
struct ServerProbeRuntime {
    client: Option<LanDiscoveryClient>,
    probe_timer: Timer,
    last_broadcast_sent_at: Option<f64>,
    pending_direct_probes: HashMap<String, f64>,
}

impl ServerProbeRuntime {
    fn configure(&mut self) {
        let settings = NetworkSettings::load_or_create("config/network.toml");
        self.client = if settings.client.lan_discovery {
            LanDiscoveryClient::bind(settings.client.lan_discovery_port).ok()
        } else {
            None
        };
        self.probe_timer = Timer::from_seconds(PROBE_INTERVAL_SECS, TimerMode::Repeating);
        self.last_broadcast_sent_at = None;
        self.pending_direct_probes.clear();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MultiplayerAction {
    SelectServer(usize),
    JoinServer,
    RefreshServers,
    OpenAddServer,
    OpenEditServer,
    OpenDeleteServer,
    ConfirmDelete,
    AbortDelete,
    SubmitAdd,
    SubmitEdit,
    AbortForm,
}

impl Plugin for MultiplayerUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MultiplayerUiState>()
            .insert_non_send_resource(ServerProbeRuntime::default())
            .add_systems(Startup, register_multiplayer_ui)
            .add_systems(
                OnEnter(AppState::Screen(BeforeUiState::MultiPlayer)),
                enter_multiplayer_screen,
            )
            .add_systems(
                Update,
                (
                    set_multiplayer_interaction,
                    handle_multiplayer_back_navigation,
                    poll_multiplayer_servers,
                    handle_multiplayer_actions,
                    sync_multiplayer_server_list_scrollbar,
                    sync_multiplayer_form_dialog,
                    sync_multiplayer_delete_dialog,
                    sync_multiplayer_connect_dialog,
                    sync_multiplayer_card_text,
                    sync_multiplayer_card_style,
                    enforce_multiplayer_ui_active,
                )
                    .chain()
                    .run_if(in_state(AppState::Screen(BeforeUiState::MultiPlayer))),
            )
            .add_systems(
                Update,
                ensure_multiplayer_ui_hidden_when_not_active
                    .run_if(not(in_state(AppState::Screen(BeforeUiState::MultiPlayer)))),
            )
            .add_systems(
                PostUpdate,
                apply_multiplayer_html_refresh
                    .run_if(in_state(AppState::Screen(BeforeUiState::MultiPlayer))),
            )
            .add_systems(
                OnExit(AppState::Screen(BeforeUiState::MultiPlayer)),
                (
                    hide_multiplayer_ui,
                    clear_multiplayer_interaction,
                    reset_multiplayer_ui_state,
                ),
            );
    }
}

fn register_multiplayer_ui(
    mut ui_state: ResMut<MultiplayerUiState>,
    mut probe_runtime: NonSendMut<ServerProbeRuntime>,
    mut registry: ResMut<UiRegistry>,
    asset_server: Res<AssetServer>,
    mut html_assets: ResMut<Assets<HtmlAsset>>,
) {
    ui_state.saved_servers = load_saved_servers();
    rebuild_display_servers(&mut ui_state, 0.0);
    refresh_multiplayer_content(
        &mut ui_state,
        &mut registry,
        &asset_server,
        &mut html_assets,
    );
    probe_runtime.configure();
}

fn enter_multiplayer_screen(
    time: Res<Time>,
    mut ui_state: ResMut<MultiplayerUiState>,
    mut probe_runtime: NonSendMut<ServerProbeRuntime>,
    mut registry: ResMut<UiRegistry>,
    asset_server: Res<AssetServer>,
    mut html_assets: ResMut<Assets<HtmlAsset>>,
    mut form_inputs: Query<(&CssID, &mut InputField, &mut InputValue)>,
) {
    ui_state.saved_servers = load_saved_servers();
    ui_state.pending_html_refresh = false;
    ui_state.form_dialog = None;
    ui_state.pending_delete_key = None;
    ui_state.joining_key = None;
    if ui_state.selected_key.as_ref().is_some_and(|key| {
        !ui_state
            .display_servers
            .iter()
            .any(|server| &server.key == key)
    }) {
        ui_state.selected_key = None;
    }

    probe_runtime.configure();
    rebuild_display_servers(&mut ui_state, time.elapsed_secs_f64());
    refresh_multiplayer_content(
        &mut ui_state,
        &mut registry,
        &asset_server,
        &mut html_assets,
    );
    clear_server_form_inputs(&mut form_inputs);
    activate_multiplayer_ui(&mut registry);
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
    mut ui_state: ResMut<MultiplayerUiState>,
    mut widgets: Query<(&CssID, &mut UIWidgetState)>,
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
                    ui_state.pending_html_refresh = true;
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
                        if let Some(index) = form_state.editing_saved_index {
                            if let Some(entry) = ui_state.saved_servers.get_mut(index) {
                                entry.server_name = server_name.clone();
                                entry.host = host.clone();
                                entry.port = port;
                                ui_state.selected_key = Some(entry.key());
                            }
                        }
                    }
                }

                ui_state
                    .dismissed_server_keys
                    .remove(&server_key(host.as_str(), port));
                ui_state.form_dialog = None;
                save_saved_servers(&ui_state.saved_servers);
                if rebuild_display_servers(&mut ui_state, now) {
                    ui_state.pending_html_refresh = true;
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
    mut ui_state: ResMut<MultiplayerUiState>,
    mut probe_runtime: NonSendMut<ServerProbeRuntime>,
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
        ui_state.pending_html_refresh = true;
    } else {
        rebuild_display_servers(&mut ui_state, now);
    }
}

fn apply_multiplayer_html_refresh(
    mut ui_state: ResMut<MultiplayerUiState>,
    mut registry: ResMut<UiRegistry>,
    asset_server: Res<AssetServer>,
    mut html_assets: ResMut<Assets<HtmlAsset>>,
) {
    if !ui_state.pending_html_refresh {
        return;
    }

    ui_state.pending_html_refresh = false;
    refresh_multiplayer_content(
        &mut ui_state,
        &mut registry,
        &asset_server,
        &mut html_assets,
    );
}

fn sync_multiplayer_server_list_scrollbar(
    list_divs: Query<(&CssID, &UIGenID), With<Div>>,
    scrollbars: Query<&Scrollbar>,
    mut scroll_positions: Query<&mut ScrollPosition>,
) {
    let Some(list_ui_id) = list_divs
        .iter()
        .find(|(css_id, _)| css_id.0 == MULTIPLAYER_LIST_ID)
        .map(|(_, ui_id)| ui_id.get())
    else {
        return;
    };

    for scrollbar in scrollbars.iter() {
        if scrollbar.entry != list_ui_id || !scrollbar.vertical {
            continue;
        }

        let Some(target) = scrollbar.entity else {
            continue;
        };

        if let Ok(mut scroll_position) = scroll_positions.get_mut(target) {
            scroll_position.y = scrollbar.value.clamp(scrollbar.min, scrollbar.max);
        }
    }
}

fn sync_multiplayer_form_dialog(
    ui_state: Res<MultiplayerUiState>,
    mut paragraphs: Query<(&CssID, &mut Paragraph)>,
    mut visibilities: Query<(&CssID, &mut Visibility)>,
) {
    let Some(dialog_state) = ui_state.form_dialog.as_ref() else {
        for (css_id, mut visibility) in &mut visibilities {
            if css_id.0 == MULTIPLAYER_FORM_DIALOG_ID
                || css_id.0 == MULTIPLAYER_FORM_ADD_ID
                || css_id.0 == MULTIPLAYER_FORM_EDIT_ID
            {
                *visibility = if css_id.0 == MULTIPLAYER_FORM_DIALOG_ID {
                    Visibility::Hidden
                } else {
                    Visibility::Hidden
                };
            }
        }
        return;
    };

    let title = match dialog_state.mode {
        ServerFormMode::Add => "Add Server",
        ServerFormMode::Edit => "Edit Server",
    };

    for (css_id, mut paragraph) in &mut paragraphs {
        if css_id.0 == MULTIPLAYER_FORM_TITLE_ID {
            paragraph.text = title.to_string();
        }
    }

    for (css_id, mut visibility) in &mut visibilities {
        if css_id.0 == MULTIPLAYER_FORM_DIALOG_ID {
            *visibility = Visibility::Inherited;
            continue;
        }

        if css_id.0 == MULTIPLAYER_FORM_ADD_ID {
            *visibility = if dialog_state.mode == ServerFormMode::Add {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            continue;
        }

        if css_id.0 == MULTIPLAYER_FORM_EDIT_ID {
            *visibility = if dialog_state.mode == ServerFormMode::Edit {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
}

fn sync_multiplayer_delete_dialog(
    ui_state: Res<MultiplayerUiState>,
    mut paragraphs: Query<(&CssID, &mut Paragraph)>,
    mut visibilities: Query<(&CssID, &mut Visibility)>,
) {
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

    for (css_id, mut paragraph) in &mut paragraphs {
        if css_id.0 != MULTIPLAYER_DELETE_TEXT_ID {
            continue;
        }
        paragraph.text = format!("Ar you sure to delete `{name}`?");
    }

    for (css_id, mut visibility) in &mut visibilities {
        if css_id.0 != MULTIPLAYER_DELETE_DIALOG_ID {
            continue;
        }
        *visibility = if ui_state.pending_delete_key.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

fn sync_multiplayer_connect_dialog(
    mut ui_state: ResMut<MultiplayerUiState>,
    connection_state: Res<MultiplayerConnectionState>,
    mut visibilities: Query<(&CssID, &mut Visibility)>,
) {
    if ui_state.joining_key.is_some() && connection_state.phase == MultiplayerConnectionPhase::Idle
    {
        ui_state.joining_key = None;
    }

    for (css_id, mut visibility) in &mut visibilities {
        if css_id.0 != MULTIPLAYER_CONNECT_DIALOG_ID {
            continue;
        }

        *visibility = if ui_state.joining_key.is_some()
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
    mut paragraphs: Query<(&CssID, &mut Paragraph)>,
) {
    for (css_id, mut paragraph) in &mut paragraphs {
        if let Some(index) = parse_card_index(css_id.0.as_str(), MULTIPLAYER_NAME_PREFIX) {
            if let Some(server) = ui_state.display_servers.get(index) {
                paragraph.text = format!("Server Name: {}", server.server_name);
            }
            continue;
        }

        if let Some(index) = parse_card_index(css_id.0.as_str(), MULTIPLAYER_IP_PREFIX) {
            if let Some(server) = ui_state.display_servers.get(index) {
                paragraph.text = format!("Server IP: {}", server.host);
            }
            continue;
        }

        if let Some(index) = parse_card_index(css_id.0.as_str(), MULTIPLAYER_PORT_PREFIX) {
            if let Some(server) = ui_state.display_servers.get(index) {
                paragraph.text = format!("Server Port: {}", server.port);
            }
            continue;
        }

        if let Some(index) = parse_card_index(css_id.0.as_str(), MULTIPLAYER_MOTD_PREFIX) {
            if let Some(server) = ui_state.display_servers.get(index) {
                paragraph.text = if server.online {
                    format!("MOTD: {}", server.motd)
                } else {
                    "MOTD: Can't connect to Server!".to_string()
                };
            }
            continue;
        }

        if let Some(index) = parse_card_index(css_id.0.as_str(), MULTIPLAYER_PING_PREFIX) {
            if let Some(server) = ui_state.display_servers.get(index) {
                paragraph.text = match server.ping_ms {
                    Some(ping) if server.online => format!("Ping: {ping} ms"),
                    _ => "Ping: -".to_string(),
                };
            }
        }
    }
}

fn sync_multiplayer_card_style(
    ui_state: Res<MultiplayerUiState>,
    mut borders: Query<(&CssID, &mut BorderColor)>,
    mut backgrounds: Query<(&CssID, &mut BackgroundColor)>,
) {
    for (css_id, mut border) in &mut borders {
        let Some(index) = parse_card_index(css_id.0.as_str(), MULTIPLAYER_CARD_PREFIX) else {
            continue;
        };
        let Some(server) = ui_state.display_servers.get(index) else {
            continue;
        };

        let border_color = if server.online {
            Color::srgb_u8(68, 194, 120)
        } else {
            Color::srgb_u8(199, 73, 73)
        };

        border.top = border_color;
        border.right = border_color;
        border.bottom = border_color;
        border.left = border_color;
    }

    for (css_id, mut background) in &mut backgrounds {
        let Some(index) = parse_card_index(css_id.0.as_str(), MULTIPLAYER_CARD_PREFIX) else {
            continue;
        };
        let Some(server) = ui_state.display_servers.get(index) else {
            continue;
        };

        background.0 = if ui_state.selected_key.as_ref() == Some(&server.key) {
            Color::srgba(0.12, 0.33, 0.44, 0.98)
        } else {
            Color::srgba(0.06, 0.17, 0.26, 0.95)
        };
    }
}

fn enforce_multiplayer_ui_active(mut registry: ResMut<UiRegistry>, asset_server: Res<AssetServer>) {
    if registry.get(MULTIPLAYER_UI_KEY).is_none() {
        let handle: Handle<HtmlAsset> = asset_server.load(MULTIPLAYER_UI_PATH);
        registry.add(
            MULTIPLAYER_UI_KEY.to_string(),
            HtmlSource::from_handle(handle),
        );
    }

    if is_multiplayer_ui_active(&registry) {
        return;
    }

    activate_multiplayer_ui(&mut registry);
}

fn hide_multiplayer_ui(
    mut registry: ResMut<UiRegistry>,
    mut visibility_sets: ParamSet<(
        Query<(&CssID, &mut Visibility)>,
        Query<(&Body, &mut Visibility)>,
        Query<(Entity, &Body)>,
    )>,
    mut commands: Commands,
) {
    remove_multiplayer_ui_from_registry(&mut registry);
    set_multiplayer_root_visibility(&mut visibility_sets.p0(), Visibility::Hidden);
    set_multiplayer_body_visibility(&mut visibility_sets.p1(), Visibility::Hidden);
    despawn_multiplayer_body_roots(&visibility_sets.p2(), &mut commands);
}

fn ensure_multiplayer_ui_hidden_when_not_active(
    mut visibility_sets: ParamSet<(
        Query<(&CssID, &mut Visibility)>,
        Query<(&Body, &mut Visibility)>,
    )>,
) {
    set_multiplayer_root_visibility(&mut visibility_sets.p0(), Visibility::Hidden);
    set_multiplayer_body_visibility(&mut visibility_sets.p1(), Visibility::Hidden);
}

fn clear_multiplayer_interaction(mut ui_interaction: ResMut<UiInteractionState>) {
    ui_interaction.menu_open = false;
}

fn reset_multiplayer_ui_state(mut ui_state: ResMut<MultiplayerUiState>) {
    ui_state.form_dialog = None;
    ui_state.pending_delete_key = None;
    ui_state.joining_key = None;
}

fn set_multiplayer_root_visibility(
    visibilities: &mut Query<(&CssID, &mut Visibility)>,
    visibility: Visibility,
) {
    for (css_id, mut current) in visibilities.iter_mut() {
        if css_id.0 != MULTIPLAYER_ROOT_ID {
            continue;
        }
        *current = visibility;
    }
}

fn set_multiplayer_body_visibility(
    bodies: &mut Query<(&Body, &mut Visibility)>,
    visibility: Visibility,
) {
    for (body, mut current) in bodies.iter_mut() {
        let Some(key) = body.html_key.as_deref() else {
            continue;
        };
        if key != MULTIPLAYER_UI_KEY {
            continue;
        }
        *current = visibility;
    }
}

fn despawn_multiplayer_body_roots(body_entities: &Query<(Entity, &Body)>, commands: &mut Commands) {
    for (entity, body) in body_entities.iter() {
        let Some(key) = body.html_key.as_deref() else {
            continue;
        };
        if key == MULTIPLAYER_UI_KEY {
            safe_despawn_entity(commands, entity);
        }
    }
}

fn collect_multiplayer_actions(
    widgets: &mut Query<(&CssID, &mut UIWidgetState)>,
) -> Vec<MultiplayerAction> {
    let mut actions = Vec::new();

    for (css_id, mut state) in widgets.iter_mut() {
        if let Some(index) = parse_card_index(css_id.0.as_str(), MULTIPLAYER_CARD_PREFIX) {
            if state.focused {
                state.focused = false;
                actions.push(MultiplayerAction::SelectServer(index));
            }
            if state.checked {
                state.checked = false;
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

fn refresh_multiplayer_content(
    ui_state: &mut MultiplayerUiState,
    registry: &mut UiRegistry,
    asset_server: &AssetServer,
    html_assets: &mut Assets<HtmlAsset>,
) {
    let html = generate_multiplayer_html(&ui_state.display_servers);
    let handle: Handle<HtmlAsset> = asset_server.load(MULTIPLAYER_UI_PATH);
    let stylesheet_handle = asset_server.load("ui/css/multiplayer.css");

    if let Some(asset) = html_assets.get_mut(&handle) {
        asset.html = html;
        if asset.stylesheets.is_empty() {
            asset.stylesheets.push(stylesheet_handle);
        }
    } else {
        let _ = html_assets.insert(
            handle.id(),
            HtmlAsset {
                html,
                stylesheets: vec![stylesheet_handle],
            },
        );
    }

    registry.add(
        MULTIPLAYER_UI_KEY.to_string(),
        HtmlSource::from_handle(handle),
    );
    ui_state.rendered_keys = ui_state
        .display_servers
        .iter()
        .map(|server| server.key.clone())
        .collect();
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
            motd: "Can't connect to Server!".to_string(),
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
                "Can't connect to Server!".to_string()
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
                    "Can't connect to Server!".to_string()
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

fn is_multiplayer_ui_active(registry: &UiRegistry) -> bool {
    registry
        .current
        .as_ref()
        .is_some_and(|current| current.iter().any(|name| name == MULTIPLAYER_UI_KEY))
}

fn activate_multiplayer_ui(registry: &mut UiRegistry) {
    if registry.get(MULTIPLAYER_UI_KEY).is_none() {
        return;
    }

    if let Some(current) = registry.current.as_mut() {
        current.retain(|name| name != MULTIPLAYER_UI_KEY);
        current.push(MULTIPLAYER_UI_KEY.to_string());
        registry.ui_update = true;
        return;
    }

    registry.current = Some(vec![MULTIPLAYER_UI_KEY.to_string()]);
    registry.ui_update = true;
}

fn remove_multiplayer_ui_from_registry(registry: &mut UiRegistry) {
    let mut clear_current = false;

    if let Some(current) = registry.current.as_mut() {
        current.retain(|name| name != MULTIPLAYER_UI_KEY);
        clear_current = current.is_empty();
        registry.ui_update = true;
    }

    if clear_current {
        registry.current = None;
    }
}

fn generate_multiplayer_html(servers: &[DisplayServerEntry]) -> String {
    let cards = if servers.is_empty() {
        "        <div id=\"multi-player-empty\">No servers found.</div>\n".to_string()
    } else {
        servers
            .iter()
            .enumerate()
            .map(|(index, server)| {
                let server_name = escape_html(server.server_name.as_str());
                let host = escape_html(server.host.as_str());
                let motd = if server.online {
                    escape_html(server.motd.as_str())
                } else {
                    "Can&#39;t connect to Server!".to_string()
                };
                let ping = server
                    .ping_ms
                    .map(|value| format!("{value} ms"))
                    .unwrap_or_else(|| "-".to_string());

                format!(
                    "        <div id=\"multi-player-server-card-{index}\" class=\"multi-player-server-card\">
          <div class=\"multi-player-server-main\">
            <p id=\"multi-player-server-name-{index}\" class=\"multi-player-server-name\">Server Name: {server_name}</p>
            <p id=\"multi-player-server-motd-{index}\" class=\"multi-player-server-motd\">MOTD: {motd}</p>
          </div>
          <div class=\"multi-player-server-side\">
            <p id=\"multi-player-server-ip-{index}\" class=\"multi-player-server-meta\">Server IP: {host}</p>
            <p id=\"multi-player-server-port-{index}\" class=\"multi-player-server-meta\">Server Port: {port}</p>
          </div>
          <div class=\"multi-player-server-ping-box\">
            <p id=\"multi-player-server-ping-{index}\" class=\"multi-player-server-ping\">Ping: {ping}</p>
          </div>
        </div>\n",
                    port = server.port
                )
            })
            .collect::<String>()
    };

    format!(
        "<html lang=\"en\">
  <head>
    <meta charset=\"UTF-8\" />
    <meta name=\"multi-player\" />
    <title>Multi Player</title>
    <link rel=\"stylesheet\" href=\"../css/multiplayer.css\" />
  </head>
  <body id=\"multi-player-root\">
    <div id=\"multi-player-panel\">
      <h2 id=\"multi-player-title\">Multi Player</h2>
      <div id=\"multi-player-server-list\">
{cards}      </div>
      <div id=\"multi-player-actions\">
        <button id=\"multi-player-join-server\" class=\"multi-player-action-button\">Join Server</button>
        <button id=\"multi-player-refresh-server-list\" class=\"multi-player-action-button\">Refresh</button>
        <button id=\"multi-player-add-server\" class=\"multi-player-action-button\">Add Server</button>
        <button id=\"multi-player-edit-server\" class=\"multi-player-action-button\">Edit Server</button>
        <button id=\"multi-player-delete-server\" class=\"multi-player-action-button danger\">Delete Server</button>
      </div>
    </div>
    <div id=\"multi-player-form-dialog\">
      <div id=\"multi-player-form-box\">
        <p id=\"multi-player-form-title\">Add Server</p>
        <div class=\"multi-player-form-field\">
          <label for=\"multi-player-form-name-input\" class=\"multi-player-form-label\">Server Name</label>
          <input id=\"multi-player-form-name-input\" name=\"server-name\" type=\"text\" maxlength=\"48\" placeholder=\"Server Name\" />
        </div>
        <div class=\"multi-player-form-field\">
          <label for=\"multi-player-form-address-input\" class=\"multi-player-form-label\">Server IP</label>
          <input id=\"multi-player-form-address-input\" name=\"server-address\" type=\"text\" maxlength=\"64\" placeholder=\"192.111.222.23 or 192.111.222.23:14191\" />
        </div>
        <div id=\"multi-player-form-actions\">
          <button id=\"multi-player-form-add\" class=\"multi-player-action-button\">Add</button>
          <button id=\"multi-player-form-edit\" class=\"multi-player-action-button\">Edit</button>
          <button id=\"multi-player-form-abort\" class=\"multi-player-action-button\">Abort</button>
        </div>
      </div>
    </div>
    <div id=\"multi-player-delete-dialog\">
      <div id=\"multi-player-delete-box\">
        <p id=\"multi-player-delete-text\">Ar you sure to delete ``?</p>
        <div id=\"multi-player-delete-actions\">
          <button id=\"multi-player-delete-confirm\" class=\"multi-player-action-button danger\">Confirm</button>
          <button id=\"multi-player-delete-abort\" class=\"multi-player-action-button\">Abort</button>
        </div>
      </div>
    </div>
    <div id=\"multi-player-connect-dialog\">
      <div id=\"multi-player-connect-box\">
        <p id=\"multi-player-connect-text\">Connect to Server</p>
      </div>
    </div>
  </body>
</html>
"
    )
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

impl SavedServerEntry {
    fn key(&self) -> String {
        server_key(self.host.as_str(), self.port)
    }

    fn session_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }
}
