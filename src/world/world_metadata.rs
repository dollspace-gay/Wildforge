//! World metadata and compatibility adapters.

use std::fs;

pub use crate::planet::WORLD_TOPOLOGY;
// Version 10 makes visible biome terrain obey the atlas's zonal climate and
// requires physically backed water before applying riparian vegetation.
pub const WORLD_GENERATOR_VERSION: u32 = 10;
pub(super) const MIN_SUPPORTED_WORLD_GENERATOR_VERSION: u32 = 9;

#[derive(Clone, Debug)]
pub struct WorldMeta {
    pub seed: u32,
    pub mode: String,
    pub ire: f32,
    pub day: u32,
    /// The client camera mode chosen for this world (`first`/`third`/`orbit`).
    pub camera: String,
}

fn meta_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (found, value) = line.split_once('=')?;
        (found.trim() == key).then(|| value.trim().trim_matches('"'))
    })
}

fn invalid_world_meta(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into())
}

/// Read and validate the planetary save header.
///
/// `Ok(None)` means the path has never contained a world. A legacy seed file
/// or a `world.toml` without the exact topology contract is an error: planar
/// coordinates must never be silently reinterpreted as planetary ones.
pub fn load_world_meta(dir: &std::path::Path) -> std::io::Result<Option<WorldMeta>> {
    let path = dir.join("world.toml");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if dir.join("seed").exists() {
                return Err(invalid_world_meta(
                    "legacy flat Wildforge world: this build only opens cube_sphere_v1 worlds",
                ));
            }
            return Ok(None);
        }
        Err(error) => return Err(error),
    };

    let topology = meta_value(&text, "topology").ok_or_else(|| {
        invalid_world_meta(
            "flat or unversioned Wildforge world: missing topology = \"cube_sphere_v1\"",
        )
    })?;
    if topology != WORLD_TOPOLOGY {
        return Err(invalid_world_meta(format!(
            "unsupported world topology {topology:?}; expected {WORLD_TOPOLOGY:?}"
        )));
    }

    let face_blocks = meta_value(&text, "face_blocks")
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| invalid_world_meta("planetary world is missing a valid face_blocks"))?;
    if face_blocks != crate::planet::FACE_BLOCKS {
        return Err(invalid_world_meta(format!(
            "unsupported planetary face size {face_blocks}; expected {}",
            crate::planet::FACE_BLOCKS
        )));
    }

    let world_height = meta_value(&text, "world_height")
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| invalid_world_meta("planetary world is missing a valid world_height"))?;
    if world_height != CHUNK_Y as u16 {
        return Err(invalid_world_meta(format!(
            "unsupported world height {world_height}; expected {CHUNK_Y}"
        )));
    }

    let radius = meta_value(&text, "planet_radius")
        .and_then(|value| value.parse::<f64>().ok())
        .ok_or_else(|| invalid_world_meta("planetary world is missing a valid planet_radius"))?;
    if (radius - crate::planet::PLANET_RADIUS).abs() > 0.001 {
        return Err(invalid_world_meta(format!(
            "unsupported planet radius {radius}; expected {:.6}",
            crate::planet::PLANET_RADIUS
        )));
    }

    let generator_version = meta_value(&text, "generator_version")
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or_else(|| invalid_world_meta("planetary world is missing a generator_version"))?;
    if !(MIN_SUPPORTED_WORLD_GENERATOR_VERSION..=WORLD_GENERATOR_VERSION)
        .contains(&generator_version)
    {
        return Err(invalid_world_meta(format!(
            "unsupported generator version {generator_version}; supported {MIN_SUPPORTED_WORLD_GENERATOR_VERSION}..={WORLD_GENERATOR_VERSION}"
        )));
    }

    let seed = meta_value(&text, "seed")
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or_else(|| invalid_world_meta("world metadata is missing a valid seed"))?;
    let mode = meta_value(&text, "mode").unwrap_or("survival").to_string();
    let camera = meta_value(&text, "camera").unwrap_or("first").to_string();
    let ire = meta_value(&text, "ire")
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(0.0)
        .clamp(0.0, 100.0);
    let day = meta_value(&text, "day")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    Ok(Some(WorldMeta {
        seed,
        mode,
        camera,
        ire,
        day,
    }))
}

