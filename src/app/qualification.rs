//! Magic and content qualification command adapters.

use crate::{identity, magic_qualification, mod_lint};
use std::path::PathBuf;

pub(super) fn magic(args: &[String], i: usize) {
    let Some(world) = args.get(i + 1).map(PathBuf::from) else {
        eprintln!(
            "usage: wildforge --magic-qualification <world> [--mods <directory>] [--output <report.txt>]"
        );
        std::process::exit(2);
    };
    let mods = args
        .iter()
        .position(|arg| arg == "--mods")
        .and_then(|index| args.get(index + 1))
        .map_or_else(|| PathBuf::from("mods"), PathBuf::from);
    match magic_qualification::audit_world(&world, &mods) {
        Ok(report) => {
            print!("{}", report.render());
            if let Some(output) = args
                .iter()
                .position(|arg| arg == "--output")
                .and_then(|index| args.get(index + 1))
                .map(PathBuf::from)
                && let Err(error) =
                    identity::atomic_write(&output, report.render().as_bytes(), false)
            {
                eprintln!("magic qualification report write failed: {error}");
                std::process::exit(1);
            }
            if !report.qualified {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("magic qualification failed: {error}");
            std::process::exit(1);
        }
    }
}

pub(super) fn mods(args: &[String], i: usize) {
    let Some(mods_dir) = args.get(i + 1).map(PathBuf::from) else {
        eprintln!("usage: wildforge --mod-qualification <mods_dir>");
        std::process::exit(2);
    };
    let report = mod_lint::qualify_mods(&mods_dir);
    print!("{}", report.render());
    if !report.is_qualified() {
        std::process::exit(1);
    }
}
