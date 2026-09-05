//! An exclusively allocated asset copy retained by runtime registry readers.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Only directories created by this owner are removed by Drop. Registry clones
/// share the owner through Arc; replacing a cache never invalidates a reader.
pub(crate) struct AssetSnapshot {
    root: PathBuf,
}

impl AssetSnapshot {
    pub(crate) fn copy_files(cache: &Path, source: &Path, relative: &[String]) -> io::Result<Self> {
        let snapshot = Self::allocate(cache)?;
        for name in relative {
            let destination = snapshot.root.join(name);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            // A copy, rather than a hard link, also isolates readers from
            // external edits to the ordinary cache directory.
            fs::copy(source.join(name), destination)?;
        }
        Ok(snapshot)
    }

    fn allocate(cache: &Path) -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let name = cache.file_name().ok_or_else(|| io::Error::new(
            io::ErrorKind::InvalidInput, "content cache has no directory name"))?;
        for _ in 0..64 {
            let mut sibling = OsString::from(name);
            sibling.push(format!(".reader-{}-{}", std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)));
            let root = cache.with_file_name(sibling);
            match fs::create_dir(&root) {
                Ok(()) => return Ok(Self { root }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(io::ErrorKind::AlreadyExists,
            "could not allocate an unused content snapshot directory"))
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for AssetSnapshot {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.root) {
            eprintln!("content: could not remove released asset snapshot: {error}");
        }
    }
}
