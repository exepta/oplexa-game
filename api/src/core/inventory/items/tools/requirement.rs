use crate::core::inventory::items::tools::{ToolDef, ToolLevel, ToolType};

/// Represents tool requirement used by the `core::inventory::items::tools::requirement` module.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ToolRequirement {
    pub tool_type: ToolType,
    pub min_level: ToolLevel,
}

impl ToolRequirement {
    /// Creates a new instance for the `core::inventory::items::tools::requirement` module.
    #[inline]
    pub const fn new(tool_type: ToolType, min_level: ToolLevel) -> Self {
        Self {
            tool_type,
            min_level,
        }
    }

    /// Checks whether met by in the `core::inventory::items::tools::requirement` module.
    #[inline]
    pub fn is_met_by(self, tool: Option<ToolDef>) -> bool {
        match tool {
            Some(tool) => tool.satisfies(self),
            None => false,
        }
    }
}
