//! Offline world and atlas creation with ordered progress reporting.

use crate::{planet_atlas, registry, world};
use std::{path::PathBuf, sync::Arc};

pub(super) fn create(args: &[String], i: usize) {
    let Some(seed) = args.get(i + 1).and_then(|value| value.parse::<u32>().ok()) else {
        eprintln!("usage: wildforge --create-world <seed> --output <directory>");
        std::process::exit(2);
    };
    let Some(output) = args
        .iter()
        .position(|arg| arg == "--output")
        .and_then(|index| args.get(index + 1))
        .map(PathBuf::from)
    else {
        eprintln!("usage: wildforge --create-world <seed> --output <directory>");
        std::process::exit(2);
    };
    let registry = match registry::load_validated(std::path::Path::new("mods")) {
        Ok(registry) => Arc::new(registry),
        Err(error) => {
            eprintln!("world creation rejected: {error}");
            std::process::exit(1);
        }
    };
    let content_hash = planet_atlas::genesis_content_hash(std::path::Path::new("mods"));
    let cancel = planet_atlas::CancellationToken::default();
    eprintln!("world: seed {seed}, output {}", output.display());
    if let Err(error) = world::create_world_atomic(
        &output,
        seed,
        "survival",
        content_hash,
        registry,
        &cancel,
        |progress| match progress {
            world::WorldCreationProgress::Atlas(progress) => eprintln!(
                "world: atlas [{}/{}] {}",
                progress.completed_stages + 1,
                progress.total_stages,
                progress.stage.label()
            ),
            world::WorldCreationProgress::Arcane(progress) => eprintln!(
                "world: arcane [{}/{}] {}",
                progress.completed_stages + 1,
                progress.total_stages,
                progress.stage.label()
            ),
            world::WorldCreationProgress::Homeland {
                stage,
                completed,
                total,
            } => eprintln!("world: homeland [{completed}/{total}] {stage}"),
        },
    ) {
        eprintln!("world creation failed: {error}");
        std::process::exit(1);
    }
    eprintln!("world: complete");
}

pub(super) fn generate_atlas(args: &[String], i: usize) {
    let Some(seed) = args.get(i + 1).and_then(|value| value.parse::<u32>().ok()) else {
        eprintln!("usage: wildforge --generate-atlas <seed> --output <directory>");
        std::process::exit(2);
    };
    let Some(output) = args
        .iter()
        .position(|arg| arg == "--output")
        .and_then(|index| args.get(index + 1))
        .map(PathBuf::from)
    else {
        eprintln!("usage: wildforge --generate-atlas <seed> --output <directory>");
        std::process::exit(2);
    };
    let content_hash = planet_atlas::genesis_content_hash(std::path::Path::new("mods"));
    let cancel = planet_atlas::CancellationToken::default();
    eprintln!("atlas: seed {seed}, output {}", output.display());
    let atlas = match planet_atlas::PlanetAtlas::generate(
        seed,
        content_hash,
        planet_atlas::AtlasConfig::production(),
        &cancel,
        |progress| {
            eprintln!(
                "atlas: [{}/{}] {}",
                progress.completed_stages + 1,
                progress.total_stages,
                progress.stage.label()
            );
        },
    ) {
        Ok(atlas) => atlas,
        Err(error) => {
            eprintln!("atlas: generation failed: {error}");
            std::process::exit(1);
        }
    };
    if let Err(error) = atlas
        .write_new(&output)
        .and_then(|_| planet_atlas::export_diagnostics(&atlas, &output).map(|_| ()))
    {
        eprintln!("atlas: export failed: {error}");
        std::process::exit(1);
    }
    eprintln!("atlas: complete");
}
