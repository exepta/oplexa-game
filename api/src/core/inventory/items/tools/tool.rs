use crate::core::inventory::items::tools::{ToolLevel, ToolRequirement, ToolType};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ToolDef {
    pub tool_type: ToolType,
    pub level: ToolLevel,
}

impl ToolDef {
    #[inline]
    pub const fn new(tool_type: ToolType, level: ToolLevel) -> Self {
        Self { tool_type, level }
    }

    #[inline]
    pub fn satisfies(self, requirement: ToolRequirement) -> bool {
        self.tool_type == requirement.tool_type
            && self.level.as_u8() >= requirement.min_level.as_u8()
    }
}
