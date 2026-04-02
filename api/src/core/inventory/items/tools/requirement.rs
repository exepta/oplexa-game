use crate::core::inventory::items::tools::{ToolDef, ToolLevel, ToolType};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ToolRequirement {
    pub tool_type: ToolType,
    pub min_level: ToolLevel,
}

impl ToolRequirement {
    #[inline]
    pub const fn new(tool_type: ToolType, min_level: ToolLevel) -> Self {
        Self {
            tool_type,
            min_level,
        }
    }

    #[inline]
    pub fn is_met_by(self, tool: Option<ToolDef>) -> bool {
        match tool {
            Some(tool) => tool.satisfies(self),
            None => false,
        }
    }
}
