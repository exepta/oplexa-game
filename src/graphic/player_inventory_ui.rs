use crate::core::config::GlobalConfig;
use crate::core::entities::player::Player;
use crate::core::entities::player::inventory::{
    InventorySlot, PLAYER_INVENTORY_SLOTS, PlayerInventory,
};
use crate::core::multiplayer::MultiplayerConnectionState;
use crate::core::states::states::{AppState, InGameStates};
use crate::core::ui::UiInteractionState;
use crate::core::world::block::BlockRegistry;
use crate::logic::events::block_event_handler::spawn_dropped_block_stack;
use crate::utils::key_utils::convert;
use bevy::image::TRANSPARENT_IMAGE_HANDLE;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_extended_ui::html::HtmlSource;
use bevy_extended_ui::io::HtmlAsset;
use bevy_extended_ui::registry::UiRegistry;
use bevy_extended_ui::styles::CssID;
use bevy_extended_ui::widgets::{Img, Paragraph, UIWidgetState};
use std::fs;
use std::path::Path;

const PLAYER_INVENTORY_UI_KEY: &str = "player-inventory";
const PLAYER_INVENTORY_UI_PATH: &str = "ui/html/player_inventory.html";
const PLAYER_INVENTORY_TOTAL_ID: &str = "player-inventory-total";
const PLAYER_INVENTORY_SLOT_PREFIX: &str = "player-inventory-slot-";
const PLAYER_INVENTORY_ICON_PREFIX: &str = "player-inventory-icon-";
const PLAYER_INVENTORY_FRAME_PREFIX: &str = "player-inventory-frame-";

pub struct PlayerInventoryUiPlugin;

#[derive(Resource, Debug, Default)]
struct PlayerInventoryUiState {
    open: bool,
}

#[derive(Resource, Debug, Default, Clone, Copy)]
struct InventoryDragState {
    source_slot: Option<usize>,
}

impl Plugin for PlayerInventoryUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerInventoryUiState>()
            .init_resource::<InventoryDragState>()
            .add_systems(Startup, register_player_inventory_ui)
            .add_systems(
                Update,
                (
                    toggle_player_inventory_ui,
                    handle_inventory_drag_and_drop,
                    sync_player_inventory_ui,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame(InGameStates::Game))),
            )
            .add_systems(
                OnExit(AppState::InGame(InGameStates::Game)),
                close_player_inventory_ui,
            );
    }
}

fn register_player_inventory_ui(mut registry: ResMut<UiRegistry>, asset_server: Res<AssetServer>) {
    if registry.get(PLAYER_INVENTORY_UI_KEY).is_some() {
        return;
    }

    generate_inventory_html_file();

    let handle: Handle<HtmlAsset> = asset_server.load(PLAYER_INVENTORY_UI_PATH);
    registry.add(
        PLAYER_INVENTORY_UI_KEY.to_string(),
        HtmlSource::from_handle(handle),
    );
}

fn toggle_player_inventory_ui(
    keyboard: Res<ButtonInput<KeyCode>>,
    global_config: Res<GlobalConfig>,
    asset_server: Res<AssetServer>,
    mut ui_interaction: ResMut<UiInteractionState>,
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut registry: ResMut<UiRegistry>,
    mut inventory_ui: ResMut<PlayerInventoryUiState>,
    mut drag_state: ResMut<InventoryDragState>,
) {
    let open_key =
        convert(global_config.input.ui_inventory.as_str()).expect("Invalid inventory key");
    let close_key =
        convert(global_config.input.ui_close_back.as_str()).expect("Invalid close/back key");

    if inventory_ui.open && keyboard.just_pressed(close_key) {
        inventory_ui.open = false;
        drag_state.source_slot = None;
        ui_interaction.inventory_open = false;
        set_inventory_cursor(false, &mut cursor_q);
        hide_player_inventory_ui(&mut registry);
        return;
    }

    if !keyboard.just_pressed(open_key) {
        return;
    }

    inventory_ui.open = !inventory_ui.open;
    if !inventory_ui.open {
        drag_state.source_slot = None;
    }
    ui_interaction.inventory_open = inventory_ui.open;
    set_inventory_cursor(inventory_ui.open, &mut cursor_q);
    if inventory_ui.open {
        show_player_inventory_ui(&mut registry, &asset_server);
    } else {
        hide_player_inventory_ui(&mut registry);
    }
}

