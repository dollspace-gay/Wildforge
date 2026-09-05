//! Validate and publish a complete host content transfer before replacing a registry.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::registry::{self, Registry};
use crate::content_files::AssetSnapshot;

#[derive(Debug, thiserror::Error)]
pub(crate) enum TransferError {
    #[error("host content arrived after world admission began")]
    UnexpectedPhase,
    #[error("invalid host content path: {0}")]
    InvalidPath(String),
    #[error("duplicate host content path: {0}")]
    DuplicatePath(String),
    #[error("host content cache is already being replaced")]
    Busy,
    #[error("host content I/O: {0}")]
    Io(#[from] io::Error),
    #[error("host content failed validation: {0}")]
    InvalidRegistry(String),
    #[error("host content publication failed: {publish}; restore failed: {restore}; previous content retained at {backup}")]
    Restore {
        publish: io::Error,
        restore: io::Error,
        backup: PathBuf,
    },
}

/// A fixed sibling directory is both the exclusive writer claim and recovery
/// workspace. Refuse a surviving claim instead of deleting another operation.
struct Publication {
    root: PathBuf,
    preserve: bool,
}

impl Publication {
    fn acquire(cache: &Path) -> Result<Self, TransferError> {
        if let Some(parent) = cache.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let root = cache.with_extension("transfer");
        match std::fs::create_dir(&root) {
            Ok(()) => Ok(Self { root, preserve: false }),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Err(TransferError::Busy),
            Err(error) => Err(error.into()),
        }
    }
}

impl Drop for Publication {
    fn drop(&mut self) {
        if !self.preserve
            && let Err(error) = std::fs::remove_dir_all(&self.root)
        {
            eprintln!("guest content: could not remove transfer workspace: {error}");
        }
    }
}

/// Only portable, relative data-file names may reach the cache. Scripts are
/// host-owned and excluded by the sender's content inventory.
fn validate_paths(files: &[(String, Vec<u8>)]) -> Result<(), TransferError> {
    let mut seen = HashSet::with_capacity(files.len());
    for (name, _) in files {
        if name.contains(['\\', ':', '\0'])
            || name.split('/').any(|part| matches!(part, "" | "." | ".."))
            || Path::new(name)
                .extension()
                .is_some_and(|extension| extension.as_encoded_bytes().eq_ignore_ascii_case(b"rhai"))
        {
            return Err(TransferError::InvalidPath(name.clone()));
        }
        if !seen.insert(name) {
            return Err(TransferError::DuplicatePath(name.clone()));
        }
    }
    Ok(())
}

/// Keep the established cache path (and its legacy hash contribution). Loading
/// the candidate is private until every file and content definition is valid.
pub(crate) fn install(
    cache: &Path,
    files: Vec<(String, Vec<u8>)>,
) -> Result<Arc<Registry>, TransferError> {
    validate_paths(&files)?;
    let mut publication = Publication::acquire(cache)?;
    let staging = publication.root.join("incoming");
    let backup = publication.root.join("previous");
    std::fs::create_dir(&staging)?;
    let mut relative_paths = Vec::with_capacity(files.len());
    for (relative, bytes) in files {
        let path = staging.join(&relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, bytes)?;
        relative_paths.push(relative);
    }
    let mut candidate = registry::load_validated(&staging)
        .map_err(|error| TransferError::InvalidRegistry(error.to_string()))?;

    // Retain reader storage before replacing any published cache. A failed
    // copy leaves the old cache untouched and cleans its own partial tree.
    let snapshot = AssetSnapshot::copy_files(cache, &staging, &relative_paths)?;

    let previous = match std::fs::symlink_metadata(cache) {
        Ok(metadata) if metadata.is_dir() => {
            std::fs::rename(cache, &backup)?;
            true
        }
        Ok(_) => return Err(io::Error::other("content cache is not a directory").into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    if let Err(publish) = std::fs::rename(&staging, cache) {
        if previous && let Err(restore) = std::fs::rename(&backup, cache) {
            publication.preserve = true;
            return Err(TransferError::Restore { publish, restore, backup });
        }
        return Err(publish.into());
    }
    // The public cache keeps its historical path/hash convention. Live readers
    // point to their retained copy instead of this replaceable publication.
    candidate.retain_asset_snapshot(&staging, snapshot);
    candidate.content_hash = crate::planet_atlas::genesis_content_hash(cache);
    Ok(Arc::new(candidate))
}
