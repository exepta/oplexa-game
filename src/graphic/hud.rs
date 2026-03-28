use crate::core::entities::player::inventory::PlayerInventory;
use crate::core::states::states::{AppState, InGameStates};
use crate::core::ui::{HOTBAR_SLOTS, HotbarSelectionState};
use crate::core::world::block::{BlockRegistry, SelectedBlock};
use bevy::image::TRANSPARENT_IMAGE_HANDLE;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy_extended_ui::html::HtmlSource;
use bevy_extended_ui::io::HtmlAsset;
use bevy_extended_ui::registry::UiRegistry;
use bevy_extended_ui::styles::CssID;
use bevy_extended_ui::widgets::{Img, Paragraph};

const HUD_HOTBAR_UI_KEY: &str = "hud-hotbar";
const HUD_HOTBAR_UI_PATH: &str = "ui/html/hud_hotbar.html";
const HOTBAR_SLOT_PREFIX: &str = "hud-hotbar-slot-";
const HOTBAR_ICON_PREFIX: &str = "hud-hotbar-icon-";
const HOTBAR_SELECTED_PREFIX: &str = "hud-hotbar-selected-";

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_hud_hotbar_ui)
            .add_systems(
                OnEnter(AppState::InGame(InGameStates::Game)),
                show_hud_hotbar_ui,
            )
            .add_systems(
                Update,
                (
                    cycle_hotbar_with_scroll,
                    sync_hotbar_selected_block,
                    sync_hud_hotbar_ui,
                )
                    .run_if(in_state(AppState::InGame(InGameStates::Game))),
            )
            .add_systems(
                OnExit(AppState::InGame(InGameStates::Game)),
                hide_hud_hotbar_ui,
            );
    }
}

fn register_hud_hotbar_ui(mut registry: ResMut<UiRegistry>, asset_server: Res<AssetServer>) {
    if registry.get(HUD_HOTBAR_UI_KEY).is_some() {
        return;
    }

    let handle: Handle<HtmlAsset> = asset_server.load(HUD_HOTBAR_UI_PATH);
    registry.add(
        HUD_HOTBAR_UI_KEY.to_string(),
        HtmlSource::from_handle(handle),
    );
}

fn show_hud_hotbar_ui(mut registry: ResMut<UiRegistry>, asset_server: Res<AssetServer>) {
    if registry.get(HUD_HOTBAR_UI_KEY).is_none() {
        let handle: Handle<HtmlAsset> = asset_server.load(HUD_HOTBAR_UI_PATH);
        registry.add(
            HUD_HOTBAR_UI_KEY.to_string(),
            HtmlSource::from_handle(handle),
        );
    }

    activate_ui_append(&mut registry, HUD_HOTBAR_UI_KEY);
}

fn hide_hud_hotbar_ui(mut registry: ResMut<UiRegistry>) {
    let mut clear_current = false;

    if let Some(current) = registry.current.as_mut() {
        current.retain(|name| name != HUD_HOTBAR_UI_KEY);
        clear_current = current.is_empty();
        registry.ui_update = true;
    }

    if clear_current {
        registry.current = None;
    }
}

fn cycle_hotbar_with_scroll(
    mut wheel_reader: MessageReader<MouseWheel>,
    mut hotbar_state: ResMut<HotbarSelectionState>,
) {
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

fn sync_hotbar_selected_block(
    hotbar_state: Res<HotbarSelectionState>,
    inventory: Res<PlayerInventory>,
    registry: Res<BlockRegistry>,
    mut selected: ResMut<SelectedBlock>,
) {
    let Some(slot) = inventory.slots.get(hotbar_state.selected_index).copied() else {
        selected.id = 0;
        selected.name = "air".to_string();
        return;
    };

    if slot.is_empty() {
        selected.id = 0;
        selected.name = "air".to_string();
        return;
    }

    selected.id = slot.block_id;
    selected.name = registry
        .name_opt(slot.block_id)
        .unwrap_or("air")
        .to_string();
}

fn sync_hud_hotbar_ui(
    hotbar_state: Res<HotbarSelectionState>,
    inventory: Res<PlayerInventory>,
    registry: Res<BlockRegistry>,
    asset_server: Res<AssetServer>,
    mut paragraphs: Query<(&CssID, &mut Paragraph)>,
    mut images: Query<(&CssID, &mut Img, Option<&mut ImageNode>)>,
) {
    for (css_id, mut paragraph) in &mut paragraphs {
        if let Some(slot_number) = css_id.0.strip_prefix(HOTBAR_SLOT_PREFIX) {
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
            continue;
        }

        let Some(slot_number) = css_id.0.strip_prefix(HOTBAR_SELECTED_PREFIX) else {
            continue;
        };
        let Ok(slot_index) = slot_number.parse::<usize>() else {
            continue;
        };

        paragraph.text = if slot_index.saturating_sub(1) == hotbar_state.selected_index {
            ">".to_string()
        } else {
            String::new()
        };
    }

    for (css_id, mut image, image_node_opt) in &mut images {
        let Some(slot_number) = css_id.0.strip_prefix(HOTBAR_ICON_PREFIX) else {
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
