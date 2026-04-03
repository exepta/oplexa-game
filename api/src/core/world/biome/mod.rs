pub mod func;
pub mod registry;

use bevy::prelude::*;
use serde::*;

/// Represents biome used by the `core::world::biome` module.
#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct Biome {
    pub localized_name: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub stand_alone: bool,
    #[serde(default)]
    pub subs: Option<Vec<String>>,
    #[serde(default = "default_rarity")]
    pub rarity: f32,
    #[serde(default = "default_sizes")]
    pub sizes: Vec<BiomeSize>,
    #[serde(default)]
    pub surface: BiomeSurface,
    #[serde(default)]
    pub settings: BiomeSettings,
    #[serde(default)]
    pub generation: BiomeGeneration,
}

/// Defines the possible biome size variants in the `core::world::biome` module.
#[derive(Debug, Deserialize, PartialEq, Clone)]
pub enum BiomeSize {
    VeryTiny,
    Tiny,
    Small,
    Medium,
    Large,
    Huge,
    Giant,
    Ocean,
}

impl BiomeSize {
    /// Runs the `from_str` routine for from str in the `core::world::biome` module.
    pub fn from_str(s: &str) -> Self {
        match s {
            "very_tiny" => Self::VeryTiny, // Max 4 chunks
            "tiny" => Self::Tiny,          // Max 20 chunks
            "small" => Self::Small,        // Max 56 chunks
            "medium" => Self::Medium,      // Max 98 chunks
            "large" => Self::Large,        // Max 196 chunks
            "huge" => Self::Huge,          // Max 392 chunks
            "giant" => Self::Giant,        // Max 560 chunks
            "ocean" => Self::Ocean,        // Min 600 chunks
            _ => Self::Medium,
        }
    }
}

/// Represents biome surface used by the `core::world::biome` module.
#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct BiomeSurface {
    pub top: Vec<String>,
    pub bottom: Vec<String>,
    pub sea_floor: Vec<String>,
    pub upper_zero: Vec<String>,
    pub under_zero: Vec<String>,
}

impl Default for BiomeSurface {
    /// Runs the `default` routine for default in the `core::world::biome` module.
    fn default() -> Self {
        Self {
            top: vec!["grass_block".to_string()],
            bottom: vec!["dirt_block".to_string()],
            sea_floor: vec!["sand_block".to_string()],
            upper_zero: vec!["stone_block".to_string()],
            under_zero: vec!["stone_block".to_string()],
        }
    }
}

/// Represents biome settings used by the `core::world::biome` module.
#[derive(Debug, Deserialize, PartialEq, Clone, Default)]
pub struct BiomeSettings {
    #[serde(default)]
    pub height_offset: f32,

    // ocean-only
    #[serde(default)]
    pub seafloor_amp: Option<f32>,
    #[serde(default)]
    pub seafloor_freq: Option<f32>,

    // plains/land
    #[serde(default)]
    pub land_amp: Option<f32>,
    #[serde(default)]
    pub land_freq: Option<f32>,

    // mountains
    #[serde(default)]
    pub mount_amp: Option<f32>,
    #[serde(default)]
    pub mount_freq: Option<f32>,
}

/// Represents biome generation used by the `core::world::biome` module.
#[derive(Debug, Deserialize, PartialEq, Clone, Default)]
pub struct BiomeGeneration {
    #[serde(default)]
    pub rivers: bool,
    #[serde(default = "default_river_chance")]
    pub river_chance: f32,
    #[serde(
        default = "default_river_size_between",
        deserialize_with = "deserialize_size_between"
    )]
    pub river_size_between: (i32, i32),
}

/// Runs the `default_true` routine for default true in the `core::world::biome` module.
fn default_true() -> bool {
    true
}

/// Runs the `default_rarity` routine for default rarity in the `core::world::biome` module.
fn default_rarity() -> f32 {
    0.1
}

/// Runs the `default_river_chance` routine for default river chance in the `core::world::biome` module.
fn default_river_chance() -> f32 {
    0.1
}

/// Runs the `default_river_size_between` routine for default river size between in the `core::world::biome` module.
fn default_river_size_between() -> (i32, i32) {
    (6, 16)
}

/// Deserializes size between for the `core::world::biome` module.
fn deserialize_size_between<'de, D>(de: D) -> Result<(i32, i32), D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(de)?;
    let mut it = s.split(':');
    let a = it.next().unwrap_or("8").trim().parse::<i32>().unwrap_or(8);
    let b = it
        .next()
        .unwrap_or("20")
        .trim()
        .parse::<i32>()
        .unwrap_or(20);
    Ok(if a <= b { (a, b) } else { (b, a) })
}

/// Runs the `default_sizes` routine for default sizes in the `core::world::biome` module.
fn default_sizes() -> Vec<BiomeSize> {
    vec![BiomeSize::Medium]
}