/// (seed, mode, ire) from a validated planetary `world.toml`.
pub fn read_world_meta(dir: &std::path::Path) -> (Option<u32>, String, f32) {
    let (seed, mode, ire, _, _) = read_world_meta_full(dir);
    (seed, mode, ire)
}

/// Full metadata: (seed, mode, ire, day, camera). Dynamic local weather
/// lives in the atlas snapshot, never in a world-wide metadata field.
pub fn read_world_meta_full(dir: &std::path::Path) -> (Option<u32>, String, f32, u32, String) {
    match load_world_meta(dir) {
        Ok(Some(meta)) => (Some(meta.seed), meta.mode, meta.ire, meta.day, meta.camera),
        Ok(None) | Err(_) => (None, "survival".to_string(), 0.0, 0, "first".to_string()),
    }
}

pub fn write_world_meta(
    dir: &std::path::Path,
    seed: u32,
    mode: &str,
    ire: f32,
) -> std::io::Result<()> {
    write_world_meta_full(dir, seed, mode, ire, 0, "first")
}

pub fn write_world_meta_full(
    dir: &std::path::Path,
    seed: u32,
    mode: &str,
    ire: f32,
    day: u32,
    camera: &str,
) -> std::io::Result<()> {
    let text = format!(
        "topology = \"{WORLD_TOPOLOGY}\"\nface_blocks = {}\nworld_height = {CHUNK_Y}\nplanet_radius = {:.6}\ngenerator_version = {WORLD_GENERATOR_VERSION}\nseed = {seed}\nmode = \"{mode}\"\ncamera = \"{camera}\"\nire = {ire:.2}\nday = {day}\n",
        crate::planet::FACE_BLOCKS,
        crate::planet::PLANET_RADIUS,
    );
    crate::identity::atomic_write(&dir.join("world.toml"), text.as_bytes(), false)
}

/// List compatible planetary worlds under `dir`: (name, seed), sorted.
pub fn list_worlds(dir: &std::path::Path) -> Vec<(String, u32)> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || !e.path().is_dir() {
                continue;
            }
            if let Ok(Some(meta)) = load_world_meta(&e.path())
                && crate::planet_atlas::PlanetAtlas::is_committed(&e.path())
            {
                out.push((name, meta.seed));
            }
        }
    }
    out.sort();
    out
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldBrowserEntry {
    pub name: String,
    pub playable: bool,
    pub status: String,
}

/// Cheap title-screen inspection. Full checksums and bounded file decoding
/// remain in `PlanetAtlas::load` on the cancellable entry worker; browsing a
/// save must not synchronously load roughly 180 MiB of planet state.
pub fn inspect_worlds(dir: &std::path::Path) -> Vec<WorldBrowserEntry> {
    let mut entries = Vec::new();
    let Ok(read_dir) = fs::read_dir(dir) else {
        return entries;
    };
    for entry in read_dir.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || !entry.path().is_dir() {
            continue;
        }
        let status = match load_world_meta(&entry.path()) {
            Err(error) => WorldBrowserEntry {
                name,
                playable: false,
                status: format!("INCOMPATIBLE: {error}"),
            },
            Ok(None) => WorldBrowserEntry {
                name,
                playable: false,
                status: "INCOMPLETE: NO PLANET METADATA".into(),
            },
            Ok(Some(_)) if !crate::planet_atlas::PlanetAtlas::is_committed(&entry.path()) => {
                WorldBrowserEntry {
                    name,
                    playable: false,
                    status: "INCOMPLETE OR CORRUPT PLANET ATLAS".into(),
                }
            }
            Ok(Some(_)) => {
                let manifest = fs::read_to_string(
                    crate::planet_atlas::PlanetAtlas::planet_dir(&entry.path())
                        .join("manifest.toml"),
                )
                .unwrap_or_default();
                let atlas = meta_value(&manifest, "format_version").unwrap_or("?");
                let content = meta_value(&manifest, "content_hash")
                    .and_then(|value| value.parse::<u64>().ok())
                    .map(|value| format!("{value:016x}"))
                    .unwrap_or_else(|| "unknown".into());
                WorldBrowserEntry {
                    name,
                    playable: true,
                    status: format!(
                        "READY  GENERATOR {WORLD_GENERATOR_VERSION}  ATLAS {atlas}  CONTENT {}",
                        &content[..content.len().min(8)]
                    ),
                }
            }
        };
        entries.push(status);
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

