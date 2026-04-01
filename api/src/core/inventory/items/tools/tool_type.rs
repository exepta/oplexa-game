use std::str::FromStr;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ToolType {
    Pickaxe,
    Shovel,
    Sword,
    Axe,
}

impl ToolType {
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
    type Err = ();

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
