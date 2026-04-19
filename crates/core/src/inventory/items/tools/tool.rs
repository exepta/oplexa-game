use crate::core::inventory::items::tools::{ToolLevel, ToolRequirement, ToolType};

/// Represents tool def used by the `core::inventory::items::tools::tool` module.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ToolDef {
    pub tool_type: ToolType,
    pub level: ToolLevel,
}

impl ToolDef {
    /// Creates a new instance for the `core::inventory::items::tools::tool` module.
    #[inline]
    pub const fn new(tool_type: ToolType, level: ToolLevel) -> Self {
        Self { tool_type, level }
    }

    /// Runs the `satisfies` routine for satisfies in the `core::inventory::items::tools::tool` module.
    #[inline]
    pub fn satisfies(self, requirement: ToolRequirement) -> bool {
        self.tool_type == requirement.tool_type
            && self.level.as_u8() >= requirement.min_level.as_u8()
    }
}
