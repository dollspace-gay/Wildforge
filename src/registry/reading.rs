//! File discovery distinguishes absent optional content from unreadable content.

use std::io;
use std::path::{Path, PathBuf};

pub(super) fn optional(dir: &Path, name: &str) -> Result<Option<String>, String> {
    match std::fs::read_to_string(dir.join(name)) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{name}: {error}")),
    }
}

pub(super) fn mod_dirs(root: &Path) -> io::Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut dirs = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path.metadata()?.is_dir() && path.join("mod.toml").try_exists()? {
            dirs.push(path);
        }
    }
    dirs.sort();
    Ok(dirs)
}
