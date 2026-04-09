use bevy::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Canonicalizes tree-family keys so config can use different casing/styles.
#[inline]
pub fn canonical_tree_key(raw: &str) -> String {
    raw.trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| if matches!(c, ' ' | '-' | '/') { '_' } else { c })
        .collect()
}

/// Runtime registry of all tree families and their variants.
#[derive(Resource, Debug, Clone, Default)]
pub struct TreeRegistry {
    pub by_name: HashMap<String, TreeFamily>,
}

impl TreeRegistry {
    /// Loads all tree family files (`*.json`) from a folder recursively.
    pub fn load_from_folder(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref();
        let mut registry = Self::default();
        if !dir.exists() {
            return registry;
        }

        let mut paths = Vec::new();
        collect_json_paths_recursive(dir, &mut paths);
        paths.sort();

        for path in paths {
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(mut file) = serde_json::from_str::<TreeFamilyFile>(&text) else {
                continue;
            };

            if file.name.trim().is_empty() {
                file.name = path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str())
                    .or_else(|| path.file_stem().and_then(|s| s.to_str()))
                    .unwrap_or("tree")
                    .to_string();
            }

            let key = canonical_tree_key(&file.name);
            if key.is_empty() {
                continue;
            }

            let family = registry
                .by_name
                .entry(key.clone())
                .or_insert_with(|| TreeFamily {
                    key,
                    display_name: file.name.clone(),
                    variants: Vec::new(),
                });

            for variant in file.variants {
                family.variants.push(variant.into_runtime());
            }
        }

        // Keep only families with at least one usable variant.
        registry.by_name.retain(|_, fam| !fam.variants.is_empty());
        registry
    }

    #[inline]
    pub fn get(&self, name: &str) -> Option<&TreeFamily> {
        let key = canonical_tree_key(name);
        self.by_name.get(&key)
    }

    #[inline]
    pub fn family_count(&self) -> usize {
        self.by_name.len()
    }

    #[inline]
    pub fn variant_count(&self) -> usize {
        self.by_name.values().map(|f| f.variants.len()).sum()
    }
}

#[derive(Debug, Clone)]
pub struct TreeFamily {
    pub key: String,
    pub display_name: String,
    pub variants: Vec<TreeVariant>,
}

#[derive(Debug, Clone)]
pub struct TreeVariant {
    pub id: String,
    pub weight: f32,
    pub trunk_block: String,
    pub leaves_block: String,
    pub trunk_height: (i32, i32),
    pub canopy_radius: (i32, i32),
    pub canopy_height: (i32, i32),
    pub canopy_density: f32,
}

#[derive(Debug, Deserialize)]
struct TreeFamilyFile {
    #[serde(default)]
    name: String,
    #[serde(default)]
    variants: Vec<TreeVariantFile>,
}

#[derive(Debug, Deserialize)]
struct TreeVariantFile {
    #[serde(default)]
    id: String,
    #[serde(default = "default_weight")]
    weight: f32,
    #[serde(default = "default_trunk_block")]
    trunk_block: String,
    #[serde(default = "default_leaves_block")]
    leaves_block: String,
    #[serde(default = "default_trunk_height")]
    trunk_height: [i32; 2],
    #[serde(default = "default_canopy_radius")]
    canopy_radius: [i32; 2],
    #[serde(default = "default_canopy_height")]
    canopy_height: [i32; 2],
    #[serde(default = "default_canopy_density")]
    canopy_density: f32,
}

impl TreeVariantFile {
    fn into_runtime(self) -> TreeVariant {
        TreeVariant {
            id: if self.id.trim().is_empty() {
                "variant".to_string()
            } else {
                self.id
            },
            weight: self.weight.max(0.0),
            trunk_block: self.trunk_block,
            leaves_block: self.leaves_block,
            trunk_height: normalize_range(self.trunk_height),
            canopy_radius: normalize_range(self.canopy_radius),
            canopy_height: normalize_range(self.canopy_height),
            canopy_density: self.canopy_density.clamp(0.0, 1.0),
        }
    }
}

#[inline]
fn normalize_range(v: [i32; 2]) -> (i32, i32) {
    let a = v[0].max(1);
    let b = v[1].max(1);
    if a <= b { (a, b) } else { (b, a) }
}

fn collect_json_paths_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };

    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_json_paths_recursive(&path, out);
            continue;
        }
        if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("json"))
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
}

#[inline]
fn default_weight() -> f32 {
    1.0
}

#[inline]
fn default_trunk_block() -> String {
    "oak_log_block".to_string()
}

#[inline]
fn default_leaves_block() -> String {
    "oak_leaves_block".to_string()
}

#[inline]
fn default_trunk_height() -> [i32; 2] {
    [4, 6]
}

#[inline]
fn default_canopy_radius() -> [i32; 2] {
    [2, 3]
}

#[inline]
fn default_canopy_height() -> [i32; 2] {
    [3, 4]
}

#[inline]
fn default_canopy_density() -> f32 {
    0.86
}
