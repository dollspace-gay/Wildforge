//! Creation and compatibility adapters.

use super::{World, write_world_meta};
use crate::registry::Registry;
use std::{fs, sync::Arc};

/// Create a complete production world off to the side and publish it with a
/// single directory rename. The browser therefore never sees `world.toml`
/// without both its committed immutable atlas and qualified homeland.
pub enum WorldCreationProgress {
    Atlas(crate::planet_atlas::AtlasProgress),
    Arcane(crate::arcane_geography::ArcaneGeographyProgress),
    Homeland {
        stage: String,
        completed: usize,
        total: usize,
    },
}

type HomelandPreparation<'a> = (Arc<Registry>, &'a mut dyn FnMut(WorldCreationProgress));

pub fn create_world_atomic(
    destination: &std::path::Path,
    seed: u32,
    mode: &str,
    content_hash: u64,
    reg: Arc<Registry>,
    cancel: &crate::planet_atlas::CancellationToken,
    mut progress: impl FnMut(WorldCreationProgress),
) -> std::io::Result<()> {
    reg.validate()
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    if destination.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("world path already exists: {}", destination.display()),
        ));
    }
    let atlas = crate::planet_atlas::PlanetAtlas::generate(
        seed,
        content_hash,
        crate::planet_atlas::AtlasConfig::production(),
        cancel,
        |atlas| progress(WorldCreationProgress::Atlas(atlas)),
    )
    .map_err(std::io::Error::other)?;
    publish_created_world(
        destination,
        seed,
        mode,
        atlas,
        cancel,
        Some((reg, &mut progress)),
    )
}

#[cfg(test)]
pub fn create_world_fixture_atomic(
    destination: &std::path::Path,
    seed: u32,
    mode: &str,
    side: u16,
    cancel: &crate::planet_atlas::CancellationToken,
    progress: impl FnMut(crate::planet_atlas::AtlasProgress),
) -> std::io::Result<()> {
    create_world_atomic_with_config(
        destination,
        seed,
        mode,
        0,
        crate::planet_atlas::AtlasConfig::fixture(side),
        cancel,
        progress,
    )
}

#[cfg(test)]
fn create_world_atomic_with_config(
    destination: &std::path::Path,
    seed: u32,
    mode: &str,
    content_hash: u64,
    config: crate::planet_atlas::AtlasConfig,
    cancel: &crate::planet_atlas::CancellationToken,
    progress: impl FnMut(crate::planet_atlas::AtlasProgress),
) -> std::io::Result<()> {
    if destination.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("world path already exists: {}", destination.display()),
        ));
    }
    let atlas =
        crate::planet_atlas::PlanetAtlas::generate(seed, content_hash, config, cancel, progress)
            .map_err(std::io::Error::other)?;
    publish_created_world(destination, seed, mode, atlas, cancel, None)
}

fn publish_created_world(
    destination: &std::path::Path,
    seed: u32,
    mode: &str,
    atlas: crate::planet_atlas::PlanetAtlas,
    cancel: &crate::planet_atlas::CancellationToken,
    mut homeland: Option<HomelandPreparation<'_>>,
) -> std::io::Result<()> {
    if cancel.is_cancelled() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "planet creation cancelled",
        ));
    }
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    fs::create_dir_all(parent)?;
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| std::io::Error::other("invalid world path"))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(".{name}.creating.{}.{}", std::process::id(), stamp));
    fs::create_dir(&temporary)?;
    let result = (|| {
        atlas.write_new(&temporary).map_err(std::io::Error::other)?;
        if cancel.is_cancelled() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "planet creation cancelled",
            ));
        }
        write_world_meta(&temporary, seed, mode, 0.0)?;
        if let Some((reg, progress)) = &mut homeland {
            let geography =
                crate::arcane_geography::ArcaneGeography::generate(&atlas, reg, cancel, |arcane| {
                    progress(WorldCreationProgress::Arcane(arcane))
                })
                .map_err(std::io::Error::other)?;
            geography
                .write_new(&temporary)
                .map_err(std::io::Error::other)?;
            let mut world =
                World::load_or_create_cancellable(temporary.clone(), Arc::clone(reg), cancel)?;
            world.prepare_common_spawn_cancellable(cancel, |stage, completed, total| {
                progress(WorldCreationProgress::Homeland {
                    stage: stage.to_owned(),
                    completed,
                    total,
                });
            })?;
            if cancel.is_cancelled() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "planet creation cancelled during homeland preparation",
                ));
            }
        }
        crate::persist::publish_new_directory(&temporary, destination).map_err(|error| {
            std::io::Error::new(
                error.kind(),
                format!("could not publish the completed world directory: {error}"),
            )
        })?;
        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&temporary);
    }
    result
}

/// Publish a complete development qualification world from an already
/// committed production atlas. This is deliberately narrower than ordinary
/// world creation: it exists so read-only visual locators can select one
/// immutable atlas and the capture harness can materialize that exact world
/// without regenerating or silently substituting a different planet.
pub(crate) fn create_qualification_world_from_atlas(
    destination: &std::path::Path,
    atlas: crate::planet_atlas::PlanetAtlas,
    reg: Arc<Registry>,
    cancel: &crate::planet_atlas::CancellationToken,
    mut progress: impl FnMut(WorldCreationProgress),
) -> std::io::Result<()> {
    if destination.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("world path already exists: {}", destination.display()),
        ));
    }
    let seed = atlas.manifest.seed;
    publish_created_world(
        destination,
        seed,
        "survival",
        atlas,
        cancel,
        Some((reg, &mut progress)),
    )
}
