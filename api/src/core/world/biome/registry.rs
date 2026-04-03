use crate::core::world::biome::*;
use bevy::prelude::*;
use rand::distr::Distribution;
use rand::distr::weighted::WeightedIndex;
use rand::rng;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Represents biome registry used by the `core::world::biome::registry` module.
#[derive(Resource, Debug, Default, Clone)]
pub struct BiomeRegistry {
    pub by_name: HashMap<String, Biome>,
    pub ordered_names: Vec<String>,
    pub weights: Vec<f32>,
}

impl BiomeRegistry {
    /// Loads from folder for the `core::world::biome::registry` module.
    pub fn load_from_folder(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref();
        let mut registry = Self::default();

        if !dir.exists() {
            return registry;
        }

        let mut paths: Vec<_> = match fs::read_dir(dir) {
            Ok(rd) => rd
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    p.extension()
                        .and_then(|e| e.to_str())
                        .map(|ext| ext.eq_ignore_ascii_case("json"))
                        .unwrap_or(false)
                })
                .collect(),
            Err(_) => return registry,
        };
        paths.sort();

        for path in paths {
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(mut biome) = serde_json::from_str::<Biome>(&text) else {
                continue;
            };

            if biome.name.trim().is_empty()
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            {
                biome.name = stem.to_string();
            }

            registry.register(biome);
        }

        registry
    }

    /// Registers the requested data for the `core::world::biome::registry` module.
    pub fn register(&mut self, biome: Biome) {
        let key = biome.name.clone();
        self.by_name.insert(key, biome);
        self.rebuild_cache();
    }

    /// Returns the requested data for the `core::world::biome::registry` module.
    pub fn get(&self, name: &str) -> Option<&Biome> {
        self.by_name.get(name)
    }

    /// Runs the `iter` routine for iter in the `core::world::biome::registry` module.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Biome)> {
        self.by_name.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Number of registered biomes.
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    /// Checks whether empty in the `core::world::biome::registry` module.
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    /// Runs the `random_by_rarity` routine for random by rarity in the `core::world::biome::registry` module.
    pub fn random_by_rarity(&self) -> Option<&Biome> {
        if self.ordered_names.is_empty() {
            return None;
        }
        // Safety net: WeightedIndex requires strictly positive weights.
        if self.weights.iter().all(|w| *w <= 0.0) {
            return None;
        }
        let dist = WeightedIndex::new(&self.weights).ok()?;
        let mut rng = rng();
        let idx = dist.sample(&mut rng);
        let name = &self.ordered_names[idx];
        self.by_name.get(name)
    }

    /// Runs the `rebuild_cache` routine for rebuild cache in the `core::world::biome::registry` module.
    fn rebuild_cache(&mut self) {
        let mut names: Vec<_> = self.by_name.keys().cloned().collect();
        names.sort(); // deterministic order for reproducibility
        let weights = names
            .iter()
            .map(|n| self.by_name[n].rarity.max(0.0))
            .collect();
        self.ordered_names = names;
        self.weights = weights;
    }
}
