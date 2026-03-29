use crate::core::config::{GlobalConfig, WorldGenConfig};
use crate::core::states::states::{AppState, BeforeUiState, LoadingStates};
use crate::core::ui::UiInteractionState;
use crate::core::world::chunk::ChunkMap;
use crate::core::world::fluid::{FluidMap, WaterMeshIndex};
use crate::core::world::save::{RegionCache, WorldSave, default_saves_root};
use crate::utils::key_utils::convert;
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
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SINGLE_PLAYER_UI_KEY: &str = "single-player";
const SINGLE_PLAYER_UI_PATH: &str = "ui/html/single_player.html";
const CREATE_WORLD_UI_KEY: &str = "create-world";
const CREATE_WORLD_UI_PATH: &str = "ui/html/create_world.html";
const SINGLE_PLAYER_ROOT_ID: &str = "single-player-root";
const CREATE_WORLD_ROOT_ID: &str = "create-world-root";
const SINGLE_PLAYER_WORLD_LIST_ID: &str = "single-player-world-list";

const SINGLE_PLAYER_WORLD_CARD_PREFIX: &str = "single-player-world-card-";
const SINGLE_PLAYER_CREATE_WORLD_ID: &str = "single-player-create-world";
const SINGLE_PLAYER_PLAY_WORLD_ID: &str = "single-player-play-world";
const SINGLE_PLAYER_DELETE_WORLD_ID: &str = "single-player-delete-world";
const SINGLE_PLAYER_DELETE_DIALOG_ID: &str = "single-player-delete-dialog";
const SINGLE_PLAYER_DELETE_TEXT_ID: &str = "single-player-delete-text";
const SINGLE_PLAYER_DELETE_CONFIRM_ID: &str = "single-player-delete-confirm";
const SINGLE_PLAYER_DELETE_CANCEL_ID: &str = "single-player-delete-cancel";

const CREATE_WORLD_NAME_INPUT_ID: &str = "create-world-name-input";
const CREATE_WORLD_SEED_INPUT_ID: &str = "create-world-seed-input";
const CREATE_WORLD_CREATE_ID: &str = "create-world-create";
const CREATE_WORLD_ABORT_ID: &str = "create-world-abort";

const WORLD_META_FILE: &str = "world.meta.json";
const DOUBLE_CLICK_WINDOW_SECS: f64 = 0.1;

pub struct SinglePlayerUiPlugin;

#[derive(Clone, Debug)]
struct SavedWorldEntry {
    folder_name: String,
    seed: i32,
    path: PathBuf,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SinglePlayerPage {
    #[default]
    List,
    CreateWorld,
}

#[derive(Resource, Default)]
struct SinglePlayerUiState {
    page: SinglePlayerPage,
    worlds: Vec<SavedWorldEntry>,
    selected_index: Option<usize>,
    pending_delete_index: Option<usize>,
    last_card_click: Option<(usize, f64)>,
    closing_for_world_load: bool,
}

#[derive(Serialize, Deserialize)]
struct WorldMeta {
    seed: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SinglePlayerAction {
    SelectWorld(usize),
    OpenCreateWorld,
    PlayWorld,
    DeleteWorld,
    ConfirmDelete,
    CancelDelete,
    CreateWorldSubmit,
    CreateWorldAbort,
}

impl Plugin for SinglePlayerUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SinglePlayerUiState>()
            .add_systems(Startup, register_single_player_uis)
            .add_systems(
                OnEnter(AppState::Screen(BeforeUiState::SinglePlayer)),
                enter_single_player_screen,
            )
            .add_systems(
                Update,
                (
                    set_single_player_interaction,
                    show_single_player_menu_roots,
                    handle_single_player_back_navigation,
                    handle_single_player_actions,
                    enforce_active_single_player_page_ui,
                    sync_single_player_world_list_scrollbar,
                    sync_delete_dialog,
                    sync_world_card_style,
                )
                    .chain()
                    .run_if(in_state(AppState::Screen(BeforeUiState::SinglePlayer))),
            )
            .add_systems(
                Update,
                ensure_single_player_ui_hidden_when_not_active.run_if(not(in_state(
                    AppState::Screen(BeforeUiState::SinglePlayer),
                ))),
            )
            .add_systems(
                OnExit(AppState::Screen(BeforeUiState::SinglePlayer)),
                (
                    hide_single_player_ui,
                    clear_single_player_interaction,
                    reset_single_player_ui_state,
                ),
            );
    }
}