fn close_player_inventory_ui(
    mut ui_interaction: ResMut<UiInteractionState>,
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut inventory_ui: ResMut<PlayerInventoryUiState>,
    mut registry: ResMut<UiRegistry>,
    mut drag_state: ResMut<InventoryDragState>,
) {
    if !inventory_ui.open {
        return;
    }

    inventory_ui.open = false;
    drag_state.source_slot = None;
    ui_interaction.inventory_open = false;
    set_inventory_cursor(false, &mut cursor_q);
    hide_player_inventory_ui(&mut registry);
}

fn handle_inventory_drag_and_drop(
    mouse: Res<ButtonInput<MouseButton>>,
    time: Res<Time>,
    inventory_ui: Res<PlayerInventoryUiState>,
    multiplayer_connection: Option<Res<MultiplayerConnectionState>>,
    mut drag_state: ResMut<InventoryDragState>,
    mut inventory: ResMut<PlayerInventory>,
    hovered_slots: Query<(&CssID, &UIWidgetState)>,
    player_q: Query<&Transform, With<Player>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    registry: Res<BlockRegistry>,
) {
    if !inventory_ui.open {
        drag_state.source_slot = None;
        return;
    }

    let hovered_slot = hovered_inventory_slot(&hovered_slots);

    if mouse.just_pressed(MouseButton::Left) && drag_state.source_slot.is_none() {
        if let Some(source_index) = hovered_slot {
            if inventory
                .slots
                .get(source_index)
                .is_some_and(|slot| !slot.is_empty())
            {
                drag_state.source_slot = Some(source_index);
            }
        }
    }

    if !mouse.just_released(MouseButton::Left) {
        return;
    }

    let Some(source_index) = drag_state.source_slot.take() else {
        return;
    };

    if source_index >= PLAYER_INVENTORY_SLOTS {
        return;
    }

    if let Some(target_index) = hovered_slot {
        if target_index < PLAYER_INVENTORY_SLOTS && target_index != source_index {
            inventory.slots.swap(source_index, target_index);
        }
        return;
    }

    let dropped_slot = inventory.slots[source_index];
    if dropped_slot.is_empty() {
        return;
    }

    if multiplayer_connection
        .as_ref()
        .is_some_and(|state| state.connected)
    {
        return;
    }

    let Ok(player_tf) = player_q.single() else {
        return;
    };

    let drop_pos = player_tf.translation + player_tf.forward().as_vec3() * 1.1 + Vec3::Y * 0.6;
    let world_loc = IVec3::new(
        drop_pos.x.floor() as i32,
        drop_pos.y.floor() as i32,
        drop_pos.z.floor() as i32,
    );

    inventory.slots[source_index] = InventorySlot::default();

    spawn_dropped_block_stack(
        &mut commands,
        &mut meshes,
        &registry,
        dropped_slot.block_id,
        dropped_slot.count,
        world_loc,
        time.elapsed_secs(),
    );
}

fn show_player_inventory_ui(registry: &mut UiRegistry, asset_server: &AssetServer) {
    if registry.get(PLAYER_INVENTORY_UI_KEY).is_none() {
        generate_inventory_html_file();
        let handle: Handle<HtmlAsset> = asset_server.load(PLAYER_INVENTORY_UI_PATH);
        registry.add(
            PLAYER_INVENTORY_UI_KEY.to_string(),
            HtmlSource::from_handle(handle),
        );
    }

    activate_ui_append(registry, PLAYER_INVENTORY_UI_KEY);
}

fn hide_player_inventory_ui(registry: &mut UiRegistry) {
    let mut clear_current = false;

    if let Some(current) = registry.current.as_mut() {
        current.retain(|name| name != PLAYER_INVENTORY_UI_KEY);
        clear_current = current.is_empty();
        registry.ui_update = true;
    }

    if clear_current {
        registry.current = None;
    }
}

