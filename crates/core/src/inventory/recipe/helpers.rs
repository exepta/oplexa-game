use std::fs;
use std::path::{Path, PathBuf};

/// Runs the `collect_recipe_json_paths` routine for collect recipe json paths in the `core::inventory::recipe::loader` module.
pub fn collect_recipe_json_paths(dir: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_recipe_json_paths(path.as_path(), paths);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            paths.push(path);
        }
    }
}