fn register_single_player_uis(
    mut ui_state: ResMut<SinglePlayerUiState>,
    world_gen_config: Option<Res<WorldGenConfig>>,
    mut registry: ResMut<UiRegistry>,
    asset_server: Res<AssetServer>,
    mut html_assets: ResMut<Assets<HtmlAsset>>,
) {
    let default_seed = world_gen_config.map(|cfg| cfg.seed).unwrap_or(1337);
    refresh_single_player_content(
        &mut ui_state,
        default_seed,
        &mut registry,
        &asset_server,
        &mut html_assets,
    );
    register_create_world_ui(&mut registry, &asset_server);
}

fn enter_single_player_screen(
    mut ui_state: ResMut<SinglePlayerUiState>,
    world_gen_config: Res<WorldGenConfig>,
    mut registry: ResMut<UiRegistry>,
    asset_server: Res<AssetServer>,
    mut html_assets: ResMut<Assets<HtmlAsset>>,
    mut create_inputs: Query<(&CssID, &mut InputField, &mut InputValue)>,
) {
    ui_state.page = SinglePlayerPage::List;
    ui_state.pending_delete_index = None;
    ui_state.last_card_click = None;
    ui_state.closing_for_world_load = false;

    refresh_single_player_content(
        &mut ui_state,
        world_gen_config.seed,
        &mut registry,
        &asset_server,
        &mut html_assets,
    );
    let names = ui_state
        .worlds
        .iter()
        .map(|world| world.folder_name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    info!(
        "SinglePlayer worlds from {:?}: count={}, [{}]",
        saves_root(),
        ui_state.worlds.len(),
        names
    );
    register_create_world_ui(&mut registry, &asset_server);
    clear_create_world_inputs(&mut create_inputs);
    registry.use_ui(SINGLE_PLAYER_UI_KEY);
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

fn show_single_player_menu_roots(
    ui_state: Res<SinglePlayerUiState>,
    mut visibility_sets: ParamSet<(
        Query<(&CssID, &mut Visibility)>,
        Query<(&Body, &mut Visibility)>,
    )>,
) {
    if ui_state.closing_for_world_load {
        set_single_player_root_visibility(&mut visibility_sets.p0(), Visibility::Hidden);
        set_single_player_body_visibility(&mut visibility_sets.p1(), Visibility::Hidden);
        return;
    }

    set_single_player_root_visibility(&mut visibility_sets.p0(), Visibility::Inherited);
    set_single_player_body_visibility(&mut visibility_sets.p1(), Visibility::Inherited);
}

fn hide_single_player_menu_roots(
    mut visibility_sets: ParamSet<(
        Query<(&CssID, &mut Visibility)>,
        Query<(&Body, &mut Visibility)>,
    )>,
) {
    set_single_player_root_visibility(&mut visibility_sets.p0(), Visibility::Hidden);
    set_single_player_body_visibility(&mut visibility_sets.p1(), Visibility::Hidden);
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
    mut ui_state: ResMut<SinglePlayerUiState>,
    mut ui_interaction: ResMut<UiInteractionState>,
    mut world_gen_config: ResMut<WorldGenConfig>,
    mut widgets: Query<(&CssID, &UIGenID, &mut UIWidgetState)>,
    mut create_inputs: Query<(&CssID, &mut InputField, &mut InputValue)>,
    mut visibility_sets: ParamSet<(
        Query<(&CssID, &mut Visibility)>,
        Query<(&Body, &mut Visibility)>,
        Query<(Entity, &Body)>,
    )>,
    mut registry: ResMut<UiRegistry>,
    asset_server: Res<AssetServer>,
    mut html_assets: ResMut<Assets<HtmlAsset>>,
    mut commands: Commands,
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
                    close_single_player_ui_immediately(
                        &mut registry,
                        &mut visibility_sets,
                        &mut commands,
                        &mut ui_interaction,
                    );
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
                    close_single_player_ui_immediately(
                        &mut registry,
                        &mut visibility_sets,
                        &mut commands,
                        &mut ui_interaction,
                    );
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
                refresh_single_player_content(
                    &mut ui_state,
                    world_gen_config.seed,
                    &mut registry,
                    &asset_server,
                    &mut html_assets,
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
                close_single_player_ui_immediately(
                    &mut registry,
                    &mut visibility_sets,
                    &mut commands,
                    &mut ui_interaction,
                );
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

fn enforce_active_single_player_page_ui(
    ui_state: Res<SinglePlayerUiState>,
    mut registry: ResMut<UiRegistry>,
    asset_server: Res<AssetServer>,
) {
    if ui_state.closing_for_world_load {
        return;
    }

    match ui_state.page {
        SinglePlayerPage::List => {
            if registry.get(SINGLE_PLAYER_UI_KEY).is_none() {
                let handle: Handle<HtmlAsset> = asset_server.load(SINGLE_PLAYER_UI_PATH);
                registry.add(
                    SINGLE_PLAYER_UI_KEY.to_string(),
                    HtmlSource::from_handle(handle),
                );
            }
            if !is_ui_active(&registry, SINGLE_PLAYER_UI_KEY) {
                registry.use_ui(SINGLE_PLAYER_UI_KEY);
            }
        }
        SinglePlayerPage::CreateWorld => {
            register_create_world_ui(&mut registry, &asset_server);
            if !is_ui_active(&registry, CREATE_WORLD_UI_KEY) {
                registry.use_ui(CREATE_WORLD_UI_KEY);
            }
        }
    }
}

fn sync_single_player_world_list_scrollbar(
    ui_state: Res<SinglePlayerUiState>,
    list_divs: Query<(&CssID, &UIGenID), With<Div>>,
    scrollbars: Query<&Scrollbar>,
    mut scroll_positions: Query<&mut ScrollPosition>,
) {
    if ui_state.page != SinglePlayerPage::List || ui_state.closing_for_world_load {
        return;
    }

    let Some(list_ui_id) = list_divs
        .iter()
        .find(|(css_id, _)| css_id.0 == SINGLE_PLAYER_WORLD_LIST_ID)
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

fn is_ui_active(registry: &UiRegistry, key: &str) -> bool {
    registry
        .current
        .as_ref()
        .is_some_and(|current| current.iter().any(|name| name == key))
}

fn sync_delete_dialog(
    ui_state: Res<SinglePlayerUiState>,
    mut paragraphs: Query<(&CssID, &mut Paragraph)>,
    mut visibilities: Query<(&CssID, &mut Visibility)>,
) {
    let on_list_page = ui_state.page == SinglePlayerPage::List;
    let name = ui_state
        .pending_delete_index
        .and_then(|index| ui_state.worlds.get(index))
        .map(|world| world.folder_name.as_str())
        .unwrap_or_default();

    let delete_text = format!("Ar you sure to delete `{name}`?");
    let show_dialog = on_list_page && ui_state.pending_delete_index.is_some();

    for (css_id, mut paragraph) in &mut paragraphs {
        if css_id.0 != SINGLE_PLAYER_DELETE_TEXT_ID {
            continue;
        }
        paragraph.text = delete_text.clone();
    }

    for (css_id, mut visibility) in &mut visibilities {
        if css_id.0 != SINGLE_PLAYER_DELETE_DIALOG_ID {
            continue;
        }
        *visibility = if show_dialog {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

fn sync_world_card_style(
    ui_state: Res<SinglePlayerUiState>,
    mut borders: Query<(&CssID, &mut BorderColor)>,
) {
    if ui_state.page != SinglePlayerPage::List {
        return;
    }

    let default_color = Color::srgb_u8(74, 126, 156);
    let selected_color = Color::srgb_u8(148, 227, 255);

    for (css_id, mut border) in &mut borders {
        let Some(index) = parse_world_card_index(css_id.0.as_str()) else {
            continue;
        };

        let border_color = if ui_state.selected_index == Some(index) {
            selected_color
        } else {
            default_color
        };

        border.top = border_color;
        border.right = border_color;
        border.bottom = border_color;
        border.left = border_color;
    }
}

fn hide_single_player_ui(
    mut registry: ResMut<UiRegistry>,
    mut visibility_sets: ParamSet<(
        Query<(&CssID, &mut Visibility)>,
        Query<(&Body, &mut Visibility)>,
        Query<(Entity, &Body)>,
    )>,
    mut commands: Commands,
) {
    remove_single_player_ui_from_registry(&mut registry);
    set_single_player_root_visibility(&mut visibility_sets.p0(), Visibility::Hidden);
    set_single_player_body_visibility(&mut visibility_sets.p1(), Visibility::Hidden);
    despawn_single_player_body_roots(&visibility_sets.p2(), &mut commands);
}

fn remove_single_player_ui_from_registry(registry: &mut UiRegistry) {
    let mut clear_current = false;

    if let Some(current) = registry.current.as_mut() {
        current.retain(|name| name != SINGLE_PLAYER_UI_KEY && name != CREATE_WORLD_UI_KEY);
        clear_current = current.is_empty();
        registry.ui_update = true;
    }

    if clear_current {
        registry.current = None;
    }
}

fn ensure_single_player_ui_hidden_when_not_active(
    mut visibility_sets: ParamSet<(
        Query<(&CssID, &mut Visibility)>,
        Query<(&Body, &mut Visibility)>,
    )>,
) {
    set_single_player_root_visibility(&mut visibility_sets.p0(), Visibility::Hidden);
    set_single_player_body_visibility(&mut visibility_sets.p1(), Visibility::Hidden);
}

fn clear_single_player_interaction(mut ui_interaction: ResMut<UiInteractionState>) {
    ui_interaction.menu_open = false;
}

fn reset_single_player_ui_state(mut ui_state: ResMut<SinglePlayerUiState>) {
    ui_state.page = SinglePlayerPage::List;
    ui_state.pending_delete_index = None;
    ui_state.last_card_click = None;
    ui_state.closing_for_world_load = false;
}

fn set_single_player_root_visibility(
    visibilities: &mut Query<(&CssID, &mut Visibility)>,
    visibility: Visibility,
) {
    for (css_id, mut current) in visibilities.iter_mut() {
        if css_id.0 != SINGLE_PLAYER_ROOT_ID && css_id.0 != CREATE_WORLD_ROOT_ID {
            continue;
        }
        *current = visibility;
    }
}

fn set_single_player_body_visibility(
    bodies: &mut Query<(&Body, &mut Visibility)>,
    visibility: Visibility,
) {
    for (body, mut current) in bodies.iter_mut() {
        let Some(key) = body.html_key.as_deref() else {
            continue;
        };
        if key != SINGLE_PLAYER_UI_KEY && key != CREATE_WORLD_UI_KEY {
            continue;
        }
        *current = visibility;
    }
}

fn close_single_player_ui_immediately(
    registry: &mut UiRegistry,
    visibility_sets: &mut ParamSet<(
        Query<(&CssID, &mut Visibility)>,
        Query<(&Body, &mut Visibility)>,
        Query<(Entity, &Body)>,
    )>,
    commands: &mut Commands,
    ui_interaction: &mut UiInteractionState,
) {
    remove_single_player_ui_from_registry(registry);
    set_single_player_root_visibility(&mut visibility_sets.p0(), Visibility::Hidden);
    set_single_player_body_visibility(&mut visibility_sets.p1(), Visibility::Hidden);
    despawn_single_player_body_roots(&visibility_sets.p2(), commands);
    ui_interaction.menu_open = false;
}

fn despawn_single_player_body_roots(
    body_entities: &Query<(Entity, &Body)>,
    commands: &mut Commands,
) {
    for (entity, body) in body_entities.iter() {
        let Some(key) = body.html_key.as_deref() else {
            continue;
        };

        if key == SINGLE_PLAYER_UI_KEY || key == CREATE_WORLD_UI_KEY {
            commands.entity(entity).despawn();
        }
    }
}

fn collect_single_player_actions(
    widgets: &mut Query<(&CssID, &UIGenID, &mut UIWidgetState)>,
) -> Vec<SinglePlayerAction> {
    let mut actions = Vec::new();

    for (css_id, _, mut state) in widgets.iter_mut() {
        if let Some(index) = parse_world_card_index(css_id.0.as_str()) {
            if state.focused {
                state.focused = false;
                actions.push(SinglePlayerAction::SelectWorld(index));
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

fn refresh_single_player_content(
    ui_state: &mut SinglePlayerUiState,
    default_seed: i32,
    registry: &mut UiRegistry,
    asset_server: &AssetServer,
    html_assets: &mut Assets<HtmlAsset>,
) {
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

    let html = generate_single_player_html(&ui_state.worlds);
    let handle: Handle<HtmlAsset> = asset_server.load(SINGLE_PLAYER_UI_PATH);
    let stylesheet_handle = asset_server.load("ui/css/single_player.css");
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
        SINGLE_PLAYER_UI_KEY.to_string(),
        HtmlSource::from_handle(handle),
    );
}

fn register_create_world_ui(registry: &mut UiRegistry, asset_server: &AssetServer) {
    if registry.get(CREATE_WORLD_UI_KEY).is_some() {
        return;
    }
    let handle: Handle<HtmlAsset> = asset_server.load(CREATE_WORLD_UI_PATH);
    registry.add(
        CREATE_WORLD_UI_KEY.to_string(),
        HtmlSource::from_handle(handle),
    );
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

fn generate_single_player_html(worlds: &[SavedWorldEntry]) -> String {
    let cards = if worlds.is_empty() {
        "        <div id=\"single-player-empty\">No worlds found in saves/</div>\n".to_string()
    } else {
        worlds
            .iter()
            .enumerate()
            .map(|(index, world)| {
                let name = escape_html(world.folder_name.as_str());
                format!(
                    "        <div id=\"single-player-world-card-{index}\" class=\"single-player-world-card\">
          <p class=\"single-player-world-name\">WeltName: {name}</p>
          <p class=\"single-player-world-seed\">Seed: {seed}</p>
        </div>\n",
                    seed = world.seed
                )
            })
            .collect::<String>()
    };

    format!(
        "<html lang=\"en\">
  <head>
    <meta charset=\"UTF-8\" />
    <meta name=\"single-player\" />
    <title>Single Player</title>
    <link rel=\"stylesheet\" href=\"../css/single_player.css\" />
  </head>
  <body id=\"single-player-root\">
    <div id=\"single-player-panel\">
      <h2 id=\"single-player-title\">Single Player</h2>
      <div id=\"single-player-world-list\">
{cards}      </div>
      <div id=\"single-player-actions\">
        <button id=\"single-player-create-world\" class=\"single-player-action-button\">Create World</button>
        <button id=\"single-player-play-world\" class=\"single-player-action-button\">Play World</button>
        <button id=\"single-player-delete-world\" class=\"single-player-action-button\">Delet World</button>
      </div>
    </div>
    <div id=\"single-player-delete-dialog\">
      <div id=\"single-player-delete-box\">
        <p id=\"single-player-delete-text\">Ar you sure to delete ``?</p>
        <div id=\"single-player-delete-actions\">
          <button id=\"single-player-delete-confirm\" class=\"single-player-action-button danger\">Delete</button>
          <button id=\"single-player-delete-cancel\" class=\"single-player-action-button\">Cancel</button>
        </div>
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