fn sync_player_inventory_ui(
    inventory: Res<PlayerInventory>,
    registry: Res<BlockRegistry>,
    asset_server: Res<AssetServer>,
    mut paragraphs: Query<(&CssID, &mut Paragraph)>,
    mut images: Query<(&CssID, &mut Img, Option<&mut ImageNode>)>,
) {
    for (css_id, mut paragraph) in &mut paragraphs {
        if css_id.0 == PLAYER_INVENTORY_TOTAL_ID {
            paragraph.text = format!("Items: {}", inventory.total_items());
            continue;
        }

        let Some(slot_number) = css_id.0.strip_prefix(PLAYER_INVENTORY_SLOT_PREFIX) else {
            continue;
        };

        let Ok(slot_index) = slot_number.parse::<usize>() else {
            continue;
        };

        let Some(slot) = inventory.slots.get(slot_index.saturating_sub(1)) else {
            continue;
        };

        paragraph.text = if slot.is_empty() {
            String::new()
        } else {
            slot.count.to_string()
        };
    }

    for (css_id, mut image, image_node_opt) in &mut images {
        let Some(slot_number) = css_id.0.strip_prefix(PLAYER_INVENTORY_ICON_PREFIX) else {
            continue;
        };

        let Ok(slot_index) = slot_number.parse::<usize>() else {
            continue;
        };

        let Some(slot) = inventory.slots.get(slot_index.saturating_sub(1)) else {
            continue;
        };

        let next_src = if slot.is_empty() {
            None
        } else {
            resolve_block_icon_path(&registry, &asset_server, slot.block_id)
        };

        if image.src != next_src {
            image.src = next_src.clone();
        }

        if next_src.is_none() {
            if let Some(mut image_node) = image_node_opt {
                if image_node.image.id() != TRANSPARENT_IMAGE_HANDLE.id() {
                    image_node.image = TRANSPARENT_IMAGE_HANDLE;
                }
            }
        }
    }
}

fn resolve_block_icon_path(
    registry: &BlockRegistry,
    asset_server: &AssetServer,
    block_id: u16,
) -> Option<String> {
    let block = registry.defs.get(block_id as usize)?;
    let path = asset_server.get_path(block.image.id())?;
    Some(path.path().to_string_lossy().to_string())
}

fn generate_inventory_html_file() {
    let mut slots_html = String::new();
    for i in 1..=PLAYER_INVENTORY_SLOTS {
        let index = format!("{:02}", i);
        slots_html.push_str(&format!(
            "        <div id=\"player-inventory-frame-{index}\" class=\"inventory-slot\"><img id=\"player-inventory-icon-{index}\" class=\"inventory-slot-icon\" alt=\" \" /><p id=\"player-inventory-slot-{index}\" class=\"inventory-slot-index\"></p></div>\n"
        ));
    }

    let html = format!(
        "<html lang=\"en\">
  <head>
    <meta charset=\"UTF-8\" />
    <meta name=\"player-inventory\" />
    <title>Inventory</title>
    <link rel=\"stylesheet\" href=\"../css/player_inventory.css\" />
  </head>
  <body id=\"player-inventory-root\">
    <div id=\"player-inventory-panel\">
      <h4 id=\"player-inventory-title\">Inventory</h4>
      <p id=\"player-inventory-total\">Items: 0</p>
      <div id=\"player-inventory-grid\">
{slots_html}      </div>
    </div>
  </body>
</html>
"
    );

    let path = Path::new("assets").join(PLAYER_INVENTORY_UI_PATH);
    if let Some(parent) = path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            warn!("Failed to create inventory ui directory: {error}");
            return;
        }
    }

    if let Err(error) = fs::write(&path, html) {
        warn!("Failed to write inventory ui html {:?}: {error}", path);
    }
}

fn set_inventory_cursor(
    inventory_open: bool,
    cursor_q: &mut Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let Ok(mut cursor) = cursor_q.single_mut() else {
        return;
    };

    if inventory_open {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    } else {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

fn activate_ui_append(registry: &mut UiRegistry, key: &str) {
    if registry.get(key).is_none() {
        return;
    }

    if let Some(current) = registry.current.as_mut() {
        if current.iter().any(|name| name == key) {
            return;
        }
        current.push(key.to_string());
        registry.ui_update = true;
        return;
    }

    registry.current = Some(vec![key.to_string()]);
    registry.ui_update = true;
}

fn hovered_inventory_slot(hovered_slots: &Query<(&CssID, &UIWidgetState)>) -> Option<usize> {
    hovered_slots.iter().find_map(|(css_id, state)| {
        if !state.hovered {
            return None;
        }

        let slot_number = css_id.0.strip_prefix(PLAYER_INVENTORY_FRAME_PREFIX)?;
        let slot_index = slot_number.parse::<usize>().ok()?.checked_sub(1)?;
        (slot_index < PLAYER_INVENTORY_SLOTS).then_some(slot_index)
    })
}
