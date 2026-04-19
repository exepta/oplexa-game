use std::io;
use std::path::{Path, PathBuf};

pub fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| manifest_dir.to_path_buf())
}

pub fn workspace_path(relative: impl AsRef<Path>) -> PathBuf {
    workspace_root().join(relative)
}

pub fn ensure_workspace_cwd() -> io::Result<()> {
    std::env::set_current_dir(workspace_root())
}
