use crate::core::inventory::items::tools::{ToolDef, ToolLevel, ToolRequirement, ToolType};
use crate::core::world::block::{BlockId, BlockRegistry};

const WRONG_TOOL_SPEED_MULTIPLIER: f32 = 0.3;
const HAND_SPEED_MULTIPLIER: f32 = 0.45;
const CORRECT_TOOL_BASE_MULTIPLIER: f32 = 1.35;
const CORRECT_TOOL_PER_LEVEL_MULTIPLIER: f32 = 0.25;
const NON_REQUIRED_TOOL_BASE_MULTIPLIER: f32 = 1.0;
const NON_REQUIRED_TOOL_PER_LEVEL_MULTIPLIER: f32 = 0.08;

/// Runs the `infer_tool_from_item_key` routine for infer tool from item key in the `core::inventory::items::tools::helpers` module.
#[inline]
pub fn infer_tool_from_item_key(item_key: &str) -> Option<ToolDef> {
    let key = item_key.trim().to_ascii_lowercase();
    if key.is_empty() {
        return None;
    }

    let inferred_level = if key.contains("stone") {
        ToolLevel::L2
    } else {
        ToolLevel::L1
    };

    if key.ends_with("_pickaxe") {
        return Some(ToolDef::new(ToolType::Pickaxe, inferred_level));
    }
    if key.ends_with("_shovel") {
        return Some(ToolDef::new(ToolType::Shovel, inferred_level));
    }
    if key.ends_with("_axe") {
        return Some(ToolDef::new(ToolType::Axe, inferred_level));
    }
    if key.ends_with("_sword") {
        return Some(ToolDef::new(ToolType::Sword, inferred_level));
    }
    None
}

/// Runs the `block_requirement_for_name` routine for block requirement for name in the `core::inventory::items::tools::helpers` module.
#[inline]
pub fn block_requirement_for_name(block_name: &str) -> Option<ToolRequirement> {
    infer_required_tool_type(block_name)
        .map(|tool_type| ToolRequirement::new(tool_type, ToolLevel::L1))
}

/// Runs the `infer_required_tool_type` routine for infer required tool type in the `core::inventory::items::tools::helpers` module.
#[inline]
fn infer_required_tool_type(block_name: &str) -> Option<ToolType> {
    let name = block_name.trim().to_ascii_lowercase();
    match name.as_str() {
        "stone_block" | "sand_stone_block" | "deep_stone_block" | "border_block" => {
            Some(ToolType::Pickaxe)
        }
        "dirt_block" | "grass_block" | "sand_block" | "gravel_block" | "clay_block"
        | "snow_block" => Some(ToolType::Shovel),
        "oak_log_block" | "log_block" => Some(ToolType::Axe),
        _ => None,
    }
}

/// Runs the `block_requirement_for_id` routine for block requirement for id in the `core::inventory::items::tools::helpers` module.
#[inline]
pub fn block_requirement_for_id(
    block_id: BlockId,
    registry: &BlockRegistry,
) -> Option<ToolRequirement> {
    let required_level = registry.level(block_id).min(6);
    if required_level == 0 {
        return None;
    }

    let min_level = ToolLevel::from_u8_clamped(required_level);
    let tool_type = registry
        .name_opt(block_id)
        .and_then(infer_required_tool_type)
        .unwrap_or(ToolType::Pickaxe);
    Some(ToolRequirement::new(tool_type, min_level))
}

/// Checks whether drop from block in the `core::inventory::items::tools::helpers` module.
#[inline]
pub fn can_drop_from_block(
    requirement: Option<ToolRequirement>,
    held_tool: Option<ToolDef>,
) -> bool {
    match requirement {
        Some(required) => required.is_met_by(held_tool),
        None => true,
    }
}

/// Runs the `mining_speed_multiplier` routine for mining speed multiplier in the `core::inventory::items::tools::helpers` module.
#[inline]
pub fn mining_speed_multiplier(
    requirement: Option<ToolRequirement>,
    held_tool: Option<ToolDef>,
) -> f32 {
    match requirement {
        Some(required) => match held_tool {
            Some(tool) if tool.satisfies(required) => {
                let extra = (tool.level.as_u8() - required.min_level.as_u8()) as f32;
                CORRECT_TOOL_BASE_MULTIPLIER + extra * CORRECT_TOOL_PER_LEVEL_MULTIPLIER
            }
            Some(_) => WRONG_TOOL_SPEED_MULTIPLIER,
            None => HAND_SPEED_MULTIPLIER,
        },
        None => match held_tool {
            Some(tool) => {
                NON_REQUIRED_TOOL_BASE_MULTIPLIER
                    + (tool.level.as_u8().saturating_sub(1) as f32)
                        * NON_REQUIRED_TOOL_PER_LEVEL_MULTIPLIER
            }
            None => 1.0,
        },
    }
    .max(0.05)
}
