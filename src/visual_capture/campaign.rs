//! Selection and source identity for a complete, independently stored campaign.

use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    root: String,
}

pub(super) fn evidence_root(root: &Path) -> Result<PathBuf, String> {
    let (_, selection): (Vec<u8>, Selection) = super::read_toml(
        &root.join("screenshots/current-campaign.toml"),
        "current visual campaign",
    )?;
    let path = Path::new(&selection.root);
    if !selection.root.starts_with("screenshots/")
        || !path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err("campaign root must be a repository-relative screenshots directory".into());
    }
    Ok(root.join(path))
}

pub(super) fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| matches!(i, 4 | 7) || b.is_ascii_digit())
        && value[5..7]
            .parse::<u8>()
            .is_ok_and(|month| (1..=12).contains(&month))
        && value[8..10]
            .parse::<u8>()
            .is_ok_and(|day| (1..=31).contains(&day))
}

/// Hash production sources, verifiers, and shipped content, including new files.
/// A module move cannot leave its new implementation outside a fixed file list.
pub(super) fn source_sha256(root: &Path) -> Result<String, String> {
    let mut files = ["Cargo.toml", "Cargo.lock", "build.rs"]
        .map(PathBuf::from)
        .to_vec();
    for directory in ["src", "base", "packs", "mods/gems", "tools"] {
        collect(root, Path::new(directory), &mut files)?;
    }
    files.sort();
    let mut source = Vec::new();
    for relative in files {
        let bytes = fs::read(root.join(&relative)).map_err(|error| {
            format!("read qualification source {}: {error}", relative.display())
        })?;
        source.extend_from_slice(relative.to_string_lossy().replace('\\', "/").as_bytes());
        source.push(0);
        source.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        source.extend_from_slice(&bytes);
    }
    Ok(super::sha256_hex(&source))
}

fn collect(root: &Path, relative: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(root.join(relative))
        .map_err(|error| format!("list qualification source {}: {error}", relative.display()))?
    {
        let entry = entry.map_err(|error| format!("read qualification entry: {error}"))?;
        let path = relative.join(entry.file_name());
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            collect(root, &path, files)?;
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(extension, "rs" | "wgsl" | "png" | "toml" | "rhai" | "py")
            })
        {
            files.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "wildforge-campaign-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed),
            ));
            fs::create_dir(&root).unwrap();
            for directory in ["src", "base", "packs", "mods/gems", "tools", "screenshots"] {
                fs::create_dir_all(root.join(directory)).unwrap();
            }
            for file in ["Cargo.toml", "Cargo.lock", "build.rs"] {
                fs::write(root.join(file), file).unwrap();
            }
            Self(root)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn moved_and_new_source_is_covered_but_folder_guides_are_not() {
        let fixture = Fixture::new();
        let before = source_sha256(&fixture.0).unwrap();
        fs::create_dir(fixture.0.join("src/new_owner")).unwrap();
        fs::write(fixture.0.join("src/new_owner/README.md"), "guide").unwrap();
        assert_eq!(source_sha256(&fixture.0).unwrap(), before);
        fs::write(fixture.0.join("src/new_owner/state.rs"), "source").unwrap();
        let added = source_sha256(&fixture.0).unwrap();
        assert_ne!(added, before);
        fs::rename(
            fixture.0.join("src/new_owner/state.rs"),
            fixture.0.join("src/state.rs"),
        )
        .unwrap();
        assert_ne!(source_sha256(&fixture.0).unwrap(), added);
        fs::remove_dir_all(fixture.0.join("base")).unwrap();
        assert!(source_sha256(&fixture.0).is_err());
    }

    #[test]
    fn campaign_selection_rejects_escaping_and_missing_paths() {
        let fixture = Fixture::new();
        assert!(evidence_root(&fixture.0).is_err());
        for value in ["/tmp/campaign", "screenshots/../private", "src/evidence"] {
            fs::write(
                fixture.0.join("screenshots/current-campaign.toml"),
                format!("root = {value:?}\n"),
            )
            .unwrap();
            assert!(evidence_root(&fixture.0).is_err());
        }
        fs::write(
            fixture.0.join("screenshots/current-campaign.toml"),
            "root = \"screenshots/campaign\"\n",
        )
        .unwrap();
        assert_eq!(
            evidence_root(&fixture.0).unwrap(),
            fixture.0.join("screenshots/campaign")
        );
    }

    #[test]
    fn rust_freshness_matches_the_python_campaign_fingerprint() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let output = Command::new("python3")
            .args(["-c", "from pathlib import Path; from visual_campaign.provenance import source_sha256; print(source_sha256(Path.cwd()))"])
            .env("PYTHONPATH", root.join("tools"))
            .current_dir(root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            source_sha256(root).unwrap(),
            String::from_utf8(output.stdout).unwrap().trim()
        );
    }
}
