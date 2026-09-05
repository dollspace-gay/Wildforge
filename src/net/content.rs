//! Mod content identity and transfer inventory, independent of transport.
//!
//! Historical documentation remains content. Only explicitly marked new
//! folder guides opt out, so adding maintenance instructions cannot change
//! an existing world's content identity or signed provenance.

use std::path::{Path, PathBuf};

const GUIDE_MARKER: &[u8] = b"<!-- wildforge:guide -->";

fn is_folder_guide(path: &Path, bytes: &[u8]) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("README.md" | "AGENTS.md")
    ) && bytes.starts_with(GUIDE_MARKER)
}

fn content_paths(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_none_or(|extension| extension != "rhai") {
                out.push(path);
            }
        }
    }
    let mut paths = Vec::new();
    walk(dir, &mut paths);
    paths.sort();
    paths
}

/// Hash mod data with the historical path/byte ordering.
///
/// Host-only scripts and explicitly marked folder guides are excluded.
/// Unmarked documentation retains its historical identity contribution.
pub fn content_hash(dir: &Path) -> u64 {
    fn hash_bytes(hash: &mut u64, bytes: &[u8]) {
        for &byte in bytes {
            *hash = hash.wrapping_mul(0x100000001b3) ^ u64::from(byte);
        }
    }
    let mut hash = 0xcbf29ce484222325;
    for path in content_paths(dir) {
        let bytes = std::fs::read(&path);
        if bytes
            .as_ref()
            .is_ok_and(|bytes| is_folder_guide(&path, bytes))
        {
            continue;
        }
        // Preserve even the legacy unreadable-file behavior: its path counts.
        hash_bytes(&mut hash, path.to_string_lossy().as_bytes());
        if let Ok(bytes) = bytes {
            hash_bytes(&mut hash, &bytes);
        }
    }
    hash
}

/// Collect transferable mod files as sorted relative paths and bytes.
pub fn collect_mod_files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for path in content_paths(dir) {
        if let (Ok(relative), Ok(bytes)) = (path.strip_prefix(dir), std::fs::read(&path))
            && !is_folder_guide(&path, &bytes)
        {
            out.push((relative.to_string_lossy().replace('\\', "/"), bytes));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[cfg(test)]
mod tests {
    use super::{GUIDE_MARKER, collect_mod_files, content_hash};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "wildforge-content-guides-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(root.join("example")).unwrap();
            Self(root)
        }

        fn write(&self, relative: &str, bytes: &[u8]) {
            std::fs::write(self.0.join(relative), bytes).unwrap();
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn new_guides_preserve_transport_and_signed_genesis_identity() {
        let fixture = Fixture::new();
        fixture.write("README.md", b"Historical mod documentation");
        fixture.write("example/mod.toml", b"id = 'example'\nworld_api = 2\n");
        fixture.write("example/main.rhai", b"host-only script");
        let files = collect_mod_files(&fixture.0);
        let hash = content_hash(&fixture.0);
        let genesis = crate::planet_atlas::genesis_content_hash(&fixture.0);

        // Independent characterization of the pre-refactor hash: full paths
        // followed by bytes, sorted by path, with the host script omitted.
        let mut old_hash = 0xcbf29ce484222325_u64;
        for (relative, bytes) in &files {
            let path = fixture.0.join(relative);
            for byte in path.to_string_lossy().as_bytes().iter().chain(bytes) {
                old_hash = old_hash.wrapping_mul(0x100000001b3) ^ u64::from(*byte);
            }
        }
        assert_eq!(hash, old_hash);
        assert_eq!(files.len(), 2, "legacy README remains transferable");
        fixture.write("AGENTS.md", GUIDE_MARKER);
        fixture.write("example/README.md", GUIDE_MARKER);
        fixture.write("example/AGENTS.md", GUIDE_MARKER);
        assert_eq!(content_hash(&fixture.0), hash);
        assert_eq!(collect_mod_files(&fixture.0), files);
        assert_eq!(
            crate::planet_atlas::genesis_content_hash(&fixture.0),
            genesis
        );

        fixture.write("README.md", b"Changed historical documentation");
        assert_ne!(content_hash(&fixture.0), hash);
    }

    #[test]
    fn content_cannot_opt_out_by_borrowing_a_guide_marker() {
        let fixture = Fixture::new();
        let empty = content_hash(&fixture.0);
        fixture.write("example/items.toml", GUIDE_MARKER);
        fixture.write("example/art.png", GUIDE_MARKER);
        fixture.write("example/notes.md", GUIDE_MARKER);
        assert_ne!(content_hash(&fixture.0), empty);
        assert_eq!(collect_mod_files(&fixture.0).len(), 3);
    }
}
