use crate::core::config::GlobalConfig;
use crate::core::entities::player::inventory::{PLAYER_INVENTORY_SLOTS, PlayerInventory};
use crate::core::states::states::{AppState, InGameStates};
use crate::core::world::block::BlockRegistry;
use crate::utils::key_utils::convert;
use bevy::prelude::*;
use bevy_extended_ui::html::HtmlSource;
use bevy_extended_ui::io::HtmlAsset;
use bevy_extended_ui::registry::UiRegistry;
use bevy_extended_ui::styles::CssID;
use bevy_extended_ui::widgets::{Img, Paragraph};
use std::fs;
use std::path::Path;

const PLAYER_INVENTORY_UI_KEY: &str = "player-inventory";
const PLAYER_INVENTORY_UI_PATH: &str = "ui/html/player_inventory.html";
const PLAYER_INVENTORY_TOTAL_ID: &str = "player-inventory-total";
const PLAYER_INVENTORY_SLOT_PREFIX: &str = "player-inventory-slot-";
const PLAYER_INVENTORY_ICON_PREFIX: &str = "player-inventory-icon-";

pub struct PlayerInventoryUiPlugin;

#[derive(Resource, Debug, Default)]
struct PlayerInventoryUiState {
    open: bool,
}

impl Plugin for PlayerInventoryUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerInventoryUiState>()
            .add_systems(Startup, register_player_inventory_ui)
            .add_systems(
                Update,
                (toggle_player_inventory_ui, sync_player_inventory_ui)
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
    mut registry: ResMut<UiRegistry>,
    mut inventory_ui: ResMut<PlayerInventoryUiState>,
) {
    let open_key =
        convert(global_config.input.ui_inventory.as_str()).expect("Invalid inventory key");
    let close_key =
        convert(global_config.input.ui_close_back.as_str()).expect("Invalid close/back key");

    if inventory_ui.open && keyboard.just_pressed(close_key) {
        inventory_ui.open = false;
        hide_player_inventory_ui(&mut registry);
        return;
    }

    if !keyboard.just_pressed(open_key) {
        return;
    }

    inventory_ui.open = !inventory_ui.open;
    if inventory_ui.open {
        show_player_inventory_ui(&mut registry, &asset_server);
    } else {
        hide_player_inventory_ui(&mut registry);
    }
}

fn close_player_inventory_ui(
    mut inventory_ui: ResMut<PlayerInventoryUiState>,
    mut registry: ResMut<UiRegistry>,
) {
    if !inventory_ui.open {
        return;
    }

    inventory_ui.open = false;
    hide_player_inventory_ui(&mut registry);
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

    registry.use_ui(PLAYER_INVENTORY_UI_KEY);
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
    mut images: Query<(&CssID, &mut Img)>,
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

    for (css_id, mut image) in &mut images {
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
            image.src = next_src;
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
            "        <div class=\"inventory-slot\"><img id=\"player-inventory-icon-{index}\" class=\"inventory-slot-icon\" alt=\" \" /><p id=\"player-inventory-slot-{index}\" class=\"inventory-slot-index\"></p></div>\n"
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
