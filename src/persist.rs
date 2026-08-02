//! Crash-resistant replacement for small files that are rewritten whole.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Write beside `path`, flush the complete new value, and atomically replace.
///
/// Chunk region files have their own append-before-index protocol; this helper
/// is for metadata, profiles, and compact state files that are replaced whole.
pub(crate) fn atomic_write(path: &Path, bytes: &[u8], secret: bool) -> io::Result<()> {
    #[cfg(not(unix))]
    let _ = secret;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::other("invalid output path"))?;
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temp = path.with_file_name(format!(".{name}.{}.{}.tmp", std::process::id(), sequence));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    if secret {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        replace_file(&temp, path)?;
        sync_parent(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// Remove an optional replace-in-full file and durably publish the directory
/// entry change. A missing file already represents the requested state.
pub(crate) fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => {
            let parent = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            sync_parent(parent)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Publish a newly created directory without replacing an existing target.
/// Windows scanners can briefly hold a handle without delete sharing, so the
/// final creation rename gets the same bounded retry as atomic file writes.
pub(crate) fn publish_new_directory(source: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(not(windows))]
    {
        fs::rename(source, destination)
    }
    #[cfg(windows)]
    {
        retry_transient_windows_rename(|| fs::rename(source, destination))
    }
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> io::Result<()> {
    fs::File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(temp: &Path, path: &Path) -> io::Result<()> {
    fs::rename(temp, path)
}

#[cfg(windows)]
fn replace_file(temp: &Path, path: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // Windows Defender, indexers, and backup tools can briefly open a newly
    // replaced save without FILE_SHARE_DELETE. The kernel reports that race
    // as access denied or a sharing/lock violation. Retrying the same atomic
    // replacement is safe: the source remains beside the destination until
    // one MoveFileExW succeeds.
    retry_transient_windows_rename(|| {
        // SAFETY: both buffers are NUL-terminated UTF-16 paths and remain
        // alive for the duration of the call.
        let moved = unsafe {
            MoveFileExW(
                source.as_ptr(),
                destination.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if moved != 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    })
}

#[cfg(windows)]
fn retry_transient_windows_rename(mut operation: impl FnMut() -> io::Result<()>) -> io::Result<()> {
    const TRANSIENT_WINDOWS_ERRORS: [i32; 3] = [5, 32, 33];
    const RETRY_DELAYS_MS: [u64; 6] = [0, 1, 2, 4, 8, 16];
    for (attempt, delay_ms) in RETRY_DELAYS_MS.into_iter().enumerate() {
        if delay_ms != 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }
        let error = match operation() {
            Ok(()) => return Ok(()),
            Err(error) => error,
        };
        let transient = error
            .raw_os_error()
            .is_some_and(|code| TRANSIENT_WINDOWS_ERRORS.contains(&code));
        if !transient || attempt + 1 == RETRY_DELAYS_MS.len() {
            return Err(error);
        }
    }
    unreachable!("the Windows rename retry loop always returns")
}
