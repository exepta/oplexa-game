/// Defines the possible tool level variants in the `core::inventory::items::tools::tool_level` module.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum ToolLevel {
    L1 = 1,
    L2 = 2,
    L3 = 3,
    L4 = 4,
    L5 = 5,
    L6 = 6,
}

impl Default for ToolLevel {
    /// Runs the `default` routine for default in the `core::inventory::items::tools::tool_level` module.
    fn default() -> Self {
        Self::L1
    }
}

impl ToolLevel {
    /// Runs the `as_u8` routine for as u8 in the `core::inventory::items::tools::tool_level` module.
    #[inline]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Runs the `from_u8_clamped` routine for from u8 clamped in the `core::inventory::items::tools::tool_level` module.
    #[inline]
    pub const fn from_u8_clamped(raw: u8) -> Self {
        match raw {
            0 | 1 => Self::L1,
            2 => Self::L2,
            3 => Self::L3,
            4 => Self::L4,
            5 => Self::L5,
            _ => Self::L6,
        }
    }
}
