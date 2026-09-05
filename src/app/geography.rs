//! Arcane geography inspection, exports, and explicit retrogen commands.

use std::path::PathBuf;
use crate::{arcane_ecology, arcane_geography, planet_atlas, registry};

pub(super) fn dross_atlas(args: &[String], i: usize) {
    let Some(world) = args.get(i + 1).map(PathBuf::from) else {
        eprintln!(
            "usage: wildforge --arcane-atlas <world> --layer dross [--output <directory>]"
        );
        std::process::exit(2);
    };
    let layer = args
        .iter()
        .position(|arg| arg == "--layer")
        .and_then(|index| args.get(index + 1));
    if layer.is_none_or(|layer| layer != "dross") {
        eprintln!("arcane atlas currently requires --layer dross");
        std::process::exit(2);
    }
    let output = args
        .iter()
        .position(|arg| arg == "--output")
        .and_then(|index| args.get(index + 1))
        .map_or_else(|| world.join("diagnostics/arcane-atlas"), PathBuf::from);
    let result = (|| -> Result<Vec<String>, String> {
        let atlas =
            planet_atlas::PlanetAtlas::load(&world).map_err(|error| error.to_string())?;
        let geography = arcane_geography::ArcaneGeography::load(&world, &atlas)
            .map_err(|error| error.to_string())?;
        geography
            .export_dross_diagnostics(&atlas, &output)
            .map_err(|error| error.to_string())
    })();
    match result {
        Ok(files) => println!(
            "exported {} dross atlas views to {}",
            files.len(),
            output.display()
        ),
        Err(error) => {
            eprintln!("dross atlas export failed: {error}");
            std::process::exit(1);
        }
    }
}

pub(super) fn audit(args: &[String], i: usize) {
    let Some(world) = args.get(i + 1).map(PathBuf::from) else {
        eprintln!("usage: wildforge --arcane-geography-audit <world>");
        std::process::exit(2);
    };
    match arcane_geography::audit_world(&world) {
        Ok((audit, custody_matches)) => {
            print!("{}", audit.render());
            println!("Ledger custody matches: {custody_matches}");
            if !audit.is_balanced() || !custody_matches {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("arcane geography audit failed: {error}");
            std::process::exit(1);
        }
    }
}

pub(super) fn ecology(args: &[String], i: usize) {
    let Some(world) = args.get(i + 1).map(PathBuf::from) else {
        eprintln!("usage: wildforge --arcane-ecology-audit <world>");
        std::process::exit(2);
    };
    let result = (|| -> Result<(arcane_ecology::EcologyAudit, bool), String> {
        let atlas =
            planet_atlas::PlanetAtlas::load(&world).map_err(|error| error.to_string())?;
        let registry = registry::load_validated(std::path::Path::new("mods"))
            .map_err(|error| error.to_string())?;
        let geography = arcane_geography::ArcaneGeography::load(&world, &atlas)
            .map_err(|error| error.to_string())?;
        let ecology = arcane_ecology::audit(&registry, &geography.dynamic.ecology)?;
        let (geography_audit, custody_matches) =
            arcane_geography::audit_world(&world).map_err(|error| error.to_string())?;
        Ok((ecology, geography_audit.is_balanced() && custody_matches))
    })();
    match result {
        Ok((audit, balanced)) => {
            print!("{}", audit.render());
            if !balanced {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("arcane ecology audit failed: {error}");
            std::process::exit(1);
        }
    }
}

pub(super) fn export(args: &[String], i: usize) {
    let Some(world) = args.get(i + 1).map(PathBuf::from) else {
        eprintln!("usage: wildforge --arcane-geography-export <world> --output <directory>");
        std::process::exit(2);
    };
    let Some(output) = args
        .iter()
        .position(|arg| arg == "--output")
        .and_then(|index| args.get(index + 1))
        .map(PathBuf::from)
    else {
        eprintln!("usage: wildforge --arcane-geography-export <world> --output <directory>");
        std::process::exit(2);
    };
    let result = (|| {
        let atlas = planet_atlas::PlanetAtlas::load(&world)?;
        let geography = arcane_geography::ArcaneGeography::load(&world, &atlas)
            .map_err(|error| planet_atlas::AtlasError::Corrupt(error.to_string()))?;
        geography
            .export_diagnostics(&atlas, &output)
            .map_err(|error| planet_atlas::AtlasError::Corrupt(error.to_string()))
    })();
    match result {
        Ok(report) => println!(
            "exported {} arcane maps for {} cells to {}",
            report.maps.len(),
            report.cell_count,
            output.display()
        ),
        Err(error) => {
            eprintln!("arcane geography export failed: {error}");
            std::process::exit(1);
        }
    }
}

pub(super) fn retrogen(args: &[String], i: usize) {
    let Some(world) = args.get(i + 1).map(PathBuf::from) else {
        eprintln!("usage: wildforge --arcane-geography-retrogen <world>");
        std::process::exit(2);
    };
    let result = (|| -> Result<usize, String> {
        let atlas =
            planet_atlas::PlanetAtlas::load(&world).map_err(|error| error.to_string())?;
        let registry = registry::load_validated(std::path::Path::new("mods"))
            .map_err(|error| error.to_string())?;
        let mut geography = arcane_geography::ArcaneGeography::load(&world, &atlas)
            .map_err(|error| error.to_string())?;
        let added = geography
            .apply_retrogen(&atlas, &registry)
            .map_err(|error| error.to_string())?;
        geography
            .save_retrogen(&world)
            .map_err(|error| error.to_string())?;
        Ok(added)
    })();
    match result {
        Ok(added) => println!("arcane geography retrogen added {added} finite sites"),
        Err(error) => {
            eprintln!("arcane geography retrogen failed: {error}");
            std::process::exit(1);
        }
    }
}
