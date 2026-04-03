use std::str::FromStr;

/// Defines the possible tool type variants in the `core::inventory::items::tools::tool_type` module.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ToolType {
    Pickaxe,
    Shovel,
    Sword,
    Axe,
}

impl ToolType {
    /// Runs the `as_str` routine for as str in the `core::inventory::items::tools::tool_type` module.
    #[inline]
    pub const fn as_str(self) -> &'static str {
        match self {
            ToolType::Pickaxe => "pickaxe",
            ToolType::Shovel => "shovel",
            ToolType::Sword => "sword",
            ToolType::Axe => "axe",
        }
    }
}

impl FromStr for ToolType {
    /// Type alias for err used by the `core::inventory::items::tools::tool_type` module.
    type Err = ();

    /// Runs the `from_str` routine for from str in the `core::inventory::items::tools::tool_type` module.
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let normalized = input.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "pickaxe" => Ok(ToolType::Pickaxe),
            "shovel" => Ok(ToolType::Shovel),
            "sword" => Ok(ToolType::Sword),
            "axe" => Ok(ToolType::Axe),
            _ => Err(()),
        }
    }
}